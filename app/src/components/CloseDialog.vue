<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import type { CloseAssessment } from "../closeGuard";

const props = defineProps<{ assessment: CloseAssessment }>();
const emit = defineEmits<{ keep: []; discard: [] }>();

const keepButton = ref<HTMLButtonElement>();

const plural = (n: number, one: string, many: string) => `${n} ${n === 1 ? one : many}`;

const lines = computed(() => {
  const a = props.assessment;
  const out: string[] = [];
  if (a.exporting) out.push("An export is still running. Closing now can leave some images half written.");
  if (a.neverExported) out.push(`${plural(a.neverExported, "approved scan has", "approved scans have")} not been exported.`);
  if (a.changedSinceExport) {
    out.push(`${plural(a.changedSinceExport, "scan was", "scans were")} changed after being exported, so the exported files are out of date.`);
  }
  if (a.editedUnapproved) {
    out.push(`${plural(a.editedUnapproved, "scan has", "scans have")} edits that are not approved, so an export would skip them.`);
  }
  return out;
});

function onKey(event: KeyboardEvent) {
  if (event.key === "Escape") emit("keep");
}

onMounted(() => {
  window.addEventListener("keydown", onKey);
  keepButton.value?.focus();
});
onBeforeUnmount(() => window.removeEventListener("keydown", onKey));
</script>

<template>
  <div class="backdrop">
    <div class="dialog" role="alertdialog" aria-modal="true" aria-labelledby="close-title">
      <h1 id="close-title">Close OxiCut?</h1>
      <ul>
        <li v-for="line in lines" :key="line">{{ line }}</li>
      </ul>
      <p class="muted">
        Nothing is kept between sessions yet: closing forgets the scan list and your edits. Your original scans are never
        changed.
      </p>
      <div class="actions">
        <button ref="keepButton" class="btn primary" @click="emit('keep')">Keep working</button>
        <button class="btn danger" @click="emit('discard')">
          {{ assessment.exporting ? "Close anyway" : "Close without exporting" }}
        </button>
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
  max-width: 480px;
  padding: 1.5rem 1.75rem;
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: 12px;
  box-shadow: 0 20px 60px rgb(0 0 0 / 0.35);
}
h1 {
  margin: 0 0 0.75rem;
  font-size: 22px;
  letter-spacing: -0.02em;
}
ul {
  margin: 0 0 0.9rem;
  padding-left: 1.2rem;
  display: grid;
  gap: 0.4rem;
  line-height: 1.5;
}
p {
  margin: 0 0 1.1rem;
  font-size: 12px;
  line-height: 1.5;
}
.actions {
  display: flex;
  flex-wrap: wrap;
  gap: 0.5rem;
}
</style>
