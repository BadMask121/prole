import { describe, it, expect } from "vitest";
import { acceleratorFromEvent, shortcutKeys } from "./shortcut";

const press = (code: string, mods: Partial<Record<"metaKey" | "ctrlKey" | "altKey" | "shiftKey", boolean>> = {}) => ({
  code,
  metaKey: false,
  ctrlKey: false,
  altKey: false,
  shiftKey: false,
  ...mods,
});

describe("shortcutKeys", () => {
  it("renders the default shortcut in macOS modifier order", () => {
    expect(shortcutKeys("CmdOrCtrl+Shift+A")).toEqual(["⇧", "⌘", "A"]);
  });
  it("renders a control shortcut", () => {
    expect(shortcutKeys("Ctrl+Shift+A")).toEqual(["⌃", "⇧", "A"]);
  });
  it("orders modifiers regardless of input order", () => {
    expect(shortcutKeys("Cmd+Alt+Ctrl+Digit4")).toEqual(["⌃", "⌥", "⌘", "4"]);
  });
  it("uses glyphs for named keys", () => {
    expect(shortcutKeys("Cmd+Slash")).toEqual(["⌘", "/"]);
    expect(shortcutKeys("Alt+Space")).toEqual(["⌥", "Space"]);
    expect(shortcutKeys("F5")).toEqual(["F5"]);
  });
  it("returns nothing for an empty shortcut", () => {
    expect(shortcutKeys("")).toEqual([]);
  });
});

describe("acceleratorFromEvent", () => {
  it("records modifiers and the key", () => {
    expect(acceleratorFromEvent(press("KeyA", { ctrlKey: true, shiftKey: true }))).toBe("Ctrl+Shift+A");
    expect(acceleratorFromEvent(press("Digit2", { metaKey: true, altKey: true }))).toBe("Alt+Cmd+2");
  });
  it("waits while only modifiers are held", () => {
    expect(acceleratorFromEvent(press("ShiftLeft", { shiftKey: true }))).toBeNull();
    expect(acceleratorFromEvent(press("MetaLeft", { metaKey: true }))).toBeNull();
  });
  it("rejects plain or shift-only keys that would fire while typing", () => {
    expect(acceleratorFromEvent(press("KeyA"))).toBeNull();
    expect(acceleratorFromEvent(press("KeyA", { shiftKey: true }))).toBeNull();
  });
  it("allows a bare function key", () => {
    expect(acceleratorFromEvent(press("F6"))).toBe("F6");
  });
  it("round-trips through shortcutKeys", () => {
    const accel = acceleratorFromEvent(press("Slash", { metaKey: true, shiftKey: true }))!;
    expect(shortcutKeys(accel)).toEqual(["⇧", "⌘", "/"]);
  });
});
