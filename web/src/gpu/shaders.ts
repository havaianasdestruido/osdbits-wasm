/**
 * WGSL for every pipeline.
 *
 * One vertex/fragment pair serves all three primitive kinds, switched by the
 * per-batch uniform:
 *   mode 0 — world triangles (towers, fog, flare fans, cube faces)
 *   mode 1 — world-space billboards (one instance per sprite record)
 *   mode 2 — screen-space sprites (one instance per record, pixels)
 *
 * The projection reproduces the reconstruction's view-screen matrix without a
 * mat4 uniform for the projection: x_ndc = ax*scrz*x / (z * w/2), which is the
 * GS `sx = ax*scrz*(x/z)` divided by the half screen width. Clipping is left to
 * the rasteriser (`w = view z`), so geometry behind the camera is culled by the
 * hardware instead of wrapping around.
 */

export const SHADER = /* wgsl */ `
struct Batch {
  view0: vec4f,
  view1: vec4f,
  view2: vec4f,
  view3: vec4f,
  // width, height, scrz, ax
  p0: vec4f,
  // ay, mode, texOn, unused
  p1: vec4f,
};

@group(0) @binding(0) var<uniform> b: Batch;
@group(0) @binding(1) var tex: texture_2d<f32>;
@group(0) @binding(2) var samp: sampler;

struct VSIn {
  @location(0) pos: vec3f,
  @location(1) uv: vec2f,
  @location(2) rgba: vec4f,
  @location(3) aux: f32,
};

struct VSOut {
  @builtin(position) clip: vec4f,
  @location(0) uv: vec2f,
  @location(1) color: vec4f,
  @location(2) @interpolate(flat) texOn: f32,
};

fn view_of(p: vec3f) -> vec4f {
  let v = vec4f(p, 1.0);
  return vec4f(
    dot(b.view0, v),
    dot(b.view1, v),
    dot(b.view2, v),
    dot(b.view3, v)
  );
}

// Project a view-space point (z forward) to clip space with w = z.
fn clip_of_view(v: vec3f) -> vec4f {
  let width = max(b.p0.x, 1.0);
  let height = max(b.p0.y, 1.0);
  let scrz = b.p0.z;
  let ax = b.p0.w;
  let ay = b.p1.x;
  let near = 0.5;
  return vec4f(
    ax * scrz * v.x * 2.0 / width,
    -ay * scrz * v.y * 2.0 / height,
    v.z - near,
    v.z
  );
}

@vertex
fn vs_main(input: VSIn, @builtin(vertex_index) vi: u32) -> VSOut {
  let mode = b.p1.y;
  var out: VSOut;
  out.uv = input.uv;
  out.color = input.rgba;
  out.texOn = b.p1.z;

  if (mode < 0.5) {
    // World triangle: position is already the world-space vertex.
    out.clip = clip_of_view(view_of(input.pos).xyz);
  } else if (mode < 1.5) {
    // Billboard: uv carries the half sizes, aux the roll.
    let v = view_of(input.pos);
    let hx = input.uv.x;
    let hy = input.uv.y;
    let c = cos(input.aux);
    let s = sin(input.aux);
    var corner = vec2f(-hx, -hy);
    if (vi == 1u) { corner = vec2f(hx, -hy); }
    if (vi == 2u) { corner = vec2f(-hx, hy); }
    if (vi == 3u) { corner = vec2f(hx, hy); }
    let rotated = vec2f(corner.x * c - corner.y * s, corner.x * s + corner.y * c);
    out.clip = clip_of_view(vec3f(v.x + rotated.x, v.y + rotated.y, v.z));
    out.uv = vec2f(corner.x / (2.0 * hx) + 0.5, 0.5 - corner.y / (2.0 * hy));
  } else {
    // Screen sprite: x/y are pixels, uv the size in pixels, no depth.
    let centre = input.pos.xy;
    let size = input.uv;
    var corner = vec2f(-0.5, -0.5);
    if (vi == 1u) { corner = vec2f(0.5, -0.5); }
    if (vi == 2u) { corner = vec2f(-0.5, 0.5); }
    if (vi == 3u) { corner = vec2f(0.5, 0.5); }
    let px = centre + corner * size;
    let ndc = vec2f(px.x / (b.p0.x * 0.5) - 1.0, 1.0 - px.y / (b.p0.y * 0.5));
    out.clip = vec4f(ndc, 0.0, 1.0);
    out.uv = vec2f(corner.x + 0.5, corner.y + 0.5);
  }
  return out;
}

@fragment
fn fs_main(input: VSOut) -> @location(0) vec4f {
  var colour = input.color;
  if (input.texOn > 0.5) {
    colour = textureSample(tex, samp, input.uv) * input.color;
  }
  return colour;
}
`;
