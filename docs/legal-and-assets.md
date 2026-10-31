# Legal, Clean-Room Asset, and User-Import Policy

## 1. Core Principles

`osdbits-wasm` is a clean-room technical study and browser-native visual reconstruction inspired by the PlayStation 2 OSDSYS interface documented in `aap/osdbits`.

1. **Zero Proprietary Sony Data in Repository:**
   - No Sony BIOS images (`SCPH-*.bin`, `.rom`, `.mec`, `.nvm`), `ROMDIR` containers (`TEXIMAGE`, `FNTIMAGE`, `SNDIMAGE`, `OSDSYS`), extracted `osdbits/res/*.inc` tables, proprietary font atlases, or copyrighted SPU2 sound samples may ever be committed to git, stored in test fixtures, or bundled in static build artifacts.
   - `scripts/check-repo-hygiene.mjs` runs automatically during verification and CI to enforce absence of `ROMDIR` headers (`RESET\0\0\0\0\0`), `.inc` dumps, BIOS binaries, and Sony trademark strings in default user-visible UI templates.

2. **Honest Legal Posture:**
   - Noncommercial, educational, or nostalgic intent does **not** automatically eliminate copyright or trademark risk. Accordingly, `osdbits-wasm` avoids distributing proprietary assets, avoids Sony logos/wordmarks in its default presentation, and makes no legal representation or warranty regarding third-party files supplied by end users.

3. **Zero Telemetry, Zero Monetization, Zero External Calls:**
   - The application runs 100% client-side in the user's browser.
   - No analytics, ads, tracking pixels, external CDNs, or remote asset servers are contacted at runtime.

---

## 2. Clean-Room Default Asset Pipeline (No BIOS Required)

Out of the box—including single-click startup and every automated test—`osdbits-wasm` generates all required runtime assets deterministically inside `crates/osdbits-core`:

| Asset Role | Clean-Room Default Implementation | Fidelity Status |
| :--- | :--- | :--- |
| Tower surface (`TEXOTOWR`) | Procedural 128×128 brushed architectural slate/granite noise + subtle panel seams | `Approximate` (clean-room procedural) |
| Opening cube layers (`TEXOBLPR`, `TEXOBLP`, `TEXOREF`) | Procedural 64×64 indigo/violet plasma gradients & crisp diagonal specular band | `Approximate` (clean-room procedural) |
| Atmospheric fog & lens flare (`TEXOFOG`, `TEXOFLAR`) | Procedural 64×64 Gaussian/cosine cloud puff & multi-ring hexagonal/radial flare | `Approximate` (clean-room procedural) |
| Config tunnel (`TEXCKABE`) | Procedural 64×64 seamless turbulent smoke/caustic tile (tiled 2×2 to 128×128) | `Approximate` (clean-room procedural) |
| Orb sprites (`TEXCBLUR`, `TEXCNAVI`) | Procedural 64×64 soft radial glow & bright star core | `Approximate` (clean-room procedural) |
| Config glass maps (`TEXCBUMP`, `TEXCBINV`, `TEXCFLOW`, `TEXCREFA`) | Procedural 64×64 bevel/emboss map, exact bitwise RGB complement `~TEXCBUMP`, diagonal flow gradient, and spherical highlight map | `Verified` complement law; `Approximate` procedural art |
| Rod & Config Cube Meshes (`MENUGEOM`) | Synthesized from exact mathematical dimensions documented in `docs/menu-config.md` (16-quad bevelled hexagonal prism `r=2.6, h=26, bevel=2.34@26.39`; 6-quad `±2.64` cube) | `Verified` geometry math |
| OSD Font & Button Glyphs | Clean-room proportional sans-serif UI glyph atlas + basic geometric circle/cross/triangle/square outlines | `Approximate` (clean-room procedural) |
| Opening Banner Text | Conservative non-infringing clean-room banner (`"OSDSYS // WASM"` or hidden) — never Sony trademarks | `Approximate` (legal safeguard) |
| Ambient & UI Audio | Clean-room Web Audio procedural synthesis (warm pad chord, crystalline bell partials, low warning drone, menu tick/confirm/back chimes) | `Approximate` (clean-room procedural) |

---

## 3. Optional User-Controlled Local Asset Import

Users who legally own PlayStation 2 hardware and their own dumped BIOS or `TEXIMAGE` container may optionally import it locally via the **Persistence & Assets** panel:

1. **Local-Only Processing:** Selected files are read via the browser `File` API, validated and decompressed inside WASM (`bios_import.rs` `ROMDIR` + bounds-checked LZ `expand()`), hashed with SHA-256 and FNV-1a, and stored solely in the user's local browser storage (OPFS with IndexedDB fallback).
2. **Strict Validation & Safety:**
   - Enforces file size caps, `ROMDIR` entry table bounds, and LZ output buffer bounds.
   - Corrupt, truncated, or unrecognized files fail gracefully with a clear user-facing diagnostic message while preserving existing procedural/persisted state.
3. **Instant Reversible Reset:** Users can switch back to Clean-Room Procedural Assets or purge imported data at any time via **Reset / Purge Imported Assets**.
