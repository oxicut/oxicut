<script setup lang="ts">
import { computed, ref, watch } from "vue";
import * as api from "../api";
import { boxesKey } from "../closeGuard";
import { baseName, cropFileName } from "../cropNames";
import { exportApproved, exportScan, exportableCount, removeCrop, say, selectedScan, state, turnCrop } from "../store";
import type { BoxItem, ScanItem } from "../store";
import CropLightbox from "./CropLightbox.vue";
import ViewSwitch from "./ViewSwitch.vue";
import type { LightboxItem } from "./CropLightbox.vue";

/** The longest side of the pictures on the cards, in pixels. */
const CARD_SIDE = 520;

/** Crops fetched so far, per scan, valid while the scan's boxes are still the ones they were cut for. */
const loaded = ref<Record<number, { key: string; crops: api.CropImage[] }>>({});
const loading = new Set<number>();

const scanKey = (scan: ScanItem) => `${scan.width}:${boxesKey(scan.boxes)}`;
const plain = (boxes: BoxItem[]) => boxes.map(({ cx, cy, w, h, angle }) => ({ cx, cy, w, h, angle }));
const usable = (scan: ScanItem) => scan.boxes.length > 0 && scan.width > 0 && !scan.missing;

const scans = computed<ScanItem[]>(() =>
  state.cropsScope === "scan" ? (selectedScan.value ? [selectedScan.value] : []) : state.scans.filter((s) => s.boxes.length > 0),
);

async function fetchCrops(scan: ScanItem) {
  if (!usable(scan) || loading.has(scan.id) || loaded.value[scan.id]?.key === scanKey(scan)) return;
  loading.add(scan.id);
  const key = scanKey(scan);
  try {
    const crops = await api.getCrops(scan.path, scan.width, plain(scan.boxes), CARD_SIDE);
    loaded.value[scan.id] = { key, crops };
  } catch (e) {
    say(`Could not make the crops of ${scan.name}: ${typeof e === "string" ? e : e instanceof Error ? e.message : e}`);
  } finally {
    loading.delete(scan.id);
  }
  // The boxes may have changed while this was being made.
  if (scans.value.includes(scan) && loaded.value[scan.id]?.key !== scanKey(scan)) void fetchCrops(scan);
}

// Cut the pictures for whatever is listed, a scan at a time; the cards show a spinner until theirs arrive.
watch(
  () => scans.value.map((s) => `${s.id}:${scanKey(s)}`).join("|"),
  async () => {
    for (const scan of scans.value) await fetchCrops(scan);
  },
  { immediate: true },
);

interface Card extends LightboxItem {
  picture: string | null;
  exported: boolean;
}

const cards = computed<Card[]>(() =>
  scans.value.flatMap((scan) => {
    const entry = loaded.value[scan.id];
    const fresh = entry?.key === scanKey(scan);
    const exported = scan.exportedKey !== null && scan.exportedKey === boxesKey(scan.boxes);
    return scan.boxes.map((box, index) => {
      const planned = cropFileName(state.settings.pattern, scan.name, index + 1, state.settings.format);
      // Once exported, say what was really written.
      const written = exported && scan.exportedFiles.length === scan.boxes.length ? baseName(scan.exportedFiles[index]) : null;
      const crop = fresh ? entry.crops[index] : undefined;
      return {
        scan,
        box,
        index,
        name: written ?? planned,
        width: Math.round(box.w),
        height: Math.round(box.h),
        picture: crop?.dataUrl || null,
        exported,
      };
    });
  }),
);

const lightbox = ref<number | null>(null);

const formatLabel = computed(() => ({ same: "the scan's own format", jpg: "JPEG", png: "PNG", tiff: "TIFF" })[state.settings.format]);
const where = computed(() => state.settings.outputDir || "an “OxiCut output” folder beside each scan");
const busy = computed(() => state.exporting.active);

function showInFolder(card: Card) {
  const file = card.exported ? card.scan.exportedFiles[card.index] : undefined;
  if (file) api.revealInExplorer(file).catch(() => say("That folder could not be opened."));
}
</script>

<template>
  <section class="crops">
    <header class="bar">
      <ViewSwitch />
      <span class="divider" aria-hidden="true"></span>
      <div class="scope" role="group" aria-label="Which crops">
        <button class="seg" :aria-pressed="state.cropsScope === 'scan'" @click="state.cropsScope = 'scan'">This scan</button>
        <button class="seg" :aria-pressed="state.cropsScope === 'all'" @click="state.cropsScope = 'all'">All scans</button>
      </div>
      <span class="muted count">
        {{ cards.length }} {{ cards.length === 1 ? "crop" : "crops" }}<template v-if="state.cropsScope === 'scan' && selectedScan"> from {{ selectedScan.name }}</template>
      </span>
    </header>

    <div class="scroll">
      <p v-if="!cards.length" class="empty muted">
        {{ state.cropsScope === "scan" ? "There are no boxes on this scan, so nothing will be cut out." : "No scan has any boxes yet." }}
      </p>

      <ul v-else class="grid">
        <li v-for="(card, i) in cards" :key="`${card.scan.id}-${card.box.id}`" class="card">
          <button class="picture" :title="`Look at ${card.name} larger`" @click="lightbox = i">
            <img v-if="card.picture" :src="card.picture" :alt="card.name" />
            <span v-else-if="card.scan.missing" class="gone">This scan could not be found</span>
            <span v-else class="spinner" aria-hidden="true"></span>
          </button>
          <div class="meta">
            <strong class="name" :title="card.name">{{ card.name }}</strong>
            <span v-if="state.cropsScope === 'all'" class="muted small" :title="card.scan.path">from {{ card.scan.name }}</span>
            <span class="muted small">{{ card.width }} × {{ card.height }} px</span>
            <span v-if="card.exported" class="chip ok">Exported</span>
          </div>
          <div class="actions">
            <button class="btn tiny" title="Turn this crop a quarter turn to the left" @click="turnCrop(card.scan, card.box.id, 'left')">
              <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M4 12a8 8 0 1 0 3-6.2M4 4v4h4" /></svg>
              Left
            </button>
            <button class="btn tiny" title="Turn this crop a quarter turn to the right" @click="turnCrop(card.scan, card.box.id, 'right')">
              <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M20 12a8 8 0 1 1-3-6.2M20 4v4h-4" /></svg>
              Right
            </button>
            <button class="btn tiny" title="Take this crop out (removes its box)" @click="removeCrop(card.scan, card.box.id)">
              <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M6 6l12 12M18 6L6 18" /></svg>
              Remove
            </button>
            <button v-if="card.exported" class="btn tiny" title="Show the written file in its folder" @click="showInFolder(card)">Show file</button>
          </div>
        </li>
      </ul>
    </div>

    <footer class="foot">
      <span class="muted summary">Saves as {{ formatLabel }} to {{ where }}. Change this in the Export tab.</span>
      <span class="grow"></span>
      <button class="btn" :disabled="busy || !selectedScan || !usable(selectedScan)" @click="selectedScan && exportScan(selectedScan)">
        Export this scan
      </button>
      <button class="btn primary" :disabled="busy || !exportableCount" @click="exportApproved">
        {{ busy ? `Exporting ${state.exporting.done} of ${state.exporting.total}…` : `Export all approved (${exportableCount})` }}
      </button>
    </footer>

    <CropLightbox v-if="lightbox !== null" v-model:index="lightbox" :items="cards" @close="lightbox = null" />
  </section>
</template>

<style scoped>
.crops {
  display: flex;
  flex: 1;
  flex-direction: column;
  min-width: 0;
  min-height: 0;
  background: var(--stage);
}
.bar {
  display: flex;
  align-items: center;
  gap: 1rem;
  padding: 0.5rem 0.75rem;
  background: var(--surface);
  border-bottom: 1px solid var(--border);
}
.divider {
  align-self: stretch;
  width: 1px;
  margin: 0.1rem 0;
  background: var(--border);
}
.scope {
  display: flex;
  gap: 2px;
  padding: 2px;
  background: var(--surface-2);
  border-radius: 7px;
}
.seg {
  height: 26px;
  padding: 0 0.8rem;
  border: 0;
  border-radius: 5px;
  background: transparent;
  color: var(--muted);
  cursor: pointer;
}
.seg[aria-pressed="true"] {
  background: var(--surface);
  color: var(--text);
  box-shadow: 0 1px 2px rgb(0 0 0 / 0.15);
}
.count {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.scroll {
  flex: 1;
  min-height: 0;
  padding: 1rem;
  overflow-y: auto;
}
.empty {
  margin: 3rem 0;
  text-align: center;
}
.grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(210px, 1fr));
  gap: 1rem;
  margin: 0;
  padding: 0;
  list-style: none;
}
.card {
  display: grid;
  gap: 0.5rem;
  align-content: start;
  padding: 0.6rem;
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: 10px;
}
.picture {
  display: grid;
  place-items: center;
  height: 220px;
  padding: 0;
  background: var(--stage);
  border: 0;
  border-radius: 6px;
  overflow: hidden;
  cursor: zoom-in;
}
.picture img {
  max-width: 100%;
  max-height: 100%;
  object-fit: contain;
}
.gone {
  padding: 0 1rem;
  color: var(--warn);
  font-size: 12px;
  text-align: center;
}
.spinner {
  width: 22px;
  height: 22px;
  border: 2px solid var(--border);
  border-top-color: var(--accent);
  border-radius: 50%;
  animation: spin 0.8s linear infinite;
}
@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}
.meta {
  display: grid;
  gap: 2px;
  justify-items: start;
  min-width: 0;
}
.name {
  max-width: 100%;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.small {
  font-size: 12px;
}
.actions {
  display: flex;
  flex-wrap: wrap;
  gap: 0.3rem;
}
.tiny {
  height: 26px;
  padding: 0 0.5rem;
  font-size: 12px;
}
.tiny svg {
  width: 13px;
  height: 13px;
  fill: none;
  stroke: currentColor;
  stroke-width: 1.8;
  stroke-linecap: round;
  stroke-linejoin: round;
}
.foot {
  display: flex;
  align-items: center;
  gap: 0.6rem;
  padding: 0.55rem 0.75rem;
  background: var(--surface);
  border-top: 1px solid var(--border);
}
.summary {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 12px;
}
.grow {
  flex: 1;
}
</style>
