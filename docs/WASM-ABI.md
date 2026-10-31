# WASM ABI (version 1)

The core is a plain `cdylib` with `#[no_mangle] extern "C"` exports: no
`wasm-bindgen`, no generated glue, no imports. `osd_abi_version()` must match
`ABI_VERSION` in `web/src/abi.ts`; the loader refuses to run on a mismatch.

All pointer-returning functions return `usize` (an `i32` address in wasm32) and
are only valid until the next call that rebuilds the buffer.

## Exports

| Export | Purpose |
|---|---|
| `osd_abi_version() -> u32` | must be `1` |
| `osd_init(mode, seed) -> u32` | `0` ok, `1` state error, `2` bad argument |
| `osd_reset(mode, seed) -> u32` | alias of `osd_init` |
| `osd_set_option(id, value) -> u32` | applies to the **next** `osd_init` |
| `osd_get_option(id) -> f32` | read-back (NaN for unknown ids) |
| `osd_step() -> u32` | advance one simulation frame, returns the new frame index |
| `osd_draw() -> u32` | build the frame, returns its length in `f32` words |
| `osd_frame_ptr() -> usize` | the frame buffer |
| `osd_hash_lo()`, `osd_hash_hi()` | the frame's FNV-1a hash |
| `osd_state(id) -> f32` | one simulation scalar (below) |
| `osd_event_count()`, `osd_event_kind(i)`, `osd_event_arg(i)` | queued camera events |
| `osd_pass_table_ptr() -> usize` | 240 floats: 10 opening + 10 illegal cube passes |
| `osd_last_error_ptr()`, `osd_last_error_len()` | last panic/init message |
| `osd_header_words()`, `osd_batch_words()`, `osd_vertex_words()`, `osd_cube_words()` | layout constants |
| `osd_fps() -> u32` | 60 (NTSC) or 50 (PAL) |
| `osd_install_panic_hook()` | records panic text instead of trapping silently |

### `osd_set_option` ids

`0` PAL · `1` games · `2` boots · `3` full field · `4` hardware state
(`0` none, `1` PS2 disc, `2` illegal disc) · `5` boot parameter (100..116) ·
`6` fog · `7` lights · `8` cubes · `9` camera motion · `10` text ·
`11` 16:9 letterbox bars.

### `osd_state` ids

`0` frame · `1` camera z · `2` opening state · `3` opening type · `4` scene kind
(0 opening, 1 illegal, 2 none) · `5` ended · `6` SCE text alpha · `7` illegal
text alpha · `8` flare intensity · `9` lit towers · `10` fade alpha ·
`11` scene state · `12` light seed · `13` frame words · `14` batches ·
`15` vertices · `16` cubes · `17` width · `18` height.

## Frame format (version 1)

One flat `Float32Array` per frame. Integer fields stored as bit patterns are
marked **u32** and must be read with a `Uint32Array` view (`Float32Array` would
turn them into denormals).

### Header — 64 words

| Word | Field |
|---|---|
| 0 | magic `0x4F534442` ("OSDB") **u32** |
| 1 | version `1` **u32** |
| 2 | frame index |
| 3 | mode **u32** (0 boot, 1 idle, 2 illegal, 3 menu) |
| 4–6 | opening state, opening type, scene kind |
| 7 | flags **u32** (`1` end latch, `2` black bars, `4` illegal warm, `8` ended, `16` PAL) |
| 8–19 | camera position / up / forward (vec4 each) |
| 20–35 | view matrix, row-major |
| 36–41 | `scrz`, `ax`, `ay`, rotation, virtual width, virtual height |
| 42–44 | batch, vertex, cube counts |
| 45–47 | section offsets (batches = 64) |
| 48 | total words |
| 49–50 | SCE / illegal text alpha (0..112) |
| 51–53 | hardware state **u32?** no — f32; light seed; fade alpha 0..1 |
| 54–58 | end frame, end flag, flare intensity, red flare, scene state |
| 59 | index into the batch section where cubes belong **u32** |

### Batch record — 8 words

`prim` (0 sprites, 1 triangles, 2 lines, 3 screen sprites) · `tex` (0 none,
1 wall, 2–6 fog, 7 ref, 8 blpr, 9 blp, 10 flar, 11 crbl, 12 crle, 13 screen,
14 work) · `blend` (0 alpha, 1 subtract, 2 add, 3 multiply, 4 fixed-alpha,
5 add-by-dest-alpha, 100 none) · `flags` · `vertexOffset` · `vertexCount` ·
reserved ×2.

### Vertex — 10 words

`x y z u v r g b a aux`.

* triangles: `xyz` world position, `uv` texture coordinates, `rgba` colour,
  `aux` unused.
* sprites: `xyz` world centre, `uv` **half sizes in world units**, `aux` roll
  in radians; the renderer builds a camera-facing quad.
* screen sprites: `xy` centre in virtual-screen pixels, `uv` **size in pixels**,
  no depth.
* lines: one record per trail sample, `u` = line width in pixels.

### Cube record — 128 words

`index`, `visible`, 6 face signs, base colour (3), screen centre (2),
8 `q` values, 8 corners ×4, 8 view directions ×3, 6 screen normals ×4,
6 faces ×4 light values, padding.

## Blend semantics

The GS ALPHA register is `((A - B) * C) >> 7 + D` with A/B selecting a colour
(0 = Cs, 1 = Cd, 2 = zero) and C an alpha source (0 = As, 1 = Ad, 2 = FIX), which
makes the reconstruction's `BlendModes[]` table:

| mode | A,B,C,D | Meaning | Renderer mapping |
|---|---|---|---|
| 0 | 0,2,2,1 | `Cs*FIX + Cd` | additive |
| 1 | 2,0,2,1 | `Cd - Cs*FIX` | reverse-subtract |
| 2 | 0,1,2,1 | `(Cs-Cd)*FIX + Cd` | `src-alpha` blend (approximation) |
| 3 | 1,2,2,0 | multiply | `dst * src` |
| 4 | 0,1,0,1 | `(Cs-Cd)*As + Cd` | standard alpha blend |
| 5 | 0,2,0,1 | `Cs*As + Cd` | additive |
| 8 | 0,2,1,1 | `Cs*Ad + Cd` | additive (approximated) |

`blend_for_mode()` in `cubes.rs` is the single source of truth; the TS mapping
lives in `web/src/abi.ts` (frame ids) and `web/src/gpu/renderer.ts`
(`blendState`).
