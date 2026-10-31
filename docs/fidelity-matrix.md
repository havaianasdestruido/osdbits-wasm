# Fidelity & Verification Matrix (`osdbits-wasm`)

This document maps every subsystem of `osdbits-wasm` against the preserved PS2 C/VU reverse-engineering reference (`osdbits/`, `docs/`, `vucode_*.vsm`, `ps2expand.c`) using exact status terminology:

- **Implemented**: Code exists in `crates/osdbits-core` and/or `src/`.
- **Tested**: Automated Rust, WASM, TypeScript, and/or Playwright tests execute and pass.
- **Verified**: Behavior matches upstream reverse-engineered source equations, savestate constants, and GS-dump notes (`docs/gscmp-FINDINGS.md`, `docs/glass-refraction.md`, `docs/menu-config.md`).
- **Approximate**: Intentional clean-room replacement or unverified upstream region, labeled transparently in UI and documentation.
- **Blocked**: Cannot be executed in the current sandbox due to missing hardware (specifically physical `/dev/dri` GPU silicon).
- **Deferred**: Explicitly out of scope for the current release and tracked in `TODO.MD`.

---

## Subsystem Fidelity Table

| Subsystem | Upstream Reference | Status | Notes |
| --- | --- | --- | --- |
| `osdRand` & `towerRand` LCG PRNGs | `osdbits/opening.c:41–73`, `docs/gscmp-FINDINGS.md` | **Implemented · Tested · Verified** | Exact `1103515245 * seed + 12345` (`& 0x7fffffff`) and `0x2b992ddf` sequences; savestate draw `#386 == 4002` verified in `rng.rs`. |
| Opening Tower Sprawl & 14×9 Canopy | `osdbits/opening.c:3568+`, `vucode_1.vsm` | **Implemented · Tested · Verified** | 21-slot memory history, `HIST_SRC_POS` layout, swaying column heights, and 4-light diffuse shading. |
| Opening 8-State Camera & Fly-Up | `osdbits/opening.c:1100–1320` | **Implemented · Tested · Verified** | Exact `cam_pos.z` drift, `opening_go` fly-up acceleration, completion threshold (`z >= 148.0`), and configurable post-boot policy. |
| Opening 6-Layer Fog, 4 Orbiting Lights & Trails | `osdbits/opening.c:1800–2450` | **Implemented · Tested · Verified** | Radial `sqrt(5202)` distance attenuation, scrolling cloud UVs, and additive orbiting light spheres + trails. |
| Opening 5 Refractive Glass Cubes | `osdbits/opening.c:1443–1790`, `docs/glass-refraction.md` | **Implemented · Tested · Verified** | Per-cube `copyTextureToTexture` backdrop capture (`CAPTURE_TINTED_BACKDROP`), `-0.084 * normal_xy` UV refraction offset, back-then-front face pass order, and `2x` specular reflection (`PS2_MODULATE_2X`). |
| Opening Overlay Banner | `osdbits/opening.c` (`DoSCEText`) | **Implemented · Tested · Approximate** | Uses a legally conservative clean-room banner (`OSDSYS // CLEAN-ROOM RECONSTRUCTION`) rather than Sony Computer Entertainment trademarks. |
| Indefinite Idle Tower Field (`Mode B`) | `osdbits/opening.c` (`idle` mode) | **Implemented · Tested · Verified** | Bounded indefinite canopy hover verified over 10,000+ steps with zero numeric drift and customizable `idleDrift`. |
| Illegal-Disc Sequence (`Mode C`) | `osdbits/opening.c` (`illegal` mode) | **Implemented · Tested · Approximate** | Reconstructs `z = 672 -> 800+` ascent, 7-ring crimson flare, 128 falling red fog patches, 5 red refractive cubes, and 7-language clean-room disc warning. As noted in upstream `opening.c`, `DoIllegalText` placement was not verified against a retail illegal-disc GS dump. |
| OSDSYS Main Menu & 7 RTC Orbs (`Mode D`) | `osdbits/menu.c`, `docs/menu-rendering-reverse.md` | **Implemented · Tested · Verified** | 7 RTC-driven orbs via 16-bit `MatrixDrive`, 50-sample `(age^4, age^2, age)` trails, entry scatter, and half-res `ExtraBloom` radial `MenuZoomBlur`. |
| System Configuration Smoke Tunnel, 12 Clock Rods & 5 Config Cubes | `osdbits/menuback.c`, `osdbits/menuconfig.c`, `docs/menu-config.md` | **Implemented · Tested · Verified** | `16×33` breathing `SmokeTunnel` + backdrop pre-tint capture; 12 hexagonal bevelled glass rods with active-hour `(1 - min/60)` cyan fill + 2-walk `GlassFlow` + `GlassBump`/`GlassBumpInv` reverse-subtract shading; 5 zig-zag identity-camera config cubes (`z = 47.5`) + selection kick. |
| Clock Adjustment Digit Editor & Version Overlay | `osdbits/menuconfig.c`, `docs/menu-text.md` | **Implemented · Tested · Verified** | Full `YYYY/MM/DD HH:MM:SS` field stepping, wrapping, keyboard + pointer hit-testing, and regional Cross/Circle confirm swap. |
| Memory Card `"Browser"` Showcase Subpage | `docs/OSDSYS-STRUCTURE-REPORT.txt` | **Implemented · Tested · Approximate** | Upstream `osdbits` leaves the Memory Card Browser interior unimplemented; `osdbits-wasm` provides a clearly badged (`Approximate · Browser Grid`) interactive 21-slot tower-memory showcase. |
| Clean-Room Procedural Textures, Mesh & Glyph Atlas | `procedural_assets.rs`, `osd_font.rs` | **Implemented · Tested · Approximate** | 15 deterministic clean-room RGBA8 textures satisfying `GlassBumpInv == !GlassBump` without copyrighted Sony `TEXIMAGE`/`FNTIMAGE` data. |
| Optional User `ROMDIR` / `TEXIMAGE` Validator & LZ `expand()` | `ps2expand.c`, `tools/extract-res.py`, `bios_import.rs` | **Implemented · Tested · Verified** | Exact `ps2expand.c` LZ stream decompression, `ROMDIR` discovery, PSMCT16/32 decoding, FIPS 180-4 SHA-256 hashing, and 1,500-case bounded fuzz verification. |
| Clean-Room Web Audio Synthesizer | `docs/sound.md`, `src/audio/audioEngine.ts` | **Implemented · Tested · Approximate** | Original procedural Web Audio harmonic pads and crystalline navigation chimes unlocked synchronously on user gesture; zero Sony `SNDIMAGE` ADPCM samples bundled. |
| WebGPU Renderer (`Dawn + SwiftShader Vulkan`) & Canvas 2D Fallback | `src/renderer/webgpuRenderer.ts`, `fallbackCanvasRenderer.ts` | **Implemented · Tested · Verified** | WGSL compiles with zero warnings/errors; pixel readback verified across 7 golden scenes on Chromium 153 Dawn Vulkan (`isFallbackAdapter: true`) plus Canvas 2D fallback. |
| Physical Hardware GPU Execution (`isFallbackAdapter: false`) | `tests/e2e/webgpu-hardware.spec.ts` | **Blocked** (in sandbox) | Container lacks `/dev/dri`; hardware gate is wired in `npm run test:e2e:hardware-gpu` and `.github/workflows/ci.yml`. |
| External `.ps2` Memory Card Save-Icon 3D Viewer & Full SPU2 Hardware Emulation | `TODO.MD` | **Deferred** | Documented under Deferred Features in `TODO.MD`. |
