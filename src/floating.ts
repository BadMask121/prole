import { getCurrentWindow } from "@tauri-apps/api/window";
import { listen } from "@tauri-apps/api/event";
import { call, logError, reportUncaught } from "./lib/ipc";
import { shortcutKeys } from "./lib/shortcut";

reportUncaught("the floating button");

const win = getCurrentWindow();
const pill = document.getElementById("pill")!;
const snap = document.getElementById("snap")!;
const reduceMotion = window.matchMedia("(prefers-reduced-motion: reduce)");

// Drag by pressing anywhere on the pill except the capture button.
// `data-tauri-drag-region` is unreliable on borderless windows, so we call
// startDragging() explicitly. The new position is saved on the Rust side.
pill.addEventListener("mousedown", (e) => {
  if (e.button !== 0 || e.detail > 1) return;
  if ((e.target as HTMLElement).closest("#snap")) return;
  win.startDragging().catch((err) => logError("drag floating button", err));
});

document.getElementById("grip")!.addEventListener("dblclick", () => {
  call("open_home").catch((err) => logError("open home from pill", err));
});

// start_capture reports its own failures (toast / home window), so the
// rejection only needs logging here.
snap.addEventListener("click", () => {
  call("start_capture").catch((err) => logError("capture from pill", err));
});

/** Pop in: scale up from 0.9 with an ease-out. Rare (launch or re-show), so
 * it can be felt; reduced motion keeps only the fade. */
function enter() {
  const from = reduceMotion.matches ? "none" : "scale(0.9) translateY(4px)";
  pill.animate([{ opacity: 0, transform: from }, { opacity: 1, transform: "none" }], {
    duration: 320,
    easing: "cubic-bezier(0.23, 1, 0.32, 1)",
  });
}

async function refreshTooltip() {
  try {
    const state = await call<{ capture_shortcut: string }>("get_state");
    const keys = shortcutKeys(state.capture_shortcut).join("");
    snap.title = keys ? `Capture a region (${keys})` : "Capture a region";
  } catch (err) {
    logError("floating tooltip", err);
  }
}

listen("floating-shown", () => {
  enter();
  void refreshTooltip();
});

enter();
pill.classList.add("ripple");
pill.addEventListener("animationend", () => pill.classList.remove("ripple"), { once: true });
void refreshTooltip();
