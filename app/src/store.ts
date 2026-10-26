// The app's state and the actions on it. Components read `state` and call these.

import { computed, reactive, watch } from "vue";
import * as api from "./api";
import type { AppSettings, Box, DetectEvent, Flag, IdBox, ModelStatus } from "./api";
import { History, sameBoxes } from "./history";

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
  mode: "v7-lite",
  minAreaPercent: 2,
  maxAreaPercent: 80,
  insetPx: 0,
  format: "same",
  jpegQuality: 92,
  pattern: "{source}-{n}",
  outputDir: null,
  autoApprove: true,
};

export const state = reactive({
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
  modelProgress: {} as Record<string, { downloaded: number; total: number | null }>,
  modelErrors: {} as Record<string, string>,
});

export const selectedScan = computed(() => state.scans.find((s) => s.id === state.selectedId) ?? null);
export const approvedCount = computed(() => state.scans.filter((s) => s.status === "approved").length);
export const pendingCount = computed(() => state.scans.filter((s) => s.status === "pending").length);
export const exportableCount = computed(
  () => state.scans.filter((s) => s.status === "approved" && s.boxes.length > 0).length,
);

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
  } catch (e) {
    say(`Could not load settings, using defaults: ${errorText(e)}`);
  }
  watch(
    () => ({ ...state.settings }),
    (settings) => {
      window.clearTimeout(saveTimer);
      saveTimer = window.setTimeout(() => api.saveSettings(settings).catch((e) => say(`Could not save settings: ${errorText(e)}`)), 400);
    },
    { deep: true },
  );
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
  };
}

/** Add scans, files and folders mixed; new ones are detected straight away. */
export async function addPaths(paths: string[]) {
  if (!paths.length) return;
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

export async function exportApproved() {
  if (state.exporting.active) return;
  const items = state.scans.filter((s) => s.status === "approved" && s.boxes.length > 0);
  if (!items.length) {
    say("Nothing to export yet: approve at least one scan that has boxes.");
    return;
  }
  state.exporting = { active: true, done: 0, total: items.length };
  state.exportErrors = [];
  try {
    const summary = await api.exportScans(
      items.map((s) => ({ path: s.path, boxes: s.boxes.map(({ cx, cy, w, h, angle }) => ({ cx, cy, w, h, angle })) })),
      {
        format: state.settings.format,
        jpegQuality: state.settings.jpegQuality,
        pattern: state.settings.pattern,
        outputDir: state.settings.outputDir,
      },
      (event) => {
        if (event.event === "finished") state.exporting.done++;
        if (event.event === "failed") {
          state.exporting.done++;
          state.exportErrors.push(`${event.name}: ${event.error}`);
        }
      },
    );
    state.lastExport = summary;
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

// ------------------------------------------------------------------ startup

export async function initApp() {
  await initSettings();
  void refreshModels();
  try {
    await addPaths(await api.startupPaths());
  } catch (e) {
    say(`Could not open the scans given on the command line: ${errorText(e)}`);
  }
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
