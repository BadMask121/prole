// Toast cards, used in-window (home, editor) and by the HUD window.
// Entry is a CSS transition from @starting-style, so a toast that's replaced
// mid-animation retargets smoothly instead of restarting.

export type ToastKind = "success" | "error";

export interface ToastOptions {
  kind: ToastKind;
  title: string;
  message?: string;
  action?: { label: string; run: () => void };
  /** ms before auto-dismiss; 0 keeps it until closed. */
  duration?: number;
  onClose?: () => void;
}

export const ICONS = {
  error: `<svg class="icon" viewBox="0 0 24 24"><path d="M12 6v7.5M12 18h.01"/></svg>`,
  success: `<svg class="icon" viewBox="0 0 24 24"><path d="M5 12.5l4.5 4.5L19 7.5"/></svg>`,
  close: `<svg class="icon" viewBox="0 0 24 24"><path d="M6 6l12 12M18 6 6 18"/></svg>`,
};

const MAX_VISIBLE = 3;

/** Builds a card. Text goes in via textContent: messages can carry
 * arbitrary error output and must never be parsed as HTML. */
export function createToast(opts: ToastOptions): { el: HTMLElement; close: () => void } {
  const el = document.createElement("div");
  el.className = `toast toast-${opts.kind}`;
  el.setAttribute("role", opts.kind === "error" ? "alert" : "status");

  const icon = document.createElement("span");
  icon.className = "toast-icon";
  icon.innerHTML = ICONS[opts.kind];

  const body = document.createElement("div");
  body.className = "toast-body";
  const title = document.createElement("div");
  title.className = "toast-title";
  title.textContent = opts.title;
  body.append(title);
  if (opts.message) {
    const msg = document.createElement("div");
    msg.className = "toast-message";
    msg.textContent = opts.message;
    msg.title = opts.message; // full text on hover when clamped
    body.append(msg);
  }

  const closeBtn = document.createElement("button");
  closeBtn.className = "toast-close";
  closeBtn.setAttribute("aria-label", "Dismiss");
  closeBtn.innerHTML = ICONS.close;

  let timer = 0;
  let closed = false;
  const close = () => {
    if (closed) return;
    closed = true;
    window.clearTimeout(timer);
    el.dataset.state = "closing";
    let removed = false;
    const done = () => {
      if (removed) return;
      removed = true;
      el.remove();
      opts.onClose?.();
    };
    // transitionend doesn't fire if nothing actually animates (e.g. in a
    // hidden window), so a timer backs it up.
    el.addEventListener("transitionend", done, { once: true });
    window.setTimeout(done, 220);
  };

  if (opts.action) {
    const { label, run } = opts.action;
    const btn = document.createElement("button");
    btn.className = "btn toast-action";
    btn.textContent = label;
    btn.addEventListener("click", () => {
      run();
      close();
    });
    body.append(btn);
  }
  closeBtn.addEventListener("click", close);
  el.append(icon, body, closeBtn);

  // Pause auto-dismiss while the pointer is on the card (it may be read or acted on).
  const duration = opts.duration ?? (opts.kind === "error" ? 8000 : 2600);
  const arm = () => {
    if (duration > 0 && !closed) timer = window.setTimeout(close, duration);
  };
  el.addEventListener("pointerenter", () => window.clearTimeout(timer));
  el.addEventListener("pointerleave", arm);
  arm();

  return { el, close };
}

let host: HTMLElement | null = null;

/** Show a toast at the bottom of the current window. */
export function toast(opts: ToastOptions) {
  if (!host) {
    host = document.createElement("div");
    host.className = "toasts";
    host.setAttribute("aria-live", "polite");
    document.body.append(host);
  }
  const t = createToast(opts);
  host.append(t.el);
  const live = host.querySelectorAll(".toast:not([data-state='closing'])");
  if (live.length > MAX_VISIBLE) (live[0] as HTMLElement).querySelector<HTMLButtonElement>(".toast-close")?.click();
  return t;
}
