// PS2 OSDSYS Screen-Space Fade, Letterbox & Emblem Overlay Shader (`ui_overlay.wgsl`)
// Renders fullscreen black/white/tinted fades (`DrawFullScreenFade`), `16:9` / `4:3` letterbox bars,
// clean-room `OSDBITS` Opening emblem (`DrawBannerOverlay`), and Mode C warning banner (`DrawIllegalTextOverlay`).

struct OverlayParams {
  // xy: ndc min, zw: ndc size
  ndc_rect: vec4f,
  // xy: uv min, zw: uv size
  uv_rect: vec4f,
  // rgba in [0..1]
  color: vec4f,
  // x: is_textured (1.0 or 0.0), y: tex_alpha_scale, zw: reserved
  flags: vec4f,
};

@group(0) @binding(0) var<uniform> u_ov: OverlayParams;
@group(0) @binding(1) var t_ov: texture_2d<f32>;
@group(0) @binding(2) var s_ov: sampler;

struct VsOut {
  @builtin(position) position: vec4f,
  @location(0) uv: vec2f,
};

@vertex
fn vs_main(@builtin(vertex_index) vid: u32) -> VsOut {
  var corners = array<vec2f, 6>(
    vec2f(0.0, 0.0),
    vec2f(1.0, 0.0),
    vec2f(0.0, 1.0),
    vec2f(0.0, 1.0),
    vec2f(1.0, 0.0),
    vec2f(1.0, 1.0),
  );
  let p = corners[vid];
  var out: VsOut;
  let ndc = u_ov.ndc_rect.xy + p * u_ov.ndc_rect.zw;
  out.position = vec4f(ndc.x, ndc.y, 0.0, 1.0);
  out.uv = vec2f(u_ov.uv_rect.x + p.x * u_ov.uv_rect.z, u_ov.uv_rect.y + (1.0 - p.y) * u_ov.uv_rect.w);
  return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4f {
  let texel = textureSample(t_ov, s_ov, clamp(in.uv, vec2f(0.0), vec2f(1.0)));
  let rgb = mix(u_ov.color.rgb, texel.rgb * u_ov.color.rgb, u_ov.flags.x);
  let tex_a = mix(1.0, clamp(texel.a * u_ov.flags.y, 0.0, 1.0), u_ov.flags.x);
  return vec4f(clamp(rgb, vec3f(0.0), vec3f(1.0)), clamp(tex_a * u_ov.color.a, 0.0, 1.0));
}
