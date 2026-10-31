// Node smoke test for the wasm build. NOT a browser test: it proves the
// module instantiates, the ABI answers, and the draw list is well formed and
// deterministic. WebGPU/browser behaviour is covered (capability-gated) by the
// Playwright specs, which cannot run without browser binaries — see
// docs/TEST-EVIDENCE.md.
//
// Usage: node scripts/wasm-smoke.mjs [path-to-wasm]

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";

const here = dirname(fileURLToPath(import.meta.url));
const wasmPath =
  process.argv[2] ??
  resolve(here, "../crates/osdbits-core/target/wasm32-unknown-unknown/release/osdbits_core.wasm");

const failures = [];
function check(name, cond, detail = "") {
  if (cond) {
    console.log(`ok    ${name}`);
  } else {
    failures.push(`${name}${detail ? ` (${detail})` : ""}`);
    console.log(`FAIL  ${name}${detail ? ` (${detail})` : ""}`);
  }
}

const bytes = readFileSync(wasmPath);
const { instance } = await WebAssembly.instantiate(bytes, {});
const X = instance.exports;
const memory = X.memory;

check("wasm exposes memory", memory instanceof WebAssembly.Memory);
for (const name of [
  "osd_abi_version",
  "osd_init",
  "osd_step",
  "osd_draw",
  "osd_frame_ptr",
  "osd_set_option",
  "osd_state",
  "osd_pass_table_ptr",
]) {
  check(`exports ${name}`, typeof X[name] === "function");
}

const f32 = () => {
  // Re-derive the view each time: memory can grow, detaching old views.
  return new Float32Array(memory.buffer);
};

check("ABI version is 1", X.osd_abi_version() === 1);
check("header is 64 words", X.osd_header_words() === 64);
check("batch record is 8 words", X.osd_batch_words() === 8);
check("vertex record is 10 words", X.osd_vertex_words() === 10);
check("cube record is 128 words", X.osd_cube_words() === 128);

// ---------------------------------------------------------------- boot scene
X.osd_install_panic_hook?.();
X.osd_set_option(0, 0); // PAL off
X.osd_set_option(6, 1); // fog on
X.osd_set_option(7, 1); // lights on
X.osd_set_option(8, 1); // cubes on
check("init boot mode", X.osd_init(0, 4242) === 0);

function readFrame() {
  const words = X.osd_draw();
  const ptr = X.osd_frame_ptr() / 4; // f32 index
  const view = f32();
  return { words, frame: view.subarray(ptr, ptr + words) };
}

const MAGIC = 0x4f534442;
let bad = 0;
let maxVerts = 0;
let maxBatches = 0;
const hashes = new Set();
const times = [];

for (let i = 0; i < 900; i++) {
  const t0 = performance.now();
  X.osd_step();
  const { words, frame } = readFrame();
  times.push(performance.now() - t0);

  const magic = new Uint32Array(frame.buffer, frame.byteOffset, 1)[0];
  if (magic !== MAGIC) bad++;
  if (Math.abs(frame[48] - words) > 0.5) bad++;
  const batchCount = frame[42];
  const vertexCount = frame[43];
  const offBatches = frame[45];
  const offVerts = frame[46];
  const offCubes = frame[47];
  if (offBatches !== 64) bad++;
  if (offVerts !== 64 + batchCount * 8) bad++;
  if (offCubes !== offVerts + vertexCount * 10) bad++;
  maxVerts = Math.max(maxVerts, vertexCount);
  maxBatches = Math.max(maxBatches, batchCount);
  hashes.add(`${X.osd_hash_hi()}:${X.osd_hash_lo()}`);
}

check("every frame has the magic and consistent offsets", bad === 0, `${bad} bad frames`);
check("frames differ frame to frame", hashes.size > 200, `${hashes.size} distinct hashes`);
check("draw list stays bounded", maxVerts > 0 && maxVerts < 60000, `max ${maxVerts} verts`);
check("batch count stays bounded", maxBatches > 0 && maxBatches < 64, `max ${maxBatches} batches`);
check(
  "boot scene ends",
  X.osd_state(5) === 1,
  `ended=${X.osd_state(5)} z=${X.osd_state(1).toFixed(1)} frame=${X.osd_state(0)}`,
);
check("camera climbed past the field", X.osd_state(1) > 100, `z=${X.osd_state(1)}`);
check("towers were generated", X.osd_state(9) > 0, `lit=${X.osd_state(9)}`);

times.sort((a, b) => a - b);
const pct = (p) => times[Math.min(times.length - 1, Math.floor(times.length * p))];
const p50 = pct(0.5);
const p95 = pct(0.95);
console.log(`      frame time (step+draw, node, wasm): p50 ${p50.toFixed(3)} ms p95 ${p95.toFixed(3)} ms`);
check("p50 frame time under 8 ms", p50 < 8, `${p50.toFixed(3)} ms`);
check("p95 frame time under 16 ms", p95 < 16, `${p95.toFixed(3)} ms`);

// ------------------------------------------------------------- determinism
const run = () => {
  X.osd_init(0, 4242);
  for (let i = 0; i < 300; i++) {
    X.osd_step();
    X.osd_draw();
  }
  return [X.osd_hash_hi(), X.osd_hash_lo(), X.osd_state(1)];
};
const a = run();
const b = run();
check("replay is bit-identical", a[0] === b[0] && a[1] === b[1] && a[2] === b[2], `${a} vs ${b}`);

// ------------------------------------------------------------------- idle
X.osd_init(1, 5);
for (let i = 0; i < 1200; i++) {
  X.osd_step();
  X.osd_draw();
}
check("idle never ends", X.osd_state(5) === 0);
check("idle hovers on the field", X.osd_state(2) === 0, `state=${X.osd_state(2)}`);

// ---------------------------------------------------------------- illegal
X.osd_init(2, 5);
let screenBatches = 0;
let maxIntensity = 0;
for (let i = 0; i < 600; i++) {
  X.osd_step();
  const { frame } = readFrame();
  maxIntensity = Math.max(maxIntensity, frame[56]);
  const n = frame[42];
  for (let b = 0; b < n; b++) {
    if (frame[64 + b * 8] === 3) screenBatches++;
  }
}
check("illegal scene runs", X.osd_state(4) === 1, `kind=${X.osd_state(4)}`);
check("flare intensity ramps", maxIntensity > 10, `max=${maxIntensity.toFixed(2)}`);
check("illegal scene draws screen-space sprites", screenBatches > 0, `${screenBatches}`);

// -------------------------------------------------------------------- PAL
X.osd_set_option(0, 1);
X.osd_init(0, 1);
X.osd_step();
const pal = readFrame().frame;
check("PAL virtual screen is 640x256", pal[40] === 640 && pal[41] === 256);
check("PAL fps is 50", X.osd_fps() === 50);
X.osd_set_option(0, 0);

// -------------------------------------------------------------- pass table
const tablePtr = X.osd_pass_table_ptr() / 4;
const table = f32().subarray(tablePtr, tablePtr + 240);
check("pass table opening fix", table[4] === 122);
check("pass table illegal uv zoom", Math.abs(table[120 + 10] - 0.8) < 1e-6);
check("pass table illegal colour mode", table[120 + 24 + 7] === 4);

// ------------------------------------------------------------- error paths
check("unknown mode is rejected", X.osd_init(42, 1) === 2);
const msgLen = X.osd_last_error_len();
const msg = new TextDecoder().decode(
  new Uint8Array(memory.buffer, X.osd_last_error_ptr(), msgLen),
);
check("panic/error buffer carries a message", msg.includes("unknown mode"), JSON.stringify(msg));
check("bad option id is rejected", X.osd_set_option(999, 1) === 2);

console.log(
  `\n${failures.length === 0 ? "PASS" : "FAIL"}: ${failures.length} failed check(s) out of the suite`,
);
if (failures.length > 0) {
  for (const f of failures) console.log(`  - ${f}`);
  process.exitCode = 1;
}
