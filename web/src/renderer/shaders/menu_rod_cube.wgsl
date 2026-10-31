// PS2 OSDSYS Module U Crystal Clock Rods & Config Glass Cubes Shader (`menu_rod_cube.wgsl`)
// Executes `menuconfig.c` multi-pass screen refraction (`CapturedScreen`), `TEXCFLOW` + `TEXCBUMP`
// + `TEXCBINV` (`~TEXCBUMP`) emboss sparkle, and `TEXCREFA` spherical reflection.

struct DrawParams {
  // x: is_textured, y: tex_alpha_scale, z: uv_clamp, w: reserved
  params: vec4f,
};

@group(0) @binding(0) var<uniform> u_draw: DrawParams;
@group(0) @binding(1) var t_crystal: texture_2d<f32>;
@group(0) @binding(2) var s_crystal: sampler;

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
  let uv = select(raw_uv, clamp(raw_uv, vec2f(0.002), vec2f(0.998)), u_draw.params.z > 0.5);
  let texel = textureSample(t_crystal, s_crystal, uv);

  let gs_rgb = in.gs_rgba.rgb / 128.0;
  let gs_a = in.gs_rgba.a / 128.0;
  let rgb = clamp(texel.rgb * gs_rgb, vec3f(0.0), vec3f(1.0));
  let out_a = clamp(texel.a * u_draw.params.y * gs_a, 0.0, 1.0);
  return vec4f(rgb, out_a);
}
