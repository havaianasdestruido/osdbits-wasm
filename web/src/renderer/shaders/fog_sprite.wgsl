// PS2 OSDSYS Fog, Orb Sprite & Flare Shader (`fog_sprite.wgsl`)
// Handles textured and untextured atmospheric cloud layers (`DrawFog`, `DrawIllegalFog`),
// additive orb/flare sprites (`DrawLights`, `DrawRedFlare`, `DrawOrbs`), `KABE` cylinder
// tunnel (`menuback.c`), and `DrawVignette` (`menu.c`).

struct DrawParams {
  // x: is_textured (1.0 or 0.0), y: tex_alpha_scale, z: uv_clamp, w: reserved
  params: vec4f,
};

@group(0) @binding(0) var<uniform> u_draw: DrawParams;
@group(0) @binding(1) var t_tex: texture_2d<f32>;
@group(0) @binding(2) var s_tex: sampler;

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
  let uv = select(raw_uv, clamp(raw_uv, vec2f(0.0), vec2f(1.0)), u_draw.params.z > 0.5);
  let texel = textureSample(t_tex, s_tex, uv);

  let gs_rgb = in.gs_rgba.rgb / 128.0;
  let gs_a = in.gs_rgba.a / 128.0;

  let mod_rgb = mix(gs_rgb, texel.rgb * gs_rgb, u_draw.params.x);
  let tex_alpha = mix(1.0, texel.a * u_draw.params.y, u_draw.params.x);
  let out_a = clamp(tex_alpha * gs_a, 0.0, 1.0);

  return vec4f(clamp(mod_rgb, vec3f(0.0), vec3f(1.0)), out_a);
}
