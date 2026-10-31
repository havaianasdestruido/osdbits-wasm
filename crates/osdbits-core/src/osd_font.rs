//! Clean-room proportional in-canvas OSD text & geometric button-icon emitter.
//!
//! Samples `TextureSlot::UiGlyphAtlas` (`256x128`, 16x8 grid of `16x16` cells:
//! slots `0..=94` -> ASCII `32..=126`, slots `96..=99` -> Circle, Cross, Triangle, Square).

use crate::render_list::{
    BatchKey, BlendMode, DepthMode, RenderList, TargetId, TextureSlot, pass_action,
};

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PadGlyph {
    Circle = 96,
    Cross = 97,
    Triangle = 98,
    Square = 99,
}

fn char_advance(ch: u8, scale: f32) -> f32 {
    let base = match ch {
        b' ' => 5.0,
        b'i' | b'l' | b'I' | b':' | b';' | b'.' | b',' | b'!' => 5.5,
        b'1' | b'(' | b')' | b'[' | b']' => 7.0,
        b'm' | b'w' | b'M' | b'W' => 11.5,
        c if c.is_ascii_lowercase() => 8.2,
        _ => 9.5,
    };
    base * scale
}

#[must_use]
pub fn measure_osd_text(text: &str, scale: f32) -> f32 {
    let mut w = 0.0_f32;
    for b in text.bytes() {
        if (32..=126).contains(&b) {
            w += char_advance(b, scale);
        }
    }
    w
}

#[allow(clippy::too_many_arguments)]
pub fn emit_osd_text(
    rl: &mut RenderList,
    text: &str,
    mut x: f32,
    y: f32,
    scale: f32,
    color: [u8; 4],
    screen_w: f32,
    screen_h: f32,
) {
    rl.begin_batch(BatchKey {
        texture: TextureSlot::UiGlyphAtlas,
        blend: BlendMode::SrcAlpha,
        depth: DepthMode::None,
        action: pass_action::CLAMP_UV,
        target: TargetId::MainScreen,
    });

    let cell_u = 1.0 / 16.0;
    let cell_v = 1.0 / 8.0;
    let draw_w = 13.5 * scale;
    let draw_h = 13.5 * scale;

    for b in text.bytes() {
        if !(32..=126).contains(&b) {
            continue;
        }
        if b != b' ' {
            let slot = (b - 32) as u32;
            let base_u = ((slot % 16) as f32) * cell_u;
            let base_v = ((slot / 16) as f32) * cell_v;
            let u0 = base_u + cell_u * (0.35 / 16.0);
            let v0 = base_v + cell_v * (0.35 / 16.0);
            let u1 = base_u + cell_u * (15.65 / 16.0);
            let v1 = base_v + cell_v * (15.65 / 16.0);
            rl.push_screen_rect(
                x,
                y,
                draw_w,
                draw_h,
                screen_w,
                screen_h,
                [u0, v0, u1, v1],
                color,
                0,
            );
        }
        x += char_advance(b, scale);
    }
}

#[allow(clippy::too_many_arguments)]
pub fn emit_pad_glyph(
    rl: &mut RenderList,
    glyph: PadGlyph,
    x: f32,
    y: f32,
    size: f32,
    color: [u8; 4],
    screen_w: f32,
    screen_h: f32,
) {
    rl.begin_batch(BatchKey {
        texture: TextureSlot::UiGlyphAtlas,
        blend: BlendMode::SrcAlpha,
        depth: DepthMode::None,
        action: pass_action::CLAMP_UV,
        target: TargetId::MainScreen,
    });
    let slot = glyph as u32;
    let cell_u = 1.0 / 16.0;
    let cell_v = 1.0 / 8.0;
    let u0 = ((slot % 16) as f32) * cell_u;
    let v0 = ((slot / 16) as f32) * cell_v;
    rl.push_screen_rect(
        x,
        y,
        size,
        size,
        screen_w,
        screen_h,
        [u0, v0, u0 + cell_u, v0 + cell_v],
        color,
        0,
    );
}
