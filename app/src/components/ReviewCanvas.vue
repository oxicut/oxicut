<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { Canvas, FabricImage, Point, Rect, util } from "fabric";
import {
  boxCorners,
  cornerPlaces,
  handlePoint,
  sourceSquare,
  type LoupeBox,
  type LoupePlace,
  type Point2,
} from "../loupe";
import ViewSwitch from "./ViewSwitch.vue";
import { resizeBox, type SizeChange } from "../cropSize";
import { FLAG_LABELS, newBoxId, previewFor, resizeCrop, type BoxItem, type ScanItem } from "../store";

const props = defineProps<{ scan: ScanItem | null; keysEnabled: boolean }>();
const emit = defineEmits<{
  approve: [];
  step: [delta: number];
  edit: [boxes: BoxItem[]];
  redetect: [];
  undo: [];
  redo: [];
}>();

const ACCENT = "#e8703a";
const WARNING = "#e5a03d";

const host = ref<HTMLDivElement>();
const el = ref<HTMLCanvasElement>();
const loading = ref(false);
const loadError = ref("");
const boxCount = ref(0);
const hasSelection = ref(false);
// Whether the selected box can be cut closer or wider (the buttons grey out at the limits).
const canCloser = ref(false);
const canWider = ref(false);
const zoomPercent = ref(100);
// A scan that cannot be found has no canvas to count on, so its saved boxes are the answer.
const shownBoxes = computed(() => (props.scan?.missing ? props.scan.boxes.length : boxCount.value));

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
  const [target] = selectedMarquees();
  const scan = props.scan;
  const box = target && { cx: target.left, cy: target.top, w: target.width * target.scaleX, h: target.height * target.scaleY, angle: target.angle };
  canCloser.value = !!(box && scan && resizeBox(box, "closer", scan));
  canWider.value = !!(box && scan && resizeBox(box, "wider", scan));
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

// ------------------------------------------------------------------- loupe

/** Edge of the round magnifier that follows a resize handle, in CSS pixels. */
const LOUPE_SIZE = 168;
/** Edge of each of the four loupes shown while moving or rotating (smaller, so four fit a narrow stage). */
const CORNER_LOUPE_SIZE = 136;
/** How much larger than the main view the loupe shows the scan (never below 1 screen pixel per scan pixel). */
const LOUPE_MAGNIFY = 4;

/** Four loupe surfaces: resizing uses the first, moving and rotating use all of them. */
const loupeEls = ref<HTMLCanvasElement[]>([]);
/** Which stage corner each loupe sits in, or null while it is hidden. */
const loupePlaces = ref<(LoupePlace | null)[]>([null, null, null, null]);
const loupeSize = ref(LOUPE_SIZE);
let previewImage: FabricImage | undefined;

function hideLoupe() {
  loupePlaces.value = [null, null, null, null];
}

const targetBox = (target: Rect): LoupeBox => ({
  cx: target.left,
  cy: target.top,
  w: target.width * target.scaleX,
  h: target.height * target.scaleY,
  angle: target.angle,
});

/**
 * Magnify the scan around `centre` onto `surface` (`size` CSS pixels wide), draw the box over it and
 * put a crosshair on the centre. Returns false if there is nothing to draw on.
 */
function paintLoupe(surface: HTMLCanvasElement, size: number, centre: Point2, box: LoupeBox, flagged: boolean): boolean {
  const image = previewImage;
  const scan = props.scan;
  const context = surface.getContext("2d");
  if (!canvas || !image || !scan || !host.value || !context) return false;

  const ratio = window.devicePixelRatio || 1;
  const pixels = Math.round(size * ratio);
  if (surface.width !== pixels) {
    surface.width = pixels;
    surface.height = pixels;
  }
  const magnify = Math.max(1, canvas.getZoom() * LOUPE_MAGNIFY);
  const square = sourceSquare(centre, size, magnify);
  const toLoupe = (p: { x: number; y: number }) => ({ x: (p.x - square.x) * magnify, y: (p.y - square.y) * magnify });

  context.setTransform(ratio, 0, 0, ratio, 0, 0);
  context.clearRect(0, 0, size, size);
  context.save();
  context.beginPath();
  context.arc(size / 2, size / 2, size / 2, 0, Math.PI * 2);
  context.clip();
  context.fillStyle = getComputedStyle(host.value).getPropertyValue("--stage").trim() || "#888";
  context.fillRect(0, 0, size, size);

  // The part of the square that lies on the scan; the rest stays stage-coloured.
  const k = image.width / scan.width;
  const x0 = Math.max(0, square.x);
  const y0 = Math.max(0, square.y);
  const x1 = Math.min(scan.width, square.x + square.side);
  const y1 = Math.min(scan.height, square.y + square.side);
  if (x1 > x0 && y1 > y0) {
    context.imageSmoothingEnabled = true;
    context.imageSmoothingQuality = "high";
    context.drawImage(
      image.getElement() as CanvasImageSource,
      x0 * k,
      y0 * k,
      (x1 - x0) * k,
      (y1 - y0) * k,
      (x0 - square.x) * magnify,
      (y0 - square.y) * magnify,
      (x1 - x0) * magnify,
      (y1 - y0) * magnify,
    );
  }

  const outline = boxCorners(box).map(toLoupe);
  context.beginPath();
  outline.forEach((p, i) => (i ? context.lineTo(p.x, p.y) : context.moveTo(p.x, p.y)));
  context.closePath();
  context.fillStyle = "rgba(232, 112, 58, 0.12)";
  context.fill();
  context.lineWidth = 2;
  context.strokeStyle = flagged ? WARNING : ACCENT;
  context.stroke();

  // Crosshair with a gap, so the pixel under the point stays visible.
  const mid = size / 2;
  const gap = 5;
  const reach = 16;
  for (const [width, colour] of [
    [3, "rgba(0, 0, 0, 0.6)"],
    [1, "#ffffff"],
  ] as const) {
    context.lineWidth = width;
    context.strokeStyle = colour;
    context.beginPath();
    context.moveTo(mid - reach, mid);
    context.lineTo(mid - gap, mid);
    context.moveTo(mid + gap, mid);
    context.lineTo(mid + reach, mid);
    context.moveTo(mid, mid - reach);
    context.lineTo(mid, mid - gap);
    context.moveTo(mid, mid + gap);
    context.lineTo(mid, mid + reach);
    context.stroke();
  }
  context.restore();
  return true;
}

const isFlagged = (target: Rect) => (props.scan?.flags ?? []).some((f) => f.boxId === ids.get(target));

/**
 * Magnify the scan around the resize handle being dragged. The loupe sits in the stage corner away
 * from the pointer, so it never covers what is being placed.
 */
function drawLoupe(target: Rect, corner: string, event?: Event) {
  const surface = loupeEls.value[0];
  const box = targetBox(target);
  const centre = handlePoint(box, corner);
  if (!surface || !host.value || !centre) return hideLoupe();
  loupeSize.value = LOUPE_SIZE;
  if (!paintLoupe(surface, LOUPE_SIZE, centre, box, isFlagged(target))) return hideLoupe();

  let place = loupePlaces.value[0] ?? "top-right";
  const pointer = event as MouseEvent | undefined;
  if (pointer && typeof pointer.clientX === "number") {
    const bounds = host.value.getBoundingClientRect();
    const vertical = pointer.clientY - bounds.top < LOUPE_SIZE + 40 ? "bottom" : "top";
    const horizontal = pointer.clientX - bounds.left < bounds.width / 2 ? "right" : "left";
    place = `${vertical}-${horizontal}` as LoupePlace;
  }
  loupePlaces.value = [place, null, null, null];
}

/**
 * While a box is moved or turned, magnify each of its four corners at once so any of them can be
 * aligned. Each loupe sits in the stage corner that matches where its box corner is on screen.
 */
function drawCornerLoupes(target: Rect) {
  if (!canvas) return;
  const box = targetBox(target);
  const corners = boxCorners(box);
  const [zoom, , , , x0, y0] = canvas.viewportTransform;
  const places = cornerPlaces(corners.map((c) => ({ x: c.x * zoom + x0, y: c.y * zoom + y0 })));
  loupeSize.value = CORNER_LOUPE_SIZE;
  const flagged = isFlagged(target);
  loupePlaces.value = corners.map((corner, i) => {
    const surface = loupeEls.value[i];
    return surface && paintLoupe(surface, CORNER_LOUPE_SIZE, corner, box, flagged) ? places[i] : null;
  });
}

/** A handle or box is being dragged; Fabric must not be rebuilt under it. */
let transforming = false;

/** Show other boxes (after undo or redo) without reloading the image. Keeps the selection if the box survives. */
function applyBoxes() {
  if (!canvas || !props.scan || !previewImage || transforming) return;
  const selected = selectedMarquees().map((m) => ids.get(m));
  canvas.discardActiveObject();
  canvas.remove(...marquees());
  const added = props.scan.boxes.map(makeMarquee);
  canvas.add(...added);
  const keep = added.find((m) => selected.includes(ids.get(m)));
  if (keep) canvas.setActiveObject(keep);
  restyle();
  syncState();
}

/** True while the view is the "fit to window" one; a manual zoom or pan clears it. */
let fitted = true;

function fit() {
  if (!canvas || !props.scan || props.scan.width === 0) return;
  fitted = true;
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
  previewImage = undefined;
  hideLoupe();
  canvas.remove(...canvas.getObjects());
  syncState();
  loadError.value = "";
  const scan = props.scan;
  if (!scan || scan.width === 0 || scan.status === "error") {
    loading.value = false;
    return;
  }
  if (scan.missing) {
    loading.value = false;
    loadError.value =
      "This scan could not be found: it was moved or deleted. Put the file back where it was and open the project again. Its boxes are kept.";
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
    previewImage = image;
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

/** Move every edge of the selected box a little in or out (the same step as in the larger view of a crop). */
function resizeSelected(change: SizeChange) {
  const [target] = selectedMarquees();
  const id = target && ids.get(target);
  if (id !== undefined && props.scan) resizeCrop(props.scan, id, change);
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
  if (!props.keysEnabled || event.altKey) return;
  const target = event.target as HTMLElement | null;
  if (target && ["INPUT", "SELECT", "TEXTAREA"].includes(target.tagName)) return;
  if (event.ctrlKey || event.metaKey) {
    const key = event.key.toLowerCase();
    const wantsUndo = key === "z" && !event.shiftKey;
    const wantsRedo = key === "y" || (key === "z" && event.shiftKey);
    if (!wantsUndo && !wantsRedo) return;
    event.preventDefault();
    if (transforming) return;
    if (wantsUndo) emit("undo");
    else emit("redo");
    return;
  }
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
  canvas.on("selection:cleared", () => {
    hideLoupe();
    syncState();
  });
  canvas.on("before:transform", () => (transforming = true));
  canvas.on("object:modified", () => {
    hideLoupe();
    transforming = false;
    commit();
  });

  // A magnifier follows any resize handle while it is dragged (not the rotation handle).
  canvas.on("object:scaling", (opt) => {
    const corner = opt.transform?.corner;
    if (corner && ids.has(opt.target)) drawLoupe(opt.target as Rect, corner, opt.e);
  });

  // Moving or turning a box magnifies all four of its corners, so any of them can be aligned.
  canvas.on("object:moving", (opt) => {
    if (ids.has(opt.target)) drawCornerLoupes(opt.target as Rect);
  });

  // Hold Shift while rotating to snap to 0/90/180/270.
  canvas.on("object:rotating", (opt) => {
    if ((opt.e as MouseEvent).shiftKey) opt.target.set("angle", Math.round(opt.target.angle / 90) * 90);
    if (ids.has(opt.target)) drawCornerLoupes(opt.target as Rect);
  });

  canvas.on("mouse:wheel", (opt) => {
    const event = opt.e as WheelEvent;
    const zoom = Math.min(4, Math.max(0.03, canvas!.getZoom() * 0.999 ** event.deltaY));
    canvas!.zoomToPoint(new Point(event.offsetX, event.offsetY), zoom);
    fitted = false;
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
    fitted = false;
    canvas!.relativePan(new Point((opt.e as MouseEvent).movementX, (opt.e as MouseEvent).movementY));
  });
  canvas.on("mouse:up", () => {
    panning = false;
    hideLoupe();
    transforming = false;
    canvas!.setViewportTransform(canvas!.viewportTransform);
  });

  observer = new ResizeObserver(() => {
    if (!canvas || !host.value) return;
    // Hidden (the Crops view is showing): there is no size to adapt to, and the view must stay as it was.
    if (host.value.clientWidth === 0 || host.value.clientHeight === 0) return;
    const [wasWidth, wasHeight] = [canvas.getWidth(), canvas.getHeight()];
    canvas.setDimensions({ width: host.value.clientWidth, height: host.value.clientHeight });
    if (fitted) {
      fit();
      return;
    }
    // A zoomed-in view stays where it was, centred in the new size (a side panel opening or closing
    // must not throw the user's view away).
    const view = [...canvas.viewportTransform] as typeof canvas.viewportTransform;
    view[4] += (canvas.getWidth() - wasWidth) / 2;
    view[5] += (canvas.getHeight() - wasHeight) / 2;
    canvas.setViewportTransform(view);
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
// Undo or redo put other boxes back.
watch(() => props.scan?.epoch, applyBoxes);
// Re-colour boxes as warnings come and go.
watch(() => props.scan?.flags, restyle, { deep: true });
</script>

<template>
  <section class="review">
    <header class="toolbar">
      <div class="group">
        <ViewSwitch />
        <button class="btn" :disabled="!scan?.history.canUndo" title="Undo (Ctrl+Z)" @click="emit('undo')">
          Undo
        </button>
        <button class="btn" :disabled="!scan?.history.canRedo" title="Redo (Ctrl+Y or Ctrl+Shift+Z)" @click="emit('redo')">
          Redo
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

    <div class="work">
      <nav class="rail" aria-label="Box tools">
        <button class="tool" :disabled="!ready" title="Add a box" @click="addBox">
          <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M12 5v14M5 12h14" /></svg>
          Box
        </button>
        <span class="sep" aria-hidden="true"></span>
        <button class="tool" :disabled="!hasSelection" title="Split the selected box in half" @click="splitSelected">
          <svg viewBox="0 0 24 24" aria-hidden="true"><rect x="4" y="5" width="16" height="14" rx="1.5" /><path d="M12 5v14" /></svg>
          Split
        </button>
        <button class="tool danger" :disabled="!hasSelection" title="Delete the selected box (Del)" @click="deleteSelected">
          <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M5 7h14M10 7V5h4v2M7 7l1 12h8l1-12M10 11v5M14 11v5" /></svg>
          Delete
        </button>
        <button class="tool" :disabled="!hasSelection" title="Snap the selected box to the nearest 90°" @click="straighten">
          <svg viewBox="0 0 24 24" aria-hidden="true"><rect x="6" y="8" width="12" height="9" rx="1" /><path d="M4 5.5A9 9 0 0 1 12 3M12 3l-2.5-.5M12 3l-.5 2.5" /></svg>
          Straighten
        </button>
        <span class="sep" aria-hidden="true"></span>
        <button class="tool" :disabled="!canCloser" title="Cut the selected box a little closer: every edge moves in, for example to lose a white border" @click="resizeSelected('closer')">
          <svg viewBox="0 0 24 24" aria-hidden="true"><rect x="8" y="8" width="8" height="8" /><path d="M12 2v4M10 4.5l2 2 2-2M12 22v-4M10 19.5l2-2 2 2M2 12h4M4.5 10l2 2-2 2M22 12h-4M19.5 10l-2 2 2 2" /></svg>
          Closer
        </button>
        <button class="tool" :disabled="!canWider" title="Cut the selected box a little wider: every edge moves out" @click="resizeSelected('wider')">
          <svg viewBox="0 0 24 24" aria-hidden="true"><rect x="7" y="7" width="10" height="10" /><path d="M12 6V2M10 3.5l2-2 2 2M12 18v4M10 20.5l2 2 2-2M6 12H2M3.5 10l-2 2 2 2M18 12h4M20.5 10l2 2-2 2" /></svg>
          Wider
        </button>
      </nav>

      <div ref="host" class="stage">
        <canvas ref="el"></canvas>
        <canvas
          v-for="(place, i) in loupePlaces"
          :key="i"
          v-show="place"
          :ref="(surface) => (loupeEls[i] = surface as HTMLCanvasElement)"
          class="loupe"
          :class="place"
          :style="{ width: `${loupeSize}px`, height: `${loupeSize}px` }"
          aria-hidden="true"
        ></canvas>
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
    </div>

    <footer class="infobar">
      <template v-if="scan">
        <strong class="name">{{ scan.name }}</strong>
        <span v-if="ready" class="muted">{{ scan.width }} × {{ scan.height }} px</span>
        <span class="chip" :class="{ ok: scan.status === 'approved', warn: scan.status === 'flagged' || scan.status === 'error' }">
          {{ scan.status }}
        </span>
        <span v-if="ready" class="muted">{{ shownBoxes }} {{ shownBoxes === 1 ? "item" : "items" }}</span>
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
  flex-wrap: wrap;
  align-items: center;
  gap: 0.4rem 1rem;
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
.work {
  display: flex;
  flex: 1;
  min-height: 0;
}
.rail {
  display: flex;
  flex: none;
  flex-direction: column;
  align-items: stretch;
  gap: 0.25rem;
  width: 68px;
  padding: 0.5rem 0.35rem;
  background: var(--surface);
  border-right: 1px solid var(--border);
  overflow-y: auto;
}
.sep {
  height: 1px;
  margin: 0.2rem 0.4rem;
  background: var(--border);
}
.stage {
  position: relative;
  flex: 1;
  min-width: 0;
  min-height: 0;
  background: var(--stage);
  overflow: hidden;
}
.loupe {
  position: absolute;
  z-index: 5;
  pointer-events: none;
  border: 3px solid var(--surface);
  border-radius: 50%;
  background: var(--stage);
  box-shadow: 0 4px 18px rgba(0, 0, 0, 0.45);
}
.loupe.top-left {
  top: 12px;
  left: 12px;
}
.loupe.top-right {
  top: 12px;
  right: 12px;
}
.loupe.bottom-left {
  bottom: 12px;
  left: 12px;
}
.loupe.bottom-right {
  bottom: 12px;
  right: 12px;
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
