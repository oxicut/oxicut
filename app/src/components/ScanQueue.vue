<script setup lang="ts">
import { computed, ref } from "vue";
import {
  FLAG_LABELS,
  addFolderFromDialog,
  addScansFromDialog,
  approveAllUnflagged,
  approvedCount,
  backsOf,
  frontsOf,
  pendingCount,
  removeScan,
  selectScan,
  state,
  type ScanItem,
} from "../store";
import { layout, toggleQueue } from "../layout";

const emit = defineEmits<{ newProject: [] }>();

type Filter = "all" | "review" | "approved";
const filter = ref<Filter>("all");
const filters: { id: Filter; label: string }[] = [
  { id: "all", label: "All" },
  { id: "review", label: "To review" },
  { id: "approved", label: "Approved" },
];

const visible = computed(() =>
  state.scans.filter((s) => {
    if (filter.value === "approved") return s.status === "approved";
    if (filter.value === "review") return s.status === "flagged" || s.status === "pending" || s.status === "error";
    return true;
  }),
);

function flagText(scan: ScanItem): string {
  const labels = [...new Set(scan.flags.map((f) => FLAG_LABELS[f.code]))];
  return labels.slice(0, 2).join(" · ") + (labels.length > 2 ? ` · +${labels.length - 2}` : "");
}

const chipClass = (scan: ScanItem) => ({
  ok: scan.status === "approved" && !scan.missing,
  warn: scan.status === "flagged" || scan.status === "error" || scan.missing,
});

const statusLabel = (scan: ScanItem) =>
  scan.missing
    ? "file not found"
    : scan.status === "queued"
      ? "waiting"
      : scan.status === "detecting"
        ? "detecting…"
        : scan.status;

/** Scans the "To review" filter would show: what still needs the user's attention. */
const toReview = computed(
  () => state.scans.filter((s) => s.status === "flagged" || s.status === "pending" || s.status === "error").length,
);

const railTip = (scan: ScanItem) =>
  `${scan.name}: ${statusLabel(scan)}` + (scan.width ? `, ${scan.boxes.length} ${scan.boxes.length === 1 ? "item" : "items"}` : "");
</script>

<template>
  <aside v-if="layout.queueCollapsed" class="queue rail">
    <button class="btn ghost" title="Show the scan list (Ctrl+B)" aria-label="Show the scan list" @click="toggleQueue">»</button>
    <button class="btn ghost" title="Add scans" aria-label="Add scans" @click="addScansFromDialog">+</button>
    <div class="count" :title="`${approvedCount} of ${state.scans.length} approved`">
      <strong>{{ approvedCount }}/{{ state.scans.length }}</strong>
      <div class="progress"><div class="bar" :style="{ width: `${(approvedCount / Math.max(1, state.scans.length)) * 100}%` }"></div></div>
    </div>
    <span v-if="toReview" class="chip warn" :title="`${toReview} to review`">{{ toReview }}</span>
    <ul class="strip">
      <li v-for="scan in state.scans" :key="scan.id">
        <button class="item" :class="{ active: scan.id === state.selectedId }" :title="railTip(scan)" :aria-label="railTip(scan)" @click="selectScan(scan.id)">
          <span class="thumb" :data-status="scan.status" aria-hidden="true">
            <img v-if="scan.thumbnail" :src="scan.thumbnail" alt="" />
            <span v-else-if="scan.missing" class="gone">?</span>
            <span v-else class="spinner"></span>
          </span>
        </button>
      </li>
    </ul>
    <button class="btn ghost" title="Create a new project" aria-label="Create a new project" @click="emit('newProject')">
      <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M6 3h8l4 4v14H6zM14 3v4h4M12 11v6M9 14h6" /></svg>
    </button>
  </aside>

  <aside v-else class="queue">
    <div class="head">
      <div class="row">
        <button class="btn primary grow" @click="addScansFromDialog">Add scans…</button>
        <button class="btn" title="Add every scan in a folder" @click="addFolderFromDialog">Folder…</button>
        <button class="btn ghost" title="Hide the scan list (Ctrl+B)" aria-label="Hide the scan list" @click="toggleQueue">«</button>
      </div>
      <div class="progress" :title="`${approvedCount} of ${state.scans.length} approved`">
        <div class="bar" :style="{ width: `${(approvedCount / Math.max(1, state.scans.length)) * 100}%` }"></div>
      </div>
      <div class="row">
        <span class="muted small grow">{{ approvedCount }} of {{ state.scans.length }} approved</span>
        <button v-if="pendingCount" class="btn ghost small-btn" title="Approve every scan that has nothing flagged" @click="approveAllUnflagged">
          Approve {{ pendingCount }} unflagged
        </button>
      </div>
      <div class="tabs" role="tablist">
        <button v-for="f in filters" :key="f.id" class="tab" role="tab" :aria-selected="filter === f.id" @click="filter = f.id">
          {{ f.label }}
        </button>
      </div>
    </div>

    <ul class="list">
      <li v-for="scan in visible" :key="scan.id" class="entry">
        <button class="item" :class="{ active: scan.id === state.selectedId }" @click="selectScan(scan.id)">
          <span class="thumb" :data-status="scan.status" aria-hidden="true">
            <img v-if="scan.thumbnail" :src="scan.thumbnail" alt="" />
            <span v-else-if="scan.missing" class="gone">?</span>
            <span v-else class="spinner"></span>
          </span>
          <span class="meta">
            <span class="name" :title="scan.path">{{ scan.name }}</span>
            <span class="sub">
              <span class="chip" :class="chipClass(scan)">{{ statusLabel(scan) }}</span>
              <span v-if="scan.width" class="muted">{{ scan.boxes.length }} {{ scan.boxes.length === 1 ? "item" : "items" }}</span>
              <span v-if="backsOf(scan)" class="muted" :title="`The backs are on ${backsOf(scan)!.name}`">fronts</span>
              <span v-else-if="frontsOf(scan)" class="muted" :title="`The backs of ${frontsOf(scan)!.name}`">backs</span>
            </span>
            <span v-if="scan.status === 'error'" class="flags" :title="scan.error ?? ''">{{ scan.error }}</span>
            <span v-else-if="scan.flags.length" class="flags">{{ flagText(scan) }}</span>
          </span>
        </button>
        <button class="remove" :title="`Remove ${scan.name} from the list (the file is not touched)`" @click="removeScan(scan)">×</button>
      </li>
      <li v-if="!visible.length" class="empty muted">
        {{ state.scans.length ? "Nothing in this view." : "Add scans, or drop them on the window." }}
      </li>
    </ul>

    <div class="foot">
      <button class="btn" title="Start again with an empty list. This project stays saved." @click="emit('newProject')">
        Create new project…
      </button>
    </div>
  </aside>
</template>

<style scoped>
.queue {
  display: flex;
  flex-direction: column;
  min-height: 0;
  background: var(--surface);
  border-right: var(--divider) solid var(--border);
}
.rail {
  align-items: center;
  gap: 0.4rem;
  padding: 0.5rem 0.25rem;
}
.rail .btn {
  width: 40px;
  padding: 0;
  justify-content: center;
}
.count {
  display: grid;
  justify-items: center;
  gap: 3px;
  width: 100%;
  font-size: 12px;
}
.count .progress {
  width: 40px;
}
.rail svg {
  width: 18px;
  height: 18px;
  fill: none;
  stroke: currentColor;
  stroke-width: 1.7;
  stroke-linecap: round;
  stroke-linejoin: round;
}
.foot {
  padding: 0.6rem 0.75rem;
  border-top: 1px solid var(--border);
}
.foot .btn {
  width: 100%;
  justify-content: center;
}
.strip {
  display: grid;
  align-content: start;
  gap: 4px;
  flex: 1;
  width: 100%;
  min-height: 0;
  margin: 0;
  padding: 0;
  list-style: none;
  overflow-y: auto;
}
.strip .item {
  justify-content: center;
  padding: 3px;
}
.strip .thumb {
  width: 44px;
  height: 44px;
}
.head {
  display: grid;
  gap: 0.5rem;
  padding: 0.75rem;
  border-bottom: 1px solid var(--border);
}
.small {
  font-size: 12px;
}
.small-btn {
  height: 24px;
  font-size: 12px;
  padding: 0 0.4rem;
}
.progress {
  height: 4px;
  background: var(--surface-2);
  border-radius: 2px;
  overflow: hidden;
}
.bar {
  height: 100%;
  background: var(--ok);
  transition: width 0.2s;
}
.tabs {
  display: flex;
  gap: 2px;
  padding: 2px;
  background: var(--surface-2);
  border-radius: 7px;
}
.tab {
  flex: 1;
  height: 24px;
  border: 0;
  border-radius: 5px;
  background: transparent;
  color: var(--muted);
  cursor: pointer;
}
.tab[aria-selected="true"] {
  background: var(--surface);
  color: var(--text);
  box-shadow: 0 1px 2px rgb(0 0 0 / 0.15);
}
.list {
  flex: 1;
  min-height: 0;
  margin: 0;
  padding: 0.4rem;
  list-style: none;
  overflow-y: auto;
}
.entry {
  position: relative;
}
.item {
  display: flex;
  gap: 0.65rem;
  width: 100%;
  padding: 0.5rem;
  background: transparent;
  border: 1px solid transparent;
  border-radius: 8px;
  text-align: left;
  cursor: pointer;
}
.item:hover {
  background: var(--surface-2);
}
.item.active {
  background: var(--accent-soft);
  border-color: var(--accent);
}
.item:focus-visible {
  outline: 2px solid var(--accent);
}
.thumb {
  display: grid;
  place-items: center;
  flex: none;
  width: 52px;
  height: 52px;
  background: var(--surface-2);
  border: 1px solid var(--border);
  border-left: 3px solid var(--muted);
  border-radius: 3px;
  overflow: hidden;
}
.thumb[data-status="approved"] {
  border-left-color: var(--ok);
}
.thumb[data-status="flagged"],
.thumb[data-status="error"] {
  border-left-color: var(--warn);
}
.thumb img {
  max-width: 100%;
  max-height: 100%;
  display: block;
}
.gone {
  color: var(--muted);
  font-weight: 700;
}
.spinner {
  width: 16px;
  height: 16px;
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
  gap: 3px;
  min-width: 0;
  align-content: center;
  flex: 1;
}
.name {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.sub {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  font-size: 12px;
}
.flags {
  font-size: 11px;
  color: var(--warn);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.remove {
  position: absolute;
  top: 6px;
  right: 6px;
  width: 20px;
  height: 20px;
  padding: 0;
  line-height: 1;
  border: 0;
  border-radius: 50%;
  background: transparent;
  color: var(--muted);
  cursor: pointer;
  opacity: 0;
}
.entry:hover .remove,
.remove:focus-visible {
  opacity: 1;
}
.remove:hover {
  background: var(--surface-2);
  color: var(--text);
}
.empty {
  padding: 1rem;
  text-align: center;
}
</style>
