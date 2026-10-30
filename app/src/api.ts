// Typed access to the Rust side. Every shape here mirrors one in src-tauri
// (serde renames fields to camelCase).

import { Channel, invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { openUrl, revealItemInDir } from "@tauri-apps/plugin-opener";
import type { ThemeChoice } from "./theme";

/** A box in source pixels: centre, size, and the angle of the `w` axis in degrees (clockwise on screen). */
export interface Box {
  cx: number;
  cy: number;
  w: number;
  h: number;
  angle: number;
}

export interface IdBox extends Box {
  id: number;
}

export type FlagCode = "no_boxes" | "touches_edge" | "extreme_aspect" | "area_outlier" | "overlap";

export interface Flag {
  code: FlagCode;
  boxId: number | null;
  message: string;
}

export interface ScanAnalysis {
  path: string;
  name: string;
  width: number;
  height: number;
  boxes: Box[];
  thumbnail: string;
  millis: number;
}

export type DetectorMode = "v3" | "v7-lite" | "v7";

export interface DetectSettings {
  mode: DetectorMode;
  minAreaPercent: number;
  maxAreaPercent: number;
  insetPx: number;
}

/** A project used lately, with what the Projects list shows (kept up to date as it is saved). */
export interface RecentProject {
  path: string;
  scans: number;
  approved: number;
  /** Seconds since 1970. */
  updated: number;
}

export interface AppSettings extends DetectSettings {
  theme: ThemeChoice;
  lastProject: string | null;
  recentProjects: RecentProject[];
  format: "same" | "jpg" | "png" | "tiff";
  jpegQuality: number;
  pattern: string;
  keepMetadata: boolean;
  tagSoftware: boolean;
  outputDir: string | null;
  autoApprove: boolean;
}

export type DetectEvent =
  | { event: "started"; index: number; path: string }
  | { event: "finished"; index: number; analysis: ScanAnalysis }
  | { event: "failed"; index: number; path: string; error: string }
  | { event: "skipped"; index: number; path: string };

export interface ExportOptions {
  format: AppSettings["format"];
  jpegQuality: number;
  pattern: string;
  keepMetadata: boolean;
  tagSoftware: boolean;
  outputDir: string | null;
}

export type ExportEvent =
  | { event: "started"; index: number; total: number; name: string }
  | { event: "finished"; index: number; files: string[]; sizes: number[]; millis: number }
  | { event: "failed"; index: number; name: string; error: string };

export interface ExportSummary {
  images: number;
  scans: number;
  failed: number;
  folder: string | null;
  firstFile: string | null;
}

export interface ModelStatus {
  id: string;
  label: string;
  filename: string;
  approxBytes: number;
  state: "missing" | "ready" | "corrupt";
  sizeBytes: number | null;
  usedBy: string;
}

export type DownloadEvent =
  | { event: "progress"; id: string; downloaded: number; total: number | null }
  | { event: "finished"; id: string }
  | { event: "failed"; id: string; error: string };

function channel<T>(onEvent: (event: T) => void): Channel<T> {
  const ch = new Channel<T>();
  ch.onmessage = onEvent;
  return ch;
}

export const SCAN_EXTENSIONS = ["jpg", "jpeg", "png", "tif", "tiff"];

export interface StartupArgs {
  project: string | null;
  scans: string[];
}

/** A project to open and scans to add, from the command line or a second launch. */
export const startupArgs = () => invoke<StartupArgs>("startup_args");
export const expandPaths = (paths: string[]) => invoke<string[]>("expand_paths", { paths });

export const detectScans = (paths: string[], settings: DetectSettings, onEvent: (e: DetectEvent) => void) =>
  invoke<void>("detect_scans", { paths, settings, onEvent: channel(onEvent) });
export const cancelDetection = () => invoke<void>("cancel_detection");
export const detectOne = (path: string, settings: DetectSettings) =>
  invoke<ScanAnalysis>("detect_one", { path, settings });
export const getThumbnail = (path: string) => invoke<string>("get_thumbnail", { path });
/** One straightened crop for the Crops view: a small picture and the size the exported file will have. */
export interface CropImage {
  dataUrl: string;
  width: number;
  height: number;
}

/** One box cut for real, at full resolution with the export settings, into a scratch folder. */
export interface FinalCrop {
  path: string;
  width: number;
  height: number;
  bytes: number;
  format: "jpg" | "png" | "tif";
  millis: number;
  /** A picture the screen can show (a full-size JPEG copy when the file is a TIFF). */
  dataUrl: string;
  shownAsCopy: boolean;
}

export const finalCrop = (path: string, region: Box, number: number, name: string | null, options: ExportOptions) =>
  invoke<FinalCrop>("final_crop", { path, region, number, name, options });

export const getCrops = (path: string, scanWidth: number, boxes: Box[], side: number) =>
  invoke<CropImage[]>("get_crops", { path, scanWidth, boxes, side });
export const getPreview = (path: string) => invoke<string>("get_preview", { path });
export const evaluateFlags = (boxes: IdBox[], width: number, height: number) =>
  invoke<Flag[]>("evaluate_flags", { boxes, width, height });

/** One scan to export. */
export interface ExportItem {
  path: string;
  boxes: Box[];
  /** A file name (without extension) for each box; `null` lets the naming pattern decide. */
  names: (string | null)[];
  /** A scan whose output folder to use: the backs of a pair go next to their fronts. */
  folderOf: string | null;
}

export const exportScans = (
  items: ExportItem[],
  options: ExportOptions,
  onEvent: (e: ExportEvent) => void,
) => invoke<ExportSummary>("export_scans", { items, options, onEvent: channel(onEvent) });

// ---- projects: one `.oxicut` file per batch of scans

export interface ScanRecord {
  id: number;
  path: string;
  relPath?: string | null;
  name: string;
  width: number;
  height: number;
  status: string;
  boxes: Box[];
  detectedKey: string;
  exportedKey: string | null;
  /** The files the last export wrote for this scan. */
  exportedFiles: string[];
  /** The scan holding the backs of this scan's pictures, when this is a scan of fronts. */
  backsOn?: number | null;
  /** Which picture goes with which on `backsOn`: positions in the two lists of boxes (front, back). */
  pairs?: [number, number][];
  error: string | null;
}

export interface ProjectFile {
  formatVersion: number;
  appVersion: string;
  created: number;
  updated: number;
  scans: ScanRecord[];
}

export interface OpenedProject {
  path: string;
  name: string;
  project: ProjectFile;
  /** Ids of scans whose file could not be found. */
  missing: number[];
}

export const projectsDir = () => invoke<string>("projects_dir");
export const projectNew = (name: string) => invoke<string>("project_new", { name });
export const projectOpen = (path: string) => invoke<OpenedProject>("project_open", { path });
export const projectSave = (path: string, project: ProjectFile) => invoke<void>("project_save", { path, project });
export const projectRename = (path: string, name: string) => invoke<string>("project_rename", { path, name });
export const filesExist = (paths: string[]) => invoke<boolean[]>("files_exist", { paths });

export const loadSettings = () => invoke<AppSettings>("load_settings");
export const saveSettings = (settings: AppSettings) => invoke<void>("save_settings", { settings });

export const modelsStatus = () => invoke<ModelStatus[]>("models_status");
export const modelsFolder = () => invoke<string>("models_folder");
export const appVersion = () => invoke<string>("app_version");
export const appRepository = () => invoke<string>("app_repository");
export const downloadModels = (ids: string[], onEvent: (e: DownloadEvent) => void) =>
  invoke<void>("download_models", { ids, onEvent: channel(onEvent) });

/** Native file picker for scans; empty when cancelled. */
export async function pickScans(): Promise<string[]> {
  const picked = await open({
    multiple: true,
    filters: [{ name: "Scans", extensions: SCAN_EXTENSIONS }],
  });
  return picked === null ? [] : Array.isArray(picked) ? picked : [picked];
}

/** Native file picker for a project file; `null` when cancelled. */
export async function pickProject(): Promise<string | null> {
  const picked = await open({ multiple: false, filters: [{ name: "OxiCut project", extensions: ["oxicut"] }] });
  return typeof picked === "string" ? picked : null;
}

/** Native folder picker; `null` when cancelled. */
export async function pickFolder(): Promise<string | null> {
  const picked = await open({ directory: true, multiple: false });
  return typeof picked === "string" ? picked : null;
}

export const revealInExplorer = (path: string) => revealItemInDir(path);

/** Open a web address in the user's browser. */
export const openLink = (url: string) => openUrl(url);

/** Readable size, e.g. "27 MB". */
export function formatBytes(bytes: number): string {
  if (bytes >= 1_000_000) return `${Math.round(bytes / 1_000_000)} MB`;
  if (bytes >= 1_000) return `${Math.round(bytes / 1_000)} KB`;
  return `${bytes} B`;
}
