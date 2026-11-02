import { describe, expect, it } from "vitest";
import { baseName, folderName, screenshotFileName } from "./filename";

describe("screenshotFileName", () => {
  it("uses local time, zero-padded, in macOS's style", () => {
    expect(screenshotFileName(new Date(2026, 10, 2, 9, 4, 7))).toBe("Prole 2026-11-02 at 09.04.07.png");
    expect(screenshotFileName(new Date(2026, 0, 31, 23, 59, 59))).toBe("Prole 2026-01-31 at 23.59.59.png");
  });
});

describe("baseName / folderName", () => {
  it("splits a saved path for the toast", () => {
    const p = "/Users/me/Desktop/Prole 2026-11-02 at 09.04.07.png";
    expect(baseName(p)).toBe("Prole 2026-11-02 at 09.04.07.png");
    expect(folderName(p)).toBe("Desktop");
  });

  it("handles a file at the root", () => {
    expect(folderName("/shot.png")).toBe("/");
    expect(baseName("/shot.png")).toBe("shot.png");
  });
});
