//! Look and feel: fonts (one family for Latin and Japanese so both sit on
//! the same line), icons, colors, rounding, spacing, the light/dark theme
//! and the app icon.

use std::sync::Arc;

use eframe::egui::{
    self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Stroke, TextStyle, ThemePreference,
    Vec2, epaint::text::FontTweak,
};
use serde::{Deserialize, Serialize};

pub use egui_phosphor::regular as icon;

/// The accent color of buttons, selections and sliders.
pub const ACCENT: Color32 = Color32::from_rgb(64, 132, 255);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ThemeChoice {
    #[default]
    System,
    Light,
    Dark,
}

impl ThemeChoice {
    pub const ALL: [ThemeChoice; 3] = [ThemeChoice::System, ThemeChoice::Light, ThemeChoice::Dark];

    pub fn name(self, japanese: bool) -> &'static str {
        match (self, japanese) {
            (ThemeChoice::System, false) => "Match the system",
            (ThemeChoice::System, true) => "システムに合わせる",
            (ThemeChoice::Light, false) => "Light",
            (ThemeChoice::Light, true) => "ライト",
            (ThemeChoice::Dark, false) => "Dark",
            (ThemeChoice::Dark, true) => "ダーク",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            ThemeChoice::System => icon::CIRCLE_HALF,
            ThemeChoice::Light => icon::SUN,
            ThemeChoice::Dark => icon::MOON,
        }
    }

    pub fn apply(self, ctx: &egui::Context) {
        ctx.set_theme(match self {
            ThemeChoice::System => ThemePreference::System,
            ThemeChoice::Light => ThemePreference::Light,
            ThemeChoice::Dark => ThemePreference::Dark,
        });
    }
}

/// The font family for headings and the mode switch.
pub fn bold() -> FontFamily {
    FontFamily::Name("bold".into())
}

/// A font file's bytes and the face to use in it.
type FontFile = (Vec<u8>, u32);

/// A system font with Latin and Japanese glyphs: its regular and bold files.
fn system_fonts() -> Option<(FontFile, FontFile)> {
    let candidates: &[(&str, &str, u32)] = &[
        ("/System/Library/Fonts/ヒラギノ角ゴシック W4.ttc", "/System/Library/Fonts/ヒラギノ角ゴシック W6.ttc", 0),
        ("/System/Library/Fonts/ヒラギノ角ゴシック W3.ttc", "/System/Library/Fonts/ヒラギノ角ゴシック W6.ttc", 0),
        // Meiryo UI is face 2 of the Meiryo collections.
        ("C:\\Windows\\Fonts\\meiryo.ttc", "C:\\Windows\\Fonts\\meiryob.ttc", 2),
        ("C:\\Windows\\Fonts\\YuGothM.ttc", "C:\\Windows\\Fonts\\YuGothB.ttc", 0),
        (
            "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/opentype/noto/NotoSansCJK-Bold.ttc",
            0,
        ),
    ];
    for &(regular, bold, index) in candidates {
        if let Ok(regular_bytes) = std::fs::read(regular) {
            let bold_bytes = std::fs::read(bold).unwrap_or_else(|_| regular_bytes.clone());
            return Some(((regular_bytes, index), (bold_bytes, index)));
        }
    }
    None
}

fn font(bytes: Vec<u8>, index: u32) -> Arc<FontData> {
    let mut data = FontData::from_owned(bytes);
    data.index = index;
    // CJK fonts sit high in egui's line box; nudge the glyphs down so text
    // is centred in buttons and fields.
    data.tweak = FontTweak { y_offset_factor: 0.07, ..Default::default() };
    Arc::new(data)
}

pub fn install(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let mut bold_family = Vec::new();
    if let Some(((regular, regular_index), (bold, bold_index))) = system_fonts() {
        fonts.font_data.insert("ui".into(), font(regular, regular_index));
        fonts.font_data.insert("ui-bold".into(), font(bold, bold_index));
        // The system font first, for Latin too, so both scripts match.
        if let Some(family) = fonts.families.get_mut(&FontFamily::Proportional) {
            family.insert(0, "ui".into());
        }
        if let Some(family) = fonts.families.get_mut(&FontFamily::Monospace) {
            family.push("ui".into());
        }
        bold_family.push("ui-bold".to_string());
    }
    egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
    // Bold falls back to the regular family (icons and emoji included).
    bold_family.extend(fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default());
    fonts.families.insert(bold(), bold_family);
    ctx.set_fonts(fonts);

    ctx.all_styles_mut(|style| {
        style.text_styles = [
            (TextStyle::Small, FontId::proportional(11.0)),
            (TextStyle::Body, FontId::proportional(13.5)),
            (TextStyle::Button, FontId::proportional(13.5)),
            (TextStyle::Heading, FontId::new(19.0, bold())),
            (TextStyle::Monospace, FontId::monospace(12.5)),
        ]
        .into();
        let spacing = &mut style.spacing;
        spacing.item_spacing = Vec2::new(8.0, 7.0);
        spacing.button_padding = Vec2::new(10.0, 5.0);
        spacing.interact_size = Vec2::new(40.0, 26.0);
        spacing.slider_width = 150.0;
        spacing.combo_width = 140.0;
        spacing.window_margin = egui::Margin::same(14);
        spacing.menu_margin = egui::Margin::same(8);

        let visuals = &mut style.visuals;
        let radius = CornerRadius::same(7);
        for widget in [
            &mut visuals.widgets.noninteractive,
            &mut visuals.widgets.inactive,
            &mut visuals.widgets.hovered,
            &mut visuals.widgets.active,
            &mut visuals.widgets.open,
        ] {
            widget.corner_radius = radius;
        }
        visuals.widgets.hovered.expansion = 0.0;
        visuals.widgets.active.expansion = 0.0;
        visuals.window_corner_radius = CornerRadius::same(12);
        visuals.menu_corner_radius = CornerRadius::same(10);
        visuals.selection.bg_fill = ACCENT;
        visuals.selection.stroke = Stroke::new(1.0, Color32::WHITE);
        visuals.hyperlink_color = ACCENT;
        visuals.slider_trailing_fill = true;
        if visuals.dark_mode {
            visuals.panel_fill = Color32::from_rgb(28, 30, 34);
            visuals.window_fill = Color32::from_rgb(34, 36, 41);
            visuals.extreme_bg_color = Color32::from_rgb(20, 21, 24);
            visuals.widgets.inactive.weak_bg_fill = Color32::from_rgb(48, 51, 58);
            visuals.widgets.inactive.bg_fill = Color32::from_rgb(48, 51, 58);
            visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(60, 64, 73);
            visuals.widgets.hovered.bg_fill = Color32::from_rgb(60, 64, 73);
        } else {
            visuals.panel_fill = Color32::from_rgb(246, 247, 249);
            visuals.window_fill = Color32::from_rgb(252, 252, 253);
            visuals.extreme_bg_color = Color32::WHITE;
            visuals.widgets.inactive.weak_bg_fill = Color32::from_rgb(228, 231, 236);
            visuals.widgets.inactive.bg_fill = Color32::from_rgb(222, 225, 231);
            visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(214, 219, 228);
            visuals.widgets.hovered.bg_fill = Color32::from_rgb(214, 219, 228);
            visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, Color32::from_rgb(218, 221, 227));
        }
        visuals.widgets.inactive.bg_stroke = Stroke::NONE;
        visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, ACCENT.gamma_multiply(0.6));
    });
}

/// The app icon: a rounded square with piano keys and a waveform, `size`
/// pixels square, RGBA.
pub fn app_icon(size: u32) -> egui::IconData {
    let s = size as f32;
    let mut rgba = vec![0u8; (size * size * 4) as usize];
    let radius = s * 0.22;
    let inside = |x: f32, y: f32| -> f32 {
        // Coverage of the rounded square at (x, y), anti-aliased.
        let (cx, cy) = (x.clamp(radius, s - radius), y.clamp(radius, s - radius));
        let d = ((x - cx).powi(2) + (y - cy).powi(2)).sqrt();
        (radius - d + 0.5).clamp(0.0, 1.0)
    };
    for py in 0..size {
        for px in 0..size {
            let (x, y) = (px as f32 + 0.5, py as f32 + 0.5);
            let coverage = inside(x, y);
            if coverage <= 0.0 {
                continue;
            }
            // Indigo to violet, top to bottom.
            let t = y / s;
            let mut color = [(52.0 + 60.0 * t), (64.0 - 20.0 * t), (190.0 + 20.0 * t)];
            // Keys in the lower third.
            let keys_top = s * 0.62;
            if y > keys_top && y < s * 0.9 && x > s * 0.12 && x < s * 0.88 {
                let key_width = (s * 0.76) / 7.0;
                let k = ((x - s * 0.12) / key_width).floor();
                let within = (x - s * 0.12) - k * key_width;
                color = if within < key_width * 0.06 { [70.0, 70.0, 110.0] } else { [245.0, 245.0, 250.0] };
                let black =
                    [0.0, 1.0, 3.0, 4.0, 5.0].contains(&k) && within > key_width * 0.68 && y < keys_top + s * 0.16;
                let black_left =
                    [1.0, 2.0, 4.0, 5.0, 6.0].contains(&k) && within < key_width * 0.32 && y < keys_top + s * 0.16;
                if black || black_left {
                    color = [30.0, 30.0, 45.0];
                }
            }
            // A waveform above the keys.
            let wave_y =
                s * 0.38 + (x / s * 3.0 * std::f32::consts::TAU).sin() * s * 0.1 * (1.0 - (x / s - 0.5).abs() * 1.4);
            let distance = (y - wave_y).abs();
            if x > s * 0.1 && x < s * 0.9 && distance < s * 0.03 {
                let a = (1.0 - distance / (s * 0.03)).clamp(0.0, 1.0);
                color = [
                    color[0] * (1.0 - a) + 255.0 * a,
                    color[1] * (1.0 - a) + 196.0 * a,
                    color[2] * (1.0 - a) + 70.0 * a,
                ];
            }
            let i = ((py * size + px) * 4) as usize;
            rgba[i] = color[0] as u8;
            rgba[i + 1] = color[1] as u8;
            rgba[i + 2] = color[2] as u8;
            rgba[i + 3] = (coverage * 255.0) as u8;
        }
    }
    egui::IconData { rgba, width: size, height: size }
}

/// A section heading: bold, a little larger than body text.
pub fn section(text: impl Into<String>) -> egui::RichText {
    egui::RichText::new(text).family(bold()).size(14.5)
}

/// An icon followed by a label.
pub fn labeled(glyph: &str, text: &str) -> String {
    format!("{glyph}  {text}")
}
