<script setup lang="ts">
import type { Span } from "../changelogView";

defineProps<{ spans: Span[] }>();
const emit = defineEmits<{ open: [url: string] }>();
</script>

<template>
  <template v-for="(span, i) in spans" :key="i">
    <strong v-if="span.kind === 'bold'">{{ span.text }}</strong>
    <code v-else-if="span.kind === 'code'">{{ span.text }}</code>
    <button v-else-if="span.kind === 'link'" class="link" @click="emit('open', span.url)">{{ span.text }}</button>
    <template v-else>{{ span.text }}</template>
  </template>
</template>

<style scoped>
code {
  padding: 0 0.25rem;
  background: var(--surface-2);
  border-radius: 4px;
  font-size: 0.92em;
}
.link {
  padding: 0;
  border: 0;
  background: none;
  color: var(--accent);
  font: inherit;
  text-decoration: underline;
  cursor: pointer;
}
</style>
