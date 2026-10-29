<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref } from "vue";
import * as api from "../api";
import { approvedCount, forgetProject, newProject, openProject, renameProject, say, state } from "../store";

const props = defineProps<{ start?: "list" | "new" }>();
const emit = defineEmits<{ close: [] }>();

// Opened straight into "new project" (from the scan list), the dialog shows only that and cancelling closes it.
const direct = props.start === "new";

const busy = computed(() => state.detect.active || state.exporting.active);
const exists = ref<Record<string, boolean>>({});
const mode = ref<"list" | "new" | "rename">("list");
const text = ref("");
const input = ref<HTMLInputElement | null>(null);
// A plain function ref: one of the two inputs sits inside a list, where a named ref would become an array.
const setInput = (el: unknown) => (input.value = el as HTMLInputElement | null);

const nameOf = (path: string) => (path.split(/[\\/]/).pop() ?? path).replace(/\.oxicut$/i, "");
const sameFile = (a: string, b: string) => a.toLowerCase() === b.toLowerCase();
const isCurrent = (path: string) => sameFile(path, state.project.path);
const plural = (n: number, one: string, many: string) => `${n} ${n === 1 ? one : many}`;
const when = (seconds: number) => (seconds ? new Date(seconds * 1000).toLocaleDateString() : "");

/** The open project first (its numbers are live), then the others as last saved. */
const projects = computed(() => {
  const others = state.settings.recentProjects.filter((p) => !isCurrent(p.path));
  const current = {
    path: state.project.path,
    scans: state.scans.length,
    approved: approvedCount.value,
    updated: state.settings.recentProjects.find((p) => isCurrent(p.path))?.updated ?? 0,
  };
  return [current, ...others];
});

async function checkFiles() {
  const paths = projects.value.map((p) => p.path);
  try {
    const found = await api.filesExist(paths);
    exists.value = Object.fromEntries(paths.map((p, i) => [p, found[i]]));
  } catch {
    exists.value = {};
  }
}

async function open(path: string) {
  if (await openProject(path)) emit("close");
}

async function openFile() {
  const path = await api.pickProject();
  if (path) await open(path);
}

async function begin(next: "new" | "rename") {
  mode.value = next;
  text.value = next === "rename" ? state.project.name : "New project";
  await nextTick();
  input.value?.focus();
  input.value?.select();
}

async function confirm() {
  const name = text.value.trim();
  if (!name) return;
  const done = mode.value === "new" ? await newProject(name) : await renameProject(name);
  if (!done) return;
  if (mode.value === "new") emit("close");
  else {
    mode.value = "list";
    void checkFiles();
  }
}

function cancelForm() {
  if (direct) emit("close");
  else mode.value = "list";
}

function onKey(event: KeyboardEvent) {
  if (event.key !== "Escape") return;
  if (mode.value === "list") emit("close");
  else cancelForm();
}

const reveal = (path: string) => api.revealInExplorer(path).catch(() => say("That folder could not be opened."));

onMounted(() => {
  window.addEventListener("keydown", onKey);
  void checkFiles();
  if (direct) void begin("new");
});
onBeforeUnmount(() => window.removeEventListener("keydown", onKey));
</script>

<template>
  <div class="backdrop" @mousedown.self="emit('close')">
    <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="projects-title">
      <template v-if="direct">
        <h1 id="projects-title">Start a new project</h1>
        <p class="muted lead">
          The new project starts with an empty list. The project you are working on is saved, and you can come back to it any time
          from the project name at the top of the window.
        </p>
        <p v-if="busy" class="note">Detection or export is running. You can start a new project when it has finished.</p>
      </template>
      <template v-else>
        <h1 id="projects-title">Projects</h1>
        <p class="muted lead">
          A project is one file that keeps your scans, boxes and approvals. It saves itself, so you can close OxiCut and carry on
          later. Project files live in your Documents folder, in <strong>OxiCut</strong>.
        </p>
        <p v-if="busy" class="note">Detection or export is running. You can switch projects when it has finished.</p>
      </template>

      <ul v-if="!direct" class="list">
        <li v-for="p in projects" :key="p.path" class="row" :class="{ current: isCurrent(p.path) }">
          <div class="info">
            <template v-if="isCurrent(p.path) && mode === 'rename'">
              <input :ref="setInput" v-model="text" class="name-input" aria-label="Project name" @keydown.enter="confirm" />
            </template>
            <strong v-else class="name">{{ nameOf(p.path) }}</strong>
            <span class="muted small">
              {{ plural(p.scans, "scan", "scans") }}, {{ p.approved }} approved<template v-if="when(p.updated)"> · {{ when(p.updated) }}</template>
            </span>
            <span v-if="exists[p.path] === false" class="chip warn">file not found</span>
          </div>
          <div class="buttons">
            <template v-if="isCurrent(p.path)">
              <template v-if="mode === 'rename'">
                <button class="btn primary" :disabled="!text.trim()" @click="confirm">Save name</button>
                <button class="btn" @click="mode = 'list'">Cancel</button>
              </template>
              <template v-else>
                <span class="chip ok">Open now</span>
                <button class="btn" @click="begin('rename')">Rename</button>
                <button class="btn" @click="reveal(p.path)">Show in folder</button>
              </template>
            </template>
            <template v-else>
              <button class="btn" :disabled="busy || exists[p.path] === false" @click="open(p.path)">Open</button>
              <button class="btn ghost" title="Take it off this list (the file is not deleted)" @click="forgetProject(p.path)">
                Remove from list
              </button>
            </template>
          </div>
        </li>
      </ul>

      <div v-if="mode === 'new'" class="new">
        <input :ref="setInput" v-model="text" placeholder="Name, for example: Grandma's postcards" aria-label="New project name" @keydown.enter="confirm" />
        <button class="btn primary" :disabled="!text.trim() || busy" @click="confirm">Create</button>
        <button class="btn" @click="cancelForm">Cancel</button>
      </div>

      <div v-if="!direct" class="actions">
        <button class="btn" :disabled="busy || mode === 'new'" @click="begin('new')">New project…</button>
        <button class="btn" :disabled="busy" @click="openFile">Open a file…</button>
        <span class="grow"></span>
        <button class="btn primary" @click="emit('close')">Close</button>
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
  width: min(600px, 100%);
  max-height: calc(100vh - 2rem);
  overflow-y: auto;
  padding: 1.5rem 1.75rem;
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: 12px;
  box-shadow: 0 20px 60px rgb(0 0 0 / 0.35);
}
h1 {
  margin: 0 0 0.5rem;
  font-size: 22px;
  letter-spacing: -0.02em;
}
.lead {
  margin: 0 0 1rem;
  font-size: 13px;
  line-height: 1.5;
}
.note {
  margin: 0 0 1rem;
  padding: 0.5rem 0.7rem;
  background: var(--warn-soft);
  color: var(--warn);
  border-radius: 6px;
  font-size: 12px;
}
.list {
  display: grid;
  gap: 0.4rem;
  margin: 0 0 1rem;
  padding: 0;
  list-style: none;
}
.row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 0.75rem;
  padding: 0.65rem 0.8rem;
  border: 1px solid var(--border);
  border-radius: 8px;
}
.row.current {
  border-color: var(--accent);
  background: var(--accent-soft);
}
.info {
  display: grid;
  gap: 2px;
  min-width: 0;
}
.name {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.small {
  font-size: 12px;
}
.buttons {
  display: flex;
  flex: none;
  align-items: center;
  gap: 0.4rem;
}
.name-input,
.new input {
  width: 100%;
}
.new {
  display: flex;
  gap: 0.5rem;
  margin-bottom: 1rem;
}
.actions {
  display: flex;
  align-items: center;
  gap: 0.5rem;
}
.grow {
  flex: 1;
}
</style>
