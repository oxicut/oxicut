<script setup lang="ts">
// "About this picture": details that belong to one picture and win over its scan's.
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { detailsFor, setCropDetails } from "../store";
import { noDetails, type Details } from "../details";
import type { BoxItem, ScanItem } from "../store";
import DetailsFields from "./DetailsFields.vue";

const props = defineProps<{ scan: ScanItem; box: BoxItem; name: string }>();
const emit = defineEmits<{ close: [] }>();

/** What the picture has of its own (empty fields mean "use the scan's"). */
const own = ref<Details>({ ...noDetails(), ...(props.scan.cropDetails[props.box.id] ?? {}), people: [...(props.scan.cropDetails[props.box.id]?.people ?? [])] });
const inherited = computed(() => props.scan.details);
const closeButton = ref<HTMLButtonElement>();

function save() {
  setCropDetails(props.scan, props.box.id, own.value);
  emit("close");
}

function clear() {
  own.value = noDetails();
}

/** What the picture will carry, so it is clear what "empty" leads to. */
const result = computed(() => detailsFor({ ...props.scan, cropDetails: { [props.box.id]: own.value } }, props.box.id));

function onKey(event: KeyboardEvent) {
  if (event.key === "Escape") emit("close");
}
onMounted(() => {
  window.addEventListener("keydown", onKey);
  closeButton.value?.focus();
});
onBeforeUnmount(() => window.removeEventListener("keydown", onKey));
</script>

<template>
  <div class="backdrop" @mousedown.self="emit('close')">
    <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="about-picture-title">
      <h1 id="about-picture-title">About this picture</h1>
      <p class="muted">{{ name }}</p>
      <p class="muted small">
        Fill in only what is different for this picture. Anything you leave empty is taken from the scan.
      </p>
      <DetailsFields v-model="own" :inherited="inherited" :id-prefix="`crop-${scan.id}-${box.id}`" />
      <p class="muted small">It will be saved as: {{ [result.date, result.place].filter(Boolean).join(" · ") || "no date or place" }}.</p>
      <div class="actions">
        <button class="btn" @click="clear">Use the scan's details</button>
        <span class="grow"></span>
        <button class="btn" @click="emit('close')">Cancel</button>
        <button ref="closeButton" class="btn primary" @click="save">Save</button>
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
  gap: 0.5rem;
  width: min(460px, 100%);
  max-height: 92vh;
  padding: 1.4rem 1.6rem 1.2rem;
  overflow-y: auto;
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: 12px;
  box-shadow: 0 20px 60px rgb(0 0 0 / 0.35);
}
h1 {
  margin: 0;
  font-size: 18px;
}
p {
  margin: 0;
  line-height: 1.5;
}
.small {
  font-size: 12px;
}
.actions {
  display: flex;
  gap: 0.5rem;
  margin-top: 0.5rem;
}
.grow {
  flex: 1;
}
</style>
