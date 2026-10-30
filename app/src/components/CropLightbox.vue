<script lang="ts">
import type { BoxItem, ScanItem } from "../store";

/** One crop the Crops view can show larger. */
export interface LightboxItem {
  scan: ScanItem;
  box: BoxItem;
  index: number;
  name: string;
  /** The name an export gives the file when it comes from a pair (without extension); `null`: the naming pattern decides. */
  chosenName: string | null;
  /** The size the exported file will have. */
  width: number;
  height: number;
}
</script>

<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import * as api from "../api";
import { boxesKey } from "../closeGuard";
import { resizeBox, type SizeChange } from "../cropSize";
import { exportOptions, resizeCrop, say, undo } from "../store";

const props = defineProps<{ items: LightboxItem[]; index: number }>();
const emit = defineEmits<{ close: []; "update:index": [index: number] }>();

const item = computed(() => props.items[props.index]);
const errorText = (e: unknown) => (typeof e === "string" ? e : e instanceof Error ? e.message : String(e));

// ---- the quick look: cut from the screen preview, in milliseconds
const picture = ref<string | null>(null);
const failed = ref("");

let token = 0;
async function loadPreview() {
  const current = item.value;
  const mine = ++token;
  picture.value = null;
  failed.value = "";
  if (!current || current.scan.missing) return;
  try {
    const { cx, cy, w, h, angle } = current.box;
    const [crop] = await api.getCrops(current.scan.path, current.scan.width, [{ cx, cy, w, h, angle }], 1800);
    if (mine === token) picture.value = crop?.dataUrl || null;
  } catch (e) {
    if (mine === token) failed.value = errorText(e);
  }
}

// ---- the final result: the full-resolution file an export would write, made in a scratch folder
const real = ref<api.FinalCrop | null>(null);
const showing = ref<"preview" | "real">("preview");
const cutting = ref(false);
const cutError = ref("");
const actualSize = ref(false);
/** Final results prepared so far, valid while the boxes and the export settings are the ones they were made for. */
const made = new Map<string, api.FinalCrop>();

function cutKey(current: LightboxItem): string {
  const { format, jpegQuality, pattern, keepMetadata, tagSoftware } = exportOptions();
  return [current.scan.id, current.index, boxesKey([current.box]), current.chosenName, format, jpegQuality, pattern, keepMetadata, tagSoftware].join("|");
}

function showRemembered() {
  const current = item.value;
  real.value = (current && made.get(cutKey(current))) || null;
  showing.value = real.value ? "real" : "preview";
  actualSize.value = false;
  cutError.value = "";
}

async function showFinalResult() {
  const current = item.value;
  if (!current || cutting.value || current.scan.missing) return;
  const key = cutKey(current);
  cutting.value = true;
  cutError.value = "";
  try {
    const { cx, cy, w, h, angle } = current.box;
    const done = await api.finalCrop(current.scan.path, { cx, cy, w, h, angle }, current.index + 1, current.chosenName, exportOptions());
    made.set(key, done);
    // The lightbox may have moved on to another crop while this was being cut.
    if (item.value === current) {
      real.value = done;
      showing.value = "real";
      actualSize.value = false;
    }
  } catch (e) {
    cutError.value = errorText(e);
  } finally {
    cutting.value = false;
  }
}

const readableSize = (bytes: number) => (bytes >= 1_000_000 ? `${(bytes / 1_000_000).toFixed(1)} MB` : `${Math.max(1, Math.round(bytes / 1000))} KB`);
const formatName = (f: string) => ({ jpg: "JPEG", png: "PNG", tif: "TIFF" })[f] ?? f.toUpperCase();

const shown = computed(() => (showing.value === "real" && real.value ? real.value.dataUrl : picture.value));
const canTurnZoom = computed(() => showing.value === "real" && !!real.value);

function showInFolder() {
  if (real.value) api.revealInExplorer(real.value.path).catch(() => say("That folder could not be opened."));
}

// ---- cutting the crop closer or wider: the box itself changes, so the marquee on the scan does too
/** Set when the final result was showing, so it is prepared again for the new size. */
let refreshFinal = false;

const canResize = (change: SizeChange) => !!item.value && !item.value.scan.missing && !cutting.value && !!resizeBox(item.value.box, change, item.value.scan);
const canCloser = computed(() => canResize("closer"));
const canWider = computed(() => canResize("wider"));
const canUndo = computed(() => !!item.value && !cutting.value && item.value.scan.history.canUndo);

function resize(change: SizeChange) {
  const current = item.value;
  if (!current) return;
  refreshFinal = showing.value === "real";
  if (!resizeCrop(current.scan, current.box.id, change)) refreshFinal = false;
}

function undoLast() {
  const current = item.value;
  if (!current) return;
  refreshFinal = showing.value === "real";
  undo(current.scan);
}

// ---- moving between crops
const step = (delta: number) => {
  const next = props.index + delta;
  if (next >= 0 && next < props.items.length) emit("update:index", next);
};

function onKey(event: KeyboardEvent) {
  if (event.key === "Escape") emit("close");
  else if (event.key === "ArrowLeft") step(-1);
  else if (event.key === "ArrowRight") step(1);
  else return;
  event.preventDefault();
  event.stopPropagation();
}

// Another crop, or the same one with a new size: look again.
watch(
  () => [props.index, item.value?.name, item.value ? boxesKey([item.value.box]) : ""],
  async () => {
    showRemembered();
    const again = refreshFinal;
    refreshFinal = false;
    await loadPreview();
    if (again) await showFinalResult();
  },
  { immediate: true },
);
// Capture phase, so the arrows step through crops instead of through scans.
onMounted(() => window.addEventListener("keydown", onKey, true));
onBeforeUnmount(() => window.removeEventListener("keydown", onKey, true));
</script>

<template>
  <div v-if="item" class="backdrop" role="dialog" aria-modal="true" :aria-label="item.name" @mousedown.self="emit('close')">
    <div class="stage" :class="{ actual: actualSize }">
      <img
        v-if="shown"
        :src="shown"
        :alt="item.name"
        :class="{ zoomable: canTurnZoom }"
        :title="canTurnZoom ? (actualSize ? 'Click to fit the window again' : 'Click to see every pixel (100%)') : undefined"
        @click="canTurnZoom && (actualSize = !actualSize)"
      />
      <p v-else-if="cutError || failed" class="problem">{{ cutError || failed }}</p>
      <p v-else-if="item.scan.missing" class="problem">This scan could not be found.</p>
      <span v-else class="spinner" aria-hidden="true"></span>
      <div v-if="cutting" class="cutting"><span class="spinner" aria-hidden="true"></span> Preparing the final picture…</div>
    </div>

    <p v-if="showing === 'real' && real" class="facts">
      Final result: {{ real.width }} × {{ real.height }} px, {{ formatName(real.format) }}, {{ readableSize(real.bytes) }}.
      <span v-if="real.shownAsCopy">This kind of file cannot be shown in the window, so you see a full-size copy that looks the same.</span>
      This is only for looking at: nothing has been exported.
    </p>
    <p v-else-if="cutError && picture" class="facts problem">{{ cutError }}</p>

    <div class="final" role="group" aria-label="Adjust this crop and see the final result">
      <button
        class="btn"
        :disabled="!canCloser"
        :title="canCloser ? 'Cut a little more off every edge, for example to lose a white border' : 'The crop cannot get any smaller'"
        @click="resize('closer')"
      >
        Cut closer
      </button>
      <button
        class="btn"
        :disabled="!canWider"
        :title="canWider ? 'Include a little more around every edge' : 'The crop already reaches the edge of the scan'"
        @click="resize('wider')"
      >
        Cut wider
      </button>
      <button class="btn" :disabled="!canUndo" title="Undo the last change to this crop" @click="undoLast">Undo</button>
      <span class="divider" aria-hidden="true"></span>
      <button
        class="btn"
        :class="{ primary: !real }"
        :disabled="cutting || item.scan.missing"
        :title="real ? 'Prepare it again, with the current export settings' : 'Cut this picture at full size, exactly as an export would, and show it here without exporting anything'"
        @click="showFinalResult"
      >
        {{ cutting ? "Preparing…" : real ? "Refresh final result" : "Show final result" }}
      </button>
      <div v-if="real" class="which" role="group" aria-label="Which picture">
        <button class="seg" :aria-pressed="showing === 'preview'" @click="showing = 'preview'">Preview</button>
        <button class="seg" :aria-pressed="showing === 'real'" @click="showing = 'real'">Final result</button>
      </div>
      <button v-if="real" class="btn" title="Show the scratch file in its folder" @click="showInFolder">Show file</button>
    </div>

    <footer class="bar">
      <button class="btn" :disabled="index === 0" @click="step(-1)">Previous</button>
      <span class="label">
        <strong>{{ item.name }}</strong>
        <span class="muted">{{ item.width }} × {{ item.height }} px · {{ index + 1 }} of {{ items.length }}</span>
      </span>
      <button class="btn" :disabled="index === items.length - 1" @click="step(1)">Next</button>
      <button class="btn" @click="emit('close')">Close</button>
    </footer>
  </div>
</template>

<style scoped>
.backdrop {
  position: fixed;
  inset: 0;
  z-index: 30;
  display: grid;
  grid-template-rows: minmax(0, 1fr) auto auto auto;
  gap: 0.6rem;
  padding: 1rem;
  background: rgb(0 0 0 / 0.84);
}
.stage {
  position: relative;
  display: grid;
  place-items: center;
  min-height: 0;
  overflow: hidden;
}
.stage img {
  max-width: 100%;
  max-height: 100%;
  object-fit: contain;
  box-shadow: 0 10px 40px rgb(0 0 0 / 0.5);
}
.stage img.zoomable {
  cursor: zoom-in;
}
/* Every pixel: the picture at its own size, scrolled when it is bigger than the window. */
.stage.actual {
  display: block;
  overflow: auto;
}
.stage.actual img {
  max-width: none;
  max-height: none;
  object-fit: none;
  cursor: zoom-out;
}
.problem {
  color: #fff;
}
.facts {
  margin: 0;
  color: rgb(255 255 255 / 0.85);
  font-size: 12px;
  line-height: 1.5;
  text-align: center;
}
.facts.problem {
  color: #ffb4a8;
}
.cutting {
  position: absolute;
  inset: auto 0 1rem 0;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 0.6rem;
  color: #fff;
}
.bar {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 0.75rem;
}
.final {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: center;
  gap: 0.5rem 0.75rem;
}
.divider {
  align-self: stretch;
  width: 1px;
  margin: 0.15rem 0.25rem;
  background: rgb(255 255 255 / 0.25);
}
.label {
  display: grid;
  min-width: 190px;
  text-align: center;
  color: #fff;
}
.label .muted {
  color: rgb(255 255 255 / 0.7);
  font-size: 12px;
}
.which {
  display: flex;
  gap: 2px;
  padding: 2px;
  background: rgb(255 255 255 / 0.14);
  border-radius: 7px;
}
.which .seg {
  height: 26px;
  padding: 0 0.7rem;
  border: 0;
  border-radius: 5px;
  background: transparent;
  color: rgb(255 255 255 / 0.8);
  cursor: pointer;
}
.which .seg[aria-pressed="true"] {
  background: var(--surface);
  color: var(--text);
}
.spinner {
  width: 30px;
  height: 30px;
  border: 3px solid rgb(255 255 255 / 0.25);
  border-top-color: #fff;
  border-radius: 50%;
  animation: spin 0.8s linear infinite;
}
.cutting .spinner {
  width: 18px;
  height: 18px;
  border-width: 2px;
}
@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}
</style>
