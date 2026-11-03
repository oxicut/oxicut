<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { openLink, whatsNew } from "../api";
import { blocksOf, type Block } from "../changelogView";
import { say, state } from "../store";
import { entriesSince, entryFor, parseChangelog, type ChangelogEntry } from "../updates";
import ChangelogSpans from "./ChangelogSpans.vue";

const emit = defineEmits<{ close: [] }>();

const closeButton = ref<HTMLButtonElement>();
const loading = ref(true);
const failed = ref(false);
const changelog = ref<ChangelogEntry[]>([]);

/** What to show: every entry newer than this build up to the newest, or this build's own entry when it is the newest. */
const entries = computed<ChangelogEntry[]>(() => {
  if (state.update) return entriesSince(changelog.value, state.version, state.update.version);
  const own = entryFor(changelog.value, state.version);
  return own ? [own] : [];
});

const shown = computed<{ version: string; date: string; blocks: Block[] }[]>(() =>
  entries.value.length
    ? entries.value.map((e) => ({ version: e.version, date: e.date, blocks: blocksOf(e.body) }))
    : // The release's own notes stand in until there is a changelog entry for it.
      state.update?.notes
      ? [{ version: state.update.version, date: "", blocks: blocksOf(state.update.notes) }]
      : [],
);

function open(url: string) {
  openLink(url).catch(() => say("That page could not be opened."));
}

function getIt() {
  if (state.update) open(state.update.url);
}

function onKey(event: KeyboardEvent) {
  if (event.key === "Escape") emit("close");
}

onMounted(async () => {
  window.addEventListener("keydown", onKey);
  closeButton.value?.focus();
  try {
    const text = await whatsNew();
    changelog.value = text ? parseChangelog(text) : [];
  } catch {
    failed.value = true;
  } finally {
    loading.value = false;
  }
});
onBeforeUnmount(() => window.removeEventListener("keydown", onKey));
</script>

<template>
  <div class="backdrop" @mousedown.self="emit('close')">
    <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="new-title">
      <h1 id="new-title">{{ state.update ? "What is new" : "What is new in this version" }}</h1>
      <p v-if="state.update" class="lead">Version {{ state.update.version }} is available. You have {{ state.version }}.</p>

      <div class="notes" tabindex="0" aria-label="The list of changes">
        <p v-if="loading" class="muted">Looking for the list of changes…</p>

        <template v-else-if="shown.length">
          <section v-for="entry in shown" :key="entry.version" class="entry">
            <h2>
              Version {{ entry.version }}<span v-if="entry.date" class="muted date"> · {{ entry.date }}</span>
            </h2>
            <template v-for="(block, i) in entry.blocks" :key="i">
              <h3 v-if="block.kind === 'heading'"><ChangelogSpans :spans="block.spans" @open="open" /></h3>
              <p v-else-if="block.kind === 'paragraph'"><ChangelogSpans :spans="block.spans" @open="open" /></p>
              <ul v-else>
                <li v-for="(item, k) in block.items" :key="k"><ChangelogSpans :spans="item" @open="open" /></li>
              </ul>
            </template>
          </section>
        </template>

        <p v-else-if="failed" class="muted">The list of changes could not be loaded. You may be offline.</p>
        <p v-else-if="state.update" class="muted">There is no list of changes for this version yet.</p>
        <p v-else class="muted">There are no notes for this version yet.</p>
      </div>

      <div class="actions">
        <button v-if="state.update" class="btn primary" @click="getIt">Get version {{ state.update.version }}</button>
        <button ref="closeButton" class="btn" @click="emit('close')">Close</button>
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
  gap: 0.6rem;
  width: min(560px, 100%);
  max-height: calc(100vh - 2rem);
  padding: 1.5rem 1.75rem;
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: 12px;
  box-shadow: 0 20px 60px rgb(0 0 0 / 0.35);
}
h1 {
  margin: 0;
  font-size: 22px;
  letter-spacing: -0.02em;
}
.lead {
  margin: 0;
  color: var(--muted);
}
.notes {
  min-height: 0;
  max-height: 55vh;
  overflow: auto;
  padding: 0.25rem 0.1rem;
  line-height: 1.5;
}
.notes:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: 2px;
}
.entry + .entry {
  margin-top: 1.1rem;
  padding-top: 1rem;
  border-top: 1px solid var(--border);
}
h2 {
  margin: 0 0 0.3rem;
  font-size: 16px;
}
.date {
  font-size: 13px;
  font-weight: 400;
}
h3 {
  margin: 0.7rem 0 0.2rem;
  font-size: 12px;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.04em;
  color: var(--muted);
}
p {
  margin: 0.3rem 0;
}
ul {
  margin: 0.2rem 0;
  padding-left: 1.2rem;
}
li + li {
  margin-top: 0.2rem;
}
.actions {
  display: flex;
  justify-content: flex-end;
  gap: 0.5rem;
  margin-top: 0.5rem;
}
</style>
