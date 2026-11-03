<script setup lang="ts">
// The four fields shared by the scan's Details tab and a picture's own dialog.
import { computed, ref, watch } from "vue";
import { describeDate } from "../dates";
import { MAX_CAPTION, MAX_TEXT, parsePeople, peopleText, type Details } from "../details";

const props = defineProps<{
  modelValue: Details;
  /** What is used when a field is left empty (a picture shows its scan's here), shown greyed out. */
  inherited?: Details;
  idPrefix: string;
}>();
const emit = defineEmits<{ "update:modelValue": [details: Details] }>();

// What is typed in the people field, kept as typed (a trailing comma must survive); it is only put back
// from the details when they change from outside (another scan, another picture).
const peopleDraft = ref(peopleText(props.modelValue.people));
watch(
  () => props.modelValue.people,
  (people) => {
    if (parsePeople(peopleDraft.value).join("|") !== people.join("|")) peopleDraft.value = peopleText(people);
  },
);
const hint = computed(() => describeDate(props.modelValue.date || ""));

function change(patch: Partial<Details>) {
  emit("update:modelValue", { ...props.modelValue, ...patch });
}
</script>

<template>
  <div class="fields">
    <label :for="`${idPrefix}-date`">Date</label>
    <input
      :id="`${idPrefix}-date`"
      type="text"
      :value="modelValue.date"
      :maxlength="MAX_TEXT"
      :placeholder="inherited?.date || '1975, circa 1980, 14 March 1982'"
      @input="change({ date: ($event.target as HTMLInputElement).value })"
    />
    <p v-if="hint" class="hint muted">{{ hint }}</p>

    <label :for="`${idPrefix}-place`">Place</label>
    <input
      :id="`${idPrefix}-place`"
      type="text"
      :value="modelValue.place"
      :maxlength="MAX_TEXT"
      :placeholder="inherited?.place || 'Lake Balaton, Hungary'"
      @input="change({ place: ($event.target as HTMLInputElement).value })"
    />

    <label :for="`${idPrefix}-caption`">Caption</label>
    <textarea
      :id="`${idPrefix}-caption`"
      rows="3"
      :value="modelValue.caption"
      :maxlength="MAX_CAPTION"
      :placeholder="inherited?.caption || 'What is happening, or what is written on it'"
      @input="change({ caption: ($event.target as HTMLTextAreaElement).value })"
    ></textarea>

    <label :for="`${idPrefix}-people`">People</label>
    <input
      :id="`${idPrefix}-people`"
      type="text"
      :value="peopleDraft"
      :placeholder="inherited?.people.length ? peopleText(inherited.people) : 'Anna, Peter, Grandma'"
      @input="
        peopleDraft = ($event.target as HTMLInputElement).value;
        change({ people: parsePeople(peopleDraft) });
      "
      @blur="peopleDraft = peopleText(modelValue.people)"
    />
    <p class="hint muted">Separate the names with commas.</p>
  </div>
</template>

<style scoped>
.fields {
  display: grid;
  gap: 0.3rem;
}
label {
  margin-top: 0.5rem;
  font-size: 12px;
  color: var(--muted);
}
input,
textarea {
  width: 100%;
  padding: 0.4rem 0.55rem;
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: 6px;
  color: var(--text);
  font: inherit;
}
textarea {
  resize: vertical;
}
input:focus,
textarea:focus {
  outline: 2px solid var(--accent);
  outline-offset: 0;
}
.hint {
  margin: 0;
  font-size: 12px;
}
</style>
