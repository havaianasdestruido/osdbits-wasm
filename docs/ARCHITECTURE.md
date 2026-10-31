# Browser port architecture — decision 001

Status: design accepted against the inspected source; browser implementation
and runtime validation remain open in `TODO.MD`.

## Single rendering owner

TypeScript owns the WebGPU device, canvas, WGSL pipelines, textures, render
passes and resource disposal. Rust owns deterministic scene state and
render-command preparation, compiled through wasm-bindgen. There will not
be a parallel Rust wgpu renderer. The preserved EE/VU implementation is a
reference and separate target, not a second browser renderer.

The Rust core must build/test natively without DOM, JS globals or GPU APIs.
A thin WASM facade exposes versioned initialization, bounded fixed-step
advance, scene events, seed/restart and frame-state inspection. Typed-array
buffers have documented ownership/lifetime; avoid per-frame JSON and GPU
readbacks. Final ABI and build-tool versions require a demonstrated Rust
and wasm32 build before acceptance.

## Simulation and source fidelity

Port source state machines, PRNG semantics, tower history and f32 math
before tuning visuals. Fixed 60 Hz logical steps reproduce NTSC integration;
PAL behavior is a separately tested policy, not a wall-clock multiplier
applied opportunistically. Clamp/cap catch-up after suspension, reject
nonfinite inputs, keep maximum object counts and explicit scene transitions.
Menu source files are substantial and must be read alongside analysis notes;
there is no basis for discarding them as “notes only.”

Every parameter belongs to an evidence class:

- **Sourced:** exact file/function/address, units, value and conversion.
- **Measured:** authorized reference, pose/seed, measurement method/error.
- **Approximate:** original browser artwork or deliberate adaptation.
- **Unknown:** unresolved semantics; no claim of verified equivalence.

Maintain a parameter ledger while porting. For example, the opening height
thresholds in `ProcessOpeningAnimation` are sourced, but a new WebGPU fog
coefficient is approximate unless derived and compared. A screenshot alone
does not prove timing, hidden state or exact shader parameters.

## Rendering translation

Write and test GS/VU-to-WebGPU transforms explicitly: axis direction,
winding, handedness, clip Z [0,1], viewport/aspect and field-to-progressive
conversion. Keep source units in simulation where practical. Preserve the
ordered tower, feedback, fog, streak and per-object glass snapshot passes.
Document any compositing approximation and cost. Bound DPR/render scale to
device limits; reuse geometry/uniform/storage buffers. Device loss ends the
old loop and releases owned resources before one recovery attempt starts.
No proprietary SDK, GS model library or BIOS is a browser dependency.

## Shell lifecycle

Use states activated by a single initial click: inactive → starting →
running; failures → recoverable error; visibility → suspended. Initiate
`AudioContext.resume()` synchronously in the click handler before awaiting
anything. Reuse a single context; a rejected audio resume must not stop the
visuals. Silent default is lawful and honest. Startup concurrently loads
WASM, validates capabilities/resources and restores settings, then begins
rendering without another button. Guard duplicate activation and stale
async completions with generation IDs. Restart/recovery must not accumulate
RAF loops or listeners.

## Persistence and imports

IndexedDB stores validated versioned settings and resource metadata.
Migration/readback must be exercised in a real browser. Serialize writes or
use revisioned atomic transactions; a save is successful only after readback.
Storage failures surface a degraded in-memory mode. Export/import is bounded
JSON, never executable code. Reset requires confirmation. OPFS is conditional
on large-file evidence, not an ordinary settings dependency. Explain eviction
and denied persistence requests; export is the user's portable backup.

The existing Python extractor is a local user-resource tool, not a browser
runtime requirement. The bounded resource parser establishes synthetic format
regressions for a future Rust validator. The advanced `--tables` helper at
`osdbits/tools/extract-res.py` remains a source-specific trusted local analysis
workflow, not a general untrusted BIOS validator. Preserve its capability and
never ship extracted output. Default browser assets must be procedural or
original; optional authorized imports must be validated before persistence.

## Quality gates

Portable CPU CI validates only what it executes. WASM instantiation,
real-WebGPU passes, audio activation, persistent-profile browser restart,
reference visuals and production-origin tests are distinct mandatory gates.
Mocks do not satisfy GPU or storage persistence acceptance. See the explicit
blocked/open tasks; no CPU-only green check makes this a working web product.
