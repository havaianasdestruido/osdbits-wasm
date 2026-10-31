//! Mode D — Module U 2D OSD text engine (`osdbits/menutext.c`), localized string tables,
//! main menu, System Configuration item list & editors, letterbox, top bar, hint bar,
//! and the interactive Memory Card / Boot History browser.

use crate::assets::{AssetPack, TextureId};
use crate::input_audio::{
    PadCooker, SoundMailbox, SE_CANCEL, SE_CONFIRM, SE_CURSOR, SE_TOGGLE, SND_CMD_SE_PLAY,
};
use crate::menu::MenuScene;
use crate::menuconfig::ConfigScene;
use crate::render_cmd::{
    DepthMode, FrameCommandBuffer, GpuVertex, GsBlendMode, RenderTargetId, SamplerWrap,
};

pub const LANG_ENGLISH: u8 = 1;
pub const LANG_FRENCH: u8 = 2;
pub const LANG_SPANISH: u8 = 3;
pub const LANG_GERMAN: u8 = 4;
pub const LANG_ITALIAN: u8 = 5;
pub const LANG_DUTCH: u8 = 6;
pub const LANG_PORTUGUESE: u8 = 7;

/// String IDs matching `osdGetString` (`0x2041B8` / `docs/menu-text.md` §3).
pub const STR_VERSION_INFO: usize = 89;
pub const STR_BROWSER: usize = 90;
pub const STR_SYSTEM_CONFIGURATION: usize = 91;
pub const STR_CLOCK_ADJUSTMENT: usize = 106;
pub const STR_SCREEN_SIZE: usize = 107;
pub const STR_DIGITAL_OUT: usize = 111;
pub const STR_COMPONENT_OUT: usize = 114;
pub const STR_LANGUAGE: usize = 117;
pub const STR_BACK: usize = 85;
pub const STR_ENTER: usize = 86;
pub const STR_OPTIONS: usize = 87;
pub const STR_DISPLAY: usize = 94;
pub const STR_ON: usize = 112;
pub const STR_OFF: usize = 113;
pub const STR_YCBCR: usize = 116;
pub const STR_RGB: usize = 115;
pub const STR_4_3: usize = 108;
pub const STR_FULL: usize = 109;
pub const STR_16_9: usize = 110;
pub const STR_ENGLISH: usize = 119;
pub const STR_FRENCH: usize = 120;
pub const STR_SPANISH: usize = 121;
pub const STR_GERMAN: usize = 122;
pub const STR_ITALIAN: usize = 123;
pub const STR_DUTCH: usize = 124;
pub const STR_PORTUGUESE: usize = 125;

/// English string table (`0x298B08`, key entries).
pub fn osd_string(language: u8, id: usize) -> &'static str {
    match language {
        LANG_FRENCH => match id {
            STR_VERSION_INFO => "Informations sur la version",
            STR_BROWSER => "Navigateur",
            STR_SYSTEM_CONFIGURATION => "Configuration du systeme",
            STR_CLOCK_ADJUSTMENT => "Reglage de l'horloge",
            STR_SCREEN_SIZE => "Format de l'ecran",
            STR_DIGITAL_OUT => "\\7r0.90SORTIE NUMERIQUE (OPTIQUE)\\7r0.00",
            STR_COMPONENT_OUT => "Sortie video composantes",
            STR_LANGUAGE => "Langue",
            STR_BACK => "Retour",
            STR_ENTER => "Entrer",
            STR_OPTIONS => "Options",
            STR_DISPLAY => "Affichage",
            STR_ON => "Marche",
            STR_OFF => "Arret",
            STR_YCBCR => "Y Cb/Pb Cr/Pr",
            STR_RGB => "RVB",
            STR_4_3 => "4:3",
            STR_FULL => "Plein",
            STR_16_9 => "16:9",
            STR_ENGLISH => "Anglais",
            STR_FRENCH => "Francais",
            STR_SPANISH => "Espagnol",
            STR_GERMAN => "Allemand",
            STR_ITALIAN => "Italien",
            STR_DUTCH => "Neerlandais",
            STR_PORTUGUESE => "Portugais",
            _ => "",
        },
        LANG_SPANISH => match id {
            STR_BROWSER => "Navegador",
            STR_SYSTEM_CONFIGURATION => "Configuracion del sistema",
            STR_CLOCK_ADJUSTMENT => "Ajuste del reloj",
            STR_SCREEN_SIZE => "Tamano de pantalla",
            STR_LANGUAGE => "Idioma",
            STR_BACK => "Volver",
            STR_ENTER => "Entrar",
            STR_OPTIONS => "Opciones",
            STR_DISPLAY => "Pantalla",
            STR_ON => "Activado",
            STR_OFF => "Desactivado",
            _ => "Navegador",
        },
        LANG_GERMAN => match id {
            STR_BROWSER => "Browser",
            STR_SYSTEM_CONFIGURATION => "Systemkonfiguration",
            STR_CLOCK_ADJUSTMENT => "Uhrzeit einstellen",
            STR_SCREEN_SIZE => "Bildschirmgroesse",
            STR_LANGUAGE => "Sprache",
            STR_BACK => "Zurueck",
            STR_ENTER => "Eingabe",
            STR_OPTIONS => "Optionen",
            STR_DISPLAY => "Anzeige",
            STR_ON => "Ein",
            STR_OFF => "Aus",
            _ => "Browser",
        },
        LANG_ITALIAN => match id {
            STR_BROWSER => "Navigatore",
            STR_SYSTEM_CONFIGURATION => "Configurazione sistema",
            STR_CLOCK_ADJUSTMENT => "Regolazione orologio",
            STR_SCREEN_SIZE => "Formato schermo",
            STR_LANGUAGE => "Lingua",
            STR_BACK => "Indietro",
            STR_ENTER => "Invio",
            STR_OPTIONS => "Opzioni",
            STR_DISPLAY => "Schermo",
            STR_ON => "Attivo",
            STR_OFF => "Disattivo",
            _ => "Navigatore",
        },
        LANG_DUTCH => match id {
            STR_BROWSER => "Verkenner",
            STR_SYSTEM_CONFIGURATION => "Systeemconfiguratie",
            STR_CLOCK_ADJUSTMENT => "Klok instellen",
            STR_SCREEN_SIZE => "Schermformaat",
            STR_LANGUAGE => "Taal",
            STR_BACK => "Terug",
            STR_ENTER => "Enter",
            STR_OPTIONS => "Opties",
            STR_DISPLAY => "Beeld",
            STR_ON => "Aan",
            STR_OFF => "Uit",
            _ => "Verkenner",
        },
        LANG_PORTUGUESE => match id {
            STR_BROWSER => "Navegador",
            STR_SYSTEM_CONFIGURATION => "Configuracao do sistema",
            STR_CLOCK_ADJUSTMENT => "Ajuste do relogio",
            STR_SCREEN_SIZE => "Tamanho do ecra",
            STR_LANGUAGE => "Idioma",
            STR_BACK => "Voltar",
            STR_ENTER => "Entrar",
            STR_OPTIONS => "Opcoes",
            STR_DISPLAY => "Ecra",
            STR_ON => "Ligado",
            STR_OFF => "Desligado",
            _ => "Navegador",
        },
        _ => match id {
            STR_VERSION_INFO => "Version Information",
            STR_BROWSER => "Browser",
            STR_SYSTEM_CONFIGURATION => "System Configuration",
            STR_CLOCK_ADJUSTMENT => "Clock Adjustment",
            STR_SCREEN_SIZE => "Screen Size",
            STR_DIGITAL_OUT => "\\7r0.90DIGITAL OUT (OPTICAL)\\7r0.00",
            STR_COMPONENT_OUT => "Component Video Out",
            STR_LANGUAGE => "Language",
            STR_BACK => "Back",
            STR_ENTER => "Enter",
            STR_OPTIONS => "Options",
            STR_DISPLAY => "Display",
            STR_ON => "On",
            STR_OFF => "Off",
            STR_YCBCR => "Y Cb/Pb Cr/Pr",
            STR_RGB => "RGB",
            STR_4_3 => "4:3",
            STR_FULL => "Full",
            STR_16_9 => "16:9",
            STR_ENGLISH => "English",
            STR_FRENCH => "French",
            STR_SPANISH => "Spanish",
            STR_GERMAN => "German",
            STR_ITALIAN => "Italian",
            STR_DUTCH => "Dutch",
            STR_PORTUGUESE => "Portuguese",
            _ => "",
        },
    }
}

/// Parses the `\7oNNN` / `\7rN.NN` escape sequences and ASCII runs into drawable glyph tokens.
#[derive(Debug, Clone, PartialEq)]
pub enum TextToken {
    Ascii(usize),    // Glyph index 0..97 (`char - 32`)
    OsdGlyph(usize), // FNTEXOSD glyph 0..35
    Newline,
}

/// Splits a string into glyph tokens, honoring `\7oNNN` (`FNTEXOSD`) and `\7rN.NN` (percentage scale) escapes.
pub fn tokenize_text(s: &str) -> Vec<TextToken> {
    let bytes = s.as_bytes();
    let mut i = 0usize;
    let mut out = Vec::new();
    while i < bytes.len() {
        let b = bytes[i];
        if b == 0x07 && i + 1 < bytes.len() {
            let cmd = bytes[i + 1] as char;
            if cmd == 'o' && i + 4 < bytes.len() {
                let d0 = (bytes[i + 2] as char).to_digit(10).unwrap_or(0);
                let d1 = (bytes[i + 3] as char).to_digit(10).unwrap_or(0);
                let d2 = (bytes[i + 4] as char).to_digit(10).unwrap_or(0);
                let glyph = (d0 * 100 + d1 * 10 + d2) as usize;
                out.push(TextToken::OsdGlyph(glyph.min(34)));
                i += 5;
                continue;
            } else if cmd == 'r' && i + 5 < bytes.len() {
                // `\7rN.NN` percentage scale escape: consume and ignore for drawing metrics
                i += 6;
                continue;
            }
            i += 2;
            continue;
        }
        if b == b'\n' {
            out.push(TextToken::Newline);
            i += 1;
            continue;
        }
        // The glyph atlas covers printable ASCII only (characters 32..=126). Anything else —
        // a UTF-8 continuation byte, a stray control code, a localised accent the Latin page
        // has no cell for — maps to '?' instead of indexing past the 97-entry metric table.
        if (32..=126).contains(&b) {
            out.push(TextToken::Ascii((b - 32) as usize));
        } else if b >= 127 {
            out.push(TextToken::Ascii(('?' as usize) - 32));
        }
        i += 1;
    }
    out
}

/// Measures a string in whole screen pixels at scale `scale` using the ROM's `osdGlyphAdvance` arithmetic
/// (`(int)(scale * width) + (int)(scale * (gap = -3.0))`).
pub fn osd_text_width(s: &str, scale: f32, pack: &AssetPack) -> i32 {
    let mut total = 0i32;
    for tok in tokenize_text(s) {
        match tok {
            TextToken::Ascii(g) => {
                let w = pack.font_ascii_metrics[g][1] as f32;
                total += (scale * w) as i32 + (scale * -3.0) as i32;
            }
            TextToken::OsdGlyph(g) => {
                let w = pack.font_osd_metrics[g][1] as f32;
                total += (scale * w) as i32 + (scale * -3.0) as i32;
            }
            TextToken::Newline => break,
        }
    }
    total
}

/// Emits one textured glyph sprite (`osdDrawGlyph`, `0x2086A0`).
///
/// FNTASCII: `col = g % 8`, `row = g / 8`, cell `32×40`, `u0 = col*32 + inset`, `u1 = u0 + width`.
/// The drawn height is `(int)(scale*32*0.5) << 4` scaled by `1.25 * baseScale` -> 20 px at scale 1.0 NTSC.
/// FNTEXOSD: `col = g % 16`, `row = g / 16`.
#[allow(clippy::too_many_arguments)]
pub fn emit_glyph(
    cmd_buf: &mut FrameCommandBuffer,
    target: RenderTargetId,
    tex: TextureId,
    pen_x: i32,
    pen_y: i32,
    glyph: usize,
    osd_glyph: bool,
    scale: f32,
    base_scale: f32,
    rgba: [u8; 4],
    pack: &AssetPack,
) {
    let (cols, inset, width) = if osd_glyph {
        let m = pack.font_osd_metrics[glyph.min(34)];
        (16usize, m[0], m[1])
    } else {
        let m = pack.font_ascii_metrics[glyph.min(96)];
        (8usize, m[0], m[1])
    };
    let idx = if osd_glyph {
        glyph.min(34)
    } else {
        glyph.min(96)
    };
    let row = idx / cols;
    let col = idx % cols;

    let h16 = (((scale * 32.0 * 0.5) as i32) << 4) as f32;
    let glyph_h16 = (h16 * 1.25 * base_scale) as i32;
    let w16 = ((scale * width as f32) as i32) << 4;

    let u0 = (col as f32 * 32.0) + inset as f32;
    let u1 = u0 + width as f32;
    let v0 = row as f32 * 40.0;
    let v1 = v0 + 40.0;

    let atlas_w = if osd_glyph { 512.0f32 } else { 256.0f32 };
    let atlas_h = if osd_glyph { 80.0f32 } else { 480.0f32 };

    let color = [
        rgba[0] as f32 / 128.0,
        rgba[1] as f32 / 128.0,
        rgba[2] as f32 / 128.0,
        rgba[3] as f32 / 128.0,
    ];
    let z = 0.02f32;

    cmd_buf.push_quad(
        target,
        tex,
        GsBlendMode::SourceAlpha,
        SamplerWrap::Clamp,
        DepthMode::AlwaysNoWrite,
        128,
        GpuVertex {
            pos: [pen_x as f32, pen_y as f32, z, 1.0],
            uv: [u0 / atlas_w, v0 / atlas_h],
            color,
        },
        GpuVertex {
            pos: [
                (pen_x << 4).wrapping_add(0) as f32 * 0.0 + pen_x as f32 + (w16 as f32 / 16.0),
                pen_y as f32,
                z,
                1.0,
            ],
            uv: [u1 / atlas_w, v0 / atlas_h],
            color,
        },
        GpuVertex {
            pos: [
                pen_x as f32,
                pen_y as f32 + (glyph_h16 as f32 / 16.0),
                z,
                1.0,
            ],
            uv: [u0 / atlas_w, v1 / atlas_h],
            color,
        },
        GpuVertex {
            pos: [
                pen_x as f32 + (w16 as f32 / 16.0),
                pen_y as f32 + (glyph_h16 as f32 / 16.0),
                z,
                1.0,
            ],
            uv: [u1 / atlas_w, v1 / atlas_h],
            color,
        },
    );
}

/// Draws a string starting at `(x, y)` (`drawTextL` / `0x21DC28`).
#[allow(clippy::too_many_arguments)]
pub fn draw_text_l(
    cmd_buf: &mut FrameCommandBuffer,
    target: RenderTargetId,
    x: i32,
    y: i32,
    rgba: [u8; 4],
    alpha: u8,
    s: &str,
    scale: f32,
    base_scale: f32,
    pack: &AssetPack,
) -> i32 {
    let mut pen_x = x;
    let ink = [rgba[0], rgba[1], rgba[2], alpha];
    for tok in tokenize_text(s) {
        match tok {
            TextToken::Ascii(g) => {
                emit_glyph(
                    cmd_buf,
                    target,
                    TextureId::FntAscii,
                    pen_x,
                    y,
                    g,
                    false,
                    scale,
                    base_scale,
                    ink,
                    pack,
                );
                pen_x += ((scale * pack.font_ascii_metrics[g][1] as f32) as i32)
                    + ((scale * -3.0) as i32);
            }
            TextToken::OsdGlyph(g) => {
                emit_glyph(
                    cmd_buf,
                    target,
                    TextureId::FntExOsd,
                    pen_x,
                    y,
                    g,
                    true,
                    scale,
                    base_scale,
                    ink,
                    pack,
                );
                pen_x +=
                    ((scale * pack.font_osd_metrics[g][1] as f32) as i32) + ((scale * -3.0) as i32);
            }
            TextToken::Newline => break,
        }
    }
    pen_x
}

/// Draws a centred string (`drawTextC` / `0x21DC88`). Alpha below 16 is skipped (`osdbits/menutext.c`).
#[allow(clippy::too_many_arguments)]
pub fn draw_text_c(
    cmd_buf: &mut FrameCommandBuffer,
    target: RenderTargetId,
    x: i32,
    y: i32,
    rgba: [u8; 4],
    alpha: u8,
    s: &str,
    scale: f32,
    base_scale: f32,
    pack: &AssetPack,
) {
    if alpha < 16 {
        return;
    }
    let w = osd_text_width(s, scale, pack);
    draw_text_l(
        cmd_buf,
        target,
        x - w / 2,
        y,
        rgba,
        alpha,
        s,
        scale,
        base_scale,
        pack,
    );
}

/// Main menu items (`0x27BE80`): Browser, System Configuration, Version Information (bonus).
pub const MAIN_MENU_ITEMS: [usize; 3] = [STR_BROWSER, STR_SYSTEM_CONFIGURATION, STR_VERSION_INFO];

/// Colours (`0x27B830` / `0x27B840` / `0x27B850` / `0x27B860`).
pub const COL_SELECTED: [u8; 4] = [30, 110, 156, 128];
pub const COL_UNSELECTED: [u8; 4] = [44, 44, 44, 128];
pub const COL_DIM: [u8; 4] = [96, 96, 96, 128];
pub const COL_TITLE: [u8; 4] = [110, 110, 0, 128];
pub const COL_HINT: [u8; 4] = [96, 96, 96, 128];

/// System Configuration items (`0x27BD10`).
pub const CONFIG_ITEMS: [usize; 5] = [
    STR_CLOCK_ADJUSTMENT,
    STR_SCREEN_SIZE,
    STR_DIGITAL_OUT,
    STR_COMPONENT_OUT,
    STR_LANGUAGE,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScreenMode {
    MainMenu,
    SystemConfig,
    ClockEditor,
    ScreenSizeEditor,
    DigitalOutEditor,
    ComponentEditor,
    LanguageEditor,
    VersionInfo,
    MemoryCardBrowser,
}

/// User-facing configuration state persisted across sessions (Mode E).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OsdSettings {
    pub language: u8,
    pub screen_size: u8,    // 0 = 4:3, 1 = Full, 2 = 16:9
    pub digital_out: u8,    // 0 = Off, 1 = On
    pub component_out: u8,  // 0 = Y Cb/Pb Cr/Pr, 1 = RGB
    pub region_japan: bool, // true = Circle Enter / Cross Back
}

impl Default for OsdSettings {
    fn default() -> Self {
        Self {
            language: LANG_ENGLISH,
            screen_size: 0,
            digital_out: 0,
            component_out: 0,
            region_japan: false,
        }
    }
}

/// Complete Mode D interactive UI state machine (`menu.c` + `menuconfig.c` + `menutext.c`).
#[derive(Debug, Clone)]
pub struct OsdUi {
    pub screen: ScreenMode,
    pub menu_cursor: usize,
    pub config_cursor: usize,
    pub settings: OsdSettings,
    pub clock_hours: u32,
    pub clock_minutes: u32,
    pub clock_seconds: u32,
    pub clock_field: usize, // 0 = year, 1 = month, 2 = day, 3 = hour, 4 = minute, 5 = second
    pub clock_year: u32,
    pub clock_month: u32,
    pub clock_day: u32,
    pub clock_hold: bool,
    pub saved_clock: (u32, u32, u32),
    pub boot_history_play_counts: [u8; 21],
    pub browser_slot: usize,
    pub notification: Option<String>,
    pub notification_frames: u32,
    pub base_scale: f32,
    pub version_info_open: bool,
}

impl Default for OsdUi {
    fn default() -> Self {
        Self::new()
    }
}

impl OsdUi {
    pub fn new() -> Self {
        Self {
            screen: ScreenMode::MainMenu,
            menu_cursor: 0,
            config_cursor: 0,
            settings: OsdSettings::default(),
            clock_hours: 12,
            clock_minutes: 34,
            clock_seconds: 56,
            clock_field: 3,
            clock_year: 2000,
            clock_month: 1,
            clock_day: 1,
            clock_hold: false,
            saved_clock: (12, 34, 56),
            boot_history_play_counts: [0u8; 21],
            browser_slot: 0,
            notification: None,
            notification_frames: 0,
            base_scale: 1.0,
            version_info_open: false,
        }
    }

    pub fn notify(&mut self, msg: &str) {
        self.notification = Some(msg.to_string());
        self.notification_frames = 180;
    }

    pub fn step_notification(&mut self) {
        if self.notification_frames > 0 {
            self.notification_frames -= 1;
            if self.notification_frames == 0 {
                self.notification = None;
            }
        }
    }

    /// Returns the list of selectable value strings and range for the item being edited.
    pub fn value_count(&self, item: usize) -> usize {
        match item {
            0 => 6, // clock fields
            1 => 3, // screen size: 4:3 / Full / 16:9
            2 => 2, // digital out: On / Off
            3 => 2, // component: Y Cb/Pb Cr/Pr / RGB
            4 => 7, // 7 languages
            _ => 1,
        }
    }

    pub fn item_value_str(&self, item: usize) -> &'static str {
        match item {
            0 => "",
            1 => match self.settings.screen_size {
                1 => osd_string(self.settings.language, STR_FULL),
                2 => osd_string(self.settings.language, STR_16_9),
                _ => osd_string(self.settings.language, STR_4_3),
            },
            2 => {
                if self.settings.digital_out == 1 {
                    osd_string(self.settings.language, STR_ON)
                } else {
                    osd_string(self.settings.language, STR_OFF)
                }
            }
            3 => {
                if self.settings.component_out == 0 {
                    osd_string(self.settings.language, STR_YCBCR)
                } else {
                    osd_string(self.settings.language, STR_RGB)
                }
            }
            4 => language_name(self.settings.language),
            _ => "",
        }
    }

    /// Runs the interactive UI state machine for one frame (`0x228278`, `0x2279B8`, `0x227BE8`, `0x21E870`).
    pub fn handle_input(
        &mut self,
        pad: &PadCooker,
        sound: &mut SoundMailbox,
        menu: &mut MenuScene,
        config: &mut ConfigScene,
    ) {
        // Escape/cancel to main menu never traps the user
        match self.screen {
            ScreenMode::MainMenu => {
                if ((pad.press & crate::input_audio::PAD_UP) != 0
                    || (pad.dir_press & crate::input_audio::PAD_UP) != 0)
                    && self.menu_cursor > 0
                {
                    self.menu_cursor -= 1;
                    sound.dispatch(SND_CMD_SE_PLAY as i32, 1, SE_CURSOR as i32, 0);
                }
                if ((pad.press & crate::input_audio::PAD_DOWN) != 0
                    || (pad.dir_press & crate::input_audio::PAD_DOWN) != 0)
                    && self.menu_cursor < MAIN_MENU_ITEMS.len() - 1
                {
                    self.menu_cursor += 1;
                    sound.dispatch(SND_CMD_SE_PLAY as i32, 1, SE_CURSOR as i32, 0);
                }
                if (pad.press & crate::input_audio::PAD_CIRCLE) != 0 {
                    sound.dispatch(SND_CMD_SE_PLAY as i32, 1, SE_CONFIRM as i32, 0);
                    match self.menu_cursor {
                        0 => {
                            self.screen = ScreenMode::MemoryCardBrowser;
                        }
                        1 => {
                            self.screen = ScreenMode::SystemConfig;
                            self.config_cursor = 0;
                            menu.start_config(sound);
                        }
                        _ => {
                            self.screen = ScreenMode::VersionInfo;
                            self.version_info_open = true;
                        }
                    }
                }
                if (pad.press & crate::input_audio::PAD_TRIANGLE) != 0 {
                    sound.dispatch(SND_CMD_SE_PLAY as i32, 1, SE_CONFIRM as i32, 0);
                    self.screen = ScreenMode::VersionInfo;
                    self.version_info_open = true;
                }
            }
            ScreenMode::SystemConfig => {
                if (pad.press & crate::input_audio::PAD_CROSS) != 0 {
                    sound.dispatch(SND_CMD_SE_PLAY as i32, 1, SE_CANCEL as i32, 0);
                    self.screen = ScreenMode::MainMenu;
                    menu.leave_config(sound);
                    return;
                }
                if (pad.press & crate::input_audio::PAD_UP) != 0
                    || (pad.dir_press & crate::input_audio::PAD_UP) != 0
                {
                    self.config_cursor =
                        (self.config_cursor + CONFIG_ITEMS.len() - 1) % CONFIG_ITEMS.len();
                    config.cursor = self.config_cursor;
                    sound.dispatch(SND_CMD_SE_PLAY as i32, 1, SE_CURSOR as i32, 0);
                }
                if (pad.press & crate::input_audio::PAD_DOWN) != 0
                    || (pad.dir_press & crate::input_audio::PAD_DOWN) != 0
                {
                    self.config_cursor = (self.config_cursor + 1) % CONFIG_ITEMS.len();
                    config.cursor = self.config_cursor;
                    sound.dispatch(SND_CMD_SE_PLAY as i32, 1, SE_CURSOR as i32, 0);
                }
                if (pad.press & crate::input_audio::PAD_CIRCLE) != 0 {
                    sound.dispatch(SND_CMD_SE_PLAY as i32, 1, SE_CONFIRM as i32, 0);
                    config.kick_cube(self.config_cursor);
                    self.screen = match self.config_cursor {
                        0 => {
                            self.saved_clock =
                                (self.clock_hours, self.clock_minutes, self.clock_seconds);
                            self.clock_seconds = 0;
                            self.clock_hold = true;
                            menu.clock.hold = true;
                            menu.orb_scale_target = 0.0;
                            ScreenMode::ClockEditor
                        }
                        1 => ScreenMode::ScreenSizeEditor,
                        2 => ScreenMode::DigitalOutEditor,
                        3 => ScreenMode::ComponentEditor,
                        _ => ScreenMode::LanguageEditor,
                    };
                }
                if (pad.press & crate::input_audio::PAD_TRIANGLE) != 0 && self.config_cursor == 0 {
                    sound.dispatch(SND_CMD_SE_PLAY as i32, 1, SE_CONFIRM as i32, 0);
                    self.screen = ScreenMode::VersionInfo;
                    self.version_info_open = true;
                }
            }
            ScreenMode::ClockEditor => {
                let n = 6usize;
                if (pad.press & crate::input_audio::PAD_LEFT) != 0 {
                    self.clock_field = (self.clock_field + n - 1) % n;
                    sound.dispatch(SND_CMD_SE_PLAY as i32, 1, SE_CURSOR as i32, 0);
                }
                if (pad.press & crate::input_audio::PAD_RIGHT) != 0 {
                    self.clock_field = (self.clock_field + 1) % n;
                    sound.dispatch(SND_CMD_SE_PLAY as i32, 1, SE_CURSOR as i32, 0);
                }
                let up = (pad.press & crate::input_audio::PAD_UP) != 0
                    || (pad.dir_press & crate::input_audio::PAD_UP) != 0;
                let down = (pad.press & crate::input_audio::PAD_DOWN) != 0
                    || (pad.dir_press & crate::input_audio::PAD_DOWN) != 0;
                if up || down {
                    sound.dispatch(SND_CMD_SE_PLAY as i32, 1, SE_CURSOR as i32, 0);
                    self.step_clock_field(if up { 1 } else { usize::MAX });
                }
                if (pad.press & crate::input_audio::PAD_CIRCLE) != 0 {
                    // Apply (`0x21EAE0`): unpin the clock, orbs return
                    sound.dispatch(SND_CMD_SE_PLAY as i32, 1, SE_CONFIRM as i32, 0);
                    self.clock_hold = false;
                    menu.clock.hold = false;
                    menu.orb_scale_target = 1.0;
                    self.screen = ScreenMode::SystemConfig;
                } else if (pad.press & crate::input_audio::PAD_CROSS) != 0 {
                    // Cancel (`0x21EB30`): restore the pre-edit snapshot
                    sound.dispatch(SND_CMD_SE_PLAY as i32, 1, SE_CANCEL as i32, 0);
                    self.clock_hours = self.saved_clock.0;
                    self.clock_minutes = self.saved_clock.1;
                    self.clock_seconds = self.saved_clock.2;
                    self.clock_hold = false;
                    menu.clock.hold = false;
                    menu.orb_scale_target = 1.0;
                    self.screen = ScreenMode::SystemConfig;
                }
            }
            ScreenMode::ScreenSizeEditor
            | ScreenMode::DigitalOutEditor
            | ScreenMode::ComponentEditor
            | ScreenMode::LanguageEditor => {
                if (pad.press & crate::input_audio::PAD_CROSS) != 0 {
                    sound.dispatch(SND_CMD_SE_PLAY as i32, 1, SE_CANCEL as i32, 0);
                    self.screen = ScreenMode::SystemConfig;
                    return;
                }
                let item = self.config_cursor;
                let n = self.value_count(item);
                let mut changed = false;
                if (pad.press & crate::input_audio::PAD_LEFT) != 0 {
                    changed = true;
                    self.decrement_value(item, n);
                }
                if (pad.press & crate::input_audio::PAD_RIGHT) != 0 {
                    changed = true;
                    self.increment_value(item, n);
                }
                if (pad.press & crate::input_audio::PAD_CIRCLE) != 0 {
                    sound.dispatch(SND_CMD_SE_PLAY as i32, 1, SE_CONFIRM as i32, 0);
                    self.screen = ScreenMode::SystemConfig;
                } else if changed {
                    sound.dispatch(SND_CMD_SE_PLAY as i32, 1, SE_TOGGLE as i32, 0);
                }
            }
            ScreenMode::VersionInfo => {
                if (pad.press & (crate::input_audio::PAD_CROSS | crate::input_audio::PAD_CIRCLE))
                    != 0
                {
                    sound.dispatch(SND_CMD_SE_PLAY as i32, 1, SE_CANCEL as i32, 0);
                    self.version_info_open = false;
                    self.screen = if menu.config_active {
                        ScreenMode::SystemConfig
                    } else {
                        ScreenMode::MainMenu
                    };
                }
            }
            ScreenMode::MemoryCardBrowser => {
                if (pad.press & crate::input_audio::PAD_CROSS) != 0 {
                    sound.dispatch(SND_CMD_SE_PLAY as i32, 1, SE_CANCEL as i32, 0);
                    self.screen = ScreenMode::MainMenu;
                    return;
                }
                if (pad.press & crate::input_audio::PAD_UP) != 0
                    || (pad.dir_press & crate::input_audio::PAD_UP) != 0
                {
                    self.browser_slot = (self.browser_slot + 20) % 21;
                    sound.dispatch(SND_CMD_SE_PLAY as i32, 1, SE_CURSOR as i32, 0);
                }
                if (pad.press & crate::input_audio::PAD_DOWN) != 0
                    || (pad.dir_press & crate::input_audio::PAD_DOWN) != 0
                {
                    self.browser_slot = (self.browser_slot + 1) % 21;
                    sound.dispatch(SND_CMD_SE_PLAY as i32, 1, SE_CURSOR as i32, 0);
                }
                if (pad.press & crate::input_audio::PAD_LEFT) != 0 {
                    let s = &mut self.boot_history_play_counts[self.browser_slot];
                    *s = s.saturating_sub(1);
                    sound.dispatch(SND_CMD_SE_PLAY as i32, 1, SE_TOGGLE as i32, 0);
                }
                if (pad.press & crate::input_audio::PAD_RIGHT) != 0 {
                    let s = &mut self.boot_history_play_counts[self.browser_slot];
                    *s = (*s + 1).min(99);
                    sound.dispatch(SND_CMD_SE_PLAY as i32, 1, SE_TOGGLE as i32, 0);
                }
            }
        }
    }

    fn increment_value(&mut self, item: usize, n: usize) {
        match item {
            1 => self.settings.screen_size = (self.settings.screen_size + 1) % (n as u8),
            2 => self.settings.digital_out = (self.settings.digital_out + 1) % 2,
            3 => self.settings.component_out = (self.settings.component_out + 1) % 2,
            4 => {
                let next = if self.settings.language >= 7 {
                    1
                } else {
                    self.settings.language + 1
                };
                self.settings.language = next;
            }
            _ => {}
        }
    }

    fn decrement_value(&mut self, item: usize, n: usize) {
        match item {
            1 => {
                if self.settings.screen_size == 0 {
                    self.settings.screen_size = (n - 1) as u8;
                } else {
                    self.settings.screen_size -= 1;
                }
            }
            2 => self.settings.digital_out = (self.settings.digital_out + 1) % 2,
            3 => self.settings.component_out = (self.settings.component_out + 1) % 2,
            4 => {
                let prev = if self.settings.language <= 1 {
                    7
                } else {
                    self.settings.language - 1
                };
                self.settings.language = prev;
            }
            _ => {}
        }
    }

    fn step_clock_field(&mut self, dir: usize) {
        let days_in_month = Self::days_in_month(self.clock_year, self.clock_month);
        match self.clock_field {
            0 => {
                // year 2000..2099
                self.clock_year = if dir == 1 {
                    if self.clock_year >= 2099 {
                        2000
                    } else {
                        self.clock_year + 1
                    }
                } else if self.clock_year <= 2000 {
                    2099
                } else {
                    self.clock_year - 1
                };
            }
            1 => {
                self.clock_month = if dir == 1 {
                    if self.clock_month >= 12 {
                        1
                    } else {
                        self.clock_month + 1
                    }
                } else if self.clock_month <= 1 {
                    12
                } else {
                    self.clock_month - 1
                };
            }
            2 => {
                self.clock_day = if dir == 1 {
                    if self.clock_day >= days_in_month {
                        1
                    } else {
                        self.clock_day + 1
                    }
                } else if self.clock_day <= 1 {
                    days_in_month
                } else {
                    self.clock_day - 1
                };
            }
            3 => {
                self.clock_hours = if dir == 1 {
                    (self.clock_hours + 1) % 24
                } else {
                    (self.clock_hours + 23) % 24
                }
            }
            4 => {
                self.clock_minutes = if dir == 1 {
                    (self.clock_minutes + 1) % 60
                } else {
                    (self.clock_minutes + 59) % 60
                }
            }
            _ => {
                self.clock_seconds = if dir == 1 {
                    (self.clock_seconds + 1) % 60
                } else {
                    (self.clock_seconds + 59) % 60
                }
            }
        }
        if self.clock_day > Self::days_in_month(self.clock_year, self.clock_month) {
            self.clock_day = Self::days_in_month(self.clock_year, self.clock_month);
        }
    }

    pub fn days_in_month(year: u32, month: u32) -> u32 {
        match month {
            2 => {
                if (year % 4 == 0 && year % 100 != 0) || year % 400 == 0 {
                    29
                } else {
                    28
                }
            }
            4 | 6 | 9 | 11 => 30,
            _ => 31,
        }
    }
}

pub fn language_name(lang: u8) -> &'static str {
    match lang {
        2 => "French",
        3 => "Spanish",
        4 => "German",
        5 => "Italian",
        6 => "Dutch",
        7 => "Portuguese",
        _ => "English",
    }
}

/// Emits the complete 2D UI overlay for the active screen (Mode D):
/// item labels (`0x228110` / `0x227560`), value editors, letterbox bars, top date/time bar,
/// bottom `TEXCMARU` button hint bar, notifications, and the Memory Card / Boot History browser.
pub fn emit_ui(cmd_buf: &mut FrameCommandBuffer, ui: &OsdUi, menu: &MenuScene, pack: &AssetPack) {
    let screen_w = 640.0f32;
    let screen_h = 448.0f32;
    let lang = ui.settings.language;
    let fade = if menu.fade_mode == 2 {
        menu.fade_alpha
    } else {
        128
    };

    // Letterbox bars (4:3 and 16:9 keep them; Full drops them), drawn behind the text
    if ui.settings.screen_size != 1 {
        let content = screen_w * 0.5625 * 0.88;
        let bar = ((screen_h - content) * 0.5).max(0.0);
        cmd_buf.push_sprite_2d(
            RenderTargetId::Screen,
            TextureId::WhiteDummy,
            GsBlendMode::SourceAlpha,
            SamplerWrap::Clamp,
            128,
            0.0,
            0.0,
            screen_w,
            bar,
            0.0,
            0.0,
            1.0,
            1.0,
            [0, 0, 0, 128],
        );
        cmd_buf.push_sprite_2d(
            RenderTargetId::Screen,
            TextureId::WhiteDummy,
            GsBlendMode::SourceAlpha,
            SamplerWrap::Clamp,
            128,
            0.0,
            screen_h - bar,
            screen_w,
            screen_h,
            0.0,
            0.0,
            1.0,
            1.0,
            [0, 0, 0, 128],
        );
    }

    match ui.screen {
        ScreenMode::MainMenu => {
            let y0 = (screen_h as i32) / 2 - 28;
            for (i, strid) in MAIN_MENU_ITEMS.iter().enumerate() {
                let col = if i == ui.menu_cursor {
                    COL_SELECTED
                } else {
                    COL_UNSELECTED
                };
                draw_text_c(
                    cmd_buf,
                    RenderTargetId::Screen,
                    430,
                    y0 + (i as i32) * 36,
                    col,
                    fade,
                    osd_string(lang, *strid),
                    1.0,
                    ui.base_scale,
                    pack,
                );
            }
            draw_hint_bar(
                cmd_buf,
                &[
                    (0, osd_string(lang, STR_ENTER)),
                    (1, osd_string(lang, STR_OPTIONS)),
                ],
                fade,
                pack,
            );
        }
        ScreenMode::SystemConfig
        | ScreenMode::ClockEditor
        | ScreenMode::ScreenSizeEditor
        | ScreenMode::DigitalOutEditor
        | ScreenMode::ComponentEditor
        | ScreenMode::LanguageEditor => {
            let title_y = 96i32;
            let label_y = title_y + 26;
            let value_y = title_y + 52;
            draw_text_c(
                cmd_buf,
                RenderTargetId::Screen,
                430,
                title_y,
                COL_TITLE,
                fade,
                osd_string(lang, STR_SYSTEM_CONFIGURATION),
                1.0,
                ui.base_scale,
                pack,
            );

            // Page marker arrow (`\7o020` on FNTEXOSD)
            draw_text_themed(
                cmd_buf,
                430,
                label_y,
                COL_DIM,
                fade,
                "\\7o020",
                pack,
                ui.base_scale,
            );

            if (ui.screen as u32) >= (ScreenMode::ClockEditor as u32) {
                // Item header + expanded editor row
                draw_text_c(
                    cmd_buf,
                    RenderTargetId::Screen,
                    430,
                    label_y,
                    COL_SELECTED,
                    fade,
                    osd_string(lang, CONFIG_ITEMS[ui.config_cursor]),
                    1.0,
                    ui.base_scale,
                    pack,
                );
                match ui.screen {
                    ScreenMode::ClockEditor => emit_clock_editor(cmd_buf, ui, value_y, pack, fade),
                    ScreenMode::ScreenSizeEditor => emit_value_options(
                        cmd_buf,
                        ui,
                        value_y,
                        &[
                            osd_string(lang, STR_4_3),
                            osd_string(lang, STR_FULL),
                            osd_string(lang, STR_16_9),
                        ],
                        ui.settings.screen_size as usize,
                        pack,
                        fade,
                    ),
                    ScreenMode::DigitalOutEditor => emit_value_options(
                        cmd_buf,
                        ui,
                        value_y,
                        &[osd_string(lang, STR_ON), osd_string(lang, STR_OFF)],
                        if ui.settings.digital_out == 1 { 0 } else { 1 },
                        pack,
                        fade,
                    ),
                    ScreenMode::ComponentEditor => emit_value_options(
                        cmd_buf,
                        ui,
                        value_y,
                        &[osd_string(lang, STR_YCBCR), osd_string(lang, STR_RGB)],
                        ui.settings.component_out as usize,
                        pack,
                        fade,
                    ),
                    _ => emit_value_options(
                        cmd_buf,
                        ui,
                        value_y,
                        &[
                            osd_string(lang, STR_ENGLISH),
                            osd_string(lang, STR_FRENCH),
                            osd_string(lang, STR_SPANISH),
                            osd_string(lang, STR_GERMAN),
                            osd_string(lang, STR_ITALIAN),
                            osd_string(lang, STR_DUTCH),
                            osd_string(lang, STR_PORTUGUESE),
                        ],
                        (ui.settings.language.max(1) as usize) - 1,
                        pack,
                        fade,
                    ),
                }
            } else {
                draw_text_c(
                    cmd_buf,
                    RenderTargetId::Screen,
                    430,
                    label_y,
                    COL_SELECTED,
                    fade,
                    osd_string(lang, CONFIG_ITEMS[ui.config_cursor]),
                    1.0,
                    ui.base_scale,
                    pack,
                );
                if ui.config_cursor == 0 {
                    emit_clock_row(cmd_buf, ui, value_y, false, pack, fade);
                } else {
                    draw_text_c(
                        cmd_buf,
                        RenderTargetId::Screen,
                        430,
                        value_y,
                        COL_DIM,
                        fade,
                        ui.item_value_str(ui.config_cursor),
                        1.0,
                        ui.base_scale,
                        pack,
                    );
                }
            }

            let mut hints: Vec<(u8, &'static str)> = vec![
                (1, osd_string(lang, STR_ENTER)),
                (2, osd_string(lang, STR_BACK)),
            ];
            if ui.screen == ScreenMode::SystemConfig && ui.config_cursor == 0 {
                hints.push((4, osd_string(lang, STR_OPTIONS)));
            }
            draw_hint_bar(cmd_buf, &hints, fade, pack);
        }
        ScreenMode::VersionInfo => {
            cmd_buf.push_sprite_2d(
                RenderTargetId::Screen,
                TextureId::WhiteDummy,
                GsBlendMode::SourceAlpha,
                SamplerWrap::Clamp,
                128,
                120.0,
                120.0,
                520.0,
                328.0,
                0.0,
                0.0,
                1.0,
                1.0,
                [10, 20, 40, 200],
            );
            draw_text_c(
                cmd_buf,
                RenderTargetId::Screen,
                320,
                150,
                COL_TITLE,
                128,
                osd_string(lang, STR_VERSION_INFO),
                1.0,
                ui.base_scale,
                pack,
            );
            let lines = [
                "PlayStation 2 OSDSYS",
                "osdbits-wasm reconstruction",
                "Original reverse engineering by aap",
                "Rust + WebAssembly + WebGPU",
                "Version 1.0.0",
            ];
            for (i, line) in lines.iter().enumerate() {
                draw_text_c(
                    cmd_buf,
                    RenderTargetId::Screen,
                    320,
                    192 + (i as i32) * 24,
                    COL_DIM,
                    110,
                    line,
                    0.9,
                    ui.base_scale,
                    pack,
                );
            }
            draw_hint_bar(cmd_buf, &[(2, osd_string(lang, STR_BACK))], 128, pack);
        }
        ScreenMode::MemoryCardBrowser => {
            draw_text_c(
                cmd_buf,
                RenderTargetId::Screen,
                320,
                56,
                COL_TITLE,
                128,
                "Memory Card / Boot History",
                1.0,
                ui.base_scale,
                pack,
            );
            draw_text_c(
                cmd_buf,
                RenderTargetId::Screen,
                320,
                82,
                COL_DIM,
                110,
                "These 21 slots generate the Opening tower field",
                0.85,
                ui.base_scale,
                pack,
            );
            for slot in 0..21usize {
                let col_i = slot % 3;
                let row_i = slot / 3;
                let x = 64 + (col_i as i32) * 184;
                let y = 124 + (row_i as i32) * 24;
                let count = ui.boot_history_play_counts[slot];
                let label = format!("Slot {:02}: {:2} plays", slot + 1, count);
                let col = if slot == ui.browser_slot {
                    COL_SELECTED
                } else if count > 0 {
                    COL_DIM
                } else {
                    [40, 40, 40, 128]
                };
                draw_text_l(
                    cmd_buf,
                    RenderTargetId::Screen,
                    x,
                    y,
                    col,
                    if slot == ui.browser_slot { 128 } else { 100 },
                    &label,
                    0.8,
                    ui.base_scale,
                    pack,
                );
            }
            let active = ui
                .boot_history_play_counts
                .iter()
                .filter(|&&c| c > 0)
                .count();
            let summary = format!("{active} active slots - LEFT/RIGHT change - CROSS back");
            draw_text_c(
                cmd_buf,
                RenderTargetId::Screen,
                320,
                430,
                COL_DIM,
                110,
                &summary,
                0.8,
                ui.base_scale,
                pack,
            );
            draw_hint_bar(cmd_buf, &[(2, osd_string(lang, STR_BACK))], 128, pack);
        }
    }

    // Top date/time bar (visible inside System Configuration, `0x226A60`)
    if menu.config_active || (ui.screen as u32) >= (ScreenMode::SystemConfig as u32) {
        let date = format!(
            "{:04}/{:02}/{:02}",
            ui.clock_year, ui.clock_month, ui.clock_day
        );
        let time = format!(
            "{:02}:{:02}:{:02}",
            ui.clock_hours, ui.clock_minutes, ui.clock_seconds
        );
        draw_text_l(
            cmd_buf,
            RenderTargetId::Screen,
            20,
            14,
            COL_HINT,
            fade,
            &date,
            0.82,
            ui.base_scale,
            pack,
        );
        let tw = osd_text_width(&time, 0.82, pack);
        draw_text_l(
            cmd_buf,
            RenderTargetId::Screen,
            620 - tw,
            14,
            COL_HINT,
            fade,
            &time,
            0.82,
            ui.base_scale,
            pack,
        );
    }

    if let Some(msg) = &ui.notification {
        draw_text_c(
            cmd_buf,
            RenderTargetId::Screen,
            320,
            screen_h as i32 - 70,
            [255, 240, 180, 128],
            120,
            msg,
            0.85,
            ui.base_scale,
            pack,
        );
    }
}

fn draw_text_themed(
    cmd_buf: &mut FrameCommandBuffer,
    x: i32,
    y: i32,
    rgba: [u8; 4],
    alpha: u8,
    s: &str,
    pack: &AssetPack,
    base_scale: f32,
) {
    let w = osd_text_width(s, 1.0, pack);
    draw_text_l(
        cmd_buf,
        RenderTargetId::Screen,
        x - w / 2,
        y,
        rgba,
        alpha,
        s,
        1.0,
        base_scale,
        pack,
    );
}

fn emit_value_options(
    cmd_buf: &mut FrameCommandBuffer,
    ui: &OsdUi,
    y: i32,
    options: &[&str],
    selected: usize,
    pack: &AssetPack,
    fade: u8,
) {
    let scale = 1.0f32;
    let mut total = 0i32;
    for opt in options {
        total += osd_text_width(opt, scale, pack) + osd_text_width(" ", scale, pack);
    }
    let mut x = 430 - total / 2;
    for (i, opt) in options.iter().enumerate() {
        let col = if i == selected {
            COL_SELECTED
        } else {
            COL_UNSELECTED
        };
        let end = draw_text_l(
            cmd_buf,
            RenderTargetId::Screen,
            x,
            y,
            col,
            fade,
            opt,
            scale,
            ui.base_scale,
            pack,
        );
        x = end + osd_text_width(" ", scale, pack);
    }
}

fn emit_clock_row(
    cmd_buf: &mut FrameCommandBuffer,
    ui: &OsdUi,
    y: i32,
    editing: bool,
    pack: &AssetPack,
    fade: u8,
) {
    let template = "0000/00/00 00:00:00";
    let mut x = 430 - osd_text_width(template, 1.0, pack) / 2;
    let fields = [
        format!("{:04}", ui.clock_year),
        format!("{:02}", ui.clock_month),
        format!("{:02}", ui.clock_day),
        format!("{:02}", ui.clock_hours),
        format!("{:02}", ui.clock_minutes),
        format!("{:02}", ui.clock_seconds),
    ];
    let seps = ["/", "/", " ", ":", ":", ""];
    for i in 0..6 {
        let col = if editing && i == ui.clock_field {
            COL_SELECTED
        } else if editing {
            COL_UNSELECTED
        } else {
            COL_DIM
        };
        let end = draw_text_l(
            cmd_buf,
            RenderTargetId::Screen,
            x,
            y,
            col,
            fade,
            &fields[i],
            1.0,
            ui.base_scale,
            pack,
        );
        x = end;
        if !seps[i].is_empty() {
            let end = draw_text_l(
                cmd_buf,
                RenderTargetId::Screen,
                x,
                y,
                COL_DIM,
                fade,
                seps[i],
                1.0,
                ui.base_scale,
                pack,
            );
            x = end;
        }
    }
}

fn emit_clock_editor(
    cmd_buf: &mut FrameCommandBuffer,
    ui: &OsdUi,
    y: i32,
    pack: &AssetPack,
    fade: u8,
) {
    emit_clock_row(cmd_buf, ui, y, true, pack, fade);
}

/// Button icons in `TEXCMARU` (`0x21D590`, 4 icons of 16×16 in a 64×64 sheet).
pub const ICON_CROSS: usize = 1;
pub const ICON_CIRCLE: usize = 0;
pub const ICON_SQUARE: usize = 2;
pub const ICON_TRIANGLE: usize = 3;

fn emit_icon(cmd_buf: &mut FrameCommandBuffer, icon: usize, x: f32, y: f32, alpha: u8) {
    let u0 = (icon % 4) as f32 * 0.25;
    let v0 = 0.0f32;
    let u1 = u0 + 0.25;
    let v1 = 0.25;
    let a = alpha as f32 / 128.0;
    cmd_buf.push_quad(
        RenderTargetId::Screen,
        TextureId::TexcMaru,
        GsBlendMode::SourceAlpha,
        SamplerWrap::Clamp,
        DepthMode::AlwaysNoWrite,
        128,
        GpuVertex {
            pos: [x, y, 0.03, 1.0],
            uv: [u0, v0],
            color: [1.0, 1.0, 1.0, a],
        },
        GpuVertex {
            pos: [x + 16.0, y, 0.03, 1.0],
            uv: [u1, v0],
            color: [1.0, 1.0, 1.0, a],
        },
        GpuVertex {
            pos: [x, y + 16.0, 0.03, 1.0],
            uv: [u0, v1],
            color: [1.0, 1.0, 1.0, a],
        },
        GpuVertex {
            pos: [x + 16.0, y + 16.0, 0.03, 1.0],
            uv: [u1, v1],
            color: [1.0, 1.0, 1.0, a],
        },
    );
}

fn draw_hint_bar(
    cmd_buf: &mut FrameCommandBuffer,
    hints: &[(u8, &'static str)],
    alpha: u8,
    pack: &AssetPack,
) {
    let y = 410.0f32;
    let mut x = 32.0f32;
    for (icon, label) in hints {
        emit_icon(cmd_buf, *icon as usize, x, y, alpha);
        draw_text_l(
            cmd_buf,
            RenderTargetId::Screen,
            (x + 24.0) as i32,
            (y + 1.0) as i32,
            COL_HINT,
            alpha,
            label,
            0.8,
            1.0,
            pack,
        );
        x += 32.0 + osd_text_width(label, 0.8, pack) as f32;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input_audio::{PadCooker, PAD_DOWN, PAD_UP, PHYS_CANCEL_WORLD, PHYS_CONFIRM_WORLD};

    #[test]
    fn test_text_tokenizer_and_escapes() {
        let toks = tokenize_text("PlayStation\x07o004");
        assert_eq!(toks.len(), 12);
        assert_eq!(toks[0], TextToken::Ascii(('P' as usize) - 32));
        assert_eq!(toks[11], TextToken::OsdGlyph(4));

        let scale_toks = tokenize_text("A\x07r0.90B\x07r0.00");
        assert_eq!(scale_toks, vec![TextToken::Ascii(33), TextToken::Ascii(34)]);
    }

    #[test]
    fn test_tokenizer_never_indexes_past_the_atlas_for_non_ascii() {
        // UTF-8 continuation bytes, control codes and a typographic middle dot must all be
        // mapped into the atlas range instead of panicking.
        let toks = tokenize_text("a \u{00b7} ←/→ \u{00e9} \u{0001}z");
        for tok in &toks {
            if let TextToken::Ascii(g) = tok {
                assert!(*g < 97, "glyph index {g} out of atlas range");
            }
        }
    }

    #[test]
    fn test_days_in_month_leap_years() {
        assert_eq!(OsdUi::days_in_month(2000, 2), 29);
        assert_eq!(OsdUi::days_in_month(1900, 2), 28);
        assert_eq!(OsdUi::days_in_month(2004, 2), 29);
        assert_eq!(OsdUi::days_in_month(2023, 2), 28);
        assert_eq!(OsdUi::days_in_month(2023, 4), 30);
        assert_eq!(OsdUi::days_in_month(2023, 12), 31);
    }

    #[test]
    fn test_main_menu_navigation_and_system_config_entry() {
        let mut mbox = SoundMailbox::new();
        let mut ui = OsdUi::new();
        let mut menu = MenuScene::new(1, 12, 34, 56);
        let mut cfg = ConfigScene::new();

        let mut pad = PadCooker::new(true);
        pad.step(PAD_DOWN);
        ui.handle_input(&pad, &mut mbox, &mut menu, &mut cfg);
        assert_eq!(ui.menu_cursor, 1);
        pad.step(0);
        ui.handle_input(&pad, &mut mbox, &mut menu, &mut cfg);

        pad.step(PHYS_CONFIRM_WORLD);
        ui.handle_input(&pad, &mut mbox, &mut menu, &mut cfg);
        assert_eq!(ui.screen, ScreenMode::SystemConfig);
        assert!(menu.config_active);

        pad.step(0);
        ui.handle_input(&pad, &mut mbox, &mut menu, &mut cfg);
        pad.step(PHYS_CANCEL_WORLD);
        ui.handle_input(&pad, &mut mbox, &mut menu, &mut cfg);
        assert_eq!(ui.screen, ScreenMode::MainMenu);
        assert!(!menu.config_active);
    }

    #[test]
    fn test_clock_editor_cancel_restores_snapshot() {
        let mut mbox = SoundMailbox::new();
        let mut ui = OsdUi::new();
        let mut menu = MenuScene::new(1, 12, 34, 56);
        let mut cfg = ConfigScene::new();
        let mut pad = PadCooker::new(true);

        ui.screen = ScreenMode::SystemConfig;
        ui.config_cursor = 0;
        pad.step(PHYS_CONFIRM_WORLD);
        ui.handle_input(&pad, &mut mbox, &mut menu, &mut cfg);
        assert_eq!(ui.screen, ScreenMode::ClockEditor);
        assert!(ui.clock_hold);

        pad.step(0);
        ui.handle_input(&pad, &mut mbox, &mut menu, &mut cfg);
        pad.step(PAD_UP);
        ui.handle_input(&pad, &mut mbox, &mut menu, &mut cfg);
        assert_ne!(ui.clock_hours, 12);

        pad.step(0);
        ui.handle_input(&pad, &mut mbox, &mut menu, &mut cfg);
        pad.step(PHYS_CANCEL_WORLD);
        ui.handle_input(&pad, &mut mbox, &mut menu, &mut cfg);
        assert_eq!(ui.screen, ScreenMode::SystemConfig);
        assert_eq!(ui.clock_hours, 12);
        assert!(!ui.clock_hold);
    }
}
