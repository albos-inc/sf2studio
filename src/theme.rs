//! Look and feel: fonts (one family for Latin and Japanese so both sit on
//! the same line), icons, colors, rounding, spacing, the light/dark theme
//! and the app icon (drawn in `assets/icon.svg`).

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

/// The app icon (`assets/icon.svg`, rendered by `cargo run --example make_icon`).
pub fn app_icon() -> egui::IconData {
    egui::IconData { rgba: include_bytes!("../assets/icon-256.rgba").to_vec(), width: 256, height: 256 }
}

/// A section heading: bold, a little larger than body text.
pub fn section(text: impl Into<String>) -> egui::RichText {
    egui::RichText::new(text).family(bold()).size(14.5)
}

/// An icon followed by a label.
pub fn labeled(glyph: &str, text: &str) -> String {
    format!("{glyph}  {text}")
}
