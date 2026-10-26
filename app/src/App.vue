<script setup lang="ts">
import { computed, onBeforeUnmount, ref } from "vue";
import ReviewCanvas from "./components/ReviewCanvas.vue";
import ScanQueue from "./components/ScanQueue.vue";
import SidePanel from "./components/SidePanel.vue";
import WelcomeDialog from "./components/WelcomeDialog.vue";
import { MODELS, mockScans } from "./mock";

const WELCOME_KEY = "oxcut.welcomeSeen";

function welcomeSeen(): boolean {
  try {
    return localStorage.getItem(WELCOME_KEY) === "1";
  } catch {
    return false;
  }
}

const scans = ref(mockScans());
const selectedId = ref(scans.value[0].id);
const showWelcome = ref(!welcomeSeen());
const installed = ref(new Set<string>());
const notice = ref("");
let noticeTimer: number | undefined;

const selected = computed(() => scans.value.find((s) => s.id === selectedId.value) ?? scans.value[0]);
const approvedCount = computed(() => scans.value.filter((s) => s.status === "approved").length);
const modelsReady = computed(() => MODELS.every((m) => installed.value.has(m.id)));

function say(message: string) {
  notice.value = message;
  window.clearTimeout(noticeTimer);
  noticeTimer = window.setTimeout(() => (notice.value = ""), 4000);
}
onBeforeUnmount(() => window.clearTimeout(noticeTimer));

function step(delta: number) {
  const index = scans.value.findIndex((s) => s.id === selectedId.value);
  const next = scans.value[index + delta];
  if (next) selectedId.value = next.id;
}

function approveAndAdvance() {
  selected.value.status = "approved";
  step(1);
}

function closeWelcome() {
  showWelcome.value = false;
  try {
    localStorage.setItem(WELCOME_KEY, "1");
  } catch {
    /* storage unavailable: the dialog just shows again next launch */
  }
}

function markInstalled(id: string) {
  installed.value = new Set(installed.value).add(id);
}

function downloadAllFromWelcome() {
  closeWelcome();
  say("Open Models in the right panel to follow the download.");
}
</script>

<template>
  <div class="app">
    <header class="top">
      <div class="brand">Ox<span>Cut</span></div>
      <span class="chip" :class="modelsReady ? 'ok' : 'warn'">
        {{ modelsReady ? "Models ready" : "Fast mode (models not downloaded)" }}
      </span>
      <span class="grow"></span>
      <span class="chip accent">UI preview · sample data</span>
      <button class="btn" @click="say('Batch detection is not wired up yet (UI preview).')">Detect all</button>
      <button class="btn primary" @click="say('Export is not wired up yet (UI preview).')">
        Export approved ({{ approvedCount }})
      </button>
    </header>

    <ScanQueue :scans="scans" :selected-id="selectedId" @select="selectedId = $event" @notice="say" />

    <ReviewCanvas
      :scan="selected"
      :keys-enabled="!showWelcome"
      @approve="approveAndAdvance"
      @step="step"
      @notice="say"
    />

    <SidePanel :installed="installed" @download="markInstalled" @notice="say" />

    <footer class="status">
      <span>{{ notice || "Ready" }}</span>
      <span class="grow"></span>
      <span class="muted">{{ scans.length }} scans · {{ approvedCount }} approved</span>
    </footer>

    <WelcomeDialog v-if="showWelcome" @download-models="downloadAllFromWelcome" @skip="closeWelcome" />
  </div>
</template>

<style scoped>
.app {
  display: grid;
  grid-template-columns: 280px minmax(0, 1fr) 320px;
  grid-template-rows: auto minmax(0, 1fr) auto;
  grid-template-areas:
    "top top top"
    "queue review panel"
    "status status status";
  height: 100%;
}
.top {
  grid-area: top;
  display: flex;
  align-items: center;
  gap: 0.75rem;
  padding: 0.5rem 0.75rem;
  background: var(--surface);
  border-bottom: 1px solid var(--border);
}
.brand {
  font-size: 18px;
  font-weight: 700;
  letter-spacing: -0.02em;
}
.brand span {
  color: var(--accent);
}
.grow {
  flex: 1;
}
.status {
  grid-area: status;
  display: flex;
  gap: 1rem;
  padding: 0.3rem 0.75rem;
  background: var(--surface);
  border-top: 1px solid var(--border);
  font-size: 12px;
}
.app > :nth-child(2) {
  grid-area: queue;
}
.app > :nth-child(3) {
  grid-area: review;
}
.app > :nth-child(4) {
  grid-area: panel;
}
</style>
