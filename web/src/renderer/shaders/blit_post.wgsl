// PS2 OSDSYS Offscreen Capture, Temporal ExtraBuf2 Glow, Motion Blur & Menu Zoom Blur (`blit_post.wgsl`)
// Covers:
// - mode 0: `CubeCaptureBuffer` (`PSMCT24` + `TEXA{0x7f, AEM=1}`: non-black alpha = `127/128`, black = `0`)
// - mode 1: Scaled/zoomed UV blit with PS2 color/alpha modulation (`ExtraBuf2`, `MotionBlur`, `MenuZoomBlur`, `BackdropTint`)
// - mode 2: Final swapchain presentation blit (with aspect-ratio viewport framing & forced opaque alpha = `1.0`)

struct BlitParams {
  // xy: uv_offset, zw: uv_scale
  uv_rect: vec4f,
  // rgb: PS2 GS tint (128 = 1.0), a: PS2 GS alpha (128 = 1.0)
  gs_color: vec4f,
  // x: mode (0 = capture_texa, 1 = modulated_blit, 2 = present_opaque), yzw: reserved
  mode_vec: vec4f,
};

@group(0) @binding(0) var<uniform> u_blit: BlitParams;
@group(0) @binding(1) var t_src: texture_2d<f32>;
@group(0) @binding(2) var s_src: sampler;

struct VsOut {
  @builtin(position) position: vec4f,
  @location(0) uv: vec2f,
};

@vertex
fn vs_main(@builtin(vertex_index) vid: u32) -> VsOut {
  var out: VsOut;
  let x = select(-1.0, 3.0, vid == 1u);
  let y = select(-1.0, 3.0, vid == 2u);
  out.position = vec4f(x, y, 0.0, 1.0);
  let base_uv = vec2f((x + 1.0) * 0.5, (1.0 - y) * 0.5);
  out.uv = u_blit.uv_rect.xy + base_uv * u_blit.uv_rect.zw;
  return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4f {
  let c = textureSample(t_src, s_src, clamp(in.uv, vec2f(0.0), vec2f(1.0)));
  let mode = u_blit.mode_vec.x;

  if (mode < 0.5) {
    // Mode 0: PSMCT24 + TEXA{0x7f, AEM=1}
    let non_black = (c.r + c.g + c.b) > 0.001;
    let a = select(0.0, 127.0 / 128.0, non_black);
    return vec4f(c.rgb, a);
  }

  if (mode < 1.5) {
    // Mode 1: PS2 GS modulated feedback / zoom blur / backdrop tint
    let rgb = clamp(c.rgb * (u_blit.gs_color.rgb / 128.0), vec3f(0.0), vec3f(1.0));
    let a = clamp(u_blit.gs_color.a / 128.0, 0.0, 1.0);
    return vec4f(rgb, a);
  }

  // Mode 2: Final swapchain presentation
  let rgb = clamp(c.rgb * (u_blit.gs_color.rgb / 128.0), vec3f(0.0), vec3f(1.0));
  return vec4f(rgb, 1.0);
}
