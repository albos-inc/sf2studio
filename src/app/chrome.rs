//! The window's chrome: the top bar (mode switch, language, theme, help
//! menu) and the Help, Updates and About windows.

use std::sync::mpsc::{Receiver, channel};

use eframe::egui::{self, Align, Color32, CornerRadius, Layout, RichText, Sense, Stroke, TextureHandle, Vec2};

use super::{Mode, StudioApp};
use crate::theme::{self, ACCENT, ThemeChoice, icon};

pub const REPOSITORY: &str = "https://github.com/albos-inc/sf2studio";
const LATEST_RELEASE: &str = "https://api.github.com/repos/albos-inc/sf2studio/releases/latest";
const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The result of looking for a newer version.
pub enum UpdateCheck {
    Checking(Receiver<Result<Option<(String, String)>, String>>),
    /// The newest release's version and page, or `None` if there is none.
    Done(Result<Option<(String, String)>, String>),
}

#[derive(Default)]
pub struct Chrome {
    pub help: bool,
    pub about: bool,
    pub updates: bool,
    pub update: Option<UpdateCheck>,
    pub icon: Option<TextureHandle>,
}

/// `v1.2.3` → (1, 2, 3).
fn version_numbers(text: &str) -> (u32, u32, u32) {
    let mut parts = text.trim_start_matches('v').split('.').map(|p| p.parse::<u32>().unwrap_or(0));
    (parts.next().unwrap_or(0), parts.next().unwrap_or(0), parts.next().unwrap_or(0))
}

fn fetch_latest() -> Result<Option<(String, String)>, String> {
    // The system's TLS (Security on macOS, SChannel on Windows).
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .tls_config(ureq::tls::TlsConfig::builder().provider(ureq::tls::TlsProvider::NativeTls).build())
        .build()
        .into();
    let response = agent
        .get(LATEST_RELEASE)
        .header("User-Agent", concat!("sf2studio/", env!("CARGO_PKG_VERSION")))
        .header("Accept", "application/vnd.github+json")
        .call();
    let mut response = match response {
        Ok(response) => response,
        // No release published yet.
        Err(ureq::Error::StatusCode(404)) => return Ok(None),
        Err(error) => return Err(error.to_string()),
    };
    let body = response.body_mut().read_to_string().map_err(|e| e.to_string())?;
    let json: serde_json::Value = serde_json::from_str(&body).map_err(|e| e.to_string())?;
    let tag = json["tag_name"].as_str().unwrap_or_default().to_string();
    let page = json["html_url"].as_str().unwrap_or(REPOSITORY).to_string();
    Ok(Some((tag, page)))
}

/// A row of buttons that look like one control; returns the index clicked.
fn segmented(ui: &mut egui::Ui, items: &[(&str, &str)], selected: usize) -> Option<usize> {
    let mut clicked = None;
    let dark = ui.visuals().dark_mode;
    let track = if dark { Color32::from_rgb(40, 43, 49) } else { Color32::from_rgb(226, 229, 235) };
    egui::Frame::new().fill(track).corner_radius(CornerRadius::same(9)).inner_margin(3).show(ui, |ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        ui.horizontal(|ui| {
            for (i, (glyph, label)) in items.iter().enumerate() {
                let on = i == selected;
                let text = RichText::new(format!("{glyph}  {label}")).family(theme::bold()).size(14.0).color(if on {
                    Color32::WHITE
                } else {
                    ui.visuals().text_color()
                });
                let button = egui::Button::new(text)
                    .fill(if on { ACCENT } else { Color32::TRANSPARENT })
                    .stroke(Stroke::NONE)
                    .corner_radius(CornerRadius::same(7))
                    .min_size(Vec2::new(116.0, 30.0));
                if ui.add(button).clicked() {
                    clicked = Some(i);
                }
            }
        });
    });
    clicked
}

impl StudioApp {
    pub(super) fn top_bar(&mut self, ui: &mut egui::Ui) {
        let japanese = self.japanese;
        let t = |en: &'static str, ja: &'static str| if japanese { ja } else { en };
        ui.horizontal(|ui| {
            let icon_texture = self
                .chrome
                .icon
                .get_or_insert_with(|| {
                    let data = theme::app_icon(64);
                    let image = egui::ColorImage::from_rgba_unmultiplied([64, 64], &data.rgba);
                    ui.ctx().load_texture("app-icon", image, egui::TextureOptions::LINEAR)
                })
                .clone();
            ui.add(egui::Image::new(&icon_texture).fit_to_exact_size(Vec2::splat(26.0)));
            ui.label(RichText::new("sf2studio").family(theme::bold()).size(17.0));
            ui.add_space(14.0);
            let modes = [Mode::Create, Mode::Tune, Mode::Compare];
            let items = [
                (icon::MAGIC_WAND, t("Create", "作成")),
                (icon::PIANO_KEYS, t("Tune", "調整再生")),
                (icon::SCALES, t("Compare", "比較")),
            ];
            let selected = modes.iter().position(|&m| m == self.mode).unwrap_or(0);
            if let Some(i) = segmented(ui, &items, selected) {
                self.set_mode(modes[i]);
            }

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.menu_button(RichText::new(format!("{}  {}", icon::QUESTION, t("Help", "ヘルプ"))), |ui| {
                    if ui.button(format!("{}  {}", icon::BOOK_OPEN, t("How to use", "使い方"))).clicked() {
                        self.chrome.help = true;
                        ui.close();
                    }
                    if ui
                        .button(format!("{}  {}", icon::ARROWS_CLOCKWISE, t("Check for updates", "更新を確認")))
                        .clicked()
                    {
                        self.chrome.updates = true;
                        self.check_for_updates(ui.ctx());
                        ui.close();
                    }
                    if ui
                        .button(format!(
                            "{}  {}",
                            icon::GITHUB_LOGO,
                            t("Open the project page", "プロジェクトのページを開く")
                        ))
                        .clicked()
                    {
                        ui.ctx().open_url(egui::OpenUrl::new_tab(REPOSITORY));
                        ui.close();
                    }
                    ui.separator();
                    if ui.button(format!("{}  {}", icon::INFO, t("About sf2studio", "sf2studio について"))).clicked()
                    {
                        self.chrome.about = true;
                        ui.close();
                    }
                });
                ui.menu_button(RichText::new(self.theme.icon()).size(16.0), |ui| {
                    for choice in ThemeChoice::ALL {
                        let label = format!("{}  {}", choice.icon(), choice.name(japanese));
                        if ui.selectable_label(self.theme == choice, label).clicked() {
                            self.theme = choice;
                            choice.apply(ui.ctx());
                            ui.close();
                        }
                    }
                })
                .response
                .on_hover_text(t("Theme", "テーマ"));
                ui.menu_button(
                    RichText::new(format!("{}  {}", icon::GLOBE, if japanese { "日本語" } else { "English" })),
                    |ui| {
                        if ui.selectable_label(!self.japanese, "English").clicked() {
                            self.japanese = false;
                            ui.close();
                        }
                        if ui.selectable_label(self.japanese, "日本語").clicked() {
                            self.japanese = true;
                            ui.close();
                        }
                    },
                );
                if let Some(error) = &self.player_error {
                    ui.colored_label(Color32::LIGHT_RED, format!("{}  {error}", icon::SPEAKER_SLASH));
                }
            });
        });
    }

    fn check_for_updates(&mut self, ctx: &egui::Context) {
        if matches!(self.chrome.update, Some(UpdateCheck::Checking(_))) {
            return;
        }
        let (sender, receiver) = channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = sender.send(fetch_latest());
            ctx.request_repaint();
        });
        self.chrome.update = Some(UpdateCheck::Checking(receiver));
    }

    /// The Help, Updates and About windows.
    pub(super) fn chrome_windows(&mut self, ctx: &egui::Context) {
        let japanese = self.japanese;
        let t = |en: &'static str, ja: &'static str| if japanese { ja } else { en };

        if let Some(UpdateCheck::Checking(receiver)) = &self.chrome.update
            && let Ok(result) = receiver.try_recv()
        {
            self.chrome.update = Some(UpdateCheck::Done(result));
        }

        let centre = ctx.content_rect().center();
        let mut open = self.chrome.help;
        egui::Window::new(format!("{}  {}", icon::BOOK_OPEN, t("How to use sf2studio", "sf2studio の使い方")))
            .open(&mut open)
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(centre)
            .default_width(560.0)
            .default_height(560.0)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    help_section(ui, icon::MAGIC_WAND, t("Create", "作成"), &[
                        t("Start from a preset of an SF2, one recording per note, or the built-in synthesis.", "既存の SF2 のプリセット、1 音ずつの録音、または内蔵の合成から始めます。"),
                        t("Shape it: level and tone, touch (velocity range, curve, layers), sustain, tuning and space.", "音量と音色、タッチ（強さの差・カーブ・レイヤー）、余韻、調律と広がりを整えます。"),
                        t("It is made again a moment after each change; play it on the keyboard, save it, or send it to Compare.", "変更するたびに少し待って作り直します。鍵盤で弾いて確かめ、保存するか比較に送ります。"),
                    ]);
                    help_section(ui, icon::PIANO_KEYS, t("Tune", "調整再生"), &[
                        t("Play the keyboard: higher on a key is softer, lower is louder. Hold it as long as you like, or fix velocity and length.", "鍵盤を弾きます。上の方ほど弱く、下の方ほど強く鳴ります。押している間鳴り、強さや長さを固定することもできます。"),
                        t("On release the note is rendered in every lane with its spectrogram, harmonics and level.", "離すと、その音を全レーンで書き出し、スペクトログラム・倍音・音量を表示します。"),
                        t("Adjust sf2synth on the right while you listen, and save the settings as TOML.", "右側で sf2synth を調整しながら聞き、設定を TOML に保存できます。"),
                    ]);
                    help_section(ui, icon::SCALES, t("Compare", "比較"), &[
                        t("Each lane is a synthesizer (sf2synth or the macOS sampler) with an instrument file and preset, or a WAV.", "各レーンは、シンセ（sf2synth か Mac 標準）と音源ファイル・音色の組み合わせ、または WAV です。"),
                        t("Every lane is heard by default; switch outputs on and off while playing, or solo one.", "既定ではすべてのレーンが鳴ります。再生中に出力を切り替えたり、1 つだけ鳴らしたりできます。"),
                        t("Measurements chart velocity against level and brightness, and decay per key. The blind test tells whether you can hear a difference.", "計測グラフで、ベロシティごとの音量・明るさと、鍵盤ごとの減衰を比べます。ブラインドテストで本当に聞き分けられるかを確かめます。"),
                    ]);
                    ui.add_space(8.0);
                    ui.label(RichText::new(t("Keys", "キー操作")).family(theme::bold()).size(15.0));
                    egui::Grid::new("keys").num_columns(2).striped(true).spacing([24.0, 6.0]).show(ui, |ui| {
                        for (key, what) in [
                            ("Space", t("Play / pause", "再生 / 一時停止")),
                            ("1 – 9", t("Tune: hear that lane · Compare: its output on/off", "調整再生: そのレーンを聞く · 比較: 出力のオン・オフ")),
                            ("Shift + 1 – 9", t("Compare: hear only that lane", "比較: そのレーンだけを鳴らす")),
                            ("Tab", t("Next lane alone", "次のレーンだけを鳴らす")),
                            ("← / →", t("Back / forward 5 s", "5 秒戻る / 進む")),
                            ("Home", t("To the start", "先頭へ")),
                            ("Shift + drag", t("Loop a range", "範囲をループ")),
                            ("L", t("Clear the loop", "ループを解除")),
                            (t("Pinch / Ctrl + scroll", "ピンチ / Ctrl + スクロール"), t("Zoom the timeline (double-click: all)", "タイムラインの拡大縮小（ダブルクリックで全体）")),
                        ] {
                            ui.monospace(key);
                            ui.label(what);
                            ui.end_row();
                        }
                    });
                });
            });
        self.chrome.help = open;

        let mut open = self.chrome.updates;
        egui::Window::new(format!("{}  {}", icon::ARROWS_CLOCKWISE, t("Updates", "更新の確認")))
            .open(&mut open)
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(centre)
            .resizable(false)
            .collapsible(false)
            .show(ctx, |ui| {
                ui.label(format!("{} {VERSION}", t("This version:", "この版:")));
                ui.add_space(4.0);
                match &self.chrome.update {
                    None | Some(UpdateCheck::Checking(_)) => {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(t("Looking for a newer version…", "新しい版を探しています…"));
                        });
                    }
                    Some(UpdateCheck::Done(Ok(None))) => {
                        ui.label(t("No release has been published yet.", "まだリリースが公開されていません。"));
                    }
                    Some(UpdateCheck::Done(Ok(Some((tag, page))))) => {
                        if version_numbers(tag) > version_numbers(VERSION) {
                            ui.colored_label(
                                ACCENT,
                                format!("{} {tag}", t("A newer version is out:", "新しい版があります:")),
                            );
                            if ui
                                .button(format!(
                                    "{}  {}",
                                    icon::ARROW_SQUARE_OUT,
                                    t("Open the download page", "ダウンロードのページを開く")
                                ))
                                .clicked()
                            {
                                ctx.open_url(egui::OpenUrl::new_tab(page));
                            }
                        } else {
                            ui.label(format!("{}  {}", icon::CHECK, t("You have the latest version.", "最新版です。")));
                        }
                    }
                    Some(UpdateCheck::Done(Err(error))) => {
                        ui.colored_label(
                            Color32::LIGHT_RED,
                            format!("{} {error}", t("Couldn't check:", "確認できませんでした:")),
                        );
                    }
                }
            });
        self.chrome.updates = open;

        let mut open = self.chrome.about;
        let icon_texture = self.chrome.icon.clone();
        egui::Window::new(format!("{}  {}", icon::INFO, t("About sf2studio", "sf2studio について")))
            .open(&mut open)
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(centre)
            .resizable(false)
            .collapsible(false)
            .default_width(380.0)
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    if let Some(texture) = &icon_texture {
                        ui.add(egui::Image::new(texture).fit_to_exact_size(Vec2::splat(72.0)));
                    }
                    ui.label(RichText::new("sf2studio").family(theme::bold()).size(22.0));
                    ui.label(format!("{} {VERSION}", t("Version", "バージョン")));
                    ui.add_space(6.0);
                    ui.label(t(
                        "Make, tune and compare SF2 instruments.",
                        "SF2 音源を作り、調整し、聞き比べるためのアプリです。",
                    ));
                    ui.add_space(8.0);
                    ui.label("© 2026 Albos Inc.");
                    ui.label(t("Licensed under MIT or Apache-2.0.", "ライセンス: MIT または Apache-2.0"));
                    ui.add_space(8.0);
                    ui.label(RichText::new(t("Built with", "使っているもの")).family(theme::bold()));
                    for (name, what) in [
                        ("sf2synth", t("SF2 synthesizer", "SF2 シンセ")),
                        ("egui / eframe", t("user interface", "画面")),
                        ("cpal · midir", t("audio and MIDI", "音声と MIDI")),
                        ("Phosphor Icons", t("icons", "アイコン")),
                    ] {
                        ui.label(format!("{name} — {what}"));
                    }
                    ui.add_space(8.0);
                    if ui.link(format!("{}  {REPOSITORY}", icon::GITHUB_LOGO)).clicked() {
                        ctx.open_url(egui::OpenUrl::new_tab(REPOSITORY));
                    }
                    ui.add_space(6.0);
                    ui.label(
                        RichText::new(t(
                            "\"SoundFont\" is a registered trademark of Creative Technology Ltd.",
                            "「SoundFont」は Creative Technology Ltd. の登録商標です。",
                        ))
                        .small()
                        .weak(),
                    );
                });
            });
        self.chrome.about = open;
    }
}

fn help_section(ui: &mut egui::Ui, glyph: &str, title: &str, lines: &[&str]) {
    ui.add_space(6.0);
    ui.label(RichText::new(format!("{glyph}  {title}")).family(theme::bold()).size(15.0));
    for line in lines {
        ui.horizontal_wrapped(|ui| {
            let (rect, _) = ui.allocate_exact_size(Vec2::new(10.0, 14.0), Sense::hover());
            ui.painter().circle_filled(rect.center(), 2.0, ui.visuals().weak_text_color());
            ui.label(*line);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Talks to GitHub: `cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn asks_github_for_the_latest_release() {
        fetch_latest().unwrap();
    }

    #[test]
    fn compares_versions() {
        assert!(version_numbers("v0.2.0") > version_numbers("0.1.9"));
        assert!(version_numbers("1.0.0") > version_numbers("v0.10.3"));
        assert_eq!(version_numbers("v0.1.0"), (0, 1, 0));
    }
}
