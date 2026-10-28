/** How the interface picks its colours: follow Windows, or always light, or always dark. */
export type ThemeChoice = "system" | "light" | "dark";

export const THEME_KEY = "oxicut.theme";

export const THEME_CHOICES: { id: ThemeChoice; label: string; hint: string }[] = [
  { id: "system", label: "Follow system", hint: "Uses your Windows setting for light or dark." },
  { id: "light", label: "Light", hint: "Always light." },
  { id: "dark", label: "Dark", hint: "Always dark." },
];

/** Anything unknown (a missing field, an edited settings file) means follow the system. */
export function parseTheme(value: unknown): ThemeChoice {
  return value === "light" || value === "dark" ? value : "system";
}

/**
 * `data-theme` on the root element selects the palette in style.css; without it the palette follows
 * the system's setting.
 */
export function applyThemeToRoot(root: { dataset: Record<string, string | undefined> }, choice: ThemeChoice) {
  if (choice === "system") delete root.dataset.theme;
  else root.dataset.theme = choice;
}
