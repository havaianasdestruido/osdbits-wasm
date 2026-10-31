# UILess Presentation: In-App UI Only

osdbits-wasm follows the UILess `no-ui-browser-apps` rules
(<https://github.com/havaianasdestruido/UILess>, `SKILL.md`). The page contains only the
application: one fullscreen canvas on black. Nothing else is visible at rest.

## What the page contains

| Surface | Where it is drawn | Notes |
| --- | --- | --- |
| Title / start | canvas (pixel raster) | `START OSDSYS`, `INITIAL SCENE`, and `PROCEED WITH CANVAS 2D` when WebGPU is missing |
| Scene corner pop-up | canvas | at most 5 buttons: `OPEN`, `IDLE`, `RED`, `MENU`, `OPTIONS`; shown on activity or with Tab, auto-hides after about 2.5 s idle |
| Mode C warning | canvas | localized message plus provenance note |
| Mode D menu | canvas | entries with `[SOURCED]` / `[APPROX]` / `[EXT]` tags, clock editor, key hints |
| Mode E options | canvas, full-screen panel | every former drawer control as a row (see below) |
| Diagnostics | canvas, options sub-screen | live telemetry and provenance |
| Error | canvas | one `SCENE ERROR` box with a `RECOVER SCENE` row |
| Fatal startup error | canvas | one line `OSDBITS ERROR: ...`; the detail goes to the console |

The DOM holds only `#app`, the canvas `#osdbits-canvas` (with `aria-label` and
`role="application"`), and a visually hidden `#osdbits-sr-status` line that mirrors the
selected row for screen readers. There are no buttons, inputs, labels, headers, footers,
panels, or toasts in the DOM. Fonts are not loaded: text uses the in-repo 5x7 pixel font
(`web/src/ui/pixelFont.ts`).

The metadata is the only non-canvas content: `<title>OSDBITS // WASM</title>`, an SVG favicon,
and a web manifest (`web/public/manifest.webmanifest`).

## How it is drawn

- `web/src/ui/pixelRaster.ts` is a logical RGBA raster at 240 rows high. Its width follows the
  canvas aspect ratio and is clamped to 240..640 columns.
- `web/src/ui/inAppScreens.ts` lays out every screen. It returns hit regions in raster
  coordinates, and pointer input is resolved against exactly what was drawn that frame.
- WebGPU: `WebGpuRenderer.renderFrame(..., ui)` runs an extra pass after the scene that
  composites the raster with nearest sampling. While the scene is paused, `presentUi(ui)`
  re-presents the UI over the last `sceneColor` without re-running the scene.
- Canvas 2D: the same raster is composited with `drawImage`. A paused scene repaints at about 20 fps.
- Scene goldens read `sceneColor` only, so the UI layer does not change visual regression output.

## Controls

Pointer: click any row, button, or hint. Wheel scrolls the options and diagnostics lists.
Mode E sliders accept clicks on the bar.

Keys:

| Context | Keys |
| --- | --- |
| Title | Up/Down or W/S select, Left/Right or A/D change, Enter/Space/Z activate |
| Scene | 1-4 switch scene (A/B/C/D), Tab pins the pop-up, C options, I diagnostics, M mute, R recover |
| Mode D menu | Arrows or WASD move, Enter/Space/Z confirm, Esc/Backspace/X back, O version |
| Options | Up/Down or W/S move, Left/Right or A/D change (Shift for larger steps), Enter activates, Esc/X/Backspace back, C closes, I diagnostics |

Help for these keys is also drawn in-app: the options screen has a `CONTROLS` section, and the
Mode D menu shows its own hints.

## Options screen (Mode E)

Every setting from the former customization drawer is a row:

- Display and timing: video standard (NTSC/PAL), aspect framing, resolution scale, OSD language, active scene.
- Towers and seed: tower field preset, simulated games (enabled for the simulated preset), deterministic seed, animation speed.
- Visual passes: fog, light trails, glass cubes, ExtraBuf glow, motion/zoom blur, reduced motion, opening-to-menu, 16:9 letterbox.
- Audio mixer: master, ambient, and UI foley volume, plus audio output on/off.
- Resources and save data: import a local resource file (BIOS/TEXIMAGE, optional), export and import settings JSON, and a two-step `RESET TO DEFAULTS` followed by `CONFIRM RESET TO DEFAULTS` or `CANCEL RESET`.
- Diagnostics: opens the diagnostics sub-screen.

Settings persist through `StorageManager` exactly as before. The status line shows the last
save verification.

## Accessibility

- The canvas is focusable, labelled, and announces the selected row through the hidden status line.
- Menu rows and hints are disabled (not drawn as clickable) while the core ignores input during its transitions.
- The cursor hides after about 3 s of pointer idle in scene mode.
- `prefers-reduced-motion` seeds the reduced-motion setting on first visit.

## Verification

- `tests/unit/phase9-ui.test.ts`: at rest the DOM has no controls, the title is drawn in-app, and invalid resource files are rejected in the options status.
- `tests/e2e/ui-audio-shell.spec.ts`: the at-rest DOM check (`expectOnlyApplicationVisible`), title click start with synchronous audio unlock, Mode C, Mode D, every Mode E control path including two-step reset, diagnostics, error recovery, pop-up auto-hide, and the no-WebGPU fallback.
- `tests/e2e/menu-input-readiness.spec.ts`: samples every frame and fails if any menu region is clickable while input is gated.
- `tests/e2e/support/uiHarness.ts`: helpers that click in-app regions through the real canvas pointer path.
- `tests/visual` and `tests/perf` are unchanged. Their goldens and budgets read the scene only.
