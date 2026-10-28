// Global-shortcut strings ("CmdOrCtrl+Shift+A") ↔ what the UI shows (⇧ ⌘ A)
// and what a KeyboardEvent records. Names follow the global-hotkey parser.

type Mod = "Ctrl" | "Alt" | "Shift" | "Cmd";

const MOD_ORDER: Mod[] = ["Ctrl", "Alt", "Shift", "Cmd"]; // macOS order: ⌃⌥⇧⌘
const MOD_GLYPH: Record<Mod, string> = { Ctrl: "⌃", Alt: "⌥", Shift: "⇧", Cmd: "⌘" };
const MOD_ALIAS: Record<string, Mod> = {
  CTRL: "Ctrl",
  CONTROL: "Ctrl",
  ALT: "Alt",
  OPTION: "Alt",
  SHIFT: "Shift",
  CMD: "Cmd",
  COMMAND: "Cmd",
  SUPER: "Cmd",
  CMDORCTRL: "Cmd",
  CMDORCONTROL: "Cmd",
  COMMANDORCTRL: "Cmd",
  COMMANDORCONTROL: "Cmd",
};
const KEY_GLYPH: Record<string, string> = {
  SPACE: "Space",
  ENTER: "↩",
  TAB: "⇥",
  BACKSPACE: "⌫",
  DELETE: "⌦",
  ARROWUP: "↑",
  ARROWDOWN: "↓",
  ARROWLEFT: "←",
  ARROWRIGHT: "→",
  BACKQUOTE: "`",
  MINUS: "-",
  EQUAL: "=",
  BRACKETLEFT: "[",
  BRACKETRIGHT: "]",
  BACKSLASH: "\\",
  SEMICOLON: ";",
  QUOTE: "'",
  COMMA: ",",
  PERIOD: ".",
  SLASH: "/",
};
const NAMED_CODES = [
  "Space", "Enter", "Tab", "Backspace", "Delete", "Home", "End", "PageUp", "PageDown",
  "ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight", "Backquote", "Minus", "Equal",
  "BracketLeft", "BracketRight", "Backslash", "Semicolon", "Quote", "Comma", "Period", "Slash",
];

/** Keycap labels for a shortcut, modifiers first in macOS order. "" → []. */
export function shortcutKeys(accelerator: string): string[] {
  const mods = new Set<Mod>();
  let key = "";
  for (const raw of accelerator.split("+")) {
    const token = raw.trim().toUpperCase();
    if (!token) continue;
    if (MOD_ALIAS[token]) mods.add(MOD_ALIAS[token]);
    else key = keyLabel(token);
  }
  const caps = MOD_ORDER.filter((m) => mods.has(m)).map((m) => MOD_GLYPH[m]);
  return key ? [...caps, key] : caps;
}

function keyLabel(token: string): string {
  if (/^KEY[A-Z]$/.test(token)) return token.slice(3);
  if (/^DIGIT\d$/.test(token)) return token.slice(5);
  if (KEY_GLYPH[token]) return KEY_GLYPH[token];
  if (/^F\d{1,2}$/.test(token) || token.length === 1) return token;
  return token[0] + token.slice(1).toLowerCase();
}

type KeyInfo = Pick<KeyboardEvent, "code" | "metaKey" | "ctrlKey" | "altKey" | "shiftKey">;

/**
 * The shortcut a key press describes, or null while it isn't usable yet:
 * a lone modifier, or a plain key with only ⇧ (that would fire while typing).
 * Function keys may stand alone.
 */
export function acceleratorFromEvent(e: KeyInfo): string | null {
  const key = keyFromCode(e.code);
  if (!key) return null;
  if (!(e.ctrlKey || e.altKey || e.metaKey) && !/^F\d{1,2}$/.test(key)) return null;
  const mods: Mod[] = [];
  if (e.ctrlKey) mods.push("Ctrl");
  if (e.altKey) mods.push("Alt");
  if (e.shiftKey) mods.push("Shift");
  if (e.metaKey) mods.push("Cmd");
  return [...mods, key].join("+");
}

function keyFromCode(code: string): string | null {
  if (/^Key[A-Z]$/.test(code)) return code.slice(3);
  if (/^Digit\d$/.test(code)) return code.slice(5);
  if (/^F\d{1,2}$/.test(code)) return code;
  return NAMED_CODES.includes(code) ? code : null;
}
