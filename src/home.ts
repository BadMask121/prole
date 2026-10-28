import { getCurrentWindow, LogicalSize } from "@tauri-apps/api/window";
import { listen } from "@tauri-apps/api/event";
import { call, logError, reportUncaught } from "./lib/ipc";
import { acceleratorFromEvent, shortcutKeys } from "./lib/shortcut";
import { toast } from "./lib/toast";

reportUncaught("Prole");

interface AppState {
  settings: { floating_enabled: boolean; launch_at_login: boolean; hotkey: string };
  screen_recording: boolean;
  hotkey_error: string | null;
  floating_visible: boolean;
  version: string;
  log_path: string;
}

const WIDTH = 440;
const win = getCurrentWindow();
const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;

const shell = $("shell");
const ctaKeys = $("ctaKeys");
const perm = $("perm");
const permTitle = $("permTitle");
const permHint = $("permHint");
const permSteps = $("permSteps");
const floating = $<HTMLInputElement>("floating_enabled");
const launch = $<HTMLInputElement>("launch_at_login");
const hotkeyBtn = $<HTMLButtonElement>("hotkey");
const hotkeyHint = $("hotkeyHint");
const HOTKEY_HINT = "Works from any app";

let state: AppState | null = null;
let recording = false;

function keycaps(target: HTMLElement, accelerator: string, empty: string) {
  const keys = shortcutKeys(accelerator);
  target.replaceChildren(
    ...(keys.length
      ? keys.map((k) => Object.assign(document.createElement("span"), { className: "kbd", textContent: k }))
      : [document.createTextNode(empty)]),
  );
}

function render(s: AppState) {
  state = s;
  keycaps(ctaKeys, s.settings.hotkey, "");
  floating.checked = s.floating_visible;
  launch.checked = s.settings.launch_at_login;
  if (!recording) keycaps(hotkeyBtn, s.settings.hotkey, "None");
  hotkeyBtn.classList.toggle("is-error", Boolean(s.hotkey_error));
  hotkeyHint.textContent = s.hotkey_error ?? HOTKEY_HINT;
  hotkeyHint.classList.toggle("is-error", Boolean(s.hotkey_error));

  perm.dataset.state = s.screen_recording ? "ok" : "missing";
  permTitle.textContent = s.screen_recording ? "Screen Recording allowed" : "Screen Recording is off";
  permHint.textContent = s.screen_recording
    ? "Prole can capture your screen."
    : "macOS blocks captures until you allow Prole.";
  permSteps.hidden = s.screen_recording;
  $("version").textContent = `Prole ${s.version} · also in your menu bar`;
  fitWindow();
}

/** Size the window to its content, so the permission steps never scroll. */
function fitWindow() {
  const height = Math.ceil(shell.getBoundingClientRect().height);
  if (Math.abs(window.innerHeight - height) > 1) {
    win.setSize(new LogicalSize(WIDTH, height)).catch((e) => logError("fit home window", e));
  }
}

async function refresh() {
  try {
    render(await call<AppState>("get_state"));
  } catch (e) {
    logError("load home state", e);
    toast({ kind: "error", title: "Couldn't load Prole's settings", message: String(e) });
  }
}

/** Run an action; on failure show why in a toast and resync the UI. */
async function attempt(title: string, run: () => Promise<unknown>) {
  try {
    await run();
  } catch (e) {
    logError(title, e);
    toast({ kind: "error", title, message: (e as Error).message });
  } finally {
    await refresh();
  }
}

$("capture").addEventListener("click", () => {
  // Rust hides this window during the snip and reports its own failures.
  call("start_capture").catch((e) => logError("capture from home", e));
});

floating.addEventListener("change", () =>
  attempt("Couldn't change the floating button", () =>
    call("set_setting", { key: "floating_enabled", value: floating.checked }),
  ),
);

launch.addEventListener("change", () =>
  attempt("Couldn't change Launch at Login", () =>
    call("set_setting", { key: "launch_at_login", value: launch.checked }),
  ),
);

$("openSettings").addEventListener("click", () =>
  attempt("Couldn't open System Settings", () => call("open_screen_recording_settings")),
);
$("restart").addEventListener("click", () => attempt("Couldn't restart Prole", () => call("restart")));
$("openLog").addEventListener("click", () => attempt("Couldn't open the log", () => call("open_log")));
$("quit").addEventListener("click", () => void call("quit"));

// ── Shortcut recorder ─────────────────────────────────────────────
// Click, then press the new combination. Esc cancels, ⌫ clears it.
function startRecording() {
  if (recording) return;
  recording = true;
  hotkeyBtn.classList.add("is-recording");
  hotkeyBtn.textContent = "Press keys";
  hotkeyHint.textContent = "Esc to cancel · ⌫ to turn off";
  hotkeyHint.classList.remove("is-error");
  call("set_hotkey_suspended", { suspended: true }).catch((e) => logError("suspend hotkey", e));
}

async function stopRecording(next?: string) {
  if (!recording) return;
  recording = false;
  hotkeyBtn.classList.remove("is-recording");
  if (next !== undefined && next !== state?.settings.hotkey) {
    await attempt("That shortcut can't be used", () => call("set_setting", { key: "hotkey", value: next }));
  } else {
    await call("set_hotkey_suspended", { suspended: false }).catch((e) => logError("resume hotkey", e));
    await refresh();
  }
}

hotkeyBtn.addEventListener("click", startRecording);
hotkeyBtn.addEventListener("blur", () => void stopRecording());
hotkeyBtn.addEventListener("keydown", (e) => {
  if (!recording) return;
  e.preventDefault();
  e.stopPropagation();
  if (e.key === "Escape") return void stopRecording();
  if (e.key === "Backspace" || e.key === "Delete") return void stopRecording("");
  const accel = acceleratorFromEvent(e);
  if (accel) void stopRecording(accel);
  else hotkeyBtn.replaceChildren(...shortcutKeys(heldModifiers(e)).map(kbd));
});

function heldModifiers(e: KeyboardEvent) {
  return [e.ctrlKey && "Ctrl", e.altKey && "Alt", e.shiftKey && "Shift", e.metaKey && "Cmd"]
    .filter(Boolean)
    .join("+");
}

function kbd(text: string) {
  return Object.assign(document.createElement("span"), { className: "kbd", textContent: text });
}

// Esc closes the window (it's one click away from the menu bar or pill).
window.addEventListener("keydown", (e) => {
  if (e.key === "Escape" && !recording) void win.close();
});

// ── Lifecycle ─────────────────────────────────────────────────────
listen("home-shown", () => void refresh());
listen("permission-needed", async () => {
  await refresh();
  perm.classList.remove("nudge");
  void perm.offsetWidth; // restart the animation
  perm.classList.add("nudge");
});
win.onFocusChanged(({ payload: focused }) => {
  if (focused) void refresh(); // permission may have changed in System Settings
});

// The intro stagger plays once; dropping the class afterwards frees
// `animation` for the permission card's nudge.
window.setTimeout(() => document.body.classList.remove("intro"), 700);

void refresh();
