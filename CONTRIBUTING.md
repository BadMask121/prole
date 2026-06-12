# Contributing to Prole

Thanks for your interest in improving Prole. Bug reports, feature ideas, and pull
requests are all welcome.

## Reporting bugs and requesting features

Open an [issue](https://github.com/BadMask121/prole/issues). For bugs, please include:

- your macOS version and Mac (Apple Silicon or Intel),
- what you did, what you expected, and what actually happened,
- a snip if it helps (you have the perfect tool for that).

## Development setup

**Prerequisites:** macOS, [Rust](https://rustup.rs) (the repo pins a toolchain via
`rust-toolchain.toml`), [Node.js](https://nodejs.org) 20+, and [pnpm](https://pnpm.io).

```sh
pnpm install
pnpm tauri dev      # build + launch the app (menu-bar accessory, no main window)
```

The frontend changes hot-reload. The Rust side recompiles on save.

## Project layout

- `src/` — the markup editor and small webviews (vanilla TypeScript + Vite).
  - `editor.ts` / `editor.html` — the snip markup editor (tools, text, copy).
  - `floating.ts` / `floating.html` — the floating capture button.
  - `settings.ts`, `permission.ts` — settings and the Screen Recording guide.
- `src-tauri/` — the Rust app (Tauri v2): tray, windows, capture, clipboard, hotkey.
- `docs/` — architecture, stack decisions, and the original implementation plan.

## Tests

Please run the tests before opening a PR:

```sh
pnpm test           # frontend (vitest)
pnpm test:rust      # Rust (cargo test)
pnpm test:all       # both
```

Behavior that can't be unit-tested lives in [`QA.md`](QA.md). If your change touches the
capture, editor, floating button, or permissions, walk the relevant QA items.

## Pull requests

- Keep PRs focused and write a clear description of the change and why.
- Match the surrounding code style (the project uses plain TypeScript and idiomatic Rust;
  no framework in the webviews).
- Link any related issue.
- Make sure `pnpm test:all` passes.

## Code signing (optional, for local dev)

The app is ad-hoc signed. For a stable self-signed identity so the Screen Recording grant
persists across dev rebuilds, run `scripts/create-signing-cert.sh` once, then
`scripts/sign-dev.sh` after a build. This is only for local development.

## License

By contributing, you agree that your contributions will be licensed under the
[MIT License](LICENSE).
