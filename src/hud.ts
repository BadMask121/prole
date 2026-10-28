import { listen } from "@tauri-apps/api/event";
import { call, logError } from "./lib/ipc";
import { createToast } from "./lib/toast";

interface Toast {
  kind: "success" | "error";
  title: string;
  message: string;
  action: "open_screen_recording" | "open_log" | "open_home" | null;
}

const ACTIONS: Record<NonNullable<Toast["action"]>, { label: string; cmd: string }> = {
  open_screen_recording: { label: "Open System Settings", cmd: "open_screen_recording_settings" },
  open_log: { label: "View log", cmd: "open_log" },
  open_home: { label: "Open Prole", cmd: "open_home" },
};

const slot = document.getElementById("slot")!;
let current: { close: () => void } | null = null;

function show(t: Toast) {
  // Replacing: the old card fades out while the new one fades in over it.
  current?.close();
  const action = t.action ? ACTIONS[t.action] : undefined;
  const card = createToast({
    kind: t.kind,
    title: t.title,
    message: t.message,
    action: action && {
      label: action.label,
      run: () => void call(action.cmd).catch((e) => logError(`HUD action ${action.cmd}`, e)),
    },
    onClose: () => {
      if (current === card) current = null;
      // Hide the window once the last card has gone, so it never blocks clicks.
      if (!slot.querySelector(".toast")) void call("hide_hud").catch(() => {});
    },
  });
  current = card;
  slot.append(card.el);
}

listen<Toast>("hud-toast", (e) => {
  void call("take_hud_toast").catch(() => {}); // consumed — don't replay on reload
  show(e.payload);
});

// A toast raised before this page loaded is waiting in Rust.
call<Toast | null>("take_hud_toast")
  .then((t) => {
    if (t) show(t);
    else if (!current) return call("hide_hud");
  })
  .catch((e) => logError("HUD load", e));
