// The version check, on the screen's side: which release is the newest, and which changelog entries are
// new to this build. Pure, so it is tested without a window. The fetching is in `src-tauri/src/updates.rs`.

/** A release as the engine hands it over (GitHub's list, reduced). */
export interface Release {
  /** The tag, for example `v0.1.0`. */
  version: string;
  name: string;
  notes: string;
  /** The release page. */
  url: string;
  publishedAt: string;
  prerelease: boolean;
}

interface Parsed {
  numbers: [number, number, number];
  /** Text after a dash (`beta.1`); a version with one is older than the same version without. */
  pre: string | null;
}

const VERSION = /^v?(\d+)(?:\.(\d+))?(?:\.(\d+))?(?:-([0-9A-Za-z.-]+))?(?:\+[0-9A-Za-z.-]+)?$/;

export function parseVersion(text: string): Parsed | null {
  const m = VERSION.exec(text.trim());
  if (!m) return null;
  return { numbers: [Number(m[1]), Number(m[2] ?? 0), Number(m[3] ?? 0)], pre: m[4] ?? null };
}

function comparePre(a: string, b: string): number {
  const x = a.split(".");
  const y = b.split(".");
  for (let i = 0; i < Math.max(x.length, y.length); i++) {
    if (x[i] === undefined) return -1;
    if (y[i] === undefined) return 1;
    const nx = /^\d+$/.test(x[i]);
    const ny = /^\d+$/.test(y[i]);
    if (nx && ny && Number(x[i]) !== Number(y[i])) return Number(x[i]) < Number(y[i]) ? -1 : 1;
    if (nx !== ny) return nx ? -1 : 1;
    if (!(nx && ny) && x[i] !== y[i]) return x[i] < y[i] ? -1 : 1;
  }
  return 0;
}

/** Negative when `a` is older than `b`, positive when newer, 0 when the same; `null` when either is not a version. */
export function compareVersions(a: string, b: string): number | null {
  const x = parseVersion(a);
  const y = parseVersion(b);
  if (!x || !y) return null;
  for (let i = 0; i < 3; i++) if (x.numbers[i] !== y.numbers[i]) return x.numbers[i] < y.numbers[i] ? -1 : 1;
  if (x.pre === y.pre) return 0;
  if (x.pre === null) return 1;
  if (y.pre === null) return -1;
  return comparePre(x.pre, y.pre);
}

/** `a` is a newer version than `b`. Anything that is not a version is not newer. */
export const isNewer = (a: string, b: string): boolean => (compareVersions(a, b) ?? 0) > 0;

/** The release with the highest version (pre-releases count; things that are not versions are skipped). */
export function pickNewest(releases: Release[]): Release | null {
  let best: Release | null = null;
  for (const release of releases) {
    if (!parseVersion(release.version)) continue;
    if (!best || isNewer(release.version, best.version)) best = release;
  }
  return best;
}

/** The version without a leading `v`, for showing to people. */
export const shownVersion = (tag: string): string => tag.trim().replace(/^v/i, "");

export interface ChangelogEntry {
  /** Without the `v` or brackets: `0.1.0`. */
  version: string;
  date: string;
  /** The markdown under the heading. */
  body: string;
}

const HEADING = /^##\s+\[?v?(\d+(?:\.\d+){0,2}(?:-[0-9A-Za-z.-]+)?)\]?(?:\s*[-\u2013\u2014]\s*(.*?))?\s*$/;

/** The versions in a changelog written in the Keep a Changelog style. Text before the first version, and sections such as "Unreleased", are left out. */
export function parseChangelog(markdown: string): ChangelogEntry[] {
  const entries: ChangelogEntry[] = [];
  let current: ChangelogEntry | null = null;
  for (const line of markdown.replace(/\r\n?/g, "\n").split("\n")) {
    if (/^##\s/.test(line)) {
      const m = HEADING.exec(line);
      current = m ? { version: m[1], date: (m[2] ?? "").trim(), body: "" } : null;
      if (current) entries.push(current);
      continue;
    }
    if (current) current.body += `${line}\n`;
  }
  return entries.map((e) => ({ ...e, body: e.body.trim() }));
}

/** The entries newer than `current` and no newer than `newest`, newest first. */
export function entriesSince(entries: ChangelogEntry[], current: string, newest: string): ChangelogEntry[] {
  return entries
    .filter((e) => isNewer(e.version, current) && (compareVersions(e.version, newest) ?? 1) <= 0)
    .sort((a, b) => compareVersions(b.version, a.version) ?? 0);
}

/** The entry for exactly this version. */
export function entryFor(entries: ChangelogEntry[], version: string): ChangelogEntry | null {
  return entries.find((e) => compareVersions(e.version, version) === 0) ?? null;
}
