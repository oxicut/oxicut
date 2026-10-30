<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted } from "vue";
import { linkBacks, state, whyNotBacks } from "../store";
import type { ScanItem } from "../store";

const props = defineProps<{ front: ScanItem }>();
const emit = defineEmits<{ close: [] }>();

/** Every other scan, with the reason it cannot be chosen (if there is one). */
const choices = computed(() =>
  state.scans
    .filter((s) => s.id !== props.front.id)
    .map((scan) => ({
      scan,
      problem: whyNotBacks(props.front, scan) ?? (scan.boxes.length === 0 ? "No pictures were found on it yet." : null),
    })),
);

function choose(scan: ScanItem) {
  if (linkBacks(props.front, scan)) emit("close");
}

function onKey(event: KeyboardEvent) {
  if (event.key === "Escape") emit("close");
}
onMounted(() => window.addEventListener("keydown", onKey));
onBeforeUnmount(() => window.removeEventListener("keydown", onKey));
</script>

<template>
  <div class="backdrop" @mousedown.self="emit('close')">
    <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="backs-title">
      <h1 id="backs-title">Which scan has the backs?</h1>
      <p class="muted">
        Choose the scan you made after turning the pictures over. OxiCut matches each front on
        <strong>{{ front.name }}</strong> with its back, and saves them as a pair.
      </p>

      <p v-if="!choices.length" class="muted">There is no other scan yet. Add the scan of the backs first.</p>
      <ul v-else class="list">
        <li v-for="{ scan, problem } in choices" :key="scan.id">
          <button class="choice" :disabled="!!problem" @click="choose(scan)">
            <img v-if="scan.thumbnail" :src="scan.thumbnail" alt="" />
            <span v-else class="blank" aria-hidden="true"></span>
            <span class="text">
              <strong>{{ scan.name }}</strong>
              <span class="muted small">{{ problem ?? `${scan.boxes.length} ${scan.boxes.length === 1 ? "picture" : "pictures"}` }}</span>
            </span>
          </button>
        </li>
      </ul>

      <div class="actions">
        <button class="btn" @click="emit('close')">Cancel</button>
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
  display: grid;
  gap: 0.75rem;
  width: min(460px, 100%);
  max-height: 90vh;
  padding: 1.5rem 1.75rem 1.25rem;
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: 12px;
  box-shadow: 0 20px 60px rgb(0 0 0 / 0.35);
}
h1 {
  margin: 0;
  font-size: 18px;
}
p {
  margin: 0;
  line-height: 1.5;
}
.list {
  display: grid;
  gap: 0.4rem;
  margin: 0;
  padding: 0;
  overflow-y: auto;
  list-style: none;
}
.choice {
  display: flex;
  align-items: center;
  gap: 0.75rem;
  width: 100%;
  padding: 0.4rem 0.6rem;
  text-align: left;
  background: var(--surface-2);
  border: 1px solid var(--border);
  border-radius: 8px;
  color: var(--text);
  cursor: pointer;
}
.choice:hover:not(:disabled) {
  border-color: var(--accent);
}
.choice:disabled {
  opacity: 0.55;
  cursor: not-allowed;
}
.choice img,
.blank {
  flex: none;
  width: 44px;
  height: 44px;
  object-fit: cover;
  background: var(--stage);
  border-radius: 4px;
}
.text {
  display: grid;
  min-width: 0;
}
.text strong {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.small {
  font-size: 12px;
}
.actions {
  display: flex;
  justify-content: flex-end;
}
</style>
