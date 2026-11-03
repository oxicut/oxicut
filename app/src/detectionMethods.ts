// The three detection methods, in plain words. The technical names (v3, v7-lite, v7) stay in
// brackets, so they still say which one is meant, and a future detection method can be added here
// with its own name. Pure, so the wording is tested without a window.

import type { DetectorMode } from "./api";

export interface DetectionMethod {
  /** What the settings file and the engine call it. */
  mode: DetectorMode;
  /** What people read. */
  name: string;
  /** One line for the list of choices. */
  hint: string;
  /** A few sentences for the note under the choice. */
  about: string;
}

/** In the order of the choices: the most accurate first. */
export const METHODS: DetectionMethod[] = [
  {
    mode: "v7",
    name: "Careful",
    hint: "most accurate",
    about:
      "The Careful (v7) detection method also uses models, files downloaded once, to find the prints. It weighs that against colour, edges and texture, " +
      "and moves each side onto the photograph's own edge. It is the most accurate and takes a few seconds per scan.",
  },
  {
    mode: "v7-lite",
    name: "Standard",
    hint: "also trims white borders",
    about:
      "The Standard (v7-lite) detection method takes the boxes of the Basic (v3) method and moves each side onto the photograph's own edge when it finds a clear border. " +
      "It can still miss or merge prints; the Careful (v7) detection method (it needs a one-time download) is better at counting them.",
  },
  {
    mode: "v3",
    name: "Basic",
    hint: "colour and edges only",
    about:
      "The Basic (v3) detection method works on colour and edges only. On prints with white borders the box can run out onto the border or its shadow.",
  },
];

export const methodFor = (mode: string): DetectionMethod | undefined => METHODS.find((f) => f.mode === mode);

/** "Standard (v7-lite)": the plain name, with the technical one in brackets. */
export const methodName = (mode: string): string => {
  const method = methodFor(mode);
  return method ? `${method.name} (${method.mode})` : mode;
};

/** "Standard": for places too small for the brackets. */
export const methodShort = (mode: string): string => methodFor(mode)?.name ?? mode;
