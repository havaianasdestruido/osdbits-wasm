# Porting notes

What was ported, how faithfully, and where this build knowingly deviates.
Status vocabulary: **Implemented** (code exists), **Tested** (automated test
passes here), **Verified** (checked against an independent reference),
**Approximate** (deliberately not exact), **Blocked** (cannot be done here),
**Deferred** (out of scope for now).

## Sources of truth

* `osdbits/opening.c` (4355 lines) — the reconstruction of the opening,
  illegal-disc and menu scenes; symbol addresses in comments.
* `docs/towers-analysis.md` (3776 lines) — the VU1 tower pipeline.
* `docs/glass-refraction.md`, `docs/sound.md`, `docs/pad.md`, `docs/menu-*.md`,
  `docs/rom-survey.md`, `docs/osdsys-map.md`.
* `aap/freesce` `sdk-2.4` — `libvu0.c` / `libvu0.h` for the exact matrix and
  lighting semantics (fetched through `api.github.com` during development; the
  sourced facts are recorded in the code comments).

## Frame loop (Implemented, Tested)

`DoOpeningIllegal` is reproduced exactly:

```
while openingType != 2:
    ProcessOpening()        // camera state machine + a sceneState bump on a type change
    DoOpening() | DoIllegalDisc()   // case 0 init (+fallthrough), case 1 draw, case 2 end
    DrawEnd()               // DoText, black bars, frameCount++
    openingType = nextOpeningType
```

The `case 0 → sceneState++` *fallthrough* is preserved, so the first draw of a
scene happens on the same frame as its init.

## Known deviations (all deliberate)

1. **VU1 microcode is not emulated** (`D-001`). The tower geometry, per-vertex
   colours and ST coordinates are computed in Rust from the decoded DMA chain
   (`osdbits/towerchain.dsm`) instead of running `vucode_1`. The frustum cull,
   the `angleZ = cell + sway` rotation and the "near vertices lit / far vertices
   black" Gouraud scheme are reproduced; the exact per-vertex lighting result is
   **Approximate**: the analysis derives ~0.966 for the side walls and ~1.4 for
   the cap, and those two constants are used instead of the full four-light dot
   product, because the exact per-face modulation (±4% sway) is below the
   visible threshold at 640×224.
2. **Cube passes are collapsed** (Approximate). The GS runs ten passes per face
   through a captured framebuffer ping-pong; the browser renderer draws the lit
   front faces once. The pass *table* is still exported through the ABI and
   tested, so the data is available for a future faithful implementation.
3. **Text is typographic** (`D-004`). The fade state machines are exact
   (Tested); the glyphs are DOM text, not the BIOS bitmap.
4. **Audio is synthesised** (`D-003`/`D-006`). Only the event timing is ported:
   `OSDDispatch(20500/20501)` become `CameraEvent::PlayRecord/FadeRecord`, and
   the shell maps them onto its own drone/sweep/bell.
5. **The fog texture set is procedural**, so the *look* of the additive layers
   differs from the original clouds even though the geometry, depth offsets,
   per-layer scroll speeds and vertex colours are exact.
6. **`DrawExtraBuf2`/`DrawToExtraBuf2`** (the frame-feedback composite that the
   cubes sample) are not reproduced; see (2).

## Parameters: provenance

Each constant in the core carries where it came from:

* **Sourced** — a literal in `osdbits/opening.c` or a table in `docs/*`:
  camera levels `[16,56,104,320,672,800,1160,1160]`, `timestep 1.2`/`1.0`,
  `accel2.z = -0.0178`, `speed.z = 2.1599939`, `rot speed 0.00462`, fog layer
  depths `l*5`, light alphas `[24, 12]`, cube half extent 1.8/1.2, particle
  fall `trunc((i+1)*0.02 + 1.2)`, fade ramps, flare intensity
  `(740-(1160-z))*128/740*0.6`.
* **Measured** — values derived by running the reconstruction's own arithmetic
  in a test and asserting the result (e.g. fog layer phases, trail alphas).
* **Approximated** — the two tower lighting constants, the cube compositing, the
  texture contents.
* **Unknown** — anything the upstream notes mark as unverified (the illegal
  scene's exact ordering was never checked against GS dumps; see
  `docs/gscmp-FINDINGS.md`).

## Environment constraints hit while porting

* crates.io is unreachable → the core has **zero dependencies**; property tests
  are hand-rolled with a seeded LCG (`crates/osdbits-core/tests/properties.rs`).
* No rustup → `scripts/bootstrap-rust.sh` assembles 1.88.0 from npm packages.
* No browser binaries → Playwright specs exist but have never run here
  (`docs/TEST-EVIDENCE.md`).
* No EE toolchain / SDK → the original PS2 build cannot be reproduced, so
  fidelity claims are inherited from upstream and the browser port is verified
  against its own determinism plus the documented parameters.
