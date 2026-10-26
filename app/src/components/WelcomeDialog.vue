<script setup lang="ts">
import { MODELS } from "../mock";

const emit = defineEmits<{ downloadModels: []; skip: [] }>();
const totalMb = MODELS.reduce((sum, m) => sum + m.sizeMb, 0);
</script>

<template>
  <div class="backdrop">
    <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="welcome-title">
      <h1 id="welcome-title">Welcome to Ox<span>Cut</span></h1>
      <p>
        OxCut splits scanned prints and postcards into separate, straightened images. Add a batch of scans, let it find
        each item, and fix any box that looks off.
      </p>
      <p>
        The most accurate detection mode uses the MobileSAM model (<strong>{{ totalMb }} MB</strong>, downloaded once).
        You can start right away with the built-in fast mode and add it later.
      </p>
      <div class="actions">
        <button class="btn primary" @click="emit('downloadModels')">Download models ({{ totalMb }} MB)</button>
        <button class="btn" @click="emit('skip')">Continue with fast mode</button>
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
  z-index: 10;
}
.dialog {
  max-width: 460px;
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
p {
  margin: 0 0 0.9rem;
  line-height: 1.55;
  color: var(--muted);
}
.actions {
  display: flex;
  flex-wrap: wrap;
  gap: 0.5rem;
  margin-top: 1.25rem;
}
</style>
