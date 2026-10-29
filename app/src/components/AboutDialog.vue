<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { openLink } from "../api";
import { say, state } from "../store";

const emit = defineEmits<{ close: [] }>();

/** The project this one is based on. */
const SCANSPLITTER = "https://github.com/Madnex/ScanSplitter";

const closeButton = ref<HTMLButtonElement>();
/** The address as people read it, without the https:// in front. */
const address = computed(() => state.repository.replace(/^https?:\/\//, ""));

function openPage() {
  openLink(state.repository).catch(() => say("The project page could not be opened."));
}

function openScanSplitter() {
  openLink(SCANSPLITTER).catch(() => say("That page could not be opened."));
}

function onKey(event: KeyboardEvent) {
  if (event.key === "Escape") emit("close");
}

onMounted(() => {
  window.addEventListener("keydown", onKey);
  closeButton.value?.focus();
});
onBeforeUnmount(() => window.removeEventListener("keydown", onKey));
</script>

<template>
  <div class="backdrop" @mousedown.self="emit('close')">
    <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="about-title">
      <h1 id="about-title" class="logo"><span>Oxi</span>Cut</h1>
      <p class="version">Version {{ state.version }}</p>
      <p class="what">OxiCut cuts scanned photos and postcards apart into separate, straightened pictures.</p>

      <div class="page">
        <button class="btn" :disabled="!state.repository" @click="openPage">Open the project page</button>
        <span class="muted address">{{ address }}</span>
      </div>

      <div class="muted credit">
        <p>Free software under the GPL-3.0.</p>
        <p>Based on <button class="link" @click="openScanSplitter">ScanSplitter</button> by Madnex.</p>
      </div>

      <div class="actions">
        <button ref="closeButton" class="btn primary" @click="emit('close')">Close</button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.backdrop {
  position: fixed;
  inset: 0;
  display: grid;
  place-items: center;
  padding: 1rem;
  background: rgb(0 0 0 / 0.5);
  z-index: 30;
}
.dialog {
  width: min(400px, 100%);
  padding: 1.75rem 2rem 1.5rem;
  text-align: center;
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: 12px;
  box-shadow: 0 20px 60px rgb(0 0 0 / 0.35);
}
.logo {
  margin: 0;
  font-size: 34px;
  letter-spacing: -0.02em;
  color: var(--brand-cut);
}
.logo span {
  color: var(--brand-oxi);
}
.version {
  margin: 0.2rem 0 1rem;
  color: var(--muted);
}
.what {
  margin: 0 0 1.25rem;
  line-height: 1.5;
}
.page {
  display: grid;
  justify-items: center;
  gap: 0.35rem;
  margin-bottom: 1.25rem;
}
.address {
  font-size: 12px;
}
.credit {
  margin: 0 0 1.25rem;
  font-size: 12px;
  line-height: 1.6;
}
.credit p {
  margin: 0;
}
.link {
  padding: 0;
  border: 0;
  background: none;
  color: var(--accent);
  font: inherit;
  text-decoration: underline;
  cursor: pointer;
}
.actions {
  display: flex;
  justify-content: center;
}
</style>
