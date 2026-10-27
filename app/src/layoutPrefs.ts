/** Which side panels are collapsed. Kept in the webview's storage, not in the settings file. */
export interface LayoutPrefs {
  queueCollapsed: boolean;
  panelCollapsed: boolean;
}

export const LAYOUT_KEY = "oxicut.layout";

/** Both panels open: also what unreadable or missing storage gives. */
export function openLayout(): LayoutPrefs {
  return { queueCollapsed: false, panelCollapsed: false };
}

export function parseLayout(text: string | null | undefined): LayoutPrefs {
  if (!text) return openLayout();
  try {
    const data: unknown = JSON.parse(text);
    const field = (name: keyof LayoutPrefs) =>
      typeof data === "object" && data !== null && (data as Record<string, unknown>)[name] === true;
    return { queueCollapsed: field("queueCollapsed"), panelCollapsed: field("panelCollapsed") };
  } catch {
    return openLayout();
  }
}

export function serializeLayout(prefs: LayoutPrefs): string {
  return JSON.stringify({ queueCollapsed: prefs.queueCollapsed, panelCollapsed: prefs.panelCollapsed });
}
