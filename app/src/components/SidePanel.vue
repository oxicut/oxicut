<script setup lang="ts">
import { onBeforeUnmount, reactive, ref } from "vue";
import { MODELS } from "../mock";

const props = defineProps<{
  /** Model ids already "downloaded" (mock state owned by App). */
  installed: Set<string>;
}>();
const emit = defineEmits<{
  download: [id: string];
  notice: [message: string];
}>();

type Tab = "settings" | "models";
const tab = ref<Tab>("settings");

const settings = reactive({
  mode: "v7",
  minArea: 2,
  maxArea: 80,
  inset: 0,
  format: "same",
  jpegQuality: 92,
  naming: "{source}-{n}",
  keepMetadata: true,
  outputDir: "",
});

// Mock download progress, 0..1 per model id. Real progress comes from oxcut-models in Phase 3.
const progress = reactive<Record<string, number>>({});
const timers = new Map<string, number>();

function startDownload(id: string) {
  if (timers.has(id) || props.installed.has(id)) return;
  progress[id] = 0;
  const timer = window.setInterval(() => {
    progress[id] = Math.min(1, progress[id] + 0.04 + Math.random() * 0.06);
    if (progress[id] >= 1) {
      window.clearInterval(timer);
      timers.delete(id);
      delete progress[id];
      emit("download", id);
    }
  }, 120);
  timers.set(id, timer);
}

onBeforeUnmount(() => timers.forEach((t) => window.clearInterval(t)));

const needsModels = () => settings.mode !== "v3" && MODELS.some((m) => !props.installed.has(m.id));
</script>

<template>
  <aside class="panel">
    <div class="tabs" role="tablist">
      <button class="tab" role="tab" :aria-selected="tab === 'settings'" @click="tab = 'settings'">Settings</button>
      <button class="tab" role="tab" :aria-selected="tab === 'models'" @click="tab = 'models'">
        Models
        <span v-if="MODELS.some((m) => !installed.has(m.id))" class="dot" aria-label="download required"></span>
      </button>
    </div>

    <div v-if="tab === 'settings'" class="body">
      <label class="field">
        <span>Detection mode</span>
        <select v-model="settings.mode">
          <option value="v7">v7 · default (best accuracy)</option>
          <option value="v5">v5 · previous</option>
          <option value="v3">v3 · fast, no models</option>
        </select>
      </label>
      <p v-if="needsModels()" class="callout">
        This mode needs the MobileSAM models. Until they are downloaded, OxCut falls back to v3.
        <button class="link" @click="tab = 'models'">Open model manager</button>
      </p>

      <div class="grid2">
        <label class="field">
          <span>Min item area (% of scan)</span>
          <input v-model.number="settings.minArea" type="number" min="0" max="100" step="0.5" />
        </label>
        <label class="field">
          <span>Max item area (% of scan)</span>
          <input v-model.number="settings.maxArea" type="number" min="0" max="100" step="1" />
        </label>
      </div>

      <label class="field">
        <span>Inset (px, shrink each box)</span>
        <input v-model.number="settings.inset" type="number" min="0" max="200" />
      </label>

      <hr />

      <label class="field">
        <span>Output format</span>
        <select v-model="settings.format">
          <option value="same">Same as source</option>
          <option value="jpg">JPEG</option>
          <option value="png">PNG</option>
          <option value="tiff">TIFF</option>
        </select>
      </label>
      <label v-if="settings.format === 'jpg'" class="field">
        <span>JPEG quality · {{ settings.jpegQuality }}</span>
        <input v-model.number="settings.jpegQuality" type="range" min="50" max="100" />
      </label>
      <label class="field">
        <span>File naming</span>
        <input v-model="settings.naming" type="text" spellcheck="false" />
      </label>
      <label class="check">
        <input v-model="settings.keepMetadata" type="checkbox" />
        Keep DPI and EXIF
      </label>

      <div class="field">
        <span>Output folder</span>
        <div class="row">
          <input
            v-model="settings.outputDir"
            class="grow"
            type="text"
            placeholder="Choose a folder…"
            readonly
          />
          <button class="btn" @click="emit('notice', 'Folder picking is not wired up yet (UI preview).')">Browse</button>
        </div>
      </div>
    </div>

    <div v-else class="body">
      <p class="muted lead">
        Models are downloaded once, checksum-verified, and stored in <code>%APPDATA%\oxcut\models</code>. Nothing is
        bundled with the installer.
      </p>

      <ul class="models">
        <li v-for="m in MODELS" :key="m.id" class="model">
          <div class="row">
            <strong class="grow">{{ m.name }}</strong>
            <span v-if="installed.has(m.id)" class="chip ok">Installed</span>
            <span v-else-if="m.id in progress" class="chip accent">Downloading</span>
            <span v-else class="chip warn">Download required</span>
          </div>
          <div class="muted small">{{ m.detail }} · {{ m.sizeMb }} MB · used by {{ m.neededBy }}</div>
          <div v-if="m.id in progress" class="bar"><div :style="{ width: `${progress[m.id] * 100}%` }"></div></div>
          <div class="row actions">
            <button
              v-if="!installed.has(m.id) && !(m.id in progress)"
              class="btn"
              @click="startDownload(m.id)"
            >
              Download
            </button>
            <button v-else-if="installed.has(m.id)" class="btn ghost" @click="emit('notice', 'Verify is not wired up yet (UI preview).')">
              Verify
            </button>
          </div>
        </li>
        <li class="model later">
          <div class="row">
            <strong class="grow">Orientation model</strong>
            <span class="chip">Later</span>
          </div>
          <div class="muted small">Detects which way is up · about 80 MB · not in v1</div>
        </li>
      </ul>

      <button
        class="btn primary"
        :disabled="MODELS.every((m) => installed.has(m.id)) || Object.keys(progress).length > 0"
        @click="MODELS.filter((m) => !installed.has(m.id)).forEach((m) => startDownload(m.id))"
      >
        Download all
      </button>
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
.dot {
  display: inline-block;
  width: 7px;
  height: 7px;
  margin-left: 6px;
  border-radius: 50%;
  background: var(--warn);
}
.body {
  display: grid;
  align-content: start;
  gap: 0.9rem;
  flex: 1;
  min-height: 0;
  padding: 0.9rem;
  overflow-y: auto;
}
.grid2 {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 0.6rem;
}
hr {
  width: 100%;
  border: 0;
  border-top: 1px solid var(--border);
}
.check {
  display: flex;
  align-items: center;
  gap: 0.5rem;
}
.callout {
  margin: 0;
  padding: 0.55rem 0.7rem;
  background: var(--warn-soft);
  color: var(--warn);
  border-radius: 6px;
  font-size: 12px;
}
.link {
  display: block;
  margin-top: 4px;
  padding: 0;
  background: none;
  border: 0;
  color: inherit;
  text-decoration: underline;
  cursor: pointer;
}
.lead {
  margin: 0;
  font-size: 12px;
  line-height: 1.5;
}
code {
  font: 12px "Cascadia Mono", Consolas, monospace;
  user-select: text;
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
.model.later {
  opacity: 0.6;
}
.small {
  font-size: 12px;
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
.actions:empty {
  display: none;
}
</style>
