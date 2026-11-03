// What a person knows about the pictures on a scan: when, where, what it shows and who is in it.
// Pure rules (no Vue), so they are tested without a window. The engine writes these into the saved
// pictures; see `crates/oxicut-core/src/details.rs` for the limits, which are the same.

import { parseDate } from "./dates.ts";

export interface Details {
  /** The date as typed ("circa 1980"). */
  date: string;
  place: string;
  caption: string;
  people: string[];
}

export const MAX_CAPTION = 2000;
export const MAX_TEXT = 200;
export const MAX_PEOPLE = 100;

export const noDetails = (): Details => ({ date: "", place: "", caption: "", people: [] });

export const isEmptyDetails = (d: Partial<Details> | undefined | null): boolean =>
  !d || (!(d.date ?? "").trim() && !(d.place ?? "").trim() && !(d.caption ?? "").trim() && !(d.people ?? []).some((p) => p.trim()));

/** "Anna, Péter; Grandma" as a list: split on commas, semicolons and new lines, each name once. */
export function parsePeople(text: string): string[] {
  const names: string[] = [];
  for (const part of text.split(/[,;\n]/)) {
    const name = part.trim().slice(0, MAX_TEXT);
    if (!name || names.some((n) => n.toLowerCase() === name.toLowerCase())) continue;
    if (names.length < MAX_PEOPLE) names.push(name);
  }
  return names;
}

export const peopleText = (people: string[]): string => people.join(", ");

/** The details with every text trimmed and cut to its limit, and the people listed once each. */
export function limitDetails(d: Partial<Details>): Details {
  return {
    date: (d.date ?? "").trim().slice(0, MAX_TEXT),
    place: (d.place ?? "").trim().slice(0, MAX_TEXT),
    caption: (d.caption ?? "").trim().slice(0, MAX_CAPTION),
    people: parsePeople((d.people ?? []).join(",")),
  };
}

/** A picture's own details win over its scan's, field by field; a person list replaces the whole list. */
export function mergeDetails(scan: Details, own?: Partial<Details> | null): Details {
  const mine = limitDetails(own ?? {});
  const base = limitDetails(scan);
  return {
    date: mine.date || base.date,
    place: mine.place || base.place,
    caption: mine.caption || base.caption,
    people: mine.people.length ? mine.people : base.people,
  };
}

/** `to` with every empty field filled from `from` (what "Copy to the other scans" does). */
export function fillEmpty(from: Details, to: Details): Details {
  const source = limitDetails(from);
  const target = limitDetails(to);
  return {
    date: target.date || source.date,
    place: target.place || source.place,
    caption: target.caption || source.caption,
    people: target.people.length ? target.people : source.people,
  };
}

/** What the export is sent for one picture. `null` when there is nothing to write. */
export interface ExportDetails {
  dateText: string;
  year?: number;
  month?: number;
  day?: number;
  approximate: boolean;
  place: string;
  caption: string;
  people: string[];
}

export function toExportDetails(d: Details): ExportDetails | null {
  const clean = limitDetails(d);
  if (isEmptyDetails(clean)) return null;
  const parsed = clean.date ? parseDate(clean.date) : null;
  return {
    dateText: clean.date,
    ...(parsed?.year !== undefined ? { year: parsed.year } : {}),
    ...(parsed?.month !== undefined ? { month: parsed.month } : {}),
    ...(parsed?.day !== undefined ? { day: parsed.day } : {}),
    approximate: parsed?.approximate ?? false,
    place: clean.place,
    caption: clean.caption,
    people: clean.people,
  };
}

/** A short line for a picture's card: "circa 1980 · Balaton". Empty when there is nothing to say. */
export function detailsLine(d: Details): string {
  const short = (text: string) => (text.length > 24 ? `${text.slice(0, 23)}…` : text);
  return [short(d.date.trim()), short(d.place.trim())].filter(Boolean).join(" · ");
}
