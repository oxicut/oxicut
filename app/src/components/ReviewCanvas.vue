<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { Canvas, FabricImage, Point, Rect, util } from "fabric";
import { FLAG_LABELS, newBoxId, previewFor, type BoxItem, type ScanItem } from "../store";

const props = defineProps<{ scan: ScanItem | null; keysEnabled: boolean }>();
const emit = defineEmits<{
  approve: [];
  step: [delta: number];
  edit: [boxes: BoxItem[]];
  redetect: [];
}>();

const ACCENT = "#e8703a";
const WARNING = "#e5a03d";

const host = ref<HTMLDivElement>();
const el = ref<HTMLCanvasElement>();
const loading = ref(false);
const loadError = ref("");
const boxCount = ref(0);
const hasSelection = ref(false);
const zoomPercent = ref(100);

let canvas: Canvas | undefined;
let observer: ResizeObserver | undefined;
let buildToken = 0;
const ids = new WeakMap<object, number>();

/** The scan is decoded and boxes exist (or are known to be none). */
const ready = computed(() => !!props.scan && props.scan.width > 0 && props.scan.status !== "error");
const busyDetecting = computed(() => props.scan?.status === "queued" || props.scan?.status === "detecting");
const flagMessages = computed(() => {
  const seen = new Set<string>();
  return (props.scan?.flags ?? []).filter((f) => !seen.has(f.code) && seen.add(f.code)).map((f) => FLAG_LABELS[f.code]);
});

function marquees(): Rect[] {
  return (canvas?.getObjects() ?? []).filter((o): o is Rect => ids.has(o));
}

function syncState() {
  boxCount.value = marquees().length;
  hasSelection.value = (canvas?.getActiveObjects() ?? []).some((o) => ids.has(o));
}

/** Line thickness that stays about 2.5 screen pixels at any zoom. */
function restyle() {
  if (!canvas) return;
  const width = Math.max(1, 2.5 / canvas.getZoom());
  const flagged = new Set((props.scan?.flags ?? []).map((f) => f.boxId));
  for (const marquee of marquees()) {
    const warn = flagged.has(ids.get(marquee) ?? -1);
    marquee.set({ strokeWidth: width, stroke: warn ? WARNING : ACCENT, borderColor: warn ? WARNING : ACCENT });
  }
  canvas.requestRenderAll();
}

function makeMarquee(box: BoxItem): Rect {
  const rect = new Rect({
    left: box.cx,
    top: box.cy,
    width: box.w,
    height: box.h,
    angle: box.angle,
    originX: "center",
    originY: "center",
    fill: "rgba(232, 112, 58, 0.12)",
    strokeUniform: true,
    cornerStyle: "circle",
    cornerColor: ACCENT,
    cornerStrokeColor: "#ffffff",
    cornerSize: 13,
    transparentCorners: false,
    lockScalingFlip: true,
  });
  ids.set(rect, box.id);
  return rect;
}

const normaliseAngle = (a: number) => ((((a + 180) % 360) + 360) % 360) - 180;

/** Read the boxes back off the canvas and tell the store. */
function commit() {
  if (!props.scan) return;
  const boxes = marquees().map((o) => {
    // Fold the drag-resize scale into the size so it stays a plain rectangle.
    const w = Math.max(10, o.width * o.scaleX);
    const h = Math.max(10, o.height * o.scaleY);
    o.set({ width: w, height: h, scaleX: 1, scaleY: 1 });
    o.setCoords();
    return { id: ids.get(o)!, cx: o.left, cy: o.top, w, h, angle: normaliseAngle(o.angle) };
  });
  emit("edit", boxes);
  syncState();
}

function fit() {
  if (!canvas || !props.scan || props.scan.width === 0) return;
  const { width, height } = props.scan;
  const cw = canvas.getWidth();
  const ch = canvas.getHeight();
  const zoom = Math.min(cw / width, ch / height) * 0.96;
  canvas.setViewportTransform([zoom, 0, 0, zoom, (cw - width * zoom) / 2, (ch - height * zoom) / 2]);
  zoomPercent.value = Math.round(zoom * 100);
  restyle();
}

async function rebuild() {
  if (!canvas) return;
  const token = ++buildToken;
  canvas.discardActiveObject();
  canvas.remove(...canvas.getObjects());
  syncState();
  loadError.value = "";
  const scan = props.scan;
  if (!scan || scan.width === 0 || scan.status === "error") {
    loading.value = false;
    return;
  }
  loading.value = true;
  try {
    const image = await FabricImage.fromURL(await previewFor(scan.path));
    if (token !== buildToken) return; // another scan was chosen meanwhile
    image.set({
      left: 0,
      top: 0,
      originX: "left",
      originY: "top",
      scaleX: scan.width / image.width,
      scaleY: scan.height / image.height,
      selectable: false,
      evented: false,
    });
    canvas.add(image);
    for (const box of scan.boxes) canvas.add(makeMarquee(box));
    syncState();
    fit();
  } catch (e) {
    if (token === buildToken) loadError.value = `Could not show this scan: ${e instanceof Error ? e.message : e}`;
  } finally {
    if (token === buildToken) loading.value = false;
  }
}

function addBox() {
  if (!canvas || !props.scan) return;
  // Centre of what is on screen, in scan pixels.
  const centre = util.transformPoint(
    new Point(canvas.getWidth() / 2, canvas.getHeight() / 2),
    util.invertTransform(canvas.viewportTransform),
  );
  const side = Math.min(props.scan.width, props.scan.height);
  const rect = makeMarquee({ id: newBoxId(), cx: centre.x, cy: centre.y, w: side * 0.4, h: side * 0.3, angle: 0 });
  canvas.add(rect);
  canvas.setActiveObject(rect);
  restyle();
  commit();
}

function selectedMarquees(): Rect[] {
  return (canvas?.getActiveObjects() ?? []).filter((o): o is Rect => ids.has(o));
}

function deleteSelected() {
  if (!canvas) return;
  const targets = selectedMarquees();
  if (!targets.length) return;
  canvas.discardActiveObject();
  canvas.remove(...targets);
  commit();
}

/** Split the selected box in half across its longer side. */
function splitSelected() {
  if (!canvas) return;
  const [target] = selectedMarquees();
  if (!target) return;
  const w = target.width * target.scaleX;
  const h = target.height * target.scaleY;
  const theta = (target.angle * Math.PI) / 180;
  const alongWidth = w >= h;
  const axis = alongWidth ? { x: Math.cos(theta), y: Math.sin(theta) } : { x: -Math.sin(theta), y: Math.cos(theta) };
  const half = (alongWidth ? w : h) / 2;
  const halves = [-1, 1].map((sign) =>
    makeMarquee({
      id: newBoxId(),
      cx: target.left + (axis.x * half * sign) / 2,
      cy: target.top + (axis.y * half * sign) / 2,
      w: alongWidth ? half : w,
      h: alongWidth ? h : half,
      angle: target.angle,
    }),
  );
  canvas.discardActiveObject();
  canvas.remove(target);
  canvas.add(...halves);
  canvas.setActiveObject(halves[0]);
  restyle();
  commit();
}

function straighten() {
  if (!canvas) return;
  for (const target of selectedMarquees()) {
    target.rotate(Math.round(target.angle / 90) * 90);
    target.setCoords();
  }
  canvas.requestRenderAll();
  commit();
}

function onKey(event: KeyboardEvent) {
  if (!props.keysEnabled || event.ctrlKey || event.metaKey || event.altKey) return;
  const target = event.target as HTMLElement | null;
  if (target && ["INPUT", "SELECT", "TEXTAREA"].includes(target.tagName)) return;
  switch (event.key) {
    case "Enter":
      emit("approve");
      break;
    case "ArrowDown":
    case "ArrowRight":
      emit("step", 1);
      break;
    case "ArrowUp":
    case "ArrowLeft":
      emit("step", -1);
      break;
    case "Delete":
    case "Backspace":
      deleteSelected();
      break;
    default:
      return;
  }
  event.preventDefault();
}

onMounted(() => {
  if (!host.value || !el.value) return;
  canvas = new Canvas(el.value, {
    width: host.value.clientWidth,
    height: host.value.clientHeight,
    selection: false,
    preserveObjectStacking: true,
  });

  canvas.on("selection:created", (e) => {
    // One box at a time: a multi-selection would move boxes as a group.
    if (e.selected.length > 1) canvas!.setActiveObject(e.selected[0]);
    syncState();
  });
  canvas.on("selection:updated", syncState);
  canvas.on("selection:cleared", syncState);
  canvas.on("object:modified", commit);

  // Hold Shift while rotating to snap to 0/90/180/270.
  canvas.on("object:rotating", (opt) => {
    if ((opt.e as MouseEvent).shiftKey) opt.target.set("angle", Math.round(opt.target.angle / 90) * 90);
  });

  canvas.on("mouse:wheel", (opt) => {
    const event = opt.e as WheelEvent;
    const zoom = Math.min(4, Math.max(0.03, canvas!.getZoom() * 0.999 ** event.deltaY));
    canvas!.zoomToPoint(new Point(event.offsetX, event.offsetY), zoom);
    zoomPercent.value = Math.round(zoom * 100);
    restyle();
    event.preventDefault();
    event.stopPropagation();
  });

  // Drag empty space (or hold Alt) to pan.
  let panning = false;
  canvas.on("mouse:down", (opt) => {
    const event = opt.e as MouseEvent;
    if (!opt.target || !ids.has(opt.target) || event.altKey) {
      panning = true;
      canvas!.setCursor("grabbing");
    }
  });
  canvas.on("mouse:move", (opt) => {
    if (!panning) return;
    canvas!.relativePan(new Point((opt.e as MouseEvent).movementX, (opt.e as MouseEvent).movementY));
  });
  canvas.on("mouse:up", () => {
    panning = false;
    canvas!.setViewportTransform(canvas!.viewportTransform);
  });

  observer = new ResizeObserver(() => {
    if (!canvas || !host.value) return;
    canvas.setDimensions({ width: host.value.clientWidth, height: host.value.clientHeight });
    fit();
  });
  observer.observe(host.value);

  window.addEventListener("keydown", onKey);
  void rebuild();
});

onBeforeUnmount(() => {
  window.removeEventListener("keydown", onKey);
  observer?.disconnect();
  void canvas?.dispose();
});

// Rebuild when another scan is chosen or detection replaced its boxes.
watch(() => [props.scan?.id, props.scan?.revision, props.scan?.width], () => void rebuild());
// Re-colour boxes as warnings come and go.
watch(() => props.scan?.flags, restyle, { deep: true });
</script>

<template>
  <section class="review">
    <header class="toolbar">
      <div class="group">
        <button class="btn" :disabled="!ready" title="Add a box" @click="addBox">+ Box</button>
        <button class="btn" :disabled="!hasSelection" title="Split the selected box in half" @click="splitSelected">
          Split
        </button>
        <button class="btn danger" :disabled="!hasSelection" title="Delete the selected box" @click="deleteSelected">
          Delete <kbd>Del</kbd>
        </button>
        <button class="btn" :disabled="!hasSelection" title="Snap the selected box to the nearest 90°" @click="straighten">
          Straighten
        </button>
        <button class="btn" :disabled="!scan || busyDetecting" title="Run detection on this scan again" @click="emit('redetect')">
          Re-detect
        </button>
      </div>
      <div class="grow"></div>
      <div class="group">
        <button class="btn ghost" :disabled="!ready" title="Fit scan to window" @click="fit">Fit · {{ zoomPercent }}%</button>
        <button class="btn primary" :disabled="!ready" @click="emit('approve')">Approve &amp; next <kbd>Enter</kbd></button>
      </div>
    </header>

    <div ref="host" class="stage">
      <canvas ref="el"></canvas>
      <div v-if="!scan" class="overlay">
        <strong>No scans yet</strong>
        <span>Drop scans or a folder here, or use <em>Add scans…</em></span>
      </div>
      <div v-else-if="scan.status === 'error'" class="overlay error">
        <strong>{{ scan.name }} could not be processed</strong>
        <span>{{ scan.error }}</span>
        <button class="btn" @click="emit('redetect')">Try again</button>
      </div>
      <div v-else-if="busyDetecting || loading" class="overlay light">
        <span class="spinner" aria-hidden="true"></span>
        <span>{{ busyDetecting ? "Finding the prints…" : "Loading the scan…" }}</span>
      </div>
      <div v-else-if="loadError" class="overlay error">
        <strong>{{ loadError }}</strong>
      </div>
    </div>

    <footer class="infobar">
      <template v-if="scan">
        <strong class="name">{{ scan.name }}</strong>
        <span class="chip" :class="{ ok: scan.status === 'approved', warn: scan.status === 'flagged' || scan.status === 'error' }">
          {{ scan.status }}
        </span>
        <span v-if="ready" class="muted">{{ boxCount }} {{ boxCount === 1 ? "item" : "items" }} · {{ scan.width }}×{{ scan.height }} px</span>
        <span v-for="message in flagMessages" :key="message" class="chip warn">{{ message }}</span>
      </template>
      <span class="grow"></span>
      <span class="muted hint">Scroll to zoom · drag background to pan · Shift while rotating snaps to 90° · ←/→ previous/next</span>
    </footer>
  </section>
</template>

<style scoped>
.review {
  display: flex;
  flex-direction: column;
  min-width: 0;
  min-height: 0;
}
.toolbar {
  display: flex;
  align-items: center;
  gap: 1rem;
  padding: 0.5rem 0.75rem;
  background: var(--surface);
  border-bottom: 1px solid var(--border);
}
.group {
  display: flex;
  gap: 0.35rem;
}
.grow {
  flex: 1;
}
.stage {
  position: relative;
  flex: 1;
  min-height: 0;
  background: var(--stage);
  overflow: hidden;
}
.overlay {
  position: absolute;
  inset: 0;
  display: grid;
  place-content: center;
  justify-items: center;
  gap: 0.5rem;
  padding: 2rem;
  text-align: center;
  color: var(--muted);
  pointer-events: none;
}
.overlay strong {
  color: var(--text);
  font-size: 15px;
}
.overlay.error {
  pointer-events: auto;
}
.overlay.error strong {
  color: var(--warn);
}
.overlay.light {
  background: color-mix(in srgb, var(--stage) 70%, transparent);
  grid-auto-flow: column;
  place-content: center;
}
.overlay :is(button) {
  pointer-events: auto;
}
.spinner {
  width: 18px;
  height: 18px;
  border: 2px solid var(--border);
  border-top-color: var(--accent);
  border-radius: 50%;
  animation: spin 0.8s linear infinite;
}
@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}
.infobar {
  display: flex;
  align-items: center;
  gap: 0.6rem;
  padding: 0.45rem 0.75rem;
  background: var(--surface);
  border-top: 1px solid var(--border);
  font-size: 12px;
  flex-wrap: wrap;
}
.name {
  font-size: 13px;
}
.hint {
  font-size: 11px;
}
</style>
