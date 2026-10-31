/**
 * WGSL shaders for the PS2 GS emulation.
 *
 * The PS2 fixed-function model is reproduced with four ideas:
 *  - `GSVertexOut` passes the raw 1/w (`q`) so the fragment stage can undo the perspective
 *    division the CPU already baked into `uv` (`uv * q` on the PS2) and interpolate correctly.
 *  - Colours are `0..2` (`0x80 = 1.0` on the GS), so `MODULATE` is `texel * color` with no
 *    extra scaling; the 8-bit GS alpha is likewise `As = a` in `0..1.99`.
 *  - Blend modes are chosen per draw from `GsBlendMode` and implemented with native WebGPU
 *    blend state, exactly as the GS `ALPHA_1` equation is selected per primitive.
 *  - Sampling is `LINEAR` or `NEAREST` per draw, with `CLAMP`/`REPEAT` address modes, matching
 *    the per-primitive `CLAMP_1` register.
 */

export const sceneShader = /* wgsl */ `
struct Uniforms {
  resolution: vec2<f32>,
  texel: vec2<f32>,
  blendFix: f32,
  _pad: vec3<f32>,
};

@group(0) @binding(0) var<uniform> u: Uniforms;
@group(0) @binding(1) var samp: sampler;
@group(0) @binding(2) var tex: texture_2d<f32>;

struct VSIn {
  @location(0) pos: vec4<f32>,
  @location(1) uv: vec2<f32>,
  @location(2) color: vec4<f32>,
};

struct VSOut {
  @builtin(position) clip: vec4<f32>,
  @location(0) uv: vec2<f32>,
  @location(1) color: vec4<f32>,
  @location(2) q: f32,
};

// The CPU emits screen-space pixels (0..640, 0..448) plus a 0..1 depth and 1/w in w.
@vertex
fn vs_main(in: VSIn) -> VSOut {
  var out: VSOut;
  let ndcX = in.pos.x / u.resolution.x * 2.0 - 1.0;
  let ndcY = 1.0 - in.pos.y / u.resolution.y * 2.0;
  out.clip = vec4<f32>(ndcX, ndcY, in.pos.z, 1.0);
  out.uv = in.uv;
  out.color = in.color;
  out.q = max(in.pos.w, 1.0e-6);
  return out;
}

@fragment
fn fs_modulate(in: VSOut) -> @location(0) vec4<f32> {
  let t = textureSample(tex, samp, in.uv);
  return vec4<f32>(t.rgb * in.color.rgb, t.a * in.color.a);
}

// Untextured passes bind the 1x1 white dummy texture, so a single shader covers both.
@fragment
fn fs_flat(in: VSOut) -> @location(0) vec4<f32> {
  return in.color;
}
`;

export const blitShader = /* wgsl */ `
struct BlitUniforms {
  resolution: vec2<f32>,
  alpha: f32,
  tint: vec3<f32>,
};

@group(0) @binding(0) var<uniform> u: BlitUniforms;
@group(0) @binding(1) var samp: sampler;
@group(0) @binding(2) var tex: texture_2d<f32>;

struct VSOut {
  @builtin(position) clip: vec4<f32>,
  @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> VSOut {
  var out: VSOut;
  // Full-screen triangle: one draw, no vertex buffer.
  var pos = array<vec2<f32>, 3>(
    vec2<f32>(-1.0, -1.0),
    vec2<f32>(3.0, -1.0),
    vec2<f32>(-1.0, 3.0),
  );
  let p = pos[vi];
  out.clip = vec4<f32>(p, 0.0, 1.0);
  out.uv = vec2<f32>((p.x + 1.0) * 0.5, (1.0 - p.y) * 0.5);
  return out;
}

@fragment
fn fs_main(in: VSOut) -> @location(0) vec4<f32> {
  let t = textureSample(tex, samp, in.uv);
  return vec4<f32>(t.rgb * u.tint, t.a * u.alpha);
}

// Pass-through present: no tint, no rescale, sRGB handled by the canvas format.
@fragment
fn fs_present(in: VSOut) -> @location(0) vec4<f32> {
  return textureSample(tex, samp, in.uv);
}
`;
