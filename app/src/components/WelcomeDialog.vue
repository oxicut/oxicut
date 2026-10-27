<script setup lang="ts">
import { computed } from "vue";
import { formatBytes } from "../api";
import { downloadModels, state } from "../store";

const emit = defineEmits<{ addScans: []; close: [] }>();

const samModels = computed(() => state.models.filter((m) => m.id.startsWith("mobilesam")));
const samReady = computed(() => samModels.value.length > 0 && samModels.value.every((m) => m.state === "ready"));
const samBytes = computed(() => samModels.value.reduce((sum, m) => sum + m.approxBytes, 0));
const downloading = computed(() => samModels.value.some((m) => m.id in state.modelProgress));
const failed = computed(() => samModels.value.map((m) => state.modelErrors[m.id]).find(Boolean));
const percent = computed(() => {
  const done = samModels.value.reduce((sum, m) => sum + (state.modelProgress[m.id]?.downloaded ?? 0), 0);
  return samBytes.value ? Math.min(100, Math.round((done / samBytes.value) * 100)) : 0;
});

async function fetchModels() {
  await downloadModels(samModels.value.filter((m) => m.state !== "ready").map((m) => m.id));
  // Someone who asked for the better detector here wants to use it.
  if (samReady.value && state.settings.mode === "v7-lite") state.settings.mode = "v7";
}
</script>

<template>
  <div class="backdrop">
    <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="welcome-title">
      <h1 id="welcome-title">Welcome to <span>Oxi</span>Cut</h1>
      <p>OxiCut splits flatbed scans of photo prints and postcards into separate, straightened images.</p>
      <ol>
        <li><strong>Add</strong> scans or a whole folder: use the button, or drop them on the window.</li>
        <li><strong>Check</strong> the boxes OxiCut drew. Drag, resize or rotate any of them; <kbd>Enter</kbd> approves and moves on. Scans with nothing suspicious are approved for you.</li>
        <li><strong>Export</strong> the approved scans. Your originals are never changed.</li>
      </ol>
      <div v-if="samModels.length" class="models">
        <template v-if="samReady">
          <p>The v7 models are on this computer, so the most accurate detector is ready.</p>
        </template>
        <template v-else>
          <p>
            OxiCut works right away with <strong>v7-lite</strong>. For the most accurate detector (<strong>v7</strong>, better at
            counting prints) it can fetch a model once, about {{ formatBytes(samBytes) }}.
          </p>
          <button class="btn" :disabled="downloading" @click="fetchModels">
            {{ downloading ? `Downloading… ${percent}%` : "Download the v7 models" }}
          </button>
          <p v-if="failed" class="error">Could not download: {{ failed }} You can carry on with v7-lite and try again in the Models tab.</p>
        </template>
      </div>
      <div class="actions">
        <button class="btn primary" @click="emit('addScans')">Add scans…</button>
        <button class="btn" @click="emit('close')">Got it</button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.models {
  display: grid;
  justify-items: start;
  gap: 0.5rem;
  padding: 0.75rem 0.9rem;
  border: 1px solid var(--border);
  border-radius: 8px;
}
.models p {
  margin: 0;
  font-size: 13px;
}
.error {
  color: var(--warn);
}
.backdrop {
  position: fixed;
  inset: 0;
  display: grid;
  place-items: center;
  padding: 1rem;
  background: rgb(0 0 0 / 0.5);
  z-index: 10;
}
.dialog {
  max-width: 480px;
  padding: 1.75rem;
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: 12px;
  box-shadow: 0 20px 60px rgb(0 0 0 / 0.35);
}
h1 {
  margin: 0 0 0.75rem;
  font-size: 26px;
  letter-spacing: -0.02em;
}
h1 span {
  color: var(--accent);
}
p,
ol {
  margin: 0 0 0.9rem;
  line-height: 1.55;
  color: var(--muted);
}
ol {
  padding-left: 1.2rem;
  display: grid;
  gap: 0.5rem;
}
strong {
  color: var(--text);
}
.actions {
  display: flex;
  flex-wrap: wrap;
  gap: 0.5rem;
  margin-top: 1.25rem;
}
</style>
