<div align="center">
  <img src="src-tauri/icons/128x128@2x.png" width="96" height="96" alt="Prole" />
  <h1>Prole</h1>
  <p><strong>Snip anything on screen. Paste it anywhere.</strong></p>
  <p>
    <a href="https://prole.jeffrey.build/download/Prole.dmg"><img src="https://img.shields.io/badge/Download-macOS%20.dmg-d97757" alt="Download" /></a>
    <img src="https://img.shields.io/badge/platform-macOS%2012%2B-111" alt="macOS 12+" />
    <img src="https://img.shields.io/badge/license-MIT-blue" alt="MIT" />
    <img src="https://img.shields.io/badge/built%20with-Rust%20%2B%20Tauri-orange" alt="Rust + Tauri" />
  </p>
  <p><a href="https://prole.jeffrey.build">prole.jeffrey.build</a></p>
</div>

---

Prole is a free, native macOS menu-bar app for capturing any region of your screen, marking it up,
and pasting it wherever you want. Spot a weird error, a confusing chart, or a layout that feels off,
draw a box around it, annotate it, and paste the image straight into Claude, ChatGPT, Slack, an
email, or any app that takes an image.

It captures **pixels and your notes only**. No DOM, no browser extension, no source-code extraction.

## How it works

1. **Snip it.** Trigger from the always-on-top floating button, the menu bar, or a global hotkey, then draw a box around anything on screen.
2. **Mark it up.** Add rectangles, arrows, and a freehand pen, and use the text tool to write your question (at any size) right on the image.
3. **Paste it anywhere.** Hit Copy and the annotated image lands on your clipboard. Paste it into Claude, ChatGPT, Slack, email, or any app that accepts an image.

## Features

- **Floating capture button** that sits on top of everything, draggable to wherever you park it. One click to snip.
- **Markup tools:** rectangle, arrow, freehand pen, and a text tool that writes directly on the image (with adjustable font size).
- **Three ways to trigger:** floating button, menu-bar item, or a configurable global hotkey.
- **Launch at login** toggle.
- **Multi-monitor aware** capture.
- Lives in the menu bar as an accessory app (no Dock icon, no main window).

## Install

**Download (easiest):** grab the latest `.dmg` from **[prole.jeffrey.build](https://prole.jeffrey.build)**.

Prole is ad-hoc signed (not yet notarized by Apple), so on first launch:

1. Open the `.dmg` and drag **Prole** into your **Applications** folder.
2. Right-click (or Control-click) **Prole** and choose **Open**, then confirm. You only do this once.
   (Or in a terminal: `xattr -dr com.apple.quarantine /Applications/Prole.app`.)
3. On your first capture, grant **Screen Recording** for Prole in *System Settings → Privacy & Security → Screen Recording*.

## Build from source

**Prerequisites:** macOS, [Rust](https://rustup.rs) (the repo pins a toolchain via `rust-toolchain.toml`),
[Node.js](https://nodejs.org) 20+, and [pnpm](https://pnpm.io).

```sh
pnpm install
pnpm tauri dev      # run the app in development
pnpm tauri build    # produce a release Prole.app + .dmg
```

Run the tests:

```sh
pnpm test           # frontend (vitest)
pnpm test:rust      # Rust (cargo test)
pnpm test:all       # both
```

### Releasing

`scripts/release.sh` builds a universal (Apple Silicon + Intel) `.dmg` locally and uploads it to the
download host. See the script header for environment overrides. There is also `scripts/sign-dev.sh`
and `scripts/create-signing-cert.sh` for a stable self-signed identity so the Screen Recording grant
persists across dev iterations.

## Tech stack

Rust + [Tauri v2](https://v2.tauri.app) for the native shell, system integration, and clipboard.
Vanilla TypeScript + [Vite](https://vite.dev) for the markup editor and small webviews. Screen
capture wraps the macOS `screencapture` tool. See [`docs/`](docs/) for the architecture, stack
decisions, and the original implementation plan.

## Inspiration

Inspired by [agentation](https://www.agentation.com/), which does this beautifully inside the
browser DOM. Prole takes the same "point at it and ask" idea to the whole desktop: pixels and your
notes from any app, with nothing to install in the browser.

## License

[MIT](LICENSE) © Jeffrey Emakpor
