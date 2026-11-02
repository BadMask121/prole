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
- [ ] On a full, notched menu bar the icon first appears near the clock (not hidden behind the notch); a ⌘-drag to a new spot sticks.
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
- [ ] Save (button, ⌘S) writes `Prole <date> at <time>.png` straight into the save folder, no panel; two saves in the same second give `… (2).png`, never an overwrite.
- [ ] ⇧⌘S (or ⌥/⇧-click Save) opens the save panel as a sheet on the editor, starting in the save folder; Cancel → nothing, no toast. The panel doesn't change the save folder.
- [ ] Either way the editor stays open with a "Saved to <folder>" toast whose Show in Finder selects the file, and the file opens in Preview with the markup.
- [ ] After saving, one Esc closes the editor (no discard warning); new markup after the save brings the warning back.
- [ ] At the window's narrowest, the toolbar doesn't overlap: Cancel hides and Save shows just its icon.

## Home window settings
- [ ] Floating button switch shows/hides the pill live and agrees with the tray checkmark.
- [ ] Capture shortcut: click, press a combo → saved and works from any app; Esc cancels; ⌫ turns it off.
- [ ] A combo already taken by another app shows an error under the shortcut and the old one stays.
- [ ] Recording ⌘ + one key (⌘A) or a macOS screenshot combo (⇧⌘3/⇧⌘5) is refused with an explanation.
- [ ] "Use ⇧⌘4" on: ⇧⌘4 opens Prole's crosshair, not a Desktop file. Quit Prole → ⇧⌘4 is macOS's again. Off → macOS's immediately.
- [ ] Launch at login is reflected in System Settings → General → Login Items.
- [ ] Save folder shows the macOS screenshot folder (Desktop on a stock Mac) until changed; Change… opens a folder picker (New Folder works), and the new path shows at once and sticks across relaunch.
- [ ] Rename/delete the picked folder → the row says "Not found: …" in red and ⌘S in the editor opens the save panel instead.

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
