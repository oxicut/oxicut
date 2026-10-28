import { getCurrentWindow } from "@tauri-apps/api/window";
import { THEME_KEY, applyThemeToRoot, parseTheme, type ThemeChoice } from "./theme";

/** The choice used last time, readable before the settings file has loaded (avoids a flash of the wrong theme). */
export function cachedTheme(): ThemeChoice {
  try {
    return parseTheme(localStorage.getItem(THEME_KEY));
  } catch {
    return "system";
  }
}

/** Colour the page, remember the choice for the next start, and tint the window's own title bar to match. */
export function applyTheme(choice: ThemeChoice) {
  applyThemeToRoot(document.documentElement, choice);
  try {
    localStorage.setItem(THEME_KEY, choice);
  } catch {
    /* storage unavailable: the settings file still has it */
  }
  getCurrentWindow()
    .setTheme(choice === "system" ? null : choice)
    .catch(() => {});
}
