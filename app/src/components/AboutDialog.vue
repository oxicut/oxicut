<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { openLicences, openLink } from "../api";
import { openWhatsNew, say, state } from "../store";
import UpdateIcon from "./UpdateIcon.vue";

const emit = defineEmits<{ close: [] }>();

/** The website, where people go to download and read how it works. */
const HOMEPAGE = "https://oxicut.github.io";

/** The project this one is based on. */
const SCANSPLITTER = "https://github.com/Madnex/ScanSplitter";

const closeButton = ref<HTMLButtonElement>();
/** The address as people read it, without the https:// in front. */
const address = computed(() => state.repository.replace(/^https?:\/\//, ""));

function openHomepage() {
  openLink(HOMEPAGE).catch(() => say("The website could not be opened."));
}

/** The code, for programmers. */
function openPage() {
  openLink(state.repository).catch(() => say("The project page could not be opened."));
}

/** The What's new window opens on its own; this one gets out of its way. */
function showWhatsNew() {
  emit("close");
  openWhatsNew();
}

function showLicences() {
  openLicences().catch(() => say("The list of licences could not be opened. It is installed together with OxiCut."));
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
      <div v-if="state.update" class="update" role="status">
        <p class="line">
          <UpdateIcon @opened="emit('close')" />
          <span>Version {{ state.update.version }} is available.</span>
        </p>
        <button class="link" @click="showWhatsNew">See what's new</button>
      </div>
      <p v-else class="quiet">
        <span v-if="state.updateStatus === 'newest'" class="muted">You have the newest version.</span>
        <button class="link" @click="showWhatsNew">What is new in this version</button>
      </p>
      <p class="what">OxiCut cuts scanned photos and postcards apart into separate, straightened pictures.</p>

      <div class="page">
        <button class="btn primary" @click="openHomepage">Open the OxiCut website</button>
        <span class="muted address">oxicut.github.io</span>
      </div>

      <div class="page">
        <button class="btn primary" :disabled="!state.repository" @click="openPage">OxiCut Project Page: For Developers</button>
        <span class="muted address">{{ address }}</span>
      </div>

      <div class="muted credit">
        <p>Free software under the GPL-3.0.</p>
        <p>Based on <button class="link" @click="openScanSplitter">ScanSplitter</button> by Madnex.</p>
        <p><button class="link" @click="showLicences">Licences of the parts OxiCut is built from</button></p>
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
  margin: 0.2rem 0 0.4rem;
  color: var(--muted);
}
.update {
  display: grid;
  justify-items: center;
  gap: 0.15rem;
  margin: 0 0 1rem;
  font-size: 13px;
}
.update .line {
  display: flex;
  align-items: center;
  gap: 0.3rem;
  margin: 0;
}
.quiet {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: center;
  gap: 0.4rem;
  margin: 0 0 1rem;
  font-size: 12px;
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
/* Both buttons are the same size, wide enough for the longer label. */
.page .btn {
  justify-content: center;
  width: 17.5rem;
  max-width: 100%;
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
