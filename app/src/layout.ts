import { reactive, watch } from "vue";
import { LAYOUT_KEY, parseLayout, serializeLayout, type LayoutPrefs } from "./layoutPrefs";

function stored(): string | null {
  try {
    return localStorage.getItem(LAYOUT_KEY);
  } catch {
    return null;
  }
}

export const layout = reactive<LayoutPrefs>(parseLayout(stored()));

watch(layout, (prefs) => {
  try {
    localStorage.setItem(LAYOUT_KEY, serializeLayout(prefs));
  } catch {
    /* storage unavailable: the layout just resets next launch */
  }
});

export const toggleQueue = () => (layout.queueCollapsed = !layout.queueCollapsed);
export const togglePanel = () => (layout.panelCollapsed = !layout.panelCollapsed);
