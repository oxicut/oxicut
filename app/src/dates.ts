// Reading a date out of what a person types about an old photo: "1975", "circa 1980", "summer 1975",
// "14 March 1982", "14.3.1982". Pure, so the rules are tested without a window.
//
// The wording is always kept as typed; this only finds the year (and month and day when they are
// given) so the picture can carry a date programs understand.

export interface ParsedDate {
  year?: number;
  month?: number;
  day?: number;
  /** The wording says "about", "circa", "1980s": the year is a best guess. */
  approximate: boolean;
}

const MONTHS: Record<string, number> = {
  january: 1, jan: 1, february: 2, feb: 2, march: 3, mar: 3, april: 4, apr: 4, may: 5, june: 6, jun: 6,
  july: 7, jul: 7, august: 8, aug: 8, september: 9, sep: 9, sept: 9, october: 10, oct: 10,
  november: 11, nov: 11, december: 12, dec: 12,
};

const SEASONS = new Set(["spring", "summer", "autumn", "fall", "winter"]);
const APPROXIMATE = /(^|[^a-z])(circa|ca|c|about|around|approx|approximately|abt|roughly|early|late|mid)\.?(?=[^a-z]|$)|~|\?/i;

/** The newest year accepted: next year, in case a date is a little ahead. */
const latestYear = () => new Date().getFullYear() + 1;
const EARLIEST_YEAR = 1800;

const validYear = (y: number) => y >= EARLIEST_YEAR && y <= latestYear();

function daysIn(year: number, month: number): number {
  return new Date(year, month, 0).getDate();
}

/** `null` when no year can be found. */
export function parseDate(text: string): ParsedDate | null {
  const wording = text.trim();
  if (!wording) return null;
  let approximate = APPROXIMATE.test(wording);

  // 1975-06-14, 1975-06, 1975/6/14
  let m = wording.match(/(?<!\d)(\d{4})[-/.](\d{1,2})(?:[-/.](\d{1,2}))?(?!\d)/);
  if (m && validYear(+m[1]!)) {
    const year = +m[1]!;
    const month = +m[2]!;
    const day = m[3] ? +m[3] : undefined;
    if (month >= 1 && month <= 12) {
      return { year, month, ...(day && day >= 1 && day <= daysIn(year, month) ? { day } : {}), approximate };
    }
    return { year, approximate };
  }

  // 14.3.1982, 14/03/1982, 14-3-1982 (day first)
  m = wording.match(/(?<!\d)(\d{1,2})[./-](\d{1,2})[./-](\d{4})(?!\d)/);
  if (m && validYear(+m[3]!)) {
    const day = +m[1]!;
    const month = +m[2]!;
    const year = +m[3]!;
    if (month >= 1 && month <= 12) {
      return { year, month, ...(day >= 1 && day <= daysIn(year, month) ? { day } : {}), approximate };
    }
    return { year, approximate };
  }

  // Words: "14 March 1982", "March 1982", "1982 March", "summer 1975"
  const words = wording.toLowerCase().match(/[a-z]+/g) ?? [];
  const yearMatch = [...wording.matchAll(/(?<!\d)(\d{4})(s?)(?!\d)/g)].find((x) => validYear(+x[1]!));
  if (!yearMatch) return null;
  const year = +yearMatch[1]!;
  // "1980s" means the decade: the year is a guess.
  if (yearMatch[2]) approximate = true;
  // "may" is also an ordinary word, so it only counts next to a number (or when it is all there is).
  const isMonth = (w: string) =>
    MONTHS[w] !== undefined &&
    (w !== "may" || /\bmay\b\s*[\d,]/i.test(wording) || /\d\s*(?:st|nd|rd|th)?\s*(?:of\s+)?may\b/i.test(wording) || words.length === 1);
  const monthWord = words.find(isMonth);
  if (monthWord && !words.some((w) => SEASONS.has(w))) {
    const month = MONTHS[monthWord]!;
    // A day is a one or two digit number that is not the year: "14 March 1982", "March 14th, 1982".
    const dayMatch = wording.replace(yearMatch[0], " ").match(/(?<!\d)(\d{1,2})(?:st|nd|rd|th)?(?!\d)/);
    const day = dayMatch && +dayMatch[1]! >= 1 && +dayMatch[1]! <= daysIn(year, month) ? +dayMatch[1]! : undefined;
    return { year, month, ...(day ? { day } : {}), approximate };
  }
  return { year, approximate };
}

const MONTH_NAMES = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];

/** What a person sees under the date field, in plain words. */
export function describeDate(text: string): string {
  if (!text.trim()) return "";
  const parsed = parseDate(text);
  if (!parsed || parsed.year === undefined) return "No year found, so it is kept as you wrote it.";
  const parts = [parsed.day !== undefined ? String(parsed.day) : "", parsed.month !== undefined ? MONTH_NAMES[parsed.month - 1]! : "", String(parsed.year)].filter(Boolean);
  return `Saved as ${parsed.approximate ? "about " : ""}${parts.join(" ")}.`;
}
