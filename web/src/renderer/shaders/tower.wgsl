// PS2 OSDSYS VU1 Tower Shader (`tower.wgsl`)
// Implements `vucode_1` Gouraud-lit monolith rendering with 6-level mipmapped `TEXOWAL0`,
// PS2 GS `STQ` linear screen-space interpolation, `CLAMP_1` top-cap edge streaking,
// reverse-Z `[0, 1]` depth (`greater-equal`), and `0x80 = 1.0` overbright modulation.

struct DrawParams {
  // x: is_textured, y: tex_alpha_scale, z: uv_clamp, w: reserved
  params: vec4f,
};

@group(0) @binding(0) var<uniform> u_draw: DrawParams;
@group(0) @binding(1) var t_diffuse: texture_2d<f32>;
@group(0) @binding(2) var s_diffuse: sampler;

struct VertexIn {
  @location(0) clip_pos: vec4f,
  @location(1) uv_q: vec4f,
  @location(2) gs_rgba: vec4f,
};

struct VertexOut {
  @builtin(position) position: vec4f,
  @location(0) @interpolate(linear) stq: vec3f,
  @location(1) @interpolate(linear) gs_rgba: vec4f,
};

@vertex
fn vs_main(in: VertexIn) -> VertexOut {
  var out: VertexOut;
  let q = max(in.uv_q.z, 1e-6);
  out.position = in.clip_pos;
  out.stq = vec3f(in.uv_q.xy * q, q);
  out.gs_rgba = in.gs_rgba;
  return out;
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4f {
  let raw_uv = in.stq.xy / max(in.stq.z, 1e-6);
  let clamped_uv = clamp(raw_uv, vec2f(0.001953125), vec2f(0.998046875));
  let uv = mix(fract(raw_uv), clamped_uv, u_draw.params.z);

  let sample_rgba = textureSample(t_diffuse, s_diffuse, uv);
  let gs_rgb_scale = in.gs_rgba.rgb / 128.0;
  let lit_rgb = clamp(sample_rgba.rgb * gs_rgb_scale, vec3f(0.0), vec3f(1.0));
  let tex_a = sample_rgba.a * u_draw.params.y;
  let out_a = clamp(tex_a * (in.gs_rgba.a / 128.0), 0.0, 1.0);
  return vec4f(lit_rgb, out_a);
}
