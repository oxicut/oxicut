<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from "vue";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { getCurrentWindow, type CloseRequestedEvent } from "@tauri-apps/api/window";
import CloseDialog from "./components/CloseDialog.vue";
import ProjectsDialog from "./components/ProjectsDialog.vue";
import CropsView from "./components/CropsView.vue";
import ReviewCanvas from "./components/ReviewCanvas.vue";
import ScanQueue from "./components/ScanQueue.vue";
import AboutDialog from "./components/AboutDialog.vue";
import SettingsDialog from "./components/SettingsDialog.vue";
import SidePanel from "./components/SidePanel.vue";
import WelcomeDialog from "./components/WelcomeDialog.vue";
import type { StartupArgs } from "./api";
import { layout, togglePanel, toggleQueue } from "./layout";
import {
  addPaths,
  addScansFromDialog,
  approve,
  approvedCount,
  cancelDetection,
  closeAssessment,
  exportApproved,
  exportableCount,
  flushProject,
  flushSettings,
  handleLaunch,
  initApp,
  redetect,
  redo,
  selectedScan,
  state,
  step,
  undo,
  updateBoxes,
} from "./store";

const WELCOME_KEY = "oxicut.welcomeSeen";

function welcomeSeen(): boolean {
  try {
    return localStorage.getItem(WELCOME_KEY) === "1";
  } catch {
    return false;
  }
}

const showWelcome = ref(!welcomeSeen());
const dragging = ref(false);
const showClose = ref(false);
const showSettings = ref(false);
const showAbout = ref(false);
const showProjects = ref(false);
/** Which part of the Projects dialog opens first: the list, or straight into starting a new project. */
const projectsStart = ref<"list" | "new">("list");

function openProjects(start: "list" | "new") {
  projectsStart.value = start;
  showProjects.value = true;
}
let unlisten: (() => void) | undefined;
let unlistenClose: (() => void) | undefined;
let unlistenLaunch: (() => void) | undefined;

function closeWelcome() {
  showWelcome.value = false;
  try {
    localStorage.setItem(WELCOME_KEY, "1");
  } catch {
    /* storage unavailable: the dialog just shows again next launch */
  }
}

function addFromWelcome() {
  closeWelcome();
  void addScansFromDialog();
}

/** Every way of closing the window comes through here: ask first if work would be lost. */
async function onCloseRequested(event: CloseRequestedEvent) {
  event.preventDefault();
  if (showClose.value) return;
  await flushSettings();
  await flushProject();
  if (closeAssessment.value.needsConfirm) showClose.value = true;
  else await getCurrentWindow().destroy();
}

/** Ctrl+B hides or shows the scan list, Ctrl+Shift+B the side panel, Ctrl+, opens the settings. */
function onKeydown(event: KeyboardEvent) {
  if (!event.ctrlKey || event.altKey || event.metaKey) return;
  const key = event.key.toLowerCase();
  if (key !== "b" && key !== ",") return;
  if (showWelcome.value || showClose.value || showProjects.value || showAbout.value) return;
  const target = event.target as HTMLElement | null;
  if (target && ["INPUT", "SELECT", "TEXTAREA"].includes(target.tagName)) return;
  event.preventDefault();
  if (key === ",") showSettings.value = true;
  else if (showSettings.value) return;
  else if (event.shiftKey) togglePanel();
  else toggleQueue();
}

function approveAndAdvance() {
  if (!selectedScan.value) return;
  approve(selectedScan.value);
  step(1);
}

onMounted(async () => {
  window.addEventListener("keydown", onKeydown);
  // Files dropped from Explorer arrive with their real paths.
  unlisten = await getCurrentWebview().onDragDropEvent((event) => {
    const payload = event.payload;
    if (payload.type === "enter" || payload.type === "over") dragging.value = true;
    else if (payload.type === "leave") dragging.value = false;
    else if (payload.type === "drop") {
      dragging.value = false;
      void addPaths(payload.paths);
    }
  });
  unlistenClose = await getCurrentWindow().onCloseRequested(onCloseRequested);
  // Double-clicking another project (or scans) while this window is open arrives here, not in a new window.
  unlistenLaunch = await listen<StartupArgs>("second-instance", (event) => void handleLaunch(event.payload));
  void initApp();
});

onBeforeUnmount(() => {
  window.removeEventListener("keydown", onKeydown);
  unlisten?.();
  unlistenClose?.();
  unlistenLaunch?.();
});
</script>

<template>
  <div class="app" :class="{ 'queue-collapsed': layout.queueCollapsed, 'panel-collapsed': layout.panelCollapsed }">
    <header class="top">
      <div class="brand"><span>Oxi</span>Cut</div>
      <button class="btn ghost project" title="Projects: open, start or rename" @click="openProjects('list')">
        {{ state.project.name || "Project" }} ▾
      </button>
      <span class="chip">Detector {{ state.settings.mode }}</span>
      <span class="grow"></span>
      <template v-if="state.detect.active">
        <span class="muted small">Detecting {{ state.detect.done }} of {{ state.detect.total }}…</span>
        <div class="mini-bar"><div :style="{ width: `${(state.detect.done / Math.max(1, state.detect.total)) * 100}%` }"></div></div>
        <button class="btn" @click="cancelDetection">Cancel</button>
      </template>
      <button class="btn primary" :disabled="state.exporting.active || !exportableCount" @click="exportApproved">
        {{ state.exporting.active ? `Exporting ${state.exporting.done} of ${state.exporting.total}…` : `Export approved (${exportableCount})` }}
      </button>
    </header>

    <ScanQueue @new-project="openProjects('new')" />

    <div class="centre">
      <ReviewCanvas
        v-show="state.view === 'scan'"
        class="fill"
        :scan="selectedScan"
        :keys-enabled="!showWelcome && !showClose && !showSettings && !showProjects && !showAbout && state.view === 'scan'"
        @approve="approveAndAdvance"
        @step="step"
        @edit="selectedScan && updateBoxes(selectedScan, $event)"
        @redetect="selectedScan && redetect(selectedScan)"
        @undo="selectedScan && undo(selectedScan)"
        @redo="selectedScan && redo(selectedScan)"
      />
      <CropsView v-if="state.view === 'crops'" />
    </div>

    <SidePanel />

    <footer class="status">
      <span class="message">{{ state.notice || "Ready" }}</span>
      <span class="grow"></span>
      <span class="muted">{{ state.scans.length }} {{ state.scans.length === 1 ? "scan" : "scans" }} · {{ approvedCount }} approved</span>
      <button v-if="state.version" class="btn ghost about" title="About OxiCut" @click="showAbout = true">OxiCut {{ state.version }}</button>
      <button class="btn ghost settings" title="Settings (Ctrl+,)" @click="showSettings = true">
        <svg viewBox="0 0 24 24" aria-hidden="true">
          <circle cx="12" cy="12" r="3" />
          <path d="M19.4 15a1.7 1.7 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-1.8-.3 1.7 1.7 0 0 0-1 1.5V21a2 2 0 1 1-4 0v-.1a1.7 1.7 0 0 0-1.1-1.5 1.7 1.7 0 0 0-1.8.3l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.7 1.7 0 0 0 .3-1.8 1.7 1.7 0 0 0-1.5-1H3a2 2 0 1 1 0-4h.1a1.7 1.7 0 0 0 1.5-1.1 1.7 1.7 0 0 0-.3-1.8l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.7 1.7 0 0 0 1.8.3H9a1.7 1.7 0 0 0 1-1.5V3a2 2 0 1 1 4 0v.1a1.7 1.7 0 0 0 1 1.5 1.7 1.7 0 0 0 1.8-.3l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.7 1.7 0 0 0-.3 1.8V9a1.7 1.7 0 0 0 1.5 1H21a2 2 0 1 1 0 4h-.1a1.7 1.7 0 0 0-1.5 1z" />
        </svg>
        Settings
      </button>
    </footer>

    <div v-if="dragging" class="drop" aria-hidden="true">
      <div>Drop scans or a folder to add them</div>
    </div>

    <WelcomeDialog v-if="showWelcome" @add-scans="addFromWelcome" @close="closeWelcome" />

    <SettingsDialog v-if="showSettings" @close="showSettings = false" />

    <AboutDialog v-if="showAbout" @close="showAbout = false" />

    <ProjectsDialog v-if="showProjects" :start="projectsStart" @close="showProjects = false" />

    <CloseDialog
      v-if="showClose"
      @keep="showClose = false"
      @discard="getCurrentWindow().destroy()"
    />
  </div>
</template>

<style scoped>
.app {
  --queue-w: 280px;
  --panel-w: 320px;
  display: grid;
  grid-template-columns: var(--queue-w) minmax(0, 1fr) var(--panel-w);
  transition: grid-template-columns 0.15s ease;
  grid-template-rows: auto minmax(0, 1fr) auto;
  grid-template-areas:
    "top top top"
    "queue review panel"
    "status status status";
  height: 100%;
}
.app.queue-collapsed {
  --queue-w: 64px;
}
.app.panel-collapsed {
  --panel-w: 56px;
}
.top {
  grid-area: top;
  display: flex;
  align-items: center;
  gap: 0.75rem;
  padding: 0.5rem 0.75rem;
  background: var(--surface);
  border-bottom: var(--divider) solid var(--border);
}
.brand {
  font-size: 18px;
  font-weight: 700;
  letter-spacing: -0.02em;
  color: var(--brand-cut);
}
.brand span {
  color: var(--brand-oxi);
}
.grow {
  flex: 1;
}
.small {
  font-size: 12px;
}
.project {
  max-width: 260px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-weight: 600;
}
.settings,
.about {
  height: 22px;
  padding: 0 0.5rem;
  font-size: 12px;
}
.about {
  color: var(--muted);
}
.settings svg {
  width: 14px;
  height: 14px;
  fill: none;
  stroke: currentColor;
  stroke-width: 1.7;
  stroke-linecap: round;
  stroke-linejoin: round;
}
.mini-bar {
  width: 120px;
  height: 4px;
  background: var(--surface-2);
  border-radius: 2px;
  overflow: hidden;
}
.mini-bar div {
  height: 100%;
  background: var(--accent);
  transition: width 0.15s;
}
.status {
  grid-area: status;
  display: flex;
  align-items: center;
  gap: 1rem;
  padding: 0.3rem 0.75rem;
  background: var(--surface);
  border-top: 1px solid var(--border);
  font-size: 12px;
}
.message {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.app > :nth-child(2) {
  grid-area: queue;
}
.app > :nth-child(3) {
  grid-area: review;
}
.centre {
  display: flex;
  flex-direction: column;
  min-width: 0;
  min-height: 0;
}
.fill {
  flex: 1;
}
.app > :nth-child(4) {
  grid-area: panel;
}
.drop {
  position: fixed;
  inset: 0;
  display: grid;
  place-items: center;
  background: color-mix(in srgb, var(--bg) 75%, transparent);
  z-index: 20;
  pointer-events: none;
}
.drop div {
  padding: 1.5rem 2.5rem;
  border: 2px dashed var(--accent);
  border-radius: 14px;
  background: var(--surface);
  font-size: 16px;
}
</style>
