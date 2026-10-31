//! The scene: combines the camera state machine, tower field, fog, lights,
//! cubes, illegal-disc effects and text overlays into one deterministic
//! simulation, and turns each frame into a draw list.
//!
//! The frame loop mirrors `DoOpeningIllegal` in `osdbits/opening.c`:
//!
//! ```text
//! while openingType != 2:
//!     ProcessOpening()      // camera state machine + motion integration
//!     DoOpening() | DoIllegalDisc()   // scene init (state 0) or draw
//!     DrawEnd()             // DoText, WaitNextFrame, frameCount++
//!     openingType = nextOpeningType
//! ```
//!
//! Determinism: the simulation is a pure function of `(mode, seed, games,
//! boots, full_field, hw state, PAL, frame index)`. There is no wall clock, no
//! entropy source and no iteration over hash maps.

use crate::camera::{Camera, CameraEvent, HwState};
use crate::cubes::Cubes;
use crate::field::{HistEntry, TowerField};
use crate::fog::Fog;
use crate::frame::*;
use crate::illegal::IllegalScene;
use crate::lights::{self, Lights};
use crate::math::{
    apply, clamp, DEG2RAD, VIEW_SCREEN_AY_NTSC, VIEW_SCREEN_AY_PAL, VIEW_SCREEN_DISTANCE,
};
use crate::rng::OsdRand;
use crate::text::TextOverlay;

pub const MODE_BOOT: u32 = 0;
pub const MODE_IDLE: u32 = 1;
pub const MODE_ILLEGAL: u32 = 2;
pub const MODE_MENU: u32 = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SceneKind {
    /// The boot/towers opening (`DoOpening`).
    Opening,
    /// The illegal-disc scene (`DoIllegalDisc`).
    Illegal,
    /// Neither scene is running (menu / ended).
    None,
}

/// Tunables that are *not* part of the reconstruction. Every one defaults to
/// the faithful value.
#[derive(Clone, Copy, Debug)]
pub struct SceneOptions {
    /// PAL (640x256, 50 fps, timestep 1.2) instead of NTSC.
    pub pal: bool,
    /// Number of distinct titles in the simulated memory card (`ngames`).
    pub games: i32,
    /// Boot events applied to the simulated card (`nboots`).
    pub boots: i32,
    /// Light every field cell (the reconstruction's `ngames 0` test mode).
    pub full_field: bool,
    /// Simulated CDVD state.
    pub hw: HwState,
    /// Simulated boot parameter (values 100..116).
    pub boot_param: i32,
    /// Draw the fog layers.
    pub fog: bool,
    /// Draw the light streaks.
    pub lights: bool,
    /// Draw the refractive cubes.
    pub cubes: bool,
    /// Advance the camera state machine (accessibility: reduced motion turns
    /// this off, freezing the camera while the scene keeps animating).
    pub camera_motion: bool,
    /// Keep the SCE text overlay state machine running.
    pub text: bool,
    /// Draw the 16:9 letterbox bars (the "Screen Size: Letterbox" config).
    pub black_bars: bool,
}

impl Default for SceneOptions {
    fn default() -> Self {
        SceneOptions {
            pal: false,
            games: 16,
            boots: 120,
            full_field: false,
            hw: HwState::NoDisc,
            boot_param: 100,
            fog: true,
            lights: true,
            cubes: true,
            camera_motion: true,
            text: true,
            black_bars: false,
        }
    }
}

/// One animated billboard/quad the renderer draws with the same fixed
/// vertex layout as the frame format.
pub struct Scene {
    pub mode: u32,
    pub frame: u32,
    pub options: SceneOptions,
    pub camera: Camera,
    pub field: TowerField,
    pub history: [HistEntry; 21],
    pub fog: Fog,
    pub lights: Option<Lights>,
    pub cubes: Cubes,
    pub illegal: IllegalScene,
    pub text: TextOverlay,
    pub rng: OsdRand,
    pub kind: SceneKind,
    pub scene_state: i32,
    pub ended: bool,
    pub events: Vec<CameraEvent>,
    pub writer: FrameWriter,
    pub sce_alpha: i32,
    pub illegal_alpha: i32,
    /// Cleared until the opening's first `InitOpeningScene`.
    opening_inited: bool,
    pub last_hash: u64,
    pub last_words: usize,
}

impl Scene {
    /// Build a scene. `seed` feeds the tower-field history simulator (the
    /// reconstruction's numeric argv seed); the `osdRand` stream always starts
    /// at 1, because its draw count is what fixes `lightsSeed`.
    pub fn new(mode: u32, seed: u32, options: SceneOptions) -> Scene {
        let history = crate::field::simulate_boot_history(seed, options.games, options.boots);
        let field = TowerField::generate(&history, options.full_field);

        let mut rng = OsdRand::new();
        let mut illegal = IllegalScene::new();
        illegal.init_flare_rings();
        illegal.init_fog(&mut rng); // 385 draws, as in the real init order
        illegal.reset_scene_flags();

        let mut camera = Camera::new();
        camera.pal = options.pal;
        camera.hw = options.hw;
        camera.disc_ready = options.hw.has_disc();
        camera.boot_param = if mode == MODE_ILLEGAL {
            101
        } else {
            options.boot_param
        };
        camera.init_animation();
        if mode == MODE_IDLE {
            camera.position_speed[2] = 0.0;
        }
        let opening_type = if mode == MODE_ILLEGAL { 1 } else { 0 };
        camera.opening_type = opening_type;
        camera.next_opening_type = opening_type;

        let kind = match mode {
            MODE_ILLEGAL => SceneKind::Illegal,
            MODE_MENU => SceneKind::None,
            _ => SceneKind::Opening,
        };

        Scene {
            mode,
            frame: 1, // the reconstruction starts at evenOddFrame == 1
            options,
            camera,
            field,
            history,
            fog: Fog::new(),
            lights: None,
            cubes: Cubes::new_illegal(), // sub_219cb8 runs at init for both
            illegal,
            text: TextOverlay::new(opening_type),
            rng,
            kind,
            scene_state: 0,
            ended: false,
            events: Vec::new(),
            writer: FrameWriter::new(),
            sce_alpha: 0,
            illegal_alpha: 0,
            opening_inited: false,
            last_hash: 0,
            last_words: 0,
        }
    }

    /// `InitOpeningScene` — runs on the first draw of the towers scene.
    fn init_opening_scene(&mut self) {
        if self.opening_inited {
            return;
        }
        self.camera.init_opening_scene();
        // InitLightsCubes: lightsSeed = osdRand()%2345 + 3456 (draw #386).
        let seed = self.rng.below(2345) + 3456;
        self.lights = Some(Lights::new(seed));
        self.cubes = Cubes::new_opening();
        self.opening_inited = true;
    }

    /// `InitIllegalScene` — runs on the first frame of the illegal scene.
    fn init_illegal_scene(&mut self) {
        self.camera.init_illegal_scene();
        self.cubes = Cubes::new_illegal();
        self.illegal.reset_scene_flags();
    }

    /// Advance exactly one simulation frame.
    pub fn step(&mut self) {
        if self.ended {
            return;
        }
        self.events.clear();

        // --- ProcessOpening ---
        let requested = if self.options.camera_motion {
            self.camera.step(self.frame, &mut self.events)
        } else {
            self.camera.opening_type
        };
        if requested != self.camera.opening_type {
            self.scene_state += 1;
        }

        match self.kind {
            SceneKind::Opening => {
                if self.scene_state == 0 {
                    self.init_opening_scene();
                    self.scene_state = 1;
                }
                if self.scene_state == 1 {
                    self.advance_opening();
                } else if self.scene_state >= 2 {
                    // DoOpening case 2: end this scene.
                    self.camera.next_opening_type = 2;
                    self.scene_state = 0;
                }
            }
            SceneKind::Illegal => {
                if self.scene_state == 0 {
                    self.init_illegal_scene();
                    self.scene_state = 1;
                }
                if self.scene_state == 1 {
                    self.advance_illegal();
                } else if self.scene_state >= 2 {
                    self.camera.next_opening_type = 2;
                    self.scene_state = 0;
                }
            }
            SceneKind::None => {}
        }

        // --- DrawEnd: text, frame counter, opening type latch ---
        let (sce, ill) = self
            .text
            .step(self.camera.position[2], self.camera.opening_end_flag);
        if self.options.text {
            if let Some(a) = sce {
                self.sce_alpha = a;
            }
            if let Some(a) = ill {
                self.illegal_alpha = a;
            }
        } else {
            self.sce_alpha = 0;
            self.illegal_alpha = 0;
        }
        self.frame = self.frame.wrapping_add(1);
        self.camera.opening_type = self.camera.next_opening_type;
        if self.camera.opening_type == 2 {
            self.ended = true;
            self.kind = SceneKind::None;
        }
    }

    /// `DrawOpeningScene`'s animation half (the geometry is emitted on demand).
    fn advance_opening(&mut self) {
        self.fog.advance();
        if let Some(lights) = self.lights.as_mut() {
            if self.options.lights {
                lights.advance(self.frame);
                // DrawLights calls osdRand() once per emitted vertex (result
                // unused) - kept so the stream advance matches the original.
                for _ in 0..(4 * 4 * 2 * 4) {
                    self.rng.next_value();
                }
            }
        }
        if self.options.cubes {
            let (w, h, ax, ay) = self.projection_params();
            self.cubes
                .update(&self.camera, w, h, ax, ay, VIEW_SCREEN_DISTANCE);
        }
    }

    /// `DrawIllegalScene`'s animation half: skip the first frame, then flare,
    /// fog, cubes, fades.
    fn advance_illegal(&mut self) {
        if self.illegal.warm == 0 {
            self.illegal.warm = 1;
            self.illegal.update_intensity(self.camera.position[2]);
            return;
        }
        self.illegal.update_intensity(self.camera.position[2]);
        self.illegal.update_flare(self.frame);
        self.illegal
            .update_fades(self.camera.position[2], self.camera.opening_end_flag);
        let (w, h, ax, ay) = self.projection_params();
        if self.options.cubes {
            self.cubes
                .update(&self.camera, w, h, ax, ay, VIEW_SCREEN_DISTANCE);
        }
    }

    pub fn width(&self) -> f32 {
        if self.options.pal {
            crate::math::SCREEN_W_PAL as f32
        } else {
            crate::math::SCREEN_W_NTSC as f32
        }
    }

    pub fn height(&self) -> f32 {
        if self.options.pal {
            crate::math::SCREEN_H_PAL as f32
        } else {
            crate::math::SCREEN_H_NTSC as f32
        }
    }

    pub fn projection_params(&self) -> (f32, f32, f32, f32) {
        (
            self.width(),
            self.height(),
            crate::math::VIEW_SCREEN_AX,
            if self.options.pal {
                VIEW_SCREEN_AY_PAL
            } else {
                VIEW_SCREEN_AY_NTSC
            },
        )
    }

    /// Build this frame's draw list and update [`Scene::last_hash`].
    pub fn draw(&mut self) -> &[f32] {
        let view = self.camera.view_matrix();
        let (w, h, ax, ay) = self.projection_params();

        // Clear the illegal per-frame fades when the scene is not running.
        if self.kind != SceneKind::Illegal {
            self.illegal.fade_alpha = 0.0;
        }

        let mut writer = std::mem::take(&mut self.writer);
        writer.begin();
        writer.set(H_FRAME, self.frame as f32);
        writer.set_u32(H_MODE, self.mode);
        writer.set(H_OPENING_STATE, self.camera.opening_state as f32);
        writer.set(H_OPENING_TYPE, self.camera.opening_type as f32);
        writer.set(
            H_SCENE,
            match self.kind {
                SceneKind::Opening => 0.0,
                SceneKind::Illegal => 1.0,
                SceneKind::None => 2.0,
            },
        );
        writer.set(H_SCENE_STATE, self.scene_state as f32);
        let mut flags = 0u32;
        if self.camera.opening_end_flag != 0 {
            flags |= FLAG_END_LATCH;
        }
        if self.camera.draw_black_bars != 0 {
            flags |= FLAG_BLACK_BARS;
        }
        if self.illegal.warm != 0 {
            flags |= FLAG_ILLEGAL_WARM;
        }
        if self.ended {
            flags |= FLAG_ENDED;
        }
        if self.options.pal {
            flags |= FLAG_PAL;
        }
        writer.set_u32(H_FLAGS, flags);
        writer.set_vec4(H_CAM_POS, self.camera.position);
        writer.set_vec4(H_CAM_UP, self.camera.up);
        writer.set_vec4(H_CAM_FWD, self.camera.fwd);
        writer.set_matrix(H_VIEW, &view);
        writer.set(H_SCRZ, VIEW_SCREEN_DISTANCE);
        writer.set(H_AX, ax);
        writer.set(H_AY, ay);
        writer.set(H_ROTATION, self.camera.rotation);
        writer.set(H_WIDTH, w);
        writer.set(H_HEIGHT, h);
        writer.set(H_TEXT_SCE_ALPHA, self.sce_alpha as f32);
        writer.set(H_TEXT_ILLEGAL_ALPHA, self.illegal_alpha as f32);
        writer.set_u32(H_HW_STATE, self.options.hw.id());
        writer.set(
            H_LIGHT_SEED,
            self.lights.as_ref().map(|l| l.seed as f32).unwrap_or(0.0),
        );
        writer.set(H_FADE_ALPHA, self.illegal.fade_alpha / 128.0);
        writer.set(H_END_FRAME, self.camera.opening_end_frame as f32);
        writer.set(H_END_FLAG, self.camera.opening_end_flag as f32);
        writer.set(H_RED_FLARE, self.illegal.intensity);
        writer.set(H_ILLEGAL_INTENSITY, self.illegal.intensity);

        match self.kind {
            SceneKind::Opening => {
                self.draw_towers(&mut writer, &view, w, h);
                if self.options.fog {
                    self.draw_fog(&mut writer, &view, w, h);
                }
                if self.options.lights {
                    self.draw_lights(&mut writer, &view);
                }
                if self.options.cubes {
                    self.draw_cubes(&mut writer);
                }
                self.draw_opening_fade(&mut writer, w, h);
                if self.options.black_bars {
                    self.draw_black_bars(&mut writer, w, h, ax, ay);
                }
            }
            SceneKind::Illegal => {
                self.draw_flare(&mut writer, &view, w, h, ax, ay);
                if self.options.fog {
                    self.draw_particles(&mut writer, w, h);
                }
                if self.options.cubes {
                    self.draw_cubes(&mut writer);
                }
                self.draw_fade(&mut writer, w, h);
            }
            SceneKind::None => {
                if self.options.cubes {
                    self.draw_cubes(&mut writer);
                }
            }
        }

        let hash = frame_hash(writer.finish());
        let words = writer.data().len();
        self.writer = writer;
        self.last_hash = hash;
        self.last_words = words;
        self.writer.data()
    }

    fn draw_towers(&mut self, writer: &mut FrameWriter, view: &crate::math::Mat4, w: f32, h: f32) {
        // One per-frame sway angle for the whole field (real 0x218730).
        let sway = (((self.frame % 360) as f32 - 180.0) * DEG2RAD).sin() * 10.0 * DEG2RAD;

        writer.begin_batch(PRIM_TRIANGLES, TEX_WALL, BLEND_NONE, 0);
        for r in 0..crate::field::ROWS {
            for c in 0..crate::field::COLS {
                if self.field.flags[r][c] == 0 {
                    continue;
                }
                let pos = self.field.pos[r][c];
                let height = self.field.a[r][c];
                let f12 = self.field.f12(r, c);
                let u = TowerField::st_offset(r, c) as f32 / 256.0;
                let flat = self.field.is_flat(r, c);
                let angle = TowerField::cell_rotation_term(r, c) + if flat { 0.0 } else { sway };
                let (sin_a, cos_a) = (angle.sin(), angle.cos());

                // Cull towers wholly behind the camera / far off screen.
                let centre = apply(view, &pos);
                if !in_front_and_inside(centre, w, h, 200.0 + height) {
                    continue;
                }

                for q in TOWER_QUADS.iter() {
                    let mut world = [[0.0f32; 3]; 4];
                    let mut colors = [[0.0f32; 3]; 4];
                    let mut uvs = [[0.0f32; 2]; 4];
                    for k in 0..4 {
                        let (lx, ly) = (q.xy[k][0], q.xy[k][1]);
                        let lz = if q.sign[k] != 0 { height } else { -height };
                        // Rotation about the tower's own z axis.
                        world[k] = [
                            pos[0] + lx * cos_a - ly * sin_a,
                            pos[1] + lx * sin_a + ly * cos_a,
                            pos[2] + lz,
                        ];
                        // Near vertices lit, far vertices black (Gouraud fade),
                        // scaled by the radial grid value and the VU lighting.
                        let v = if q.sign[k] != 0 {
                            0.0
                        } else {
                            clamp(f12 * q.color_scale * q.light, 0.0, 255.0)
                        };
                        colors[k] = [v / 255.0; 3];
                        uvs[k] = [u + (k & 1) as f32, u + (k >> 1) as f32];
                    }
                    // Two triangles of the tristrip (0,1,2) (2,1,3).
                    for tri in [[0usize, 1, 2], [2, 1, 3]] {
                        writer.push_triangle(
                            [world[tri[0]], world[tri[1]], world[tri[2]]],
                            [uvs[tri[0]], uvs[tri[1]], uvs[tri[2]]],
                            [
                                [colors[tri[0]][0], colors[tri[0]][1], colors[tri[0]][2], 1.0],
                                [colors[tri[1]][0], colors[tri[1]][1], colors[tri[1]][2], 1.0],
                                [colors[tri[2]][0], colors[tri[2]][1], colors[tri[2]][2], 1.0],
                            ],
                        );
                    }
                }
            }
        }
    }

    fn draw_fog(&mut self, writer: &mut FrameWriter, view: &crate::math::Mat4, w: f32, h: f32) {
        for l in 0..crate::fog::LAYERS {
            let dz = Fog::layer_depth(l);
            writer.begin_batch(PRIM_TRIANGLES, Fog::layer_texture(l), BLEND_ADD, 0);
            for y in 0..16 {
                for x in 0..16 {
                    let mut world = [[0.0f32; 3]; 4];
                    let mut vis = false;
                    for k in 0..4 {
                        let v = self.fog.vertices[x + (k & 1)][y + (k >> 1)];
                        world[k] = [v[0], v[1], v[2] - dz];
                        let p = apply(view, &[world[k][0], world[k][1], world[k][2], 1.0]);
                        if in_front_and_inside(p, w, h, 40.0) {
                            vis = true;
                        }
                    }
                    if !vis {
                        continue;
                    }
                    let mut cols = [[0.0f32; 4]; 4];
                    let mut uvs = [[0.0f32; 2]; 4];
                    for k in 0..4 {
                        let c = self.fog.vertex_color(x + (k & 1), y + (k >> 1));
                        cols[k] = c;
                        uvs[k] = self.fog.cell_uv(l, x, y, k);
                    }
                    for tri in [[0usize, 1, 2], [2, 1, 3]] {
                        writer.push_triangle(
                            [world[tri[0]], world[tri[1]], world[tri[2]]],
                            [uvs[tri[0]], uvs[tri[1]], uvs[tri[2]]],
                            [cols[tri[0]], cols[tri[1]], cols[tri[2]]],
                        );
                    }
                }
            }
        }
    }

    fn draw_lights(&mut self, writer: &mut FrameWriter, view: &crate::math::Mat4) {
        let Some(lights) = self.lights.as_ref() else {
            return;
        };
        writer.begin_batch(PRIM_SPRITES, TEX_CRBL, BLEND_ADD, 0);
        for l in 0..lights::COUNT {
            for sprite in lights::light_sprites(lights, l).iter() {
                writer.push_sprite(
                    sprite.pos,
                    sprite.half,
                    sprite.half,
                    0.0,
                    [
                        sprite.color[0],
                        sprite.color[1],
                        sprite.color[2],
                        sprite.alpha,
                    ],
                );
            }
        }
        writer.begin_batch(PRIM_LINES, TEX_NONE, BLEND_ADD, 0);
        for l in 0..lights::COUNT {
            let pts = lights.trail_points(l);
            for (j, p) in pts.iter().enumerate() {
                let a = lights.trail_alpha(j * lights::TRAIL_SAMPLE_STEP);
                let c = lights::trail_color(l, a);
                // Cull points behind the camera (the projection would flip).
                let v = apply(view, &[p[0], p[1], p[2], 1.0]);
                if v[2] <= 0.5 {
                    continue;
                }
                writer.push_line(*p, c, 1.5);
            }
        }
    }

    fn draw_flare(
        &mut self,
        writer: &mut FrameWriter,
        view: &crate::math::Mat4,
        w: f32,
        h: f32,
        ax: f32,
        ay: f32,
    ) {
        let intensity = self.illegal.intensity;
        if intensity <= 0.0 {
            return;
        }
        // DrawFlareFans: two additive tri-fan sets (small cores, large halos).
        for small in [true, false] {
            let (center, rim) = if small {
                self.illegal.fan_colors(true)
            } else {
                self.illegal.fan_colors(false)
            };
            writer.begin_batch(PRIM_TRIANGLES, TEX_NONE, BLEND_ADD, 0);
            for i in 0..crate::illegal::FLARE_DISCS {
                let ring = if small {
                    &self.illegal.ring_small[i]
                } else {
                    &self.illegal.ring_large[i]
                };
                let (sin_z, cos_z) = (
                    self.illegal.flare_rot[i][2].sin(),
                    self.illegal.flare_rot[i][2].cos(),
                );
                let (sin_y, cos_y) = (
                    self.illegal.flare_rot[i][1].sin(),
                    self.illegal.flare_rot[i][1].cos(),
                );
                let (sin_x, cos_x) = (
                    self.illegal.flare_rot[i][0].sin(),
                    self.illegal.flare_rot[i][0].cos(),
                );
                let base = self.illegal.flare_pos[i];
                let mut world = [[0.0f32; 3]; crate::illegal::FAN_VERTS];
                for (k, wv) in world.iter_mut().enumerate() {
                    // Rotate Z then Y then X (sceVu0RotMatrix order), then
                    // translate to the disc centre.
                    let (x, y, z) = (ring[k][0], ring[k][1], ring[k][2]);
                    let (x1, y1) = (x * cos_z - y * sin_z, x * sin_z + y * cos_z);
                    let (x2, z2) = (x1 * cos_y + z * sin_y, -x1 * sin_y + z * cos_y);
                    let (y3, z3) = (y1 * cos_x - z2 * sin_x, y1 * sin_x + z2 * cos_x);
                    *wv = [base[0] + x2, base[1] + y3, base[2] + z3];
                }
                let mut visible = false;
                for wv in world.iter() {
                    if in_front_and_inside(apply(view, &[wv[0], wv[1], wv[2], 1.0]), w, h, 600.0) {
                        visible = true;
                        break;
                    }
                }
                if !visible {
                    continue;
                }
                // Tri-fan: centre plus 16 rim vertices and the wrap.
                for k in 0..16usize {
                    let a = world[0];
                    let b = world[1 + k];
                    let c = world[1 + ((k + 1) % 16)];
                    writer.push_triangle(
                        [a, b, c],
                        [[0.0, 0.0], [0.0, 0.0], [0.0, 0.0]],
                        [
                            [center[0], center[1], center[2], 0.5],
                            [rim[0], rim[1], rim[2], 0.5],
                            [rim[0], rim[1], rim[2], 0.5],
                        ],
                    );
                }
            }
        }

        // FlareThing: the lens-sprite stack, positioned at the projected centre
        // of disc 0 and pulled toward the screen centre by `scale`.
        let sprites = self.illegal.lens_sprites(self.illegal.flare_thing_n());
        // Disc 0's screen position (with the 32-frame wobble written into
        // flarePos[0][0..2] by DrawFlareSprite).
        let ph = {
            let mut p = (49 + (self.frame & 0x1f)) as f32 * 0.1;
            while p > crate::math::PI {
                p -= crate::math::TAU;
            }
            while p < -crate::math::PI {
                p += crate::math::TAU;
            }
            p
        };
        let wobble = [ph.cos() * 49.0 * 0.004, ph.sin() * 49.0 * 0.004];
        let base = [
            self.illegal.flare_pos[0][0] + wobble[0],
            self.illegal.flare_pos[0][1] + wobble[1],
            self.illegal.flare_pos[0][2],
        ];
        // Rotate the small ring's centre (origin) to the disc transform, then
        // project to virtual screen pixels.
        let v = apply(view, &[base[0], base[1], base[2], 1.0]);
        if v[2] > 0.5 {
            let sx = ax * VIEW_SCREEN_DISTANCE * v[0] / v[2];
            let sy = ay * VIEW_SCREEN_DISTANCE * v[1] / v[2];
            let px = sx + w * 0.5;
            let py = sy + h * 0.5;
            let scale = 0.9f32;
            writer.begin_batch(PRIM_SCREEN_SPRITES, TEX_FLAR, BLEND_ADD, 0);
            for sprite in sprites.iter().flatten() {
                let (size, alpha, rgb) = *sprite;
                let x = (px - w * 0.5) * scale + w * 0.5;
                let y = (py - h * 0.5) * scale + h * 0.5;
                writer.push_screen_sprite(
                    x,
                    y - size * 0.25,
                    size,
                    size * 0.5,
                    [rgb[0], rgb[1], rgb[2], alpha],
                );
            }
        }
    }

    fn draw_particles(&mut self, writer: &mut FrameWriter, w: f32, h: f32) {
        // Advance the particles once per frame; the alphas come back with the
        // state update, exactly like DrawIllegalFog.
        let alphas = self
            .illegal
            .update_particles(self.frame, self.camera.position[2]);
        let view = self.camera.view_matrix();
        let mut any = false;
        for (i, a) in alphas.iter().enumerate() {
            if a.is_none() {
                continue;
            }
            let p = self.illegal.pos[i];
            let inside = in_front_and_inside(apply(&view, &[p[0], p[1], p[2], 1.0]), w, h, 120.0);
            if !inside {
                continue;
            }
            if !any {
                writer.begin_batch(PRIM_SPRITES, TEX_FOG0, BLEND_ADD, 0);
                any = true;
            }
            let colour = &self.illegal.patch_colors[4];
            let r = colour[0] as f32 / 255.0;
            let g = colour[1] as f32 / 255.0;
            let b = colour[2] as f32 / 255.0;
            writer.push_sprite(
                [p[0], p[1], p[2]],
                15.0,
                15.0,
                self.illegal.rot[i][2],
                [r, g, b, a.unwrap()],
            );
        }
    }

    /// `sub_218bd0` — the opening's fade to black: full cover on the first
    /// frame, `(z-72)*4` during the fly-up (truncated to an int, capped at
    /// 128), full black from z 320 on. Alpha 128 is 50% black.
    pub fn opening_fade_alpha(&self) -> f32 {
        let z = self.camera.position[2];
        let mut a: f32 = if self.frame < 2 { 128.0 } else { 0.0 };
        if z > 72.0 {
            a = a.max(((z - 72.0) * 128.0 * 0.031_25) as i32 as f32);
        }
        if z >= 320.0 {
            a = 128.0;
        }
        a.clamp(0.0, 128.0)
    }

    fn draw_opening_fade(&mut self, writer: &mut FrameWriter, w: f32, h: f32) {
        let alpha = self.opening_fade_alpha() / 128.0;
        if alpha <= 0.0 {
            return;
        }
        writer.begin_batch(PRIM_SCREEN_SPRITES, TEX_NONE, BLEND_ALPHA, 0);
        writer.push_screen_sprite(w * 0.5, h * 0.5, w, h, [0.0, 0.0, 0.0, alpha]);
    }

    /// `DrawBlackBars` — 16:9 letterbox bars drawn with the subtractive blend.
    fn draw_black_bars(&mut self, writer: &mut FrameWriter, w: f32, h: f32, ax: f32, ay: f32) {
        let imgh = (w * 9.0 * ay / (ax * 16.0)) as i32;
        let bar = (h as i32 - imgh + 1) / 2;
        writer.begin_batch(PRIM_SCREEN_SPRITES, TEX_NONE, BLEND_SUB, 0);
        writer.push_screen_sprite(w * 0.5, bar as f32 * 0.5, w, bar as f32, [1.0; 4]);
        writer.push_screen_sprite(
            w * 0.5,
            (bar + imgh) as f32 + bar as f32 * 0.5,
            w,
            bar as f32,
            [1.0; 4],
        );
    }

    fn draw_fade(&mut self, writer: &mut FrameWriter, w: f32, h: f32) {
        let alpha = self.illegal.fade_alpha / 128.0;
        if alpha <= 0.0 {
            return;
        }
        writer.begin_batch(PRIM_SCREEN_SPRITES, TEX_NONE, BLEND_ALPHA, 0);
        writer.push_screen_sprite(w * 0.5, h * 0.5, w, h, [0.0, 0.0, 0.0, alpha]);
    }

    fn draw_cubes(&mut self, writer: &mut FrameWriter) {
        // Cubes are a separate section; this pins where they belong in the
        // GS draw order (everything emitted so far is behind them).
        writer.mark_cubes();
        for inst in self.cubes.instances.iter() {
            if !inst.visible {
                continue;
            }
            let mut rec = [0.0f32; CUBE_WORDS];
            rec[0] = inst.index as f32;
            rec[1] = 1.0;
            rec[2..8].copy_from_slice(&inst.face_sign);
            rec[8] = inst.base_color[0];
            rec[9] = inst.base_color[1];
            rec[10] = inst.base_color[2];
            rec[11] = inst.screen_center[0];
            rec[12] = inst.screen_center[1];
            for k in 0..8 {
                rec[13 + k] = inst.corner_q[k];
                for c in 0..4 {
                    rec[21 + k * 4 + c] = inst.corners[k][c];
                }
            }
            for k in 0..8 {
                rec[53 + k * 3..56 + k * 3].copy_from_slice(&inst.view_dir[k][..3]);
            }
            for f in 0..6 {
                for c in 0..4 {
                    rec[77 + f * 4 + c] = inst.screen_normal[f][c];
                }
            }
            for f in 0..6 {
                for k in 0..4 {
                    rec[101 + f * 4 + k] = inst.vertex_light[f][k];
                }
            }
            writer.push_cube(&rec);
        }
    }

    /// A one-line human-readable state summary (debug overlay, logs).
    pub fn describe(&self) -> String {
        format!(
            "mode={} frame={} scene={:?} state={} type={} z={:.2} towers={} ended={}",
            self.mode,
            self.frame,
            self.kind,
            self.camera.opening_state,
            self.camera.opening_type,
            self.camera.position[2],
            self.field.lit_count(),
            self.ended
        )
    }
}

/// One tower face's static geometry, decoded from `osdbits/towerchain.dsm`
/// (see `docs/PORTING-NOTES.md` for the byte-by-byte derivation).
///
/// The six DMA packets are listed in the reconstruction's `towerBlocks[]`
/// order: `{Block5, Block1, Block2, Block3, Block4, Block0}`. Block 0
/// (the far cap, all four vertices at +height but with x=y=0 in the static
/// data) is degenerate and is not emitted. The remaining five are the near
/// cap and the four side walls.
pub struct TowerQuad {
    /// Local `(x, y)` per vertex, before the per-cell z rotation.
    pub xy: [[f32; 2]; 4],
    /// 1 = the vertex is at +height (far, black), 0 = at -height (near, lit).
    pub sign: [u8; 4],
    /// Colour scale: 1.0 for the cap (block index 0), 0.8 for the walls.
    pub color_scale: f32,
    /// Per-face lighting factor derived from the reconstruction's light
    /// matrix (see `docs/PORTING-NOTES.md`); the ±4% sway modulation is not
    /// reproduced.
    pub light: f32,
}

pub const TOWER_QUADS: [TowerQuad; 5] = [
    // b = 0 -> Block5: the near cap at z = -height (all vertices near-side).
    TowerQuad {
        xy: [[2.0, -2.0], [-2.0, -2.0], [2.0, 2.0], [-2.0, 2.0]],
        sign: [0, 0, 0, 0],
        color_scale: 1.0,
        light: 1.4,
    },
    // b = 1 -> Block1: the y = -2 wall.
    TowerQuad {
        xy: [[2.0, -2.0], [2.0, -2.0], [-2.0, -2.0], [-2.0, -2.0]],
        sign: [0, 1, 0, 1],
        color_scale: 0.8,
        light: 0.966,
    },
    // b = 2 -> Block2: the y = +2 wall.
    TowerQuad {
        xy: [[2.0, 2.0], [2.0, 2.0], [-2.0, 2.0], [-2.0, 2.0]],
        sign: [1, 0, 1, 0],
        color_scale: 0.8,
        light: 0.966,
    },
    // b = 3 -> Block3: the x = +2 wall.
    TowerQuad {
        xy: [[2.0, -2.0], [2.0, 2.0], [2.0, -2.0], [2.0, 2.0]],
        sign: [1, 1, 0, 0],
        color_scale: 0.8,
        light: 0.966,
    },
    // b = 4 -> Block4: the x = -2 wall.
    TowerQuad {
        xy: [[-2.0, -2.0], [-2.0, 2.0], [-2.0, -2.0], [-2.0, 2.0]],
        sign: [0, 0, 1, 1],
        color_scale: 0.8,
        light: 0.966,
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    fn scene(mode: u32) -> Scene {
        Scene::new(mode, 4242, SceneOptions::default())
    }

    #[test]
    fn mode_constants_are_stable() {
        assert_eq!(MODE_BOOT, 0);
        assert_eq!(MODE_IDLE, 1);
        assert_eq!(MODE_ILLEGAL, 2);
        assert_eq!(MODE_MENU, 3);
    }

    #[test]
    fn tower_quads_encode_the_reconstructed_geometry() {
        // 5 emitted faces: near cap + 4 walls.
        assert_eq!(TOWER_QUADS.len(), 5);
        assert_eq!(TOWER_QUADS[0].color_scale, 1.0);
        assert_eq!(TOWER_QUADS[0].sign, [0, 0, 0, 0]);
        for q in TOWER_QUADS.iter().skip(1) {
            assert_eq!(q.color_scale, 0.8);
            assert_eq!(q.sign.iter().filter(|&&s| s == 1).count(), 2);
            // Every wall has two distinct x or y values plus the wall plane.
            assert!(q.xy[0][1] == q.xy[2][1]);
        }
        // Cap spans both x and y.
        let xs: Vec<f32> = TOWER_QUADS[0].xy.iter().map(|p| p[0]).collect();
        let ys: Vec<f32> = TOWER_QUADS[0].xy.iter().map(|p| p[1]).collect();
        assert!(xs.contains(&2.0) && xs.contains(&-2.0));
        assert!(ys.contains(&2.0) && ys.contains(&-2.0));
    }

    #[test]
    fn boot_scene_emits_towers_first_then_fog_then_sprites() {
        let mut s = scene(MODE_BOOT);
        s.step();
        s.step();
        let out = s.draw();
        let off_b = out[H_OFF_BATCHES] as usize;
        let count = out[H_BATCH_COUNT] as usize;
        assert!(count >= 8, "batches {count}");
        // Batch 0: towers (triangles, wall texture, opaque).
        assert_eq!(out[off_b], PRIM_TRIANGLES as f32);
        assert_eq!(out[off_b + 1], TEX_WALL as f32);
        assert_eq!(out[off_b + 2], BLEND_NONE as f32);
        // Then the six fog layers.
        for l in 0..6 {
            let b = off_b + (1 + l) * BATCH_WORDS;
            assert_eq!(out[b], PRIM_TRIANGLES as f32);
            assert_eq!(out[b + 1], Fog::layer_texture(l) as f32);
            assert_eq!(out[b + 2], BLEND_ADD as f32);
        }
        // Then the light sprites. (The trail-line batch only appears once the
        // ring has samples, which takes a few frames.)
        let sprites = off_b + 7 * BATCH_WORDS;
        assert_eq!(out[sprites], PRIM_SPRITES as f32);
        assert_eq!(out[sprites + 1], TEX_CRBL as f32);
        assert_eq!(count, 8);
    }

    #[test]
    fn light_trail_batch_appears_once_the_ring_has_samples() {
        let mut s = scene(MODE_BOOT);
        let mut found = false;
        for _ in 0..40 {
            s.step();
            let out = s.draw();
            let off_b = out[H_OFF_BATCHES] as usize;
            for b in 0..out[H_BATCH_COUNT] as usize {
                if out[off_b + b * BATCH_WORDS] == PRIM_LINES as f32 {
                    found = true;
                    assert_eq!(out[off_b + b * BATCH_WORDS + 2], BLEND_ADD as f32);
                }
            }
        }
        assert!(found, "no trail-line batch in 40 frames");
    }

    #[test]
    fn towers_are_culled_when_behind_the_camera() {
        let mut s = scene(MODE_BOOT);
        s.step();
        let before = s.draw()[H_VERTEX_COUNT];
        assert!(before > 0.0);
        // Jump the camera far past the field: nothing should remain.
        s.camera.position[2] = 20_000.0;
        let after = s.draw()[H_VERTEX_COUNT];
        assert!(after < before, "before {before} after {after}");
        // Only the additive fog/light batches survive (fog is behind too).
        assert!(after < 2000.0, "after {after}");
    }

    #[test]
    fn idle_mode_stays_on_the_field() {
        let mut s = scene(MODE_IDLE);
        for _ in 0..2000 {
            s.step();
            s.draw();
            assert!(!s.ended, "idle must not end");
            assert_eq!(s.camera.opening_state, 0);
        }
        assert_eq!(s.kind, SceneKind::Opening);
    }

    #[test]
    fn boot_mode_ends_once() {
        let mut s = scene(MODE_BOOT);
        let mut frames = 0;
        while !s.ended && frames < 5000 {
            s.step();
            s.draw();
            frames += 1;
        }
        assert!(s.ended, "boot never ended");
        assert_eq!(s.kind, SceneKind::None);
        assert!(frames < 3000, "took {frames} frames");
        // Further steps are inert.
        let hash_before = s.last_hash;
        s.step();
        s.draw();
        assert_eq!(s.last_hash, hash_before);
    }

    #[test]
    fn same_seed_replays_identically() {
        let mut a = scene(MODE_BOOT);
        let mut b = scene(MODE_BOOT);
        for _ in 0..600 {
            a.step();
            b.step();
            assert_eq!(a.draw()[H_VERTEX_COUNT], b.draw()[H_VERTEX_COUNT]);
            assert_eq!(a.last_hash, b.last_hash);
        }
        assert_ne!(a.last_hash, 0);
    }

    #[test]
    fn different_seed_changes_the_field_but_not_the_camera() {
        let mut a = Scene::new(MODE_BOOT, 1, SceneOptions::default());
        let mut b = Scene::new(MODE_BOOT, 2, SceneOptions::default());
        assert_ne!(a.field.lit_count(), 0);
        for _ in 0..120 {
            a.step();
            b.step();
        }
        assert_eq!(a.camera.position[2], b.camera.position[2]);
        let flat = |f: &TowerField| -> Vec<u32> {
            f.d.iter()
                .flat_map(|row| row.iter().map(|v| v.to_bits()))
                .collect()
        };
        assert_ne!(flat(&a.field), flat(&b.field));
    }

    #[test]
    fn illegal_scene_emits_fans_sprites_and_cubes() {
        let mut s = scene(MODE_ILLEGAL);
        for _ in 0..4 {
            s.step();
            s.draw();
        }
        let out = s.draw();
        assert_eq!(out[H_SCENE], 1.0);
        assert!(s.illegal.warm != 0);
        // Camera climbs into the flare band; intensity becomes positive and
        // the flare fans get emitted.
        for _ in 0..400 {
            s.step();
            s.draw();
        }
        assert!(
            s.illegal.intensity > 0.0,
            "intensity {}",
            s.illegal.intensity
        );
        // The scene starts fully faded in from black (z = 672 -> alpha 128)
        // and the fade is always a valid alpha.
        assert!((0.0..=128.0).contains(&s.illegal.fade_alpha));
        let out = s.draw();
        // At least one screen-space batch (fade and/or lens sprites) is emitted.
        let off_b = out[H_OFF_BATCHES] as usize;
        let count = out[H_BATCH_COUNT] as usize;
        let mut screen_batches = 0;
        for b in 0..count {
            if out[off_b + b * BATCH_WORDS] == PRIM_SCREEN_SPRITES as f32 {
                screen_batches += 1;
            }
        }
        assert!(screen_batches >= 1, "no screen-space batch");
    }

    #[test]
    fn effects_can_be_disabled() {
        let mut s = scene(MODE_BOOT);
        s.options.fog = false;
        s.options.lights = false;
        s.options.cubes = false;
        s.step();
        s.step();
        let out = s.draw();
        assert_eq!(out[H_BATCH_COUNT], 1.0);
        assert_eq!(out[H_CUBE_COUNT], 0.0);
    }

    #[test]
    fn reduced_motion_freezes_the_camera_but_keeps_animating() {
        let mut s = scene(MODE_BOOT);
        s.options.camera_motion = false;
        s.step();
        let z = s.camera.position[2];
        for _ in 0..30 {
            s.step();
            s.draw();
        }
        assert_eq!(s.camera.position[2], z, "camera moved");
        // The fog animation keeps running even though the camera is frozen.
        assert!(s.fog.animation[0] > 0.0, "fog animation stalled");
    }

    #[test]
    fn text_alphas_are_emitted() {
        let mut s = scene(MODE_BOOT);
        let mut seen = false;
        for _ in 0..400 {
            s.step();
            let out = s.draw();
            if out[H_TEXT_SCE_ALPHA] > 0.0 {
                seen = true;
                assert!(out[H_TEXT_SCE_ALPHA] <= 112.0);
            }
        }
        assert!(seen, "SCE text never faded in");
        assert_eq!(s.sce_alpha, s.text.sce_alpha.min(112));
    }

    #[test]
    fn text_can_be_disabled() {
        let mut s = scene(MODE_BOOT);
        s.options.text = false;
        for _ in 0..400 {
            s.step();
            assert_eq!(s.draw()[H_TEXT_SCE_ALPHA], 0.0);
        }
    }

    #[test]
    fn header_view_matrix_matches_the_camera() {
        let mut s = scene(MODE_BOOT);
        s.step();
        let out = s.draw();
        let mut m = [[0.0f32; 4]; 4];
        for r in 0..4 {
            for c in 0..4 {
                m[r][c] = out[H_VIEW + r * 4 + c];
            }
        }
        let want = s.camera.view_matrix();
        for r in 0..4 {
            for c in 0..4 {
                assert!((m[r][c] - want[r][c]).abs() < 1e-6);
            }
        }
    }

    #[test]
    fn pal_changes_the_virtual_screen() {
        let opts = SceneOptions {
            pal: true,
            ..SceneOptions::default()
        };
        let mut s = Scene::new(MODE_BOOT, 1, opts);
        s.step();
        let out = s.draw();
        assert_eq!(out[H_HEIGHT], 256.0);
        assert_eq!(out[H_WIDTH], 640.0);
        assert_eq!(out[H_FLAGS].to_bits() & FLAG_PAL, FLAG_PAL);
        assert!((out[H_AY] - VIEW_SCREEN_AY_PAL).abs() < 1e-6);
    }

    #[test]
    fn buffers_stop_growing_and_memory_is_bounded() {
        let mut s = scene(MODE_BOOT);
        let mut caps = None;
        for f in 0..600 {
            s.step();
            s.draw();
            if f == 60 {
                caps = Some(s.writer.capacities());
            }
            if f > 61 {
                assert_eq!(Some(s.writer.capacities()), caps, "buffer grew at {f}");
            }
        }
        let caps = caps.unwrap();
        // The vertices dominate: 5948 words at ~2.4 frames in, growing to the
        // steady-state draw list. A refactor that doubles the list must update
        // this budget deliberately.
        assert!(caps[1] <= 131_072, "vertex capacity {}", caps[1]);
        assert!(caps[0] <= 1024, "batch capacity {}", caps[0]);
        assert!(caps[2] <= 8 * CUBE_WORDS, "cube capacity {}", caps[2]);
    }

    #[test]
    fn frame_is_well_formed_every_step() {
        let mut s = scene(MODE_BOOT);
        for i in 0..200 {
            s.step();
            let out = s.draw();
            assert_eq!(out[H_MAGIC].to_bits(), MAGIC, "frame {i}");
            assert_eq!(out[H_VERSION].to_bits(), VERSION);
            assert_eq!(out[H_TOTAL_WORDS] as usize, out.len());
            let vc = out[H_VERTEX_COUNT] as usize;
            let bc = out[H_BATCH_COUNT] as usize;
            assert_eq!(out[H_OFF_VERTS] as usize, HEADER_WORDS + bc * BATCH_WORDS);
            assert_eq!(
                out[H_OFF_CUBES] as usize,
                out[H_OFF_VERTS] as usize + vc * VERTEX_WORDS
            );
            for w in out.iter() {
                assert!(w.is_finite(), "non-finite word in frame {i}");
            }
        }
    }

    #[test]
    fn describe_reports_the_state() {
        let s = scene(MODE_IDLE);
        let d = s.describe();
        assert!(d.contains("mode=1"));
        assert!(d.contains("towers="));
    }

    #[test]
    fn menu_mode_emits_no_scene_geometry() {
        let mut s = scene(MODE_MENU);
        for _ in 0..10 {
            s.step();
        }
        let out = s.draw();
        assert_eq!(out[H_BATCH_COUNT], 0.0);
        assert_eq!(s.kind, SceneKind::None);
        assert!(!s.ended);
    }

    #[test]
    fn illegal_scene_with_disc_hardware_does_not_crash_the_boot_path() {
        let opts = SceneOptions {
            hw: HwState::DiscPs2,
            ..SceneOptions::default()
        };
        let mut s = Scene::new(MODE_BOOT, 77, opts);
        for _ in 0..2000 {
            s.step();
            s.draw();
            if s.ended {
                break;
            }
        }
        assert!(s.camera.position[2] > 100.0);
    }
}
