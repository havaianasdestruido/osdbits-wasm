// PS2 OSDSYS Gouraud Line Trail Shader (`line_trail.wgsl`)
// Renders Opening 128-point light trails (`DrawLights`) and Module U 150-subframe
// `(age^4, age^2, age)` RGB-decaying orb trails (`DrawOrbs`) using linear screen-space Gouraud shading.

struct DrawParams {
  params: vec4f,
};

@group(0) @binding(0) var<uniform> u_draw: DrawParams;

struct VertexIn {
  @location(0) clip_pos: vec4f,
  @location(1) uv_q: vec4f,
  @location(2) gs_rgba: vec4f,
};

struct VertexOut {
  @builtin(position) position: vec4f,
  @location(0) @interpolate(linear) gs_rgba: vec4f,
};

@vertex
fn vs_main(in: VertexIn) -> VertexOut {
  var out: VertexOut;
  out.position = in.clip_pos;
  out.gs_rgba = in.gs_rgba;
  return out;
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4f {
  let _unused = u_draw.params.x;
  let rgb = clamp(in.gs_rgba.rgb / 128.0, vec3f(0.0), vec3f(1.0));
  let a = clamp(in.gs_rgba.a / 128.0, 0.0, 1.0);
  return vec4f(rgb, a);
}
