<script setup lang="ts">
import { computed, ref } from "vue";
import { formatBytes, revealInExplorer } from "../api";
import { boxesKey } from "../closeGuard";
import { baseName } from "../cropNames";
import {
  chooseOutputFolder,
  downloadModels,
  exampleFileName,
  exportApproved,
  exportableCount,
  hasFeature,
  redetectAll,
  say,
  showExportFolder,
  state,
} from "../store";
import type { ScanItem } from "../store";
import { outOfDateScans } from "../store";
import { layout, togglePanel } from "../layout";
import { METHODS, methodFor, methodName, methodShort } from "../detectionMethods";
import DetailsPanel from "./DetailsPanel.vue";

type Tab = "detect" | "details" | "export" | "downloads";

// The Details tab is built but switched off for now (docs/feature-flags.md).
const detailsOn = computed(() => hasFeature("photo-details"));
const tab = ref<Tab>("detect");

// Careful (v7) needs its extra files, so it can be chosen once both are on disk.
const v7Ready = () =>
  state.models.filter((m) => m.id.startsWith("mobilesam")).length > 0 &&
  state.models.filter((m) => m.id.startsWith("mobilesam")).every((m) => m.state === "ready");

// The button in the Downloads tab fetches the files for Careful; the picture-turning helper has its own Download line.
const modelNeedsDownload = () => state.models.some((m) => m.id !== "orientation" && m.state !== "ready");
const orientationModel = () => state.models.find((m) => m.id === "orientation");
const orientationMissing = () => (orientationModel()?.state ?? "ready") !== "ready";
const downloading = () => Object.keys(state.modelProgress).length > 0;

function percent(id: string): number {
  const p = state.modelProgress[id];
  if (!p) return 0;
  const model = state.models.find((m) => m.id === id);
  const total = p.total ?? model?.approxBytes ?? 0;
  return total ? Math.min(100, (p.downloaded / total) * 100) : 0;
}

// What the collapsed rail shows: the detection method, the output format, and whether the downloads need attention.
const formatLabel = computed(() => ({ same: "Same", jpg: "JPG", png: "PNG", tiff: "TIFF" })[state.settings.format]);
const modelsMissing = computed(() => state.settings.mode === "v7" && state.models.length > 0 && !v7Ready());
const downloadLabel = computed(() => {
  const ids = Object.keys(state.modelProgress);
  return ids.length ? `${Math.round(ids.reduce((sum, id) => sum + percent(id), 0) / ids.length)}%` : "";
});

function showTab(next: Tab) {
  tab.value = next;
  layout.panelCollapsed = false;
}

// What the exports wrote, scan by scan (the file names are kept in the project; sizes and times only for this session).
const exportedScans = computed(() => state.scans.filter((s) => s.exportedFiles.length > 0));
const editedSince = (scan: ScanItem) => scan.exportedKey !== boxesKey(scan.boxes);
const revealFile = (path: string) => revealInExplorer(path).catch(() => say("That file or folder could not be found."));

function exportSummary(scan: ScanItem): string {
  const count = scan.exportedFiles.length;
  const parts = [`${count} image${count === 1 ? "" : "s"}`];
  if (scan.exportStats) {
    parts.push(formatBytes(scan.exportStats.sizes.reduce((a, b) => a + b, 0)));
  }
  return parts.join(" · ");
}

const openModelsFolder =() =>
  state.modelsFolder && revealInExplorer(state.modelsFolder).catch(() => say("The folder for the downloads does not exist yet."));
</script>

<template>
  <aside v-if="layout.panelCollapsed" class="panel rail">
    <button class="btn ghost chevron" title="Show the side panel (Ctrl+Shift+B)" aria-label="Show the side panel" @click="togglePanel">«</button>
    <button class="tool" :aria-pressed="tab === 'detect'" :title="`Detection: ${methodName(state.settings.mode)}`" @click="showTab('detect')">
      <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M4 7h16M4 12h16M4 17h16" /><circle cx="9" cy="7" r="2" /><circle cx="15" cy="12" r="2" /><circle cx="8" cy="17" r="2" /></svg>
      {{ methodShort(state.settings.mode) }}
    </button>
    <button v-if="detailsOn" class="tool" :aria-pressed="tab === 'details'" title="About the pictures: date, place, caption, people" @click="showTab('details')">
      <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M5 4h14v16H5zM8 9h8M8 13h8M8 17h5" /></svg>
      Details
    </button>
    <button class="tool" :aria-pressed="tab === 'export'" :title="`Export: ${formatLabel}`" @click="showTab('export')">
      <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M12 15V4M8 8l4-4 4 4M5 14v5h14v-5" /></svg>
      {{ formatLabel }}
    </button>
    <button
      class="tool"
      :aria-pressed="tab === 'downloads'"
      :title="modelsMissing ? 'Downloads: Careful is chosen but its files are missing' : 'Downloads'"
      @click="showTab('downloads')"
    >
      <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M12 3l8 4.5v9L12 21l-8-4.5v-9L12 3zM4 7.5l8 4.5 8-4.5M12 12v9" /></svg>
      {{ downloadLabel || "Downloads" }}
      <span v-if="modelsMissing" class="dot" aria-hidden="true"></span>
    </button>
  </aside>

  <aside v-else class="panel">
    <div class="tabs" role="tablist">
      <button class="tab" role="tab" :aria-selected="tab === 'detect'" @click="tab = 'detect'">Detection</button>
      <button v-if="detailsOn" class="tab" role="tab" :aria-selected="tab === 'details'" @click="tab = 'details'">Details</button>
      <button class="tab" role="tab" :aria-selected="tab === 'export'" @click="tab = 'export'">Export</button>
      <button class="tab" role="tab" :aria-selected="tab === 'downloads'" @click="tab = 'downloads'">Downloads</button>
      <button class="tab-toggle" title="Hide the side panel (Ctrl+Shift+B)" aria-label="Hide the side panel" @click="togglePanel">»</button>
    </div>

    <div v-if="tab === 'detect'" class="body">
      <label class="field">
        <span>Detection method</span>
        <select v-model="state.settings.mode">
          <option v-for="method in METHODS" :key="method.mode" :value="method.mode" :disabled="method.mode === 'v7' && !v7Ready()">
            {{ methodName(method.mode) }} · {{ method.mode === "v7" && !v7Ready() ? "download its files first" : method.hint }}
          </option>
        </select>
      </label>
      <p class="callout">{{ methodFor(state.settings.mode)?.about }}</p>
      <p v-if="state.settings.mode === 'v7' && state.models.length && !v7Ready()" class="error-text">
        The files for Careful are not downloaded, so scans cannot be found with it.
        <button class="link" @click="tab = 'downloads'">Open the Downloads tab</button> to get them, or pick Standard.
      </p>

      <div class="grid2">
        <label class="field">
          <span>Smallest print (% of scan)</span>
          <input v-model.number="state.settings.minAreaPercent" type="number" min="0" max="100" step="0.5" />
        </label>
        <label class="field">
          <span>Largest print (% of scan)</span>
          <input v-model.number="state.settings.maxAreaPercent" type="number" min="0" max="100" step="1" />
        </label>
      </div>
      <label class="field">
        <span>Trim from each side (px)</span>
        <input v-model.number="state.settings.insetPx" type="number" min="0" max="500" />
      </label>
      <label class="field">
        <span>Cut around</span>
        <select v-model="state.settings.cutTo">
          <option value="card">The whole card or print</option>
          <option value="picture">Only the picture</option>
        </select>
      </label>
      <p class="muted note">
        For postcards and prints with a margin, choose the whole card. Choose only the picture to leave a white border out.
      </p>
      <label class="check">
        <input v-model="state.settings.autoOrient" type="checkbox" />
        Turn pictures the right way up
      </label>
      <div v-if="state.settings.autoOrient && orientationMissing()" class="muted note">
        <template v-if="'orientation' in state.modelProgress">
          Getting the helper that tells which way up a picture is...
          <div class="bar"><div :style="{ width: `${percent('orientation')}%` }"></div></div>
        </template>
        <template v-else>
          This needs a one-time download ({{ formatBytes(orientationModel()?.approxBytes ?? 0) }}).
          <button class="btn" :disabled="downloading()" @click="downloadModels(['orientation'])">Download</button>
          <p v-if="state.modelErrors['orientation']" class="error-text">{{ state.modelErrors["orientation"] }}</p>
        </template>
      </div>
      <label class="check">
        <input v-model="state.settings.autoApprove" type="checkbox" />
        Approve scans with nothing to flag automatically
      </label>

      <p class="muted note">These apply to scans detected from now on.</p>
      <p v-if="outOfDateScans.length" class="note stale" role="status">
        {{ outOfDateScans.length }} of {{ state.scans.length }}
        {{ state.scans.length === 1 ? "scan was" : "scans were" }} found with other settings.
      </p>
      <button
        class="btn"
        :class="{ attention: outOfDateScans.length > 0 }"
        :disabled="!state.scans.length || state.detect.active"
        @click="redetectAll"
      >
        Re-detect all {{ state.scans.length }} scans with these settings
      </button>
    </div>

    <DetailsPanel v-else-if="tab === 'details' && detailsOn" />

    <div v-else-if="tab === 'export'" class="body">
      <label class="field">
        <span>Format</span>
        <select v-model="state.settings.format">
          <option value="same">Same as the scan</option>
          <option value="jpg">JPEG</option>
          <option value="png">PNG</option>
          <option value="tiff">TIFF</option>
        </select>
      </label>
      <label v-if="state.settings.format === 'jpg' || state.settings.format === 'same'" class="field">
        <span>JPEG quality · {{ state.settings.jpegQuality }}</span>
        <input v-model.number="state.settings.jpegQuality" type="range" min="50" max="100" />
      </label>
      <p v-if="state.settings.format === 'jpg'" class="muted note">JPEG is 8-bit: a 16-bit scan is reduced.</p>
      <label class="field">
        <span>File name</span>
        <input v-model="state.settings.pattern" type="text" spellcheck="false" />
        <span class="example">e.g. {{ exampleFileName }}</span>
      </label>

      <div class="field">
        <span>Save to</span>
        <div class="row">
          <input
            class="grow"
            type="text"
            readonly
            :value="state.settings.outputDir ?? ''"
            placeholder="An “OxiCut output” folder beside each scan"
          />
          <button class="btn" @click="chooseOutputFolder">Browse</button>
          <button v-if="state.settings.outputDir" class="btn ghost" title="Go back to a folder beside each scan" @click="state.settings.outputDir = null">
            ×
          </button>
        </div>
      </div>
      <label class="check" title="Copies the scan's resolution, colour profile and photo details (such as the date taken) into each picture.">
        <input v-model="state.settings.keepMetadata" type="checkbox" />
        Keep resolution, colours and photo details
      </label>
      <label class="check" title="Writes “OxiCut” and its version into each picture's file details (the “Program name” in Windows Properties). With the box above ticked it replaces the scanner's own entry.">
        <input v-model="state.settings.tagSoftware" type="checkbox" />
        Note in each picture that OxiCut made it
      </label>
      <p class="muted note">Your scans are never changed, and existing files are never overwritten: a new file gets “ (2)” in its name.</p>

      <button class="btn primary" :disabled="state.exporting.active || !exportableCount" @click="exportApproved">
        {{ state.exporting.active ? `Exporting ${state.exporting.done} of ${state.exporting.total}…` : `Export ${exportableCount} approved scan${exportableCount === 1 ? "" : "s"}` }}
      </button>
      <div v-if="state.exporting.active" class="bar">
        <div :style="{ width: `${(state.exporting.done / Math.max(1, state.exporting.total)) * 100}%` }"></div>
      </div>
      <p v-if="state.lastExport && !state.exporting.active" class="result">
        Last export: {{ state.lastExport.images }} image{{ state.lastExport.images === 1 ? "" : "s" }}.
        <button class="link" @click="showExportFolder">Show in folder</button>
      </p>
      <ul v-if="state.exportErrors.length" class="errors">
        <li v-for="message in state.exportErrors" :key="message">{{ message }}</li>
      </ul>

      <section v-if="exportedScans.length" class="exported" aria-label="Exported files">
        <h3>Exported</h3>
        <ul class="scans">
          <li v-for="scan in exportedScans" :key="scan.id">
            <div class="line">
              <strong class="name" :title="scan.path">{{ scan.name }}</strong>
              <span v-if="editedSince(scan)" class="chip warn" title="The boxes changed after this export, so the files are out of date">edited since</span>
              <button class="link" @click="revealFile(scan.exportedFiles[0])">Show</button>
            </div>
            <span class="muted small">{{ exportSummary(scan) }}</span>
            <ul class="files">
              <li v-for="(file, i) in scan.exportedFiles" :key="file">
                <button class="link" :title="file" @click="revealFile(file)">{{ baseName(file) }}</button>
                <span v-if="scan.exportStats" class="muted small"> {{ formatBytes(scan.exportStats.sizes[i] ?? 0) }}</span>
              </li>
            </ul>
          </li>
        </ul>
      </section>
    </div>

    <div v-else class="body">
      <p class="muted lead">
        Careful and turning pictures the right way up each need a model, a file that OxiCut downloads once. They are
        optional: Standard works without them. Each is downloaded once, checked to be complete, and kept outside OxiCut's installer.
      </p>
      <ul class="models">
        <li v-for="m in state.models" :key="m.id" class="model">
          <div class="row">
            <strong class="grow">{{ m.label }}</strong>
            <span v-if="m.id in state.modelProgress" class="chip accent">Downloading</span>
            <span v-else-if="m.state === 'ready'" class="chip ok">Ready</span>
            <span v-else-if="m.state === 'corrupt'" class="chip warn">Damaged</span>
            <span v-else class="chip">Not downloaded</span>
          </div>
          <div class="muted small">{{ m.usedBy }} · {{ formatBytes(m.sizeBytes ?? m.approxBytes) }}</div>
          <div v-if="m.id in state.modelProgress" class="bar"><div :style="{ width: `${percent(m.id)}%` }"></div></div>
          <p v-if="state.modelErrors[m.id]" class="error-text">{{ state.modelErrors[m.id] }}</p>
          <div v-if="!(m.id in state.modelProgress) && m.state !== 'ready'" class="row">
            <button class="btn" :disabled="downloading()" @click="downloadModels([m.id])">
              {{ m.state === "corrupt" ? "Download again" : "Download" }}
            </button>
          </div>
        </li>
      </ul>
      <button
        v-if="modelNeedsDownload()"
        class="btn primary"
        :disabled="downloading()"
        @click="downloadModels(state.models.filter((m) => m.state !== 'ready' && m.id !== 'orientation').map((m) => m.id))"
      >
        Download the files for Careful
      </button>
      <div v-if="state.modelsFolder" class="row">
        <code class="grow path" :title="state.modelsFolder">{{ state.modelsFolder }}</code>
        <button class="btn ghost" @click="openModelsFolder">Open</button>
      </div>
    </div>
  </aside>
</template>

<style scoped>
.panel {
  display: flex;
  flex-direction: column;
  min-height: 0;
  background: var(--surface);
  border-left: var(--divider) solid var(--border);
}
.tabs {
  display: flex;
  border-bottom: 1px solid var(--border);
}
.tab-toggle {
  width: 36px;
  border: 0;
  border-bottom: 2px solid transparent;
  background: transparent;
  color: var(--muted);
  cursor: pointer;
}
.tab-toggle:hover {
  color: var(--text);
}
.rail {
  align-items: center;
  gap: 0.35rem;
  padding: 0.5rem 0.25rem;
}
.rail .tool {
  width: 100%;
}
.chevron {
  width: 36px;
  padding: 0;
  justify-content: center;
}
.tab {
  flex: 1;
  height: 40px;
  border: 0;
  border-bottom: 2px solid transparent;
  background: transparent;
  color: var(--muted);
  cursor: pointer;
}
.tab[aria-selected="true"] {
  color: var(--text);
  border-bottom-color: var(--accent);
}
.body {
  display: grid;
  grid-template-columns: minmax(0, 1fr);
  align-content: start;
  gap: 0.9rem;
  flex: 1;
  min-height: 0;
  padding: 0.9rem;
  overflow-x: hidden;
  overflow-y: auto;
}
.grid2 {
  display: grid;
  grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
  gap: 0.6rem;
}
.check {
  display: flex;
  align-items: center;
  gap: 0.5rem;
}
.check.disabled {
  color: var(--muted);
}
.callout {
  margin: 0;
  padding: 0.55rem 0.7rem;
  background: var(--warn-soft);
  color: var(--warn);
  border-radius: 6px;
  font-size: 12px;
  line-height: 1.45;
}
.note,
.small {
  margin: 0;
  font-size: 12px;
}
.example {
  font-size: 12px;
  color: var(--muted);
}
.lead {
  margin: 0;
  font-size: 12px;
  line-height: 1.5;
}
.result {
  margin: 0;
  font-size: 12px;
}
.link {
  padding: 0;
  background: none;
  border: 0;
  color: var(--accent);
  text-decoration: underline;
  cursor: pointer;
}
.exported h3 {
  margin: 0 0 0.4rem;
  font-size: 13px;
}
.scans,
.files {
  display: grid;
  gap: 0.2rem;
  margin: 0;
  padding: 0;
  list-style: none;
}
.scans {
  gap: 0.6rem;
}
.scans > li {
  display: grid;
  gap: 2px;
  padding: 0.5rem 0.6rem;
  border: 1px solid var(--border);
  border-radius: 8px;
}
.line {
  display: flex;
  align-items: center;
  gap: 0.5rem;
}
.line .name {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.files {
  margin-top: 0.2rem;
  font-size: 12px;
}
.files li {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  gap: 0.6rem;
}
.files li .muted {
  flex: none;
}
.files .link {
  max-width: 100%;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  text-align: left;
}
.errors {
  margin: 0;
  padding-left: 1.1rem;
  font-size: 12px;
  color: var(--warn);
}
.error-text {
  margin: 0;
  font-size: 12px;
  color: var(--warn);
}
.models {
  display: grid;
  gap: 0.6rem;
  margin: 0;
  padding: 0;
  list-style: none;
}
.model {
  display: grid;
  gap: 0.4rem;
  padding: 0.7rem;
  border: 1px solid var(--border);
  border-radius: 8px;
}
.bar {
  height: 5px;
  background: var(--surface-2);
  border-radius: 3px;
  overflow: hidden;
}
.bar div {
  height: 100%;
  background: var(--accent);
  transition: width 0.12s linear;
}
.path {
  min-width: 0;
  font: 11px "Cascadia Mono", Consolas, monospace;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  user-select: text;
}
</style>
