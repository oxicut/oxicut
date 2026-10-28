<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from "vue";
import { state } from "../store";
import { THEME_CHOICES } from "../theme";

const emit = defineEmits<{ close: [] }>();

const dialog = ref<HTMLElement>();

function onKey(event: KeyboardEvent) {
  if (event.key === "Escape") emit("close");
}

onMounted(() => {
  window.addEventListener("keydown", onKey);
  dialog.value?.querySelector<HTMLInputElement>("input:checked")?.focus();
});
onBeforeUnmount(() => window.removeEventListener("keydown", onKey));
</script>

<template>
  <div class="backdrop" @mousedown.self="emit('close')">
    <div ref="dialog" class="dialog" role="dialog" aria-modal="true" aria-labelledby="settings-title">
      <h1 id="settings-title">Settings</h1>

      <section>
        <h2>Appearance</h2>
        <div class="choices" role="radiogroup" aria-label="Theme">
          <label v-for="choice in THEME_CHOICES" :key="choice.id" class="choice" :class="{ on: state.settings.theme === choice.id }">
            <input v-model="state.settings.theme" type="radio" name="theme" :value="choice.id" />
            <span>
              <strong>{{ choice.label }}</strong>
              <span class="muted">{{ choice.hint }}</span>
            </span>
          </label>
        </div>
      </section>

      <div class="actions">
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
  width: min(460px, 100%);
  padding: 1.5rem 1.75rem;
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: 12px;
  box-shadow: 0 20px 60px rgb(0 0 0 / 0.35);
}
h1 {
  margin: 0 0 1rem;
  font-size: 22px;
  letter-spacing: -0.02em;
}
h2 {
  margin: 0 0 0.5rem;
  font-size: 13px;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.04em;
  color: var(--muted);
}
.choices {
  display: grid;
  gap: 0.4rem;
}
.choice {
  display: flex;
  align-items: center;
  gap: 0.75rem;
  padding: 0.7rem 0.85rem;
  border: 1px solid var(--border);
  border-radius: 8px;
  cursor: pointer;
}
.choice:hover {
  background: var(--surface-2);
}
.choice.on {
  border-color: var(--accent);
  background: var(--accent-soft);
}
.choice input {
  width: 18px;
  height: 18px;
  margin: 0;
  accent-color: var(--accent);
}
.choice span {
  display: grid;
  gap: 2px;
}
.choice .muted {
  font-size: 12px;
}
.actions {
  display: flex;
  justify-content: flex-end;
  margin-top: 1.25rem;
}
</style>
