# Prole — Manual QA

Run `pnpm tauri dev` (or install a `pnpm tauri build --bundles app` build), then
work through this list. Anything that can't be verified in code lives here. File
follow-up bd issues for failures. Errors and lifecycle events are logged to
`~/Library/Logs/com.prole.app/prole.log`.

## Launch / presence
- [ ] Opening Prole shows the home window, centred, in front.
- [ ] Opening Prole again while it runs (Finder, Spotlight) brings the home window back.
- [ ] Launch at login starts quietly: pill + menu bar icon, no home window.
- [ ] Menu bar icon is a monochrome glyph that follows light/dark menu bar; no Dock icon.
- [ ] Tray menu: Capture Region (shows the shortcut) / Open Prole… / Show Floating Button (checkmark) / Quit Prole.
- [ ] Quit from the tray and from the home window footer both exit.

## Floating button
- [ ] Pill appears fully on screen, pops in, and plays one ripple on launch.
- [ ] Drag by the grip/body; position persists across relaunch (drag, quit, reopen → same spot).
- [ ] A saved position that's now off-screen (unplugged display) falls back to the right edge.
- [ ] First click on the pill captures (no "click to focus" first). Double-click the grip opens Prole.
- [ ] Pill follows you across Spaces and hides itself during the snip.

## Capture + editor
- [ ] Capture (pill, tray, shortcut, home button) opens the crosshair; the selection opens the editor.
- [ ] Esc during selection → nothing, no error. A held/double-pressed shortcut starts only one capture.
- [ ] Rectangle, arrow, pen, text; six colours; R/A/P/T switch tools instantly.
- [ ] Text box grows as you type; text has a halo and stays readable on light and dark areas.
- [ ] ⌘Z / ⇧⌘Z undo/redo, Clear is undoable. Esc with markup asks for a second Esc.
- [ ] Copy (button, ⌘C, ⌘↩) morphs to "Copied", the editor closes, and a "Copied to clipboard" toast appears.
- [ ] Paste into Claude (or anywhere) shows the annotated image.

## Home window settings
- [ ] Floating button switch shows/hides the pill live and agrees with the tray checkmark.
- [ ] Capture shortcut: click, press a combo → saved and works from any app; Esc cancels; ⌫ turns it off.
- [ ] A combo already taken by another app shows an error under the shortcut and the old one stays.
- [ ] Launch at login is reflected in System Settings → General → Login Items.

## Permission
- [ ] With Screen Recording off, Capture brings up the home window with the red status card and steps.
- [ ] "Open" goes to Privacy & Security → Screen Recording; "Restart" relaunches Prole.
- [ ] After granting + restart, the card turns green and Capture works.

## Errors surface
- [ ] A capture/copy/settings failure shows a toast (HUD under the menu bar, or in-window) and is in the log.
- [ ] "View log" opens the log in Console.

## Display correctness
- [ ] Multi-monitor: capture on a secondary display lands the correct pixels.
- [ ] Retina: captured image is full-resolution and shown at true size in the editor.

## Automated suites (must pass before release)
- [ ] `pnpm test` — frontend tests (shortcut formatting/recording, compositor).
- [ ] `pnpm test:rust` — capture, placement, logging, clipboard tests.
- [ ] `pnpm test:all` — both.
