// Typed access to the Rust side. Every shape here mirrors one in src-tauri
// (serde renames fields to camelCase).

import { Channel, invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { revealItemInDir } from "@tauri-apps/plugin-opener";

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

export interface AppSettings extends DetectSettings {
  format: "same" | "jpg" | "png" | "tiff";
  jpegQuality: number;
  pattern: string;
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
  outputDir: string | null;
}

export type ExportEvent =
  | { event: "started"; index: number; total: number; name: string }
  | { event: "finished"; index: number; files: string[] }
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

export const startupPaths = () => invoke<string[]>("startup_paths");
export const expandPaths = (paths: string[]) => invoke<string[]>("expand_paths", { paths });

export const detectScans = (paths: string[], settings: DetectSettings, onEvent: (e: DetectEvent) => void) =>
  invoke<void>("detect_scans", { paths, settings, onEvent: channel(onEvent) });
export const cancelDetection = () => invoke<void>("cancel_detection");
export const detectOne = (path: string, settings: DetectSettings) =>
  invoke<ScanAnalysis>("detect_one", { path, settings });
export const getPreview = (path: string) => invoke<string>("get_preview", { path });
export const evaluateFlags = (boxes: IdBox[], width: number, height: number) =>
  invoke<Flag[]>("evaluate_flags", { boxes, width, height });

export const exportScans = (
  items: { path: string; boxes: Box[] }[],
  options: ExportOptions,
  onEvent: (e: ExportEvent) => void,
) => invoke<ExportSummary>("export_scans", { items, options, onEvent: channel(onEvent) });

export const loadSettings = () => invoke<AppSettings>("load_settings");
export const saveSettings = (settings: AppSettings) => invoke<void>("save_settings", { settings });

export const modelsStatus = () => invoke<ModelStatus[]>("models_status");
export const modelsFolder = () => invoke<string>("models_folder");
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

/** Native folder picker; `null` when cancelled. */
export async function pickFolder(): Promise<string | null> {
  const picked = await open({ directory: true, multiple: false });
  return typeof picked === "string" ? picked : null;
}

export const revealInExplorer = (path: string) => revealItemInDir(path);

/** Readable size, e.g. "27 MB". */
export function formatBytes(bytes: number): string {
  if (bytes >= 1_000_000) return `${Math.round(bytes / 1_000_000)} MB`;
  if (bytes >= 1_000) return `${Math.round(bytes / 1_000)} KB`;
  return `${bytes} B`;
}
