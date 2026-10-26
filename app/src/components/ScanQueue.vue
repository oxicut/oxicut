<script setup lang="ts">
import { computed, ref } from "vue";
import { FLAG_LABELS, type Scan } from "../mock";

const props = defineProps<{ scans: Scan[]; selectedId: number }>();
const emit = defineEmits<{ select: [id: number]; notice: [message: string] }>();

type Filter = "all" | "flagged" | "approved";
const filter = ref<Filter>("all");

const approvedCount = computed(() => props.scans.filter((s) => s.status === "approved").length);
const visible = computed(() =>
  filter.value === "all" ? props.scans : props.scans.filter((s) => s.status === filter.value),
);
const flagText = (scan: Scan) => scan.flags.map((flag) => FLAG_LABELS[flag]).join(" · ");
const filters: { id: Filter; label: string }[] = [
  { id: "all", label: "All" },
  { id: "flagged", label: "Flagged" },
  { id: "approved", label: "Approved" },
];
</script>

<template>
  <aside class="queue">
    <div class="head">
      <div class="row">
        <button class="btn primary grow" @click="emit('notice', 'Adding scans is not wired up yet (UI preview).')">
          Add scans…
        </button>
        <button class="btn" @click="emit('notice', 'Adding a folder is not wired up yet (UI preview).')">
          Folder…
        </button>
      </div>
      <div class="progress" :title="`${approvedCount} of ${scans.length} approved`">
        <div class="bar" :style="{ width: `${(approvedCount / Math.max(1, scans.length)) * 100}%` }"></div>
      </div>
      <div class="muted small">{{ approvedCount }} of {{ scans.length }} approved</div>
      <div class="tabs" role="tablist">
        <button
          v-for="f in filters"
          :key="f.id"
          class="tab"
          role="tab"
          :aria-selected="filter === f.id"
          @click="filter = f.id"
        >
          {{ f.label }}
        </button>
      </div>
    </div>

    <ul class="list">
      <li v-for="scan in visible" :key="scan.id">
        <button class="item" :class="{ active: scan.id === selectedId }" @click="emit('select', scan.id)">
          <span class="thumb" :data-status="scan.status" aria-hidden="true">
            <i
              v-for="(b, i) in scan.boxes.slice(0, 4)"
              :key="i"
              :style="{
                left: `${(b.cx / 2550) * 100}%`,
                top: `${(b.cy / 3507) * 100}%`,
                width: `${(b.w / 2550) * 100}%`,
                height: `${(b.h / 3507) * 100}%`,
                transform: `translate(-50%, -50%) rotate(${b.angle}deg)`,
              }"
            ></i>
          </span>
          <span class="meta">
            <span class="name">{{ scan.name }}</span>
            <span class="sub">
              <span class="chip" :class="{ ok: scan.status === 'approved', warn: scan.status === 'flagged' }">
                {{ scan.status }}
              </span>
              <span class="muted">{{ scan.boxes.length }} {{ scan.boxes.length === 1 ? "item" : "items" }}</span>
            </span>
            <span v-if="scan.flags.length" class="flags">{{ flagText(scan) }}</span>
          </span>
        </button>
      </li>
      <li v-if="!visible.length" class="empty muted">Nothing here.</li>
    </ul>
  </aside>
</template>

<style scoped>
.queue {
  display: flex;
  flex-direction: column;
  min-height: 0;
  background: var(--surface);
  border-right: 1px solid var(--border);
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
  position: relative;
  flex: none;
  width: 40px;
  height: 56px;
  background: #fbfbf9;
  border: 1px solid var(--border);
  border-left: 3px solid var(--muted);
  border-radius: 3px;
  overflow: hidden;
}
.thumb[data-status="approved"] {
  border-left-color: var(--ok);
}
.thumb[data-status="flagged"] {
  border-left-color: var(--warn);
}
.thumb i {
  position: absolute;
  background: #8aa0ad;
  opacity: 0.85;
}
.meta {
  display: grid;
  gap: 3px;
  min-width: 0;
  align-content: center;
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
}
.empty {
  padding: 1rem;
  text-align: center;
}
</style>
