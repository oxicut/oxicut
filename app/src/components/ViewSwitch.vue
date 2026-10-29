<script setup lang="ts">
import { cropCount, state } from "../store";
</script>

<template>
  <div class="switch" role="group" aria-label="What to show">
    <button class="seg" :aria-pressed="state.view === 'scan'" title="The scan with its boxes" @click="state.view = 'scan'">Scan</button>
    <button
      class="seg"
      :aria-pressed="state.view === 'crops'"
      title="Preview the cut-out, straightened pictures before you export them"
      @click="state.view = 'crops'"
    >
      Crops <span class="count">{{ cropCount }}</span>
    </button>
  </div>
</template>

<style scoped>
.switch {
  display: flex;
  flex: none;
  gap: 2px;
  padding: 2px;
  background: var(--surface-2);
  border-radius: 7px;
}
.seg {
  height: 26px;
  padding: 0 0.75rem;
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
  margin-left: 0.2rem;
  padding: 0 6px;
  border-radius: 8px;
  background: var(--surface-2);
  font-size: 11px;
}
</style>
