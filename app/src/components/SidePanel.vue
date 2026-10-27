<script setup lang="ts">
import { ref } from "vue";
import { formatBytes, revealInExplorer } from "../api";
import {
  chooseOutputFolder,
  downloadModels,
  exampleFileName,
  exportApproved,
  exportableCount,
  redetectAll,
  say,
  showExportFolder,
  state,
} from "../store";

type Tab = "detect" | "export" | "models";
const tab = ref<Tab>("detect");

// v7 runs MobileSAM, so it can be chosen once both of its files are on disk.
const v7Ready = () =>
  state.models.filter((m) => m.id.startsWith("mobilesam")).length > 0 &&
  state.models.filter((m) => m.id.startsWith("mobilesam")).every((m) => m.state === "ready");

const modelNeedsDownload = () => state.models.some((m) => m.state !== "ready");
const downloading = () => Object.keys(state.modelProgress).length > 0;

function percent(id: string): number {
  const p = state.modelProgress[id];
  if (!p) return 0;
  const model = state.models.find((m) => m.id === id);
  const total = p.total ?? model?.approxBytes ?? 0;
  return total ? Math.min(100, (p.downloaded / total) * 100) : 0;
}

const openModelsFolder = () =>
  state.modelsFolder && revealInExplorer(state.modelsFolder).catch(() => say("The models folder does not exist yet."));
</script>

<template>
  <aside class="panel">
    <div class="tabs" role="tablist">
      <button class="tab" role="tab" :aria-selected="tab === 'detect'" @click="tab = 'detect'">Detection</button>
      <button class="tab" role="tab" :aria-selected="tab === 'export'" @click="tab = 'export'">Export</button>
      <button class="tab" role="tab" :aria-selected="tab === 'models'" @click="tab = 'models'">Models</button>
    </div>

    <div v-if="tab === 'detect'" class="body">
      <label class="field">
        <span>Detector</span>
        <select v-model="state.settings.mode">
          <option value="v7" :disabled="!v7Ready()">
            v7 · most accurate{{ v7Ready() ? "" : " (download the models first)" }}
          </option>
          <option value="v7-lite">v7-lite · v3 plus edge tightening</option>
          <option value="v3">v3 · colour and edges only</option>
        </select>
      </label>
      <p class="callout">
        <template v-if="state.settings.mode === 'v3'">
          v3 works on colour and edges only. On white-bordered prints the box can run out onto the border or its shadow.
        </template>
        <template v-else-if="state.settings.mode === 'v7'">
          v7 uses the MobileSAM model to find prints, weighs it against colour, edge and texture cues, and moves each side
          onto the photograph's own edge. It is the most accurate and takes a few seconds per scan.
        </template>
        <template v-else>
          v7-lite takes v3's boxes and moves each side onto the photograph's own edge when it finds a clear border. It can
          still miss or merge prints; v7 (needs the models) is better at counting them.
        </template>
      </p>
      <p v-if="state.settings.mode === 'v7' && state.models.length && !v7Ready()" class="error-text">
        The v7 models are missing, so scans will fail with an error.
        <button class="link" @click="tab = 'models'">Open the Models tab</button> to download them, or pick v7-lite.
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
      <label class="check">
        <input v-model="state.settings.autoApprove" type="checkbox" />
        Approve scans with nothing to flag automatically
      </label>

      <p class="muted note">These apply to scans detected from now on.</p>
      <button class="btn" :disabled="!state.scans.length || state.detect.active" @click="redetectAll">
        Re-detect all {{ state.scans.length }} scans with these settings
      </button>
    </div>

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
      <label class="check disabled" title="Not supported yet">
        <input type="checkbox" disabled />
        Keep DPI and EXIF (not supported yet)
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
    </div>

    <div v-else class="body">
      <p class="muted lead">
        The v7 detector uses the MobileSAM model. It is optional: v7-lite works without it. Models are fetched once,
        checked against a fixed checksum, and kept outside OxiCut's installer.
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
          <div v-if="!(m.id in state.modelProgress) && m.state !== 'ready' && m.id !== 'orientation'" class="row">
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
        Download the MobileSAM models
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
  border-left: 1px solid var(--border);
}
.tabs {
  display: flex;
  border-bottom: 1px solid var(--border);
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
  font: 11px "Cascadia Mono", Consolas, monospace;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  user-select: text;
}
</style>
