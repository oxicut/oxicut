<script setup lang="ts">
import { openWhatsNew, state } from "../store";

const emit = defineEmits<{ opened: [] }>();

function open() {
  emit("opened");
  openWhatsNew();
}
</script>

<template>
  <button
    v-if="state.update"
    class="update-icon"
    :title="`Version ${state.update.version} is available. See what is new.`"
    :aria-label="`A newer version is available: ${state.update.version}. See what is new.`"
    @click="open"
  >
    <svg viewBox="0 0 24 24" aria-hidden="true">
      <circle cx="12" cy="12" r="9" />
      <path d="M12 16V8M8.5 11.5 12 8l3.5 3.5" />
    </svg>
  </button>
</template>

<style scoped>
.update-icon {
  display: inline-grid;
  place-items: center;
  width: 22px;
  height: 22px;
  padding: 0;
  background: transparent;
  border: 0;
  border-radius: 50%;
  color: var(--accent);
  cursor: pointer;
}
.update-icon:hover {
  background: var(--accent-soft);
}
.update-icon:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: 1px;
}
.update-icon svg {
  width: 18px;
  height: 18px;
  fill: none;
  stroke: currentColor;
  stroke-width: 1.8;
  stroke-linecap: round;
  stroke-linejoin: round;
}
</style>
