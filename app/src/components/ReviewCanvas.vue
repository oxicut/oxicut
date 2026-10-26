<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref, watch } from "vue";
import { Canvas, Gradient, Point, Rect } from "fabric";
import { FLAG_LABELS, SCAN_HEIGHT, SCAN_WIDTH, type Box, type Scan } from "../mock";

const props = defineProps<{ scan: Scan; keysEnabled: boolean }>();
const emit = defineEmits<{
  approve: [];
  step: [delta: number];
  notice: [message: string];
}>();

const PHOTO_TINTS: [string, string][] = [
  ["#6c8ea4", "#d9b38c"],
  ["#8a9a5b", "#e8d9a0"],
  ["#b5675a", "#403a4a"],
  ["#4f6d7a", "#c0d6df"],
];

const flagLabels: Record<string, string> = FLAG_LABELS;
const host = ref<HTMLDivElement>();
const el = ref<HTMLCanvasElement>();
const boxCount = ref(0);
const hasSelection = ref(false);
const zoomPercent = ref(100);

let canvas: Canvas | undefined;
let observer: ResizeObserver | undefined;
const marquees = new WeakSet<object>();

function makeMarquee(b: Box): Rect {
  const rect = new Rect({
    left: b.cx,
    top: b.cy,
    width: b.w,
    height: b.h,
    angle: b.angle,
    originX: "center",
    originY: "center",
    fill: "rgba(232, 112, 58, 0.14)",
    stroke: "#e8703a",
    strokeWidth: 3,
    strokeUniform: true,
    cornerStyle: "circle",
    cornerColor: "#e8703a",
    cornerStrokeColor: "#ffffff",
    cornerSize: 13,
    transparentCorners: false,
    borderColor: "#e8703a",
    lockScalingFlip: true,
  });
  marquees.add(rect);
  return rect;
}

function makePhoto(b: Box, index: number): Rect {
  const [from, to] = PHOTO_TINTS[index % PHOTO_TINTS.length];
  return new Rect({
    left: b.cx,
    top: b.cy,
    width: b.w - 70,
    height: b.h - 70,
    angle: b.angle,
    originX: "center",
    originY: "center",
    fill: new Gradient({
      type: "linear",
      coords: { x1: 0, y1: 0, x2: b.w - 70, y2: b.h - 70 },
      colorStops: [
        { offset: 0, color: from },
        { offset: 1, color: to },
      ],
    }),
    stroke: "#ffffff",
    strokeWidth: 70,
    selectable: false,
    evented: false,
  });
}

function marqueeObjects(): Rect[] {
  return (canvas?.getObjects() ?? []).filter((o): o is Rect => marquees.has(o));
}

function syncState() {
  boxCount.value = marqueeObjects().length;
  hasSelection.value = (canvas?.getActiveObjects() ?? []).some((o) => marquees.has(o));
}

function rebuild() {
  if (!canvas) return;
  canvas.discardActiveObject();
  canvas.remove(...canvas.getObjects());
  canvas.add(
    new Rect({
      left: 0,
      top: 0,
      width: SCAN_WIDTH,
      height: SCAN_HEIGHT,
      fill: "#fbfbf9",
      selectable: false,
      evented: false,
    }),
  );
  props.scan.boxes.forEach((b, i) => canvas!.add(makePhoto(b, i)));
  props.scan.boxes.forEach((b) => canvas!.add(makeMarquee(b)));
  syncState();
  fit();
}

function fit() {
  if (!canvas) return;
  const width = canvas.getWidth();
  const height = canvas.getHeight();
  const zoom = Math.min(width / SCAN_WIDTH, height / SCAN_HEIGHT) * 0.95;
  canvas.setViewportTransform([zoom, 0, 0, zoom, (width - SCAN_WIDTH * zoom) / 2, (height - SCAN_HEIGHT * zoom) / 2]);
  zoomPercent.value = Math.round(zoom * 100);
  canvas.requestRenderAll();
}

function addBox() {
  if (!canvas) return;
  const rect = makeMarquee({ cx: SCAN_WIDTH / 2, cy: SCAN_HEIGHT / 2, w: 900, h: 650, angle: 0 });
  canvas.add(rect);
  canvas.setActiveObject(rect);
  syncState();
}

function selectedMarquees(): Rect[] {
  return (canvas?.getActiveObjects() ?? []).filter((o): o is Rect => marquees.has(o));
}

function deleteSelected() {
  if (!canvas) return;
  const targets = selectedMarquees();
  if (!targets.length) return;
  canvas.discardActiveObject();
  canvas.remove(...targets);
  syncState();
}

/** Split the selected box in half across its longer side. */
function splitSelected() {
  if (!canvas) return;
  const [target] = selectedMarquees();
  if (!target) return;
  const width = target.width * target.scaleX;
  const height = target.height * target.scaleY;
  const theta = (target.angle * Math.PI) / 180;
  const alongWidth = width >= height;
  const axis = alongWidth ? { x: Math.cos(theta), y: Math.sin(theta) } : { x: -Math.sin(theta), y: Math.cos(theta) };
  const half = (alongWidth ? width : height) / 2;
  const halves = [-1, 1].map((sign) =>
    makeMarquee({
      cx: target.left + (axis.x * half * sign) / 2,
      cy: target.top + (axis.y * half * sign) / 2,
      w: alongWidth ? half : width,
      h: alongWidth ? height : half,
      angle: target.angle,
    }),
  );
  canvas.discardActiveObject();
  canvas.remove(target);
  canvas.add(...halves);
  canvas.setActiveObject(halves[0]);
  syncState();
}

function straighten() {
  if (!canvas) return;
  for (const target of selectedMarquees()) {
    target.rotate(Math.round(target.angle / 90) * 90);
    target.setCoords();
  }
  canvas.requestRenderAll();
}

function redetect() {
  rebuild();
  emit("notice", "Reset to the (mock) detected boxes. Real re-detection arrives with the engine.");
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
    uniformScaling: false,
  });

  canvas.on("selection:created", syncState);
  canvas.on("selection:updated", syncState);
  canvas.on("selection:cleared", syncState);

  // Hold Shift while rotating to snap to 0/90/180/270.
  canvas.on("object:rotating", (opt) => {
    if ((opt.e as MouseEvent).shiftKey) opt.target.set("angle", Math.round(opt.target.angle / 90) * 90);
  });

  canvas.on("mouse:wheel", (opt) => {
    const event = opt.e as WheelEvent;
    const zoom = Math.min(4, Math.max(0.03, canvas!.getZoom() * 0.999 ** event.deltaY));
    canvas!.zoomToPoint(new Point(event.offsetX, event.offsetY), zoom);
    zoomPercent.value = Math.round(zoom * 100);
    event.preventDefault();
    event.stopPropagation();
  });

  // Drag empty space (or hold Alt) to pan.
  let panning = false;
  canvas.on("mouse:down", (opt) => {
    const event = opt.e as MouseEvent;
    if (!opt.target || event.altKey) {
      panning = true;
      canvas!.setCursor("grabbing");
    }
  });
  canvas.on("mouse:move", (opt) => {
    if (!panning) return;
    const event = opt.e as MouseEvent;
    canvas!.relativePan(new Point(event.movementX, event.movementY));
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
  rebuild();
});

onBeforeUnmount(() => {
  window.removeEventListener("keydown", onKey);
  observer?.disconnect();
  void canvas?.dispose();
});

watch(() => props.scan.id, rebuild);
</script>

<template>
  <section class="review">
    <header class="toolbar">
      <div class="group">
        <button class="btn" title="Add a box" @click="addBox">+ Box</button>
        <button class="btn" :disabled="!hasSelection" title="Split the selected box in half" @click="splitSelected">
          Split
        </button>
        <button class="btn danger" :disabled="!hasSelection" title="Delete the selected box" @click="deleteSelected">
          Delete <kbd>Del</kbd>
        </button>
        <button class="btn" :disabled="!hasSelection" title="Snap the selected box to the nearest 90°" @click="straighten">
          Straighten
        </button>
        <button class="btn" title="Re-run detection on this scan" @click="redetect">Re-detect</button>
      </div>
      <div class="grow"></div>
      <div class="group">
        <button class="btn ghost" title="Fit scan to window" @click="fit">Fit · {{ zoomPercent }}%</button>
        <button class="btn primary" @click="emit('approve')">Approve &amp; next <kbd>Enter</kbd></button>
      </div>
    </header>

    <div ref="host" class="stage"><canvas ref="el"></canvas></div>

    <footer class="infobar">
      <strong class="name">{{ scan.name }}</strong>
      <span class="chip" :class="{ ok: scan.status === 'approved', warn: scan.status === 'flagged' }">
        {{ scan.status }}
      </span>
      <span class="muted">{{ boxCount }} {{ boxCount === 1 ? "item" : "items" }}</span>
      <span v-for="flag in scan.flags" :key="flag" class="chip warn">{{ flagLabels[flag] }}</span>
      <span class="grow"></span>
      <span class="muted hint">Scroll to zoom · drag background to pan · Shift while rotating snaps to 90° · ←/→ prev/next</span>
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
