// The app's state and the actions on it. Components read `state` and call these.

import { computed, reactive, watch } from "vue";
import { fromRecord, nextIdAfter, projectKey, toProjectFile, type RestoredStatus, type SavedScan } from "./projectRecord";
import * as api from "./api";
import type { AppSettings, Box, DetectEvent, Flag, IdBox, ModelStatus } from "./api";
import { assessClose, boxesKey } from "./closeGuard";
import { applyTheme } from "./appearance";
import { resizeBox, type SizeChange } from "./cropSize";
import { turnBox } from "./cropTurn";
import { History, sameBoxes } from "./history";
import { parseTheme } from "./theme";

export type ScanStatus = "queued" | "detecting" | "pending" | "flagged" | "approved" | "error";

export interface BoxItem extends IdBox {}

export interface ScanItem {
  id: number;
  path: string;
  name: string;
  width: number;
  height: number;
  thumbnail: string | null;
  boxes: BoxItem[];
  flags: Flag[];
  status: ScanStatus;
  error: string | null;
  /** Bumped when the boxes are replaced from outside (detection), so the canvas rebuilds. */
  revision: number;
  /** Earlier and later states of `boxes` for undo and redo. Lives as long as the scan is in the list. */
  history: History<BoxItem[]>;
  /** Bumped when undo or redo puts boxes back, so the canvas swaps them without reloading the image. */
  epoch: number;
  /** `boxesKey` of what detection produced, to tell whether the user changed anything. */
  detectedKey: string;
  /** `boxesKey` of the boxes last exported successfully. */
  exportedKey: string | null;
  /** The files the last export wrote for this scan (saved with the project). */
  exportedFiles: string[];
  /** What the last export of this scan cost in this session: milliseconds, and the size of each file. Not saved. */
  exportStats: { millis: number; sizes: number[] } | null;
  /** The scan file could not be found when the project was opened (moved or deleted). Not saved. */
  missing: boolean;
}

/** A copy that later edits cannot change. */
const snapshot = (boxes: BoxItem[]): BoxItem[] => boxes.map((box) => ({ ...box }));

export const FLAG_LABELS: Record<Flag["code"], string> = {
  no_boxes: "No item found",
  touches_edge: "Touches scan edge",
  extreme_aspect: "Unusual aspect ratio",
  area_outlier: "Much smaller than the rest",
  overlap: "Overlapping boxes",
};

const AVAILABLE_DETECTORS: string[] = ["v3", "v7-lite", "v7"];

export const defaultSettings: AppSettings = {
  theme: "system",
  mode: "v7-lite",
  minAreaPercent: 2,
  maxAreaPercent: 80,
  insetPx: 0,
  format: "same",
  jpegQuality: 92,
  pattern: "{source}-{n}",
  keepMetadata: true,
  tagSoftware: true,
  outputDir: null,
  autoApprove: true,
  lastProject: null,
  recentProjects: [],
};

export const state = reactive({
  /** The project that is open: its `.oxicut` file, name (the file name) and creation time. */
  project: { path: "", name: "", created: 0 },
  /** What the centre column shows: the scan with its boxes, or the straightened crops. */
  view: "scan" as "scan" | "crops",
  /** Which scans the Crops view lists. */
  cropsScope: "scan" as "scan" | "all",
  scans: [] as ScanItem[],
  selectedId: null as number | null,
  settings: { ...defaultSettings } as AppSettings,
  detect: { active: false, done: 0, total: 0 },
  exporting: { active: false, done: 0, total: 0 },
  exportErrors: [] as string[],
  lastExport: null as api.ExportSummary | null,
  notice: "",
  models: [] as ModelStatus[],
  modelsFolder: "",
  version: "",
  modelProgress: {} as Record<string, { downloaded: number; total: number | null }>,
  modelErrors: {} as Record<string, string>,
});

export const selectedScan = computed(() => state.scans.find((s) => s.id === state.selectedId) ?? null);
export const approvedCount = computed(() => state.scans.filter((s) => s.status === "approved").length);
export const pendingCount = computed(() => state.scans.filter((s) => s.status === "pending").length);
/** What closing the window now would throw away. */
export const closeAssessment = computed(() => assessClose(state.scans, state.exporting.active));
const canExport = (s: ScanItem) => s.status === "approved" && s.boxes.length > 0 && !s.missing;
/** Crops that the Crops view lists, over every scan. */
export const cropCount = computed(() => state.scans.reduce((sum, s) => sum + s.boxes.length, 0));
export const exportableCount = computed(() => state.scans.filter(canExport).length);

let nextScanId = 1;
let nextBoxId = 1;
let noticeTimer: number | undefined;

/** Show a short message in the status bar. */
export function say(message: string, ms = 6000) {
  state.notice = message;
  window.clearTimeout(noticeTimer);
  noticeTimer = window.setTimeout(() => (state.notice = ""), ms);
}

const withId = (box: Box): BoxItem => ({ ...box, id: nextBoxId++ });
export const newBoxId = () => nextBoxId++;
const errorText = (e: unknown) => (typeof e === "string" ? e : e instanceof Error ? e.message : String(e));

function detectSettings(): api.DetectSettings {
  const s = state.settings;
  return { mode: s.mode, minAreaPercent: s.minAreaPercent, maxAreaPercent: s.maxAreaPercent, insetPx: s.insetPx };
}

// ---------------------------------------------------------------- settings

let saveTimer: number | undefined;

export async function initSettings() {
  try {
    Object.assign(state.settings, await api.loadSettings());
    // A detector that is not available (an old or edited settings file) falls back to the default.
    if (!AVAILABLE_DETECTORS.includes(state.settings.mode)) state.settings.mode = defaultSettings.mode;
    state.settings.theme = parseTheme(state.settings.theme);
  } catch (e) {
    say(`Could not load settings, using defaults: ${errorText(e)}`);
  }
  // Whatever the file said replaces the cached choice the page started with.
  watch(() => state.settings.theme, applyTheme, { immediate: true });
  watch(
    () => ({ ...state.settings }),
    (settings) => {
      window.clearTimeout(saveTimer);
      saveTimer = window.setTimeout(() => {
        saveTimer = undefined;
        api.saveSettings(settings).catch((e) => say(`Could not save settings: ${errorText(e)}`));
      }, 400);
    },
    { deep: true },
  );
}

/** Save a settings change that is still waiting for its timer (used when the window closes). */
export async function flushSettings() {
  if (saveTimer === undefined) return;
  window.clearTimeout(saveTimer);
  saveTimer = undefined;
  try {
    await api.saveSettings({ ...state.settings });
  } catch (e) {
    say(`Could not save settings: ${errorText(e)}`);
  }
}

// --------------------------------------------------------------- detection

const waiting: ScanItem[][] = [];
let running = false;

async function refreshFlags(scan: ScanItem): Promise<void> {
  const token = (flagTokens.get(scan.id) ?? 0) + 1;
  flagTokens.set(scan.id, token);
  try {
    const flags = await api.evaluateFlags(scan.boxes, scan.width, scan.height);
    if (flagTokens.get(scan.id) !== token) return; // a newer edit superseded this answer
    scan.flags = flags;
  } catch (e) {
    say(`Could not check the boxes: ${errorText(e)}`);
  }
}
const flagTokens = new Map<number, number>();

async function settleDetected(scan: ScanItem) {
  await refreshFlags(scan);
  scan.status = scan.flags.length ? "flagged" : state.settings.autoApprove ? "approved" : "pending";
}

function applyAnalysis(scan: ScanItem, analysis: api.ScanAnalysis) {
  // A scan detected before is being detected again: its old boxes stay one undo away.
  const detectedBefore = scan.width > 0;
  scan.width = analysis.width;
  scan.height = analysis.height;
  scan.thumbnail = analysis.thumbnail;
  if (detectedBefore) scan.history.record(snapshot(scan.boxes));
  scan.boxes = analysis.boxes.map(withId);
  scan.detectedKey = boxesKey(scan.boxes);
  scan.error = null;
  scan.revision++;
  return settleDetected(scan);
}

function handleDetectEvent(batch: ScanItem[], event: DetectEvent) {
  const scan = batch[event.index];
  if (!scan) return;
  switch (event.event) {
    case "started":
      scan.status = "detecting";
      break;
    case "finished":
      state.detect.done++;
      void applyAnalysis(scan, event.analysis);
      break;
    case "failed":
      state.detect.done++;
      scan.status = "error";
      scan.error = event.error;
      break;
    case "skipped":
      state.detect.done++;
      scan.status = "error";
      scan.error = "Detection was cancelled. Use Re-detect to try again.";
      break;
  }
}

async function runDetection(items: ScanItem[]) {
  waiting.push(items);
  if (running) return;
  running = true;
  state.detect.active = true;
  try {
    while (waiting.length) {
      const batch = waiting.shift()!;
      await api.detectScans(
        batch.map((s) => s.path),
        detectSettings(),
        (event) => handleDetectEvent(batch, event),
      );
    }
  } catch (e) {
    say(`Detection stopped: ${errorText(e)}`);
    for (const scan of state.scans) {
      if (scan.status === "queued" || scan.status === "detecting") {
        scan.status = "error";
        scan.error = errorText(e);
      }
    }
  } finally {
    running = false;
    state.detect.active = false;
    state.detect.done = 0;
    state.detect.total = 0;
    const failures = state.scans.filter((s) => s.status === "error").length;
    const flagged = state.scans.filter((s) => s.status === "flagged").length;
    if (state.scans.length) {
      say(
        `Detection finished: ${approvedCount.value} approved, ${flagged} to review` +
          (failures ? `, ${failures} could not be read` : "") +
          ".",
      );
    }
  }
}

export async function cancelDetection() {
  await api.cancelDetection();
  say("Cancelling after the scans that are running now...");
}

function makeScan(path: string): ScanItem {
  const name = path.split(/[\\/]/).pop() ?? path;
  return {
    id: nextScanId++,
    path,
    name,
    width: 0,
    height: 0,
    thumbnail: null,
    boxes: [],
    flags: [],
    status: "queued",
    error: null,
    revision: 0,
    history: new History<BoxItem[]>(),
    epoch: 0,
    detectedKey: "",
    exportedKey: null,
    exportedFiles: [],
    exportStats: null,
    missing: false,
  };
}

/** Add scans, files and folders mixed; new ones are detected straight away. */
export async function addPaths(paths: string[]) {
  if (!paths.length) return;
  if (!state.project.path) {
    say("OxiCut is still starting. Try again in a moment.");
    return;
  }
  let expanded: string[];
  try {
    expanded = await api.expandPaths(paths);
  } catch (e) {
    say(`Could not read what you added: ${errorText(e)}`);
    return;
  }
  if (!expanded.length) {
    say("No JPEG, PNG or TIFF scans found in what you added.");
    return;
  }
  const known = new Set(state.scans.map((s) => s.path.toLowerCase()));
  const fresh = expanded.filter((p) => !known.has(p.toLowerCase())).map(makeScan);
  if (!fresh.length) {
    say("Those scans are already in the list.");
    return;
  }
  state.scans.push(...fresh);
  if (state.selectedId === null) state.selectedId = fresh[0].id;
  state.detect.total += fresh.length;
  void runDetection(state.scans.slice(-fresh.length));
}

export async function addScansFromDialog() {
  await addPaths(await api.pickScans());
}

export async function addFolderFromDialog() {
  const folder = await api.pickFolder();
  if (folder) await addPaths([folder]);
}

/** Run detection again on one scan, discarding its boxes. */
export async function redetect(scan: ScanItem) {
  scan.status = "detecting";
  try {
    await applyAnalysis(scan, await api.detectOne(scan.path, detectSettings()));
  } catch (e) {
    scan.status = "error";
    scan.error = errorText(e);
  }
}

/** After changing detection settings: detect every scan again. */
export function redetectAll() {
  const scans = [...state.scans];
  if (!scans.length) return;
  for (const scan of scans) {
    scan.status = "queued";
    scan.error = null;
  }
  state.detect.total += scans.length;
  void runDetection(scans);
}

// ----------------------------------------------------------------- editing

const editTimers = new Map<number, number>();

/** The user changed boxes on the canvas. */
export function updateBoxes(scan: ScanItem, boxes: BoxItem[]) {
  if (!sameBoxes(scan.boxes, boxes)) scan.history.record(snapshot(scan.boxes));
  setBoxes(scan, boxes);
}

/** Change boxes from outside the canvas (the Crops view): one undo step, and the canvas shows them when it is next seen. */
function editFromCrops(scan: ScanItem, boxes: BoxItem[]) {
  updateBoxes(scan, boxes);
  scan.epoch++;
}

/** Turn the crop of one box a quarter turn; the box on the scan looks the same but the exported picture is turned. */
export function turnCrop(scan: ScanItem, boxId: number, direction: "left" | "right") {
  editFromCrops(scan, scan.boxes.map((b) => (b.id === boxId ? turnBox(b, direction) : b)));
}

/** Cut a crop closer or wider: every edge of its box moves a little, the centre and angle stay. `false` when that is not possible. */
export function resizeCrop(scan: ScanItem, boxId: number, change: SizeChange): boolean {
  const box = scan.boxes.find((b) => b.id === boxId);
  const next = box ? resizeBox(box, change, scan) : null;
  if (!next) return false;
  editFromCrops(scan, scan.boxes.map((b) => (b.id === boxId ? next : b)));
  return true;
}

/** Remove one box (and so its crop). */
export function removeCrop(scan: ScanItem, boxId: number) {
  editFromCrops(scan, scan.boxes.filter((b) => b.id !== boxId));
}

/** Undo the last box edit on this scan. */
export function undo(scan: ScanItem) {
  const before = scan.history.undo(snapshot(scan.boxes));
  if (before) restoreBoxes(scan, before);
}

/** Redo what was undone on this scan. */
export function redo(scan: ScanItem) {
  const after = scan.history.redo(snapshot(scan.boxes));
  if (after) restoreBoxes(scan, after);
}

function restoreBoxes(scan: ScanItem, boxes: BoxItem[]) {
  setBoxes(scan, boxes);
  scan.epoch++; // tells the canvas to show them
}

/** Make `boxes` the scan's boxes and re-check them. */
function setBoxes(scan: ScanItem, boxes: BoxItem[]) {
  scan.boxes = boxes;
  scan.status = scan.status === "approved" ? "pending" : scan.status; // an edit needs a fresh OK
  window.clearTimeout(editTimers.get(scan.id));
  editTimers.set(
    scan.id,
    window.setTimeout(async () => {
      await refreshFlags(scan);
      if (scan.status !== "approved") scan.status = scan.flags.length ? "flagged" : "pending";
    }, 150),
  );
}

export function approve(scan: ScanItem) {
  if (scan.status === "queued" || scan.status === "detecting" || scan.status === "error") return;
  scan.status = "approved";
}

/** Approve every scan that is waiting and has nothing to flag. */
export function approveAllUnflagged() {
  const waitingScans = state.scans.filter((s) => s.status === "pending");
  waitingScans.forEach((s) => (s.status = "approved"));
  say(waitingScans.length ? `Approved ${waitingScans.length} scan${waitingScans.length === 1 ? "" : "s"}.` : "No unflagged scans are waiting.");
}

export function selectScan(id: number) {
  state.selectedId = id;
}

export function step(delta: number) {
  const index = state.scans.findIndex((s) => s.id === state.selectedId);
  const next = state.scans[index + delta];
  if (next) state.selectedId = next.id;
}

export function removeScan(scan: ScanItem) {
  const index = state.scans.findIndex((s) => s.id === scan.id);
  if (index < 0) return;
  state.scans.splice(index, 1);
  if (state.selectedId === scan.id) {
    state.selectedId = (state.scans[index] ?? state.scans[index - 1])?.id ?? null;
  }
}

// ------------------------------------------------------------------ preview

const previews = new Map<string, string>();
const PREVIEW_CACHE_SIZE = 6;

/** The screen preview of a scan (a few are kept in memory). */
export async function previewFor(path: string): Promise<string> {
  const cached = previews.get(path);
  if (cached) {
    previews.delete(path);
    previews.set(path, cached); // most recently used goes last
    return cached;
  }
  const url = await api.getPreview(path);
  previews.set(path, url);
  while (previews.size > PREVIEW_CACHE_SIZE) previews.delete(previews.keys().next().value as string);
  return url;
}

// ------------------------------------------------------------------- export

export async function chooseOutputFolder() {
  const folder = await api.pickFolder();
  if (folder) state.settings.outputDir = folder;
}

/** The export settings as the engine takes them. */
export function exportOptions(): api.ExportOptions {
  const s = state.settings;
  return {
    format: s.format,
    jpegQuality: s.jpegQuality,
    pattern: s.pattern,
    keepMetadata: s.keepMetadata,
    tagSoftware: s.tagSoftware,
    outputDir: s.outputDir,
  };
}

/** Export every approved scan; `true` when all of them were written. */
export async function exportApproved(): Promise<boolean> {
  if (state.exporting.active) return false;
  const items = state.scans.filter(canExport);
  if (!items.length) {
    say("Nothing to export yet: approve at least one scan that has boxes.");
    return false;
  }
  return runExport(items);
}

/** Export just this scan, approved or not (the Crops view offers it). */
export async function exportScan(scan: ScanItem): Promise<boolean> {
  if (state.exporting.active) return false;
  if (scan.missing || scan.boxes.length === 0) {
    say(scan.missing ? "This scan could not be found, so it cannot be exported." : "This scan has no boxes to export.");
    return false;
  }
  return runExport([scan]);
}

async function runExport(items: ScanItem[]): Promise<boolean> {
  state.exporting = { active: true, done: 0, total: items.length };
  state.exportErrors = [];
  // What each scan looked like when the export started; edits during the export are not covered.
  const keys = items.map((s) => boxesKey(s.boxes));
  let complete = false;
  try {
    const summary = await api.exportScans(
      items.map((s) => ({ path: s.path, boxes: s.boxes.map(({ cx, cy, w, h, angle }) => ({ cx, cy, w, h, angle })) })),
      exportOptions(),
      (event) => {
        if (event.event === "finished") {
          state.exporting.done++;
          const scan = items[event.index];
          scan.exportedKey = keys[event.index];
          scan.exportedFiles = event.files;
          scan.exportStats = { millis: event.millis, sizes: event.sizes };
        }
        if (event.event === "failed") {
          state.exporting.done++;
          state.exportErrors.push(`${event.name}: ${event.error}`);
        }
      },
    );
    state.lastExport = summary;
    complete = summary.failed === 0;
    say(
      `Exported ${summary.images} image${summary.images === 1 ? "" : "s"} from ${summary.scans - summary.failed} scan${summary.scans - summary.failed === 1 ? "" : "s"}` +
        (summary.failed ? `; ${summary.failed} failed.` : "."),
      12000,
    );
  } catch (e) {
    say(`Export failed: ${errorText(e)}`);
  } finally {
    state.exporting.active = false;
  }
  return complete;
}

export async function showExportFolder() {
  const target = state.lastExport?.firstFile ?? state.lastExport?.folder;
  if (target) await api.revealInExplorer(target).catch((e) => say(`Could not open the folder: ${errorText(e)}`));
}

// ------------------------------------------------------------------- models

export async function refreshModels() {
  try {
    state.models = await api.modelsStatus();
    state.modelsFolder = await api.modelsFolder();
  } catch (e) {
    say(`Could not check the models: ${errorText(e)}`);
  }
}

export async function downloadModels(ids: string[]) {
  for (const id of ids) {
    delete state.modelErrors[id];
    state.modelProgress[id] = { downloaded: 0, total: null };
  }
  try {
    await api.downloadModels(ids, (event) => {
      if (event.event === "progress") state.modelProgress[event.id] = { downloaded: event.downloaded, total: event.total };
      if (event.event === "finished") delete state.modelProgress[event.id];
      if (event.event === "failed") {
        delete state.modelProgress[event.id];
        state.modelErrors[event.id] = event.error;
      }
    });
  } catch (e) {
    say(`Download failed: ${errorText(e)}`);
  } finally {
    for (const id of ids) delete state.modelProgress[id];
    await refreshModels();
  }
}

// ----------------------------------------------------------------- projects
// A project is one `.oxicut` file holding the scan list, boxes and approvals. It saves itself a
// second after every change, so closing the app (or a crash) loses nothing worth mentioning.

const projectJson = computed(() => projectKey(state.scans));

/** `projectJson` as last written to (or read from) the file; saving is only needed when it differs. */
let lastSaved = "";
let projectTimer: number | undefined;
let saving: Promise<void> = Promise.resolve();
/** True while another project is being put in place, so its scans are not mistaken for edits. */
let switching = false;

const projectNameOf = (path: string) => (path.split(/[\\/]/).pop() ?? path).replace(/\.oxicut$/i, "");

async function writeProject() {
  if (!state.project.path || switching) return;
  const json = projectJson.value;
  if (json === lastSaved) return;
  try {
    await api.projectSave(state.project.path, toProjectFile(state.scans, state.project.created));
    lastSaved = json;
    rememberProject();
  } catch (e) {
    say(`Could not save the project: ${errorText(e)}`, 15000);
  }
}

/** Write any change that is still waiting for its timer (used before switching projects and on closing). */
export function flushProject(): Promise<void> {
  window.clearTimeout(projectTimer);
  projectTimer = undefined;
  saving = saving.then(writeProject);
  return saving;
}

watch(projectJson, (json) => {
  if (!state.project.path || switching || json === lastSaved) return;
  window.clearTimeout(projectTimer);
  projectTimer = window.setTimeout(() => void flushProject(), 1000);
});

/** Put the open project first in the recent list, with the numbers the Projects list shows. */
function rememberProject() {
  const entry: api.RecentProject = {
    path: state.project.path,
    scans: state.scans.length,
    approved: approvedCount.value,
    updated: Math.floor(Date.now() / 1000),
  };
  const rest = state.settings.recentProjects.filter((p) => p.path.toLowerCase() !== entry.path.toLowerCase());
  state.settings.recentProjects = [entry, ...rest].slice(0, 12);
  state.settings.lastProject = entry.path;
}

export function forgetProject(path: string) {
  state.settings.recentProjects = state.settings.recentProjects.filter((p) => p.path.toLowerCase() !== path.toLowerCase());
}

function restoreScan(saved: SavedScan & { status: RestoredStatus }, missing: boolean): ScanItem {
  return {
    ...saved,
    boxes: saved.boxes.map(withId),
    thumbnail: null,
    flags: [],
    revision: 0,
    history: new History<BoxItem[]>(),
    epoch: 0,
    exportStats: null,
    missing,
  };
}

/** Load the thumbnails of a reopened project a few at a time; the queue shows a spinner meanwhile. */
async function loadThumbnails(scans: ScanItem[]) {
  const queue = [...scans];
  const worker = async () => {
    for (let scan = queue.shift(); scan; scan = queue.shift()) {
      try {
        scan.thumbnail = await api.getThumbnail(scan.path);
      } catch {
        /* the review canvas says what is wrong with this scan when it is opened */
      }
    }
  };
  await Promise.all([worker(), worker(), worker(), worker()]);
}

/** What a freshly opened project still needs: detection for scans that never finished, flags, thumbnails. */
async function finishOpening() {
  const scans = [...state.scans];
  const lost = scans.filter((s) => s.missing).length;
  if (lost) say(`${lost} scan${lost === 1 ? " was" : "s were"} not found (moved or deleted). Their boxes are kept.`, 12000);
  const interrupted = scans.filter((s) => s.status === "queued" && !s.missing);
  if (interrupted.length) {
    state.detect.total += interrupted.length;
    void runDetection(interrupted);
  }
  void loadThumbnails(scans.filter((s) => s.width > 0 && !s.missing));
  for (const scan of scans) if (scan.boxes.length) await refreshFlags(scan);
}

/** Switch to the project in `path`. `false` if that was not possible (the reason is in the status bar). */
export async function openProject(path: string): Promise<boolean> {
  if (state.detect.active || state.exporting.active) {
    say("Wait until detection and export have finished before opening another project.");
    return false;
  }
  await flushProject();
  let opened: api.OpenedProject;
  try {
    opened = await api.projectOpen(path);
  } catch (e) {
    say(errorText(e), 15000);
    return false;
  }
  switching = true;
  try {
    previews.clear();
    const missing = new Set(opened.missing);
    nextScanId = nextIdAfter(opened.project.scans);
    state.project = { path: opened.path, name: opened.name, created: opened.project.created };
    state.scans = opened.project.scans.map((r) => restoreScan(fromRecord(r), missing.has(r.id)));
    state.selectedId = state.scans[0]?.id ?? null;
    lastSaved = projectJson.value;
    rememberProject();
  } finally {
    switching = false;
  }
  void finishOpening();
  return true;
}

/** Make a new, empty project called `name` and open it. */
export async function newProject(name: string): Promise<boolean> {
  try {
    return await openProject(await api.projectNew(name));
  } catch (e) {
    say(`Could not create the project: ${errorText(e)}`, 15000);
    return false;
  }
}

export async function renameProject(name: string): Promise<boolean> {
  await flushProject();
  try {
    const old = state.project.path;
    const path = await api.projectRename(old, name);
    state.project.path = path;
    state.project.name = projectNameOf(path);
    forgetProject(old);
    rememberProject();
    return true;
  } catch (e) {
    say(errorText(e), 12000);
    return false;
  }
}

/** A launch (or a second launch while the app is open) asked for a project and/or scans. */
export async function handleLaunch(args: api.StartupArgs) {
  if (args.project && args.project.toLowerCase() !== state.project.path.toLowerCase()) await openProject(args.project);
  if (args.scans.length) await addPaths(args.scans);
}

// ------------------------------------------------------------------ startup

/** Open the project asked for, else the one used last time, else make the first one. */
async function openStartProject(wanted: string | null) {
  if (wanted) {
    const [exists] = await api.filesExist([wanted]).catch(() => [false]);
    if (exists && (await openProject(wanted))) return;
    if (!exists) say("The project you used last could not be found, so a new one was started.", 10000);
  }
  await newProject("My scans");
}

export async function initApp() {
  await initSettings();
  void api.appVersion().then((v) => (state.version = v));
  void refreshModels();
  let launch: api.StartupArgs = { project: null, scans: [] };
  try {
    launch = await api.startupArgs();
  } catch (e) {
    say(`Could not read the command line: ${errorText(e)}`);
  }
  await openStartProject(launch.project ?? state.settings.lastProject);
  if (launch.scans.length) await addPaths(launch.scans);
}

/** Example output name for the settings panel, from the selected scan. */
export const exampleFileName = computed(() => {
  const stem = (selectedScan.value?.name ?? "scan.tif").replace(/\.[^.]+$/, "");
  const s = state.settings;
  let pattern = s.pattern.trim() || "{source}-{n}";
  if (!pattern.includes("{n}")) pattern += "-{n}";
  const extension =
    s.format === "same"
      ? ((selectedScan.value?.name.split(".").pop() ?? "png").toLowerCase().replace("jpeg", "jpg").replace("tiff", "tif"))
      : s.format === "tiff"
        ? "tif"
        : s.format;
  return `${pattern.replace("{source}", stem).replace("{n}", "1")}.${["jpg", "tif", "png"].includes(extension) ? extension : "png"}`;
});
