<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from "vue";

const emit = defineEmits<{ keep: []; discard: [] }>();

const keepButton = ref<HTMLButtonElement>();

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
      <h1 id="close-title">An export is still running</h1>
      <p class="muted">
        If you close OxiCut now, some images may be left half written. Your project is saved, and your original scans are never
        changed.
      </p>
      <div class="actions">
        <button ref="keepButton" class="btn primary" @click="emit('keep')">Keep working</button>
        <button class="btn danger" @click="emit('discard')">Close anyway</button>
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
