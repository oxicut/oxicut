<script setup lang="ts">
// The Details tab: what is known about the pictures on the selected scan.
import { computed } from "vue";
import { isEmptyDetails, type Details } from "../details";
import { copyDetailsToOthers, say, selectedScan, setScanDetails, state } from "../store";
import DetailsFields from "./DetailsFields.vue";

const scan = selectedScan;
const details = computed<Details>({
  get: () => scan.value?.details ?? { date: "", place: "", caption: "", people: [] },
  set: (next) => scan.value && setScanDetails(scan.value, next),
});

const canCopy = computed(() => state.scans.length > 1 && !!scan.value && !isEmptyDetails(scan.value.details));

function copyToOthers() {
  if (!scan.value) return;
  const changed = copyDetailsToOthers(scan.value);
  say(
    changed
      ? `Filled in what was missing on ${changed} other scan${changed === 1 ? "" : "s"}. What they already had was kept.`
      : "The other scans already have their own details, so nothing was changed.",
  );
}
</script>

<template>
  <div class="body">
    <template v-if="scan">
      <h2>About the pictures on this scan</h2>
      <p class="muted note">{{ scan.name }}</p>
      <DetailsFields v-model="details" :id-prefix="`scan-${scan.id}`" />
      <p class="muted note">
        These are saved inside every picture you export from this scan. Your scan itself is not changed. A single
        picture can have its own details: open the Crops view and choose “About this picture”.
      </p>
      <button class="btn" :disabled="!canCopy" title="Fill in what is still empty on the other scans" @click="copyToOthers">
        Copy to the other scans
      </button>
    </template>
    <p v-else class="muted">Choose a scan on the left to describe its pictures.</p>
  </div>
</template>

<style scoped>
.body {
  display: grid;
  gap: 0.5rem;
  align-content: start;
  padding: 0.9rem;
  overflow-y: auto;
}
h2 {
  margin: 0;
  font-size: 14px;
}
.note {
  margin: 0;
  font-size: 12px;
  line-height: 1.5;
}
</style>
