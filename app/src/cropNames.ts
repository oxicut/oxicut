// The file names an export will give the crops of a scan, worked out on screen so the Crops view can
// show them before anything is written. This mirrors `expand_pattern` and `sanitize_file_name` in
// `crates/oxicut-core/src/export.rs`; keep the two in step (the tests use the same cases).

const FORBIDDEN = new Set(["<", ">", ":", '"', "/", "\\", "|", "?", "*"]);
const RESERVED = new Set([
  "CON", "PRN", "AUX", "NUL",
  "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9",
  "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
]);

/** A string that is safe to use as one Windows file name. */
export function sanitizeFileName(name: string): string {
  const mapped = Array.from(name, (c) => (/\p{Cc}/u.test(c) || FORBIDDEN.has(c) ? "_" : c)).join("");
  // Windows silently drops trailing dots and spaces; leading ones go too.
  let cleaned = mapped.replace(/^[.\s]+|[.\s]+$/gu, "");
  if (Array.from(cleaned).length > 150) cleaned = Array.from(cleaned).slice(0, 150).join("");
  const stem = (cleaned.split(".")[0] ?? "").toUpperCase();
  if (cleaned === "" || RESERVED.has(stem)) cleaned = `_${cleaned}`;
  return cleaned === "_" ? "scan" : cleaned;
}

/** The name (without extension) of print number `n` of the scan called `sourceStem`. */
export function expandPattern(pattern: string, sourceStem: string, n: number): string {
  let text = pattern.trim();
  if (text === "") text = "{source}-{n}";
  if (!text.includes("{n}")) text += "-{n}";
  return sanitizeFileName(text.replace("{source}", sourceStem).replace("{n}", String(n)));
}

export type OutputFormat = "same" | "jpg" | "png" | "tiff";

/** The extension an export writes: the format chosen, or the scan's own (anything unusual becomes PNG). */
export function outputExtension(format: OutputFormat, scanName: string): string {
  if (format === "jpg") return "jpg";
  if (format === "tiff") return "tif";
  if (format === "png") return "png";
  const own = (scanName.split(".").pop() ?? "").toLowerCase();
  if (own === "jpg" || own === "jpeg") return "jpg";
  if (own === "tif" || own === "tiff") return "tif";
  return "png";
}

/** `album-07-3.png`: the file print number `n` (counting from 1) of `scanName` is written as. */
export function cropFileName(pattern: string, scanName: string, n: number, format: OutputFormat): string {
  const stem = scanName.replace(/\.[^.]+$/, "");
  return `${expandPattern(pattern, stem, n)}.${outputExtension(format, scanName)}`;
}

/** The last part of a path, whichever way its separators lean. */
export function baseName(path: string): string {
  return path.split(/[\\/]/).pop() ?? path;
}
