# Asset policy

This repository reconstructs *behaviour*, not artwork. Nothing derived from a
PlayStation 2 BIOS, a game disc or a commercial soundtrack is committed, built
into a release, or shipped by the deployed app.

## What must never be added

* Extracted BIOS resources: `osdbits/res/*.inc`, `TEXOWAL0`, `TEXOFOG*`,
  `TEXOREF`, `TEXOSCE`, the font pages, `SNDBOOTS`, `SNDWARNS`, and any other
  `SND*` bank (see `docs/sound.md` and `docs/rom-survey.md` for the inventory).
* GS dumps, memory-card images, disc images, ELF/IRX/PSF payloads.
* Screenshots or captures of a real console used as rendering input.
* Any extracted file placed under `web/public/` or bundled by Vite.

`scripts/check-no-proprietary-assets.sh` enforces the mechanical part of this
(PlayStation magics, BIOS resource paths and names, GS dump data files) and
`scripts/check-upstream-untouched.sh` proves the original C sources are still
byte-identical to upstream `ea0c687`.

## What the app ships instead

Every texture and every sound is generated at runtime:

| Original | Substitute | Where |
|---|---|---|
| `TEXOWAL0` tower wall (256², mipmapped) | Procedural vertical streak field | `web/src/textures.ts` (`makeWall`) |
| `TEXOFOG0..4` fog clouds (128²) | Radial gradients with per-layer falloff | `makeCloud` |
| `TEXOFLAR` flare sprite | Radial core + tinted halo | `makeFlare` |
| `TEXOREF` refraction environment | Ring/band interference pattern | `makeRefraction` |
| `TEXOBLP/BLPR/SCREEN/WORK` capture buffers | Soft vertical gradient | `makeBackdrop` |
| `SNDBOOTS` / boot chime | Synthesised drone + noise sweep + bell | `web/src/audio.ts` |
| SCE logo bitmap | Typographic text overlay (DOM) | `web/src/app.ts` + `index.html` |

The substitute sizes match the originals where the UVs assume them (256² wall,
128² fog/flare) so the geometry and the additive layering keep the same
behaviour.

## User-supplied resources

The user-side extraction path (`tools/extract-res.py`, `ps2expand.c`) is
documented but not wired into the browser build: a user who owns a console can
extract their own resources, and a future import path (`TODO.MD P03-008`) would
load them locally in the browser. Nothing is uploaded anywhere, and the default
deployment works with zero user-supplied files.

## Legal notes

* "PlayStation", "PS2" and "Sony Computer Entertainment" are trademarks of Sony
  Interactive Entertainment. This project is not affiliated with, endorsed by or
  sponsored by Sony.
* The text overlay renders the words of the original screen typographically
  (not the original bitmap). It can be switched off in Settings (`text`).
* The reconstruction itself derives from the open-source `aap/osdbits` analysis
  of OSDSYS behaviour; it contains no decompiled Sony code.
