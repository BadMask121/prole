import { getCurrentWindow } from "@tauri-apps/api/window";
import { listen } from "@tauri-apps/api/event";
import { call, logError, reportUncaught } from "./lib/ipc";
import { toast } from "./lib/toast";

reportUncaught("the editor");

type Tool = "rect" | "arrow" | "pen" | "text";
interface Point {
  x: number;
  y: number;
}
interface Shape {
  tool: Tool;
  color: string;
  width: number; // stroke, canvas px
  x: number;
  y: number;
  x2: number;
  y2: number;
  points?: Point[];
  text?: string;
  size?: number; // canvas px for text annotations
}

const TOOLS: Tool[] = ["rect", "arrow", "pen", "text"];
const TOOL_KEYS: Record<string, Tool> = { r: "rect", a: "arrow", p: "pen", t: "text" };
const COLORS = [
  { value: "#ff3b4f", label: "Red" },
  { value: "#ff9f0a", label: "Orange" },
  { value: "#ffd60a", label: "Yellow" },
  { value: "#30d158", label: "Green" },
  { value: "#0a84ff", label: "Blue" },
  { value: "#ffffff", label: "White" },
];
// Captures are at screen resolution, so stroke and text scale with it.
const DPR = window.devicePixelRatio || 1;
const STROKE = 2.5 * DPR;
const MIN_TEXT_SIZE = 8;
const MAX_TEXT_SIZE = 240;
const SEGMENT_WIDTH = 32;
const EASE_OUT = "cubic-bezier(0.23, 1, 0.32, 1)";

const win = getCurrentWindow();
const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;
const canvas = $<HTMLCanvasElement>("canvas");
const ctx = canvas.getContext("2d")!;
const stage = $("stage");
const stageMsg = $("stageMsg");
const dims = $("dims");
const copyBtn = $<HTMLButtonElement>("copy");
const undoBtn = $<HTMLButtonElement>("undo");
const redoBtn = $<HTMLButtonElement>("redo");
const clearBtn = $<HTMLButtonElement>("clear");
const segIndicator = $("segIndicator");
const textProps = $("textProps");
const sizeInput = $<HTMLInputElement>("sizeInput");
const reduceMotion = window.matchMedia("(prefers-reduced-motion: reduce)");

let baseImage: HTMLImageElement | null = null;
let shapes: Shape[] = [];
let undoStack: Shape[][] = [];
let redoStack: Shape[][] = [];
let tool: Tool = "rect";
let color = COLORS[0].value;
let textSize = Math.round(26 * DPR);
let drawing: Shape | null = null;
let textInput: HTMLInputElement | null = null;
let copying = false;
let frame = 0;

// ── History ───────────────────────────────────────────────────────
// Snapshots, so Clear is undoable like any other edit.
function commit(next: Shape[]) {
  undoStack.push(shapes);
  redoStack = [];
  shapes = next;
  redraw();
  syncHistory();
}

function undo() {
  const prev = undoStack.pop();
  if (!prev) return;
  redoStack.push(shapes);
  shapes = prev;
  redraw();
  syncHistory();
}

function redo() {
  const next = redoStack.pop();
  if (!next) return;
  undoStack.push(shapes);
  shapes = next;
  redraw();
  syncHistory();
}

function syncHistory() {
  undoBtn.disabled = undoStack.length === 0;
  redoBtn.disabled = redoStack.length === 0;
  clearBtn.disabled = shapes.length === 0;
}

// ── Loading ───────────────────────────────────────────────────────
function showStage(state: "loading" | "ready" | "error", message = "") {
  stage.dataset.state = state;
  if (state === "loading") {
    stageMsg.replaceChildren(Object.assign(document.createElement("div"), { className: "spinner" }));
  } else if (state === "error") {
    const retry = Object.assign(document.createElement("button"), {
      className: "btn btn-primary",
      textContent: "Capture again",
    });
    retry.addEventListener("click", () => call("start_capture").catch((e) => logError("retry capture", e)));
    stageMsg.replaceChildren(
      Object.assign(document.createElement("h2"), { textContent: "Couldn't load the capture" }),
      Object.assign(document.createElement("p"), { textContent: message }),
      retry,
    );
  }
}

async function loadCapture() {
  showStage("loading");
  try {
    const b64 = await call<string>("get_capture");
    const img = new Image();
    img.src = `data:image/png;base64,${b64}`;
    await img.decode().catch(() => {
      throw new Error("The capture isn't a readable image.");
    });
    baseImage = img;
    shapes = [];
    undoStack = [];
    redoStack = [];
    canvas.width = img.naturalWidth;
    canvas.height = img.naturalHeight;
    dims.textContent = `${img.naturalWidth} × ${img.naturalHeight}`;
    fitCanvas();
    redraw();
    syncHistory();
    showStage("ready");
    canvas.animate(
      [
        { opacity: 0, transform: reduceMotion.matches ? "none" : "scale(0.985)" },
        { opacity: 1, transform: "none" },
      ],
      { duration: 260, easing: EASE_OUT },
    );
  } catch (e) {
    logError("load capture", e);
    showStage("error", (e as Error).message);
  }
}

/** Fit the capture in the stage, never above its real size on this screen. */
function fitCanvas() {
  if (!baseImage) return;
  const r = stage.getBoundingClientRect();
  const pad = 32;
  const scale = Math.min(
    (r.width - pad * 2) / canvas.width,
    (r.height - pad * 2) / canvas.height,
    1 / DPR,
  );
  canvas.style.width = `${Math.max(1, canvas.width * scale)}px`;
  canvas.style.height = `${Math.max(1, canvas.height * scale)}px`;
}
new ResizeObserver(fitCanvas).observe(stage);

// ── Drawing ───────────────────────────────────────────────────────
function scheduleRedraw() {
  if (!frame) {
    frame = requestAnimationFrame(() => {
      frame = 0;
      redraw();
    });
  }
}

function redraw() {
  if (!baseImage) return;
  ctx.clearRect(0, 0, canvas.width, canvas.height);
  ctx.drawImage(baseImage, 0, 0);
  for (const s of shapes) drawShape(s);
  if (drawing) drawShape(drawing);
}

function isLight(hex: string) {
  const n = parseInt(hex.slice(1), 16);
  const [r, g, b] = [(n >> 16) & 255, (n >> 8) & 255, n & 255];
  return (0.299 * r + 0.587 * g + 0.114 * b) / 255 > 0.6;
}

function drawShape(s: Shape) {
  ctx.save();
  ctx.strokeStyle = s.color;
  ctx.fillStyle = s.color;
  ctx.lineWidth = s.width;
  ctx.lineCap = "round";
  ctx.lineJoin = "round";
  if (s.tool === "rect") {
    const x = Math.min(s.x, s.x2);
    const y = Math.min(s.y, s.y2);
    ctx.beginPath();
    ctx.roundRect(x, y, Math.abs(s.x2 - s.x), Math.abs(s.y2 - s.y), s.width);
    ctx.stroke();
  } else if (s.tool === "arrow") {
    const dx = s.x2 - s.x;
    const dy = s.y2 - s.y;
    const len = Math.hypot(dx, dy);
    if (len >= 1) {
      const a = Math.atan2(dy, dx);
      const head = Math.min(s.width * 4.5, len * 0.6);
      // Shaft stops inside the head so the round cap doesn't poke out of the tip.
      ctx.beginPath();
      ctx.moveTo(s.x, s.y);
      ctx.lineTo(s.x2 - Math.cos(a) * head * 0.6, s.y2 - Math.sin(a) * head * 0.6);
      ctx.stroke();
      ctx.beginPath();
      ctx.moveTo(s.x2, s.y2);
      ctx.lineTo(s.x2 - head * Math.cos(a - 0.42), s.y2 - head * Math.sin(a - 0.42));
      ctx.lineTo(s.x2 - head * Math.cos(a + 0.42), s.y2 - head * Math.sin(a + 0.42));
      ctx.closePath();
      ctx.lineWidth = s.width * 0.6;
      ctx.fill();
      ctx.stroke();
    }
  } else if (s.tool === "pen" && s.points?.length) {
    // Curve through midpoints so fast strokes don't look like polylines.
    const p = s.points;
    ctx.beginPath();
    ctx.moveTo(p[0].x, p[0].y);
    for (let i = 1; i < p.length - 1; i++) {
      ctx.quadraticCurveTo(p[i].x, p[i].y, (p[i].x + p[i + 1].x) / 2, (p[i].y + p[i + 1].y) / 2);
    }
    ctx.lineTo(p[p.length - 1].x, p[p.length - 1].y);
    ctx.stroke();
  } else if (s.tool === "text" && s.text) {
    const size = s.size ?? textSize;
    ctx.font = `600 ${size}px -apple-system, BlinkMacSystemFont, sans-serif`;
    ctx.textBaseline = "top";
    // A halo keeps text readable on any background.
    ctx.lineWidth = size * 0.16;
    ctx.strokeStyle = isLight(s.color) ? "rgba(0, 0, 0, 0.8)" : "rgba(255, 255, 255, 0.92)";
    ctx.strokeText(s.text, s.x, s.y);
    ctx.fillText(s.text, s.x, s.y);
  }
  ctx.restore();
}

function toCanvasCoords(e: MouseEvent) {
  const r = canvas.getBoundingClientRect();
  return {
    x: (e.clientX - r.left) * (canvas.width / r.width),
    y: (e.clientY - r.top) * (canvas.height / r.height),
    scale: r.width / canvas.width, // display px per canvas px
  };
}

/** Too small to be intentional (a click, not a drag). */
function isTiny(s: Shape) {
  if (s.tool === "pen") return (s.points?.length ?? 0) < 2;
  return Math.hypot(s.x2 - s.x, s.y2 - s.y) < 4 * DPR;
}

canvas.addEventListener("pointerdown", (e) => {
  if (e.button !== 0 || !baseImage || textInput) return;
  if (tool === "text") {
    e.preventDefault(); // keep focus off the canvas so the new input keeps it
    placeTextInput(e);
    return;
  }
  canvas.setPointerCapture(e.pointerId); // keep drawing if the pointer leaves the canvas
  const { x, y } = toCanvasCoords(e);
  drawing = { tool, color, width: STROKE, x, y, x2: x, y2: y, points: tool === "pen" ? [{ x, y }] : undefined };
});

canvas.addEventListener("pointermove", (e) => {
  if (!drawing) return;
  const { x, y } = toCanvasCoords(e);
  drawing.x2 = x;
  drawing.y2 = y;
  if (drawing.tool === "pen") drawing.points!.push({ x, y });
  scheduleRedraw();
});

function endStroke() {
  if (!drawing) return;
  const done = drawing;
  drawing = null;
  if (isTiny(done)) redraw();
  else commit([...shapes, done]);
}
canvas.addEventListener("pointerup", endStroke);
canvas.addEventListener("pointercancel", endStroke);

// Inline text annotation: an input at the click point. Enter or clicking
// away commits; Escape cancels.
function placeTextInput(e: MouseEvent) {
  const { x, y, scale } = toCanvasCoords(e);
  const input = document.createElement("input");
  input.type = "text";
  input.className = "text-input";
  input.style.left = `${e.clientX}px`;
  input.style.top = `${e.clientY}px`;
  input.style.fontSize = `${textSize * scale}px`;
  input.style.color = color;
  input.spellcheck = false;
  document.body.append(input);
  textInput = input;
  fitTextInput(input);
  input.addEventListener("input", () => fitTextInput(input));
  requestAnimationFrame(() => input.focus());

  let done = false;
  const finish = (keep: boolean) => {
    if (done) return;
    done = true;
    const text = input.value.trim();
    input.remove();
    if (textInput === input) textInput = null;
    if (keep && text) {
      commit([...shapes, { tool: "text", color, width: STROKE, x, y, x2: x, y2: y, text, size: textSize }]);
    }
  };
  input.addEventListener("keydown", (ev) => {
    ev.stopPropagation(); // typing "t" or "a" mustn't switch tools
    if (ev.key === "Enter") {
      ev.preventDefault();
      finish(true);
    } else if (ev.key === "Escape") {
      ev.preventDefault();
      finish(false);
    }
  });
  input.addEventListener("blur", () => finish(true));
}

/** Grow the label input with its text, so what you type is what lands on the image. */
const measureCtx = document.createElement("canvas").getContext("2d")!;
function fitTextInput(input: HTMLInputElement) {
  measureCtx.font = `600 ${input.style.fontSize} -apple-system, BlinkMacSystemFont, sans-serif`;
  const width = measureCtx.measureText(input.value || "M").width;
  input.style.width = `${Math.ceil(width + parseFloat(input.style.fontSize) * 0.6 + 8)}px`;
}

// ── Tools, colours, text size ─────────────────────────────────────
const toolButtons = Array.from(document.querySelectorAll<HTMLButtonElement>("[data-tool]"));

/** Keyboard switches are instant: they happen often and must feel immediate. */
function selectTool(next: Tool, instant = false) {
  tool = next;
  canvas.dataset.tool = next;
  for (const b of toolButtons) b.setAttribute("aria-checked", String(b.dataset.tool === next));
  if (instant) segIndicator.style.transition = "none";
  segIndicator.style.transform = `translateX(${TOOLS.indexOf(next) * SEGMENT_WIDTH}px)`;
  if (instant) {
    void segIndicator.offsetWidth;
    segIndicator.style.transition = "";
  }
  textProps.hidden = next !== "text";
}
toolButtons.forEach((b) => b.addEventListener("click", () => selectTool(b.dataset.tool as Tool)));

const swatches = $("swatches");
function selectColor(next: string) {
  color = next;
  for (const s of swatches.children) s.setAttribute("aria-checked", String((s as HTMLElement).dataset.color === next));
  if (textInput) textInput.style.color = next;
}
for (const c of COLORS) {
  const b = document.createElement("button");
  b.className = "swatch";
  b.dataset.color = c.value;
  b.style.setProperty("--swatch", c.value);
  b.setAttribute("role", "radio");
  b.setAttribute("aria-label", c.label);
  b.title = c.label;
  b.addEventListener("mousedown", (e) => e.preventDefault()); // don't commit an open text input
  b.addEventListener("click", () => selectColor(c.value));
  swatches.append(b);
}

function setTextSize(next: number) {
  if (!Number.isFinite(next)) return;
  textSize = Math.max(MIN_TEXT_SIZE, Math.min(MAX_TEXT_SIZE, Math.round(next)));
  sizeInput.value = String(textSize);
  if (textInput) {
    const scale = canvas.getBoundingClientRect().width / canvas.width;
    textInput.style.fontSize = `${textSize * scale}px`;
    fitTextInput(textInput);
  }
}
sizeInput.addEventListener("input", () => setTextSize(Number(sizeInput.value)));
for (const [id, step] of [["sizeDown", -4], ["sizeUp", 4]] as const) {
  const btn = $(id);
  btn.addEventListener("mousedown", (e) => e.preventDefault()); // keep an open text input focused
  btn.addEventListener("click", () => setTextSize(textSize + step));
}

undoBtn.addEventListener("click", undo);
redoBtn.addEventListener("click", redo);
clearBtn.addEventListener("click", () => {
  if (shapes.length) commit([]);
});

// ── Copy / close ──────────────────────────────────────────────────
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

async function onCopy() {
  if (!baseImage || copying) return;
  textInput?.blur(); // commit an in-progress label first
  copying = true;
  copyBtn.dataset.state = "busy";
  try {
    const b64 = canvas.toDataURL("image/png").split(",")[1];
    await call("copy_image", { pngBase64: b64 });
    copyBtn.dataset.state = "done";
    // Hold "Copied" long enough to read, then get out of the way.
    await sleep(reduceMotion.matches ? 250 : 450);
    await win.close();
  } catch (e) {
    logError("copy", e);
    copyBtn.dataset.state = "idle";
    copying = false;
    toast({ kind: "error", title: "Couldn't copy the image", message: (e as Error).message });
  }
}

let discardArmed = 0;
/** Esc closes, but with markup on the canvas it asks for a second press. */
function requestClose() {
  if (shapes.length && !discardArmed) {
    toast({ kind: "error", title: "Press Esc again to discard your markup", duration: 2000 });
    discardArmed = window.setTimeout(() => (discardArmed = 0), 2000);
    return;
  }
  void win.close();
}

copyBtn.addEventListener("click", onCopy);
$("cancel").addEventListener("click", () => void win.close());

window.addEventListener("keydown", (e) => {
  if (e.target === sizeInput) return;
  const mod = e.metaKey || e.ctrlKey;
  const key = e.key.toLowerCase();
  if (mod && key === "z") {
    e.preventDefault();
    if (e.shiftKey) redo();
    else undo();
  } else if (mod && (key === "c" || key === "enter")) {
    e.preventDefault();
    void onCopy();
  } else if (mod && key === "w") {
    e.preventDefault();
    void win.close();
  } else if (!mod && !e.altKey && TOOL_KEYS[key]) {
    selectTool(TOOL_KEYS[key], true);
  } else if (e.key === "Escape") {
    requestClose();
  }
});

listen("capture-updated", () => {
  copying = false;
  copyBtn.dataset.state = "idle";
  void loadCapture();
});

selectTool("rect", true);
selectColor(color);
setTextSize(textSize);
syncHistory();
void loadCapture();
