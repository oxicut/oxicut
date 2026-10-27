<script setup lang="ts">
const emit = defineEmits<{ addScans: []; close: [] }>();
</script>

<template>
  <div class="backdrop">
    <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="welcome-title">
      <h1 id="welcome-title">Welcome to <span>Oxi</span>Cut</h1>
      <p>OxiCut splits flatbed scans of photo prints and postcards into separate, straightened images.</p>
      <ol>
        <li><strong>Add</strong> scans or a whole folder: use the button, or drop them on the window.</li>
        <li><strong>Check</strong> the boxes OxiCut drew. Drag, resize or rotate any of them; <kbd>Enter</kbd> approves and moves on. Scans with nothing suspicious are approved for you.</li>
        <li><strong>Export</strong> the approved scans. Your originals are never changed.</li>
      </ol>
      <div class="actions">
        <button class="btn primary" @click="emit('addScans')">Add scans…</button>
        <button class="btn" @click="emit('close')">Got it</button>
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
  max-width: 480px;
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
p,
ol {
  margin: 0 0 0.9rem;
  line-height: 1.55;
  color: var(--muted);
}
ol {
  padding-left: 1.2rem;
  display: grid;
  gap: 0.5rem;
}
strong {
  color: var(--text);
}
.actions {
  display: flex;
  flex-wrap: wrap;
  gap: 0.5rem;
  margin-top: 1.25rem;
}
</style>
