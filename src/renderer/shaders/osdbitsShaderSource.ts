export const OSDBITS_WGSL_SOURCE = `// osdbits.wgsl — Clean-room WebGPU shader suite for PlayStation 2 OSDSYS reconstruction.
//
// Implements:
// 1. Perspective-correct clip-space batch rendering (\`vs_main\` / \`fs_main\`) with optional
//    PS2 GS \`2x\` texture modulation (\`flags & 1u\`).
// 2. Full-screen presentation blit (\`vs_blit\` / \`fs_blit\`) for aspect-accurate letterboxing.
// 3. 4-tap bilinear box-filter downsample (\`vs_blit\` / \`fs_downsample\`) for \`ExtraBloom\`
//    and \`MenuZoomBlur\` glow passes.

struct VertexIn {
  @location(0) clip_pos: vec4<f32>,
  @location(1) uv: vec2<f32>,
  @location(2) color: vec4<f32>,
  @location(3) flags: u32,
};

struct VertexOut {
  @builtin(position) position: vec4<f32>,
  @location(0) uv: vec2<f32>,
  @location(1) color: vec4<f32>,
  @location(2) @interpolate(flat) flags: u32,
};

@group(0) @binding(0) var s_diffuse: sampler;
@group(0) @binding(1) var t_diffuse: texture_2d<f32>;

@vertex
fn vs_main(in: VertexIn) -> VertexOut {
  var out: VertexOut;
  out.position = in.clip_pos;
  out.uv = in.uv;
  out.color = in.color;
  out.flags = in.flags;
  return out;
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
  let tex = textureSample(t_diffuse, s_diffuse, in.uv);
  let gain = select(1.0, 2.0, (in.flags & 1u) != 0u);
  return clamp(tex * in.color * gain, vec4<f32>(0.0), vec4<f32>(1.0));
}

struct BlitOut {
  @builtin(position) position: vec4<f32>,
  @location(0) uv: vec2<f32>,
};

@vertex
fn vs_blit(@builtin(vertex_index) vi: u32) -> BlitOut {
  var out: BlitOut;
  let x = f32(i32(vi & 1u) * 4 - 1);
  let y = f32(i32(vi & 2u) * 2 - 1);
  out.position = vec4<f32>(x, y, 0.0, 1.0);
  out.uv = vec2<f32>((x + 1.0) * 0.5, (1.0 - y) * 0.5);
  return out;
}

@fragment
fn fs_blit(in: BlitOut) -> @location(0) vec4<f32> {
  return textureSample(t_diffuse, s_diffuse, in.uv);
}

@fragment
fn fs_downsample(in: BlitOut) -> @location(0) vec4<f32> {
  let dims = vec2<f32>(textureDimensions(t_diffuse));
  let inv = 1.0 / max(dims, vec2<f32>(1.0, 1.0));
  let c0 = textureSample(t_diffuse, s_diffuse, in.uv + vec2<f32>(-0.5, -0.5) * inv);
  let c1 = textureSample(t_diffuse, s_diffuse, in.uv + vec2<f32>(0.5, -0.5) * inv);
  let c2 = textureSample(t_diffuse, s_diffuse, in.uv + vec2<f32>(-0.5, 0.5) * inv);
  let c3 = textureSample(t_diffuse, s_diffuse, in.uv + vec2<f32>(0.5, 0.5) * inv);
  return (c0 + c1 + c2 + c3) * 0.25;
}
`;
