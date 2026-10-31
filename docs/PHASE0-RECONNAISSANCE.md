# Phase 0: Repository Reconnaissance & Baseline Architecture Report

## 1. Existing Repository Inventory

The repository at `/home/user/osdbits-wasm` is a fork of `aap/osdbits` (commit `ea0c687`), a reverse-engineering and PS2 Emotion Engine (EE) / Graphics Synthesizer (GS) C reconstruction of the PlayStation 2 `OSDSYS` visual and menu subsystems (specifically targeting SCPH-39001 v1.60 USA / SCPH-30004R v1.60 PAL retail ROM behavior).

### 1.1 Source & Tooling Tree
- **`osdbits/`** — Standalone PS2 SDK (`ee-gcc`) C & VU1 implementation:
  - `main.c` (267 lines): Entrypoint (`main`), CLI mode dispatcher (`opening`, `illegal`, `menu`), double-buffered GS frame setup (`InitDraw`, `StartFrame`, `WaitNextFrame`, `SwapBuffers`), vblank handler (`VblankHandler`).
  - `inc.h` (263 lines): Shared types (`u8`..`u64`, `i8`..`i64`, `f32`, `u128`), `PadState` and `PAD_*` bitmasks, `SceneRec` depth-sorted draw record, matrix stack (`MatrixDrive`) and subsystem prototypes.
  - `res.h` / `res.c` (73 / 365 lines): `RESID` enum (86 resource slots across `FONTM`, `FNT*`, `SND*`, `TEXO*`, `TEXC*`, `ICOB*`) and compile-time `.inc` resource table loader (`GetResourcePtr`, `GetResourceSize`).
  - `opening.c` (4,356 lines): Complete Opening ("Towers"), Idle, and Illegal-Disc ("Red Screen of Death") state machines and GS/VIF1 renderers.
  - `towerchain.dsm` (128 lines) & `vucode_1.vsm` (194 lines): VU1 DMA chain and microcode for rendering a single tower's 6 quad strips (top cap + 4 walls + base collar) with 4 directional lights + ambient + distance fog.
  - `menu.c` (1,696 lines): Module U 3D main-menu scene: software RTC (`ClockSeconds`, `ClockMinutes`, `ClockHours`), 16-bit angle `MatrixDrive` stack (`mdRotX/Y/Z`, `mdTranslate`, `mdScale`), 7 orbiting orbs (`InitOrbs`, `UpdateOrbs`, `DrawOrb`) with 50-sample trails (`TRAILLEN=50`, `TRAILDRAW=49`, dual-walk to screen and work buffer 3), depth-sorted `SceneRec` list (`SceneAddOrb`, `SceneAddMesh`, `SceneWalk`), 2-pass rod bloom (`SceneFlush`), 16-segment ellipse `DrawVignette`, and global fade curtain.
  - `menuback.c` (1,005 lines): Module U backdrop & work-buffer plumbing: `TEXCKABE` 128×128 tiled 4-bit decoder (`DecodeKabeTile`), 16×33 breathing cylinder tunnel (`DrawKabeRibbon`), deep-blue composite (`{0x37, 0x28, 0x3C, 0x80}`), 5-pass bilinear `ZoomBlur` (`319.25 × 149.25` shrink/stretch), and work-buffer target switching (`extraBuf1` = wb3, `extraBuf2` = wb4).
  - `menuconfig.c` (2,250 lines): System Configuration 3D glass renderer: 90-frame (`40+40+10` NTSC) entry/exit timer choreography (`CfgTimer`), 12 hexagonal clock rods (`menuRodVerts`, 16 faces, front hour rod split at `1 - minutes/60` in `{167, 217, 255}` cyan-white, 5-pass refraction + `TEXCBUMP` emboss, 2-walk `TEXCFLOW` + `TEXCBUMP`/`TEXCBINV` bloom), and 5 spinning config cubes (`menuCubeVerts`, 6 faces, identity camera, 8-pass live-screen refraction + `TEXCBUMP` emboss + `TEXCREFA` spherical reflection & `A=0x80` mask composite).
  - `menutext.c` (2,118 lines): Module U 2D OSD text engine: `FNTASCII` (256×480, 8×12 grid of 32×40 cells, 97 glyphs) and `FNTEXOSD` (512×80, 16×2 grid of 32×40 cells, 32 special glyphs including `®`), 16-entry grey-alpha CLUT, `\7oNNN` and `\7rN.NN` escape parser, main menu ("Browser", "System Configuration", "Version Information"), System Configuration 1-at-a-time item/value display and mode-1 editors (Clock Adjustment 6-field editor freezing soft clock & sinking orbs to center, Screen Size 4:3/Full/16:9, Digital Out, Component Video Out, Language), letterbox bars, top date/time bar, and `TEXCMARU` button hint bar.
  - `pad.c` (347 lines): Controller input cooker with region Circle/Cross swap (`padRegionSwap`) and UP/DOWN auto-repeat (first repeat at 33 frames after press, `count >= 31 && count % 3 == 0`, then every 3 frames; LEFT/RIGHT press-edge only).
  - `sound.c` (446 lines): `OSDDispatch` 128-entry ring mailbox, `OSDSND` (`0x80000601`) RPC protocol, 2 VAB banks (`SNDBOOT`, `SNDOSDD`), 8 BGM slots (`SNDBOOTS`, `SNDTNNLS`, `SNDCLOKS`, `SNDTM30S`, `SNDTM60S`, `SNDLOGOS`, `SNDWARNS`, `SNDRCLKS` with volumes `66, 42, 45, 27, 27, 27, 54, 54`), UI SEs (`20992, 1, n` for `n=4` confirm, `5` toggle, `6` cursor move, `10` back/cancel), and 60-tick reverb fade-in ramp.
  - `tools/extract-res.py` (312 lines): Extracts ROMDIR containers (`FNTIMAGE`, `TEXIMAGE`, `SNDIMAGE`, `ICOIMAGE`) and `--tables` (`FONTDATA.inc` and `MENUGEOM.inc` from compressed `OSDSYS`) out of a user-supplied PS2 BIOS image.
- **Root-level reference files**:
  - `ps2expand.c` (109 lines): Exact C reconstruction of the `OSDSYS` LZSS `expand()` decompressor.
  - `vucode_1.vu`..`vucode_7.vu` & `.vsm`: Disassembled VU1 microcode programs from `0x2678f0..0x26b060`.
  - `matching/`: Instruction-level matching decompilation workspace for `opening.c` functions.
  - `docs/`: Deep reverse-engineering documentation (`OSDSYS-STRUCTURE-REPORT.txt`, `towers-analysis.md`, `glass-refraction.md`, `object-order.md`, `menu-scene.md`, `menu-draw.md`, `menu-backdrop.md`, `menu-config.md`, `menu-text.md`, `pad.md`, `sound.md`, `rom-survey.md`) and 5 reference screenshots (`opening.png`, `illegal.png`, `menu.png`, `config1.png`, `config2.png`).

---

## 2. Detailed Subsystem Mechanics & Porting Map

### 2.1 Resource Extraction, Decompression & Legal Asset Architecture
- **Legal Constraint:** No Sony BIOS dumps, ROM slices, or copyrighted extracted `.inc` textures/fonts/sounds may be committed to git or bundled in the repository.
- **Dual-Source Asset Pipeline (Rust WASM):**
  1. **Built-in Procedural Clean-Room Assets (Default / Out-of-the-Box):**
     - Synthesizes all 21 Opening/Illegal textures (`TEXOSCE`, `TEXOFOG0..4`, `TEXOWAL0`, `TEXOCRLE`, `TEXOCRBL`, `TEXOFLAR`, `TEXOREF`, `TEXOBLP`, `TEXOBLPR`, `TEXOPNGJ..P`), 8 Menu/Config textures (`TEXCKABE`, `TEXCBLUR`, `TEXCNAVI`, `TEXCBUMP`, `TEXCREFA`, `TEXCFLOW`, `TEXCBINV`, `TEXCMARU`), and 2 Font bitmaps (`FNTASCII` 256×480, `FNTEXOSD` 512×80) + 16-entry CLUT + 97-glyph `fontAsciiMetrics` + 299-entry English/localized `osdStringTable` + `MENUGEOM` (16-face hexagonal bevelled rod mesh, 6-face cube mesh, 5-cube placement table) mathematically in Rust.
  2. **User-Supplied BIOS / Container Import (Optional in Browser UI):**
     - User selects a local PS2 BIOS image (e.g., `scph39001.bin`) or resource container (`TEXIMAGE`, `FNTIMAGE`, `SNDIMAGE`) via file picker.
     - Rust WASM validates the ROMDIR structure (scanning 16-byte aligned offsets for `"RESET\0"`, `"ROMDIR\0"`, `"EXTINFO\0"`), extracts nested containers (`FNTIMAGE`, `TEXIMAGE`, `SNDIMAGE`, `OSDSYS`), runs the exact `ps2expand.c` LZSS `expand()` algorithm in Rust, decodes all textures/fonts/geometry tables, and persists the validated asset bundle into OPFS/IndexedDB with read-back verification.

### 2.2 Opening ("Towers"), Idle, and Illegal-Disc Subsystem (`osdbits/opening.c`)
- **Deterministic PRNG:** `osdRand()` LCG (`seed = seed * 1103515245 + 12345; return (seed >> 16) & 0x7fff;`).
- **Camera State Machine (`ProcessOpeningAnimation`):**
  - States 0..7 across Z milestones `{16.0, 56.0, 104.0, 320.0, 672.0, 800.0, 1160.0, 1160.0}`.
  - State 0→3: Slow downward drift into the tower field (`camPos.z` from 1160 down to 104) while 4 colored lights orbit and leave 128-segment ribbon trails.
  - State 3→6: Rapid fly-up (`camPos.z` 104 → 1160) with 3-pass zoom motion blur (`sub_218b20`) and transition fade (`sub_218bd0`).
  - Idle Mode (Mode B / `openingType == 3`): Keeps camera drifting through the tower field indefinitely without triggering the fly-up exit.
- **Tower Field (`InitTowersFog`, `DrawTowers`):**
  - 14×9 grid (`x = -7..6`, `y = -4..4`) spaced 40 units apart (`±10` jitter from `towerOffsets`).
  - Driven by 21 memory-card boot-history slots × 6-cell neighborhoods (`histCells[21][6]`). Each active play-count increments cell heights (`towerHeights`), which scale tower Z length (`1.0 + (h - 1.0) * 0.2`) and visibility.
  - 20×20 radial `towerGrid` (`1600.0 - sqrt(dx² + dy²)`).
  - Each tower is 6 quad strips (`towerchain.dsm` / `vucode_1.vsm`), shaded by 4 directional lights (`towerLightDirs`, `towerLightColors`, `towerAmbient`) with distance fog.
- **Fog (`InitFog`, `DrawFog`):**
  - 6 scrolling fog layers (`fogLayers[6]`) rendered on a 16×16 projected grid using `TEXOFOG0..4`.
- **Lights, Trails & Refractive Glass Cubes (`DrawLightsAndCubes`, `DrawCube`):**
  - 4 colored point lights (`lightColors`: cyan-blue, green, red/magenta, purple) + 128-sample history trails (`DrawTrail`, `UpdateTrail`) + `TEXOCRLE`/`TEXOCRBL` billboards.
  - 5 spinning glass cubes (`cubePositions`, `cubeScales`, `cubeRotations`), each drawn in 10 passes (5 back-face passes into `extraBuf2`, 5 front-face passes onto screen) with screen-space normal-displaced refraction (`CubeComputeUV` modes 0, 1, 2) and scrolling environment maps (`TEXOBLPR`, `TEXOREF`, `TEXOBLP`).
- **Illegal-Disc Scene (Mode C / `DoIllegalDisc`):**
  - `DrawRedFlare`: 7 spinning 16-gon polygon rings (small + large) + 5 `TEXOFLAR` additive flare sprites + `extraBuf2` feedback blur (alpha 112).
  - `DrawIllegalFog`: 128 falling 3×3 red cloud patches (`TEXOFOG0..4`) tinted `{148, 30, 38}`.
  - `DrawIllegalCubes`: 5 red-tinted (`{0.42, 0.08, 0.04}`) 10-pass refractive glass cubes.
  - `DrawIllegalFades` & `DoIllegalText`: Localized error text (`TEXOPNGJ..P`: `"Please insert a PlayStation® or PlayStation®2 format disc."`) with smooth fade-in/out.

### 2.3 Main Menu & System Configuration Subsystem (`menu.c`, `menuback.c`, `menuconfig.c`, `menutext.c`)
- **Software Clock & MatrixDrive:**
  - `ClockSeconds`, `ClockMinutes`, `ClockHours` advanced per frame (`1/60` NTSC, `1/50` PAL), frozen during Clock Adjustment editing.
  - 16-bit integer angle lookup (`mdSin`, `mdCos`, `65536 = 2π`) and 8-deep matrix stack (`MatrixDrive`).
- **7 Orbs & 50-Sample Trails (`menu.c`):**
  - 7 orbs orbiting in a cluster whose radius eases toward `10.0 + 7.25 * (minutes / 60.0)` (or collapses to `0` when editing the clock).
  - Each orb has a 4-layer billboard stack (`TEXCBLUR` / `TEXCNAVI`) and a 50-sample ribbon trail rendered both to the screen and to `wb3` (`extraBuf1`).
- **Backdrop, Composite & 5-Pass Zoom Blur (`menuback.c`):**
  - `DrawKabeRibbon`: 16×33 cylinder mesh textured with decoded 128×128 `TEXCKABE` when System Configuration is open (`MenuBackdropVisible()`).
  - Pre-tint frame copied to `wb3` and `wb4`; screen multiplied by deep-blue tint `{0x37, 0x28, 0x3C, 0x80}`; 5-pass bilinear zoom blur (`319.25 × 149.25` shrink/stretch) adds the signature radial light rays from `wb4` onto the screen.
- **System Configuration 3D Glass (`menuconfig.c`):**
  - **12 Hexagonal Clock Rods:** Arranged on a radius-20 ring tilted by the clock's hour/second hands. Front rod splits at `1 - minutes/60` with cyan-white `{167, 217, 255}` lower segment. Drawn with 5 passes (far refraction from `wb3` → `wb4`, 2-tap `TEXCBUMP` emboss, near refraction onto screen, washed-out write to `wb3`) plus 2-walk `TEXCFLOW` + `TEXCBUMP`/`TEXCBINV` additive bloom (`alpha = 30`).
  - **5 Config Cubes:** Positioned in a left-hand zig-zag column with identity camera (`z = 47.5`). Selected cube eases to `{0, 150, 200}` cyan-blue and kicks size by `-0.1` on confirm; unselected cubes ease to `{100, 100, 100}`. Drawn with 8 passes (far refraction of live tinted screen into `wb4`, 2-tap `TEXCBUMP` emboss, flat Fresnel into `wb3`, near Z-greater black prime, `wb3→wb4` left-half additive copy, near refraction into `wb3`, 2-tap `TEXCBUMP` emboss) followed by `MeshDrawCubeMask` (`TEXCREFA` env reflection + `A=0x80` mask into `wb4`, copied to `wb3` alpha and composited via `(Cs-Cd)*As + Cd`).
- **2D OSD Text & Interactive Editors (`menutext.c`):**
  - Main Menu: `"Browser"`, `"System Configuration"`, `"Version Information"` (on Triangle), plus extensible Mode D browser/scene switcher items clearly badged when going beyond retail reconstruction.
  - System Configuration: 1-at-a-time item display (`Clock Adjustment`, `Screen Size`, `DIGITAL OUT (OPTICAL)`, `Component Video Out`, `Language`) with up/down arrows (`TEXCMARU`), mode-1 value editors, top date/time bar, 4:3 / 16:9 letterbox bars, and bottom button hint bar (`Cross/Circle Enter/Back`, `Square Display`, `Triangle Version`).

---

## 3. Target Architecture (`osdbits-wasm`)

| Layer | Technology | Responsibilities |
|---|---|---|
| **Core Simulation & Asset Engine** | Rust (`crates/osdbits-core`, compiled to `wasm32-unknown-unknown` + native `x86_64` for unit/property/fuzz tests) | Deterministic math (`libvu0` & `MatrixDrive`), `osdRand` LCG, ROMDIR & LZSS `expand()` parser, texture/font decoders, procedural asset generator, Opening/Idle/Illegal/Menu/Config state machines, input cooker, `OSDDispatch` audio mailbox, and flat `#[repr(C)]` render command & vertex/instance buffer generation in linear memory. |
| **GPU Rendering Pipeline** | WebGPU + WGSL (`src/renderer/`) | Multi-target offscreen pass graph (`screen`, `wb3`, `wb4`, `snapshot`, `depth`), PS2 GS `BlendModes[0..10]` pipelines, per-primitive `CLAMP`/`REPEAT` samplers, tower/fog/orb/trail/cube/rod/tunnel/vignette/zoom-blur/text/sprite passes, and pixel readback for visual verification. |
| **Audio Subsystem** | Web Audio API (`src/audio/`) | Synchronous `AudioContext.resume()` inside the user activation click handler; real-time synthesizer & sequencer driven by the Rust `OSDDispatch` mailbox (8 BGM slots + UI click SEs `4/5/6/10` + 60-frame stereo reverb ramp) plus optional decoded `SND*` VAB/SPU2 ADPCM sample playback. |
| **Persistence Layer** | IndexedDB + OPFS (`src/storage/`) | Versioned schema (`v1` → `v2` migrations), atomic settings & boot-history store in IndexedDB, binary asset bundle storage in OPFS (with IndexedDB fallback), mandatory read-back verification after every write, corruption recovery, quota handling, and JSON export/import/reset. |
| **Browser Shell & UI** | TypeScript (`strict: true`), HTML5, CSS (`src/ui/`, `src/app/`) | Single-click activation overlay (`Start OSDSYS`), mode switcher (Modes A, B, C, D, E), keyboard/pointer/gamepad bindings, customization drawer, resource import modal, diagnostics/telemetry HUD (local only, zero external network calls), and reduced-motion accessibility support. |
