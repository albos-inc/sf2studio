//! The window: mode switch, lanes, timeline and transport.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use eframe::egui::{
    self, Align2, Color32, ColorImage, FontData, FontFamily, FontId, Key, Pos2, Rect, Sense, Shape, Stroke,
    TextureHandle, TextureOptions, Vec2,
    epaint::text::{FontInsert, FontPriority, InsertFontFamily},
};
use serde::{Deserialize, Serialize};

use sf2synth::Synthesizer;

use crate::analysis::{Analysis, FLOOR_DB, Rendered, SPECTRUM_HIGH_HZ, SPECTRUM_LOW_HZ, SPECTRUM_ROWS, to_db};
use crate::audio::{LaneAudio, Live, Player};
use crate::gm;
use crate::keyboard::{KeyEvent, Keyboard};
use crate::midi_in::MidiIn;
use crate::patterns::{NOTE_START, TestPattern, key_hz, key_name};
use crate::render::{Engine, Job, Program, Renderer, Sound, SoundFile, Source};
use crate::tuning::{SynthTuning, VelocityCurve};

const MAX_LANES: usize = 9;
const LANE_COLORS: [Color32; MAX_LANES] = [
    Color32::from_rgb(90, 170, 255),
    Color32::from_rgb(255, 160, 60),
    Color32::from_rgb(110, 210, 120),
    Color32::from_rgb(240, 110, 170),
    Color32::from_rgb(190, 140, 255),
    Color32::from_rgb(240, 220, 90),
    Color32::from_rgb(90, 220, 220),
    Color32::from_rgb(255, 120, 110),
    Color32::from_rgb(200, 200, 200),
];
/// How long the tuning sliders wait before rendering again.
const RENDER_DELAY: Duration = Duration::from_millis(250);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Mode {
    Create,
    Tune,
    Compare,
}

/// What is remembered between launches.
#[derive(Serialize, Deserialize)]
#[serde(default)]
struct Saved {
    japanese: bool,
    mode: Mode,
    program: Program,
    lanes: Vec<Source>,
    level_match: bool,
    layers: Layers,
    keyboard: KeyboardSettings,
}

/// How the on-screen keyboard plays.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
struct KeyboardSettings {
    /// Use `velocity` instead of where the key is pressed.
    fixed_velocity: bool,
    velocity: u8,
    /// Hold every note `length` seconds instead of while pressed.
    fixed_length: bool,
    length: f32,
}

impl Default for KeyboardSettings {
    fn default() -> Self {
        KeyboardSettings { fixed_velocity: false, velocity: 80, fixed_length: false, length: 2.0 }
    }
}

/// A key being held.
struct Press {
    key: u8,
    velocity: u8,
    at: Instant,
    from_midi: bool,
}

/// What the timeline shows.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
struct Layers {
    spectrogram: bool,
    envelope: bool,
    closeup: bool,
}

impl Default for Layers {
    fn default() -> Self {
        Layers { spectrogram: true, envelope: true, closeup: true }
    }
}

impl Default for Saved {
    fn default() -> Self {
        Saved {
            japanese: system_is_japanese(),
            mode: Mode::Compare,
            program: Program::Pattern(TestPattern::VelocitySweep),
            lanes: vec![Source::sf2synth(Sound { file: SoundFile::SameAsA, bank: 0, program: 0 }), reference_lane()],
            level_match: true,
            layers: Layers::default(),
            keyboard: KeyboardSettings::default(),
        }
    }
}

/// The second lane a new setup starts with: the platform's own sampler on
/// macOS (following lane A's SF2), a recording elsewhere.
fn reference_lane() -> Source {
    if cfg!(target_os = "macos") { Source::mac_sampler(Sound::same_as_a()) } else { Source::Wav { path: None } }
}

fn system_is_japanese() -> bool {
    ["LC_ALL", "LC_MESSAGES", "LANG"].iter().any(|v| std::env::var(v).map(|s| s.starts_with("ja")).unwrap_or(false))
        || cfg!(target_os = "macos") && mac_prefers_japanese()
}

#[cfg(target_os = "macos")]
fn mac_prefers_japanese() -> bool {
    std::process::Command::new("defaults")
        .args(["read", "-g", "AppleLanguages"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).split(',').next().unwrap_or("").contains("ja"))
        .unwrap_or(false)
}

#[cfg(not(target_os = "macos"))]
fn mac_prefers_japanese() -> bool {
    false
}

struct Lane {
    id: u64,
    source: Source,
    generation: u64,
    rendering: bool,
    audio: Option<Arc<Rendered>>,
    analysis: Option<Arc<Analysis>>,
    /// The spectrogram, drawn with the level-matching gain (dB) it was made for.
    texture: Option<(TextureHandle, f32)>,
    error: Option<String>,
}

impl Lane {
    fn new(id: u64, source: Source) -> Lane {
        Lane { id, source, generation: 0, rendering: false, audio: None, analysis: None, texture: None, error: None }
    }
}

/// The visible part of the timeline.
struct View {
    start: f64,
    span: f64,
    follow: bool,
    /// Half-width of the waveform close-up, milliseconds.
    zoom_ms: f32,
}

pub struct StudioApp {
    japanese: bool,
    mode: Mode,
    player: Option<Player>,
    player_error: Option<String>,
    sample_rate: u32,
    renderer: Renderer,
    program: Program,
    lanes: Vec<Lane>,
    next_id: u64,
    level_match: bool,
    /// The lane tuned (and heard, when tuning).
    focus: usize,
    /// Lanes heard when comparing.
    outputs: Vec<bool>,
    layers: Layers,
    view: View,
    loop_drag: Option<f64>,
    pending_render: Option<(u64, Instant)>,
    message: Option<String>,
    keyboard: Keyboard,
    keyboard_settings: KeyboardSettings,
    pressed: Option<Press>,
    last_note: Option<(u8, u8)>,
    midi_sounding: Vec<u8>,
    /// The lane source the live synthesizer was made for.
    live_for: Option<Source>,
    midi_in: MidiIn,
    midi_ports: Vec<String>,
    /// Show the whole program once the lanes have rendered it.
    fit_view: bool,
}

impl StudioApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> StudioApp {
        install_fonts(&cc.egui_ctx);
        cc.egui_ctx.set_visuals(egui::Visuals::dark());
        let mut saved: Saved = cc.storage.and_then(|s| eframe::get_value(s, eframe::APP_KEY)).unwrap_or_default();
        apply_arguments(&mut saved, std::env::args().skip(1));
        let (player, player_error) = match Player::new() {
            Ok(player) => (Some(player), None),
            Err(error) => (None, Some(error)),
        };
        let sample_rate = player.as_ref().map_or(48_000, |p| p.sample_rate);
        let ctx = cc.egui_ctx.clone();
        let mut app = StudioApp {
            japanese: saved.japanese,
            mode: saved.mode,
            player,
            player_error,
            sample_rate,
            renderer: Renderer::new(move || ctx.request_repaint()),
            program: saved.program,
            lanes: Vec::new(),
            next_id: 0,
            level_match: saved.level_match,
            focus: 0,
            outputs: Vec::new(),
            layers: saved.layers,
            view: View { start: 0.0, span: 30.0, follow: true, zoom_ms: 20.0 },
            loop_drag: None,
            pending_render: None,
            message: None,
            keyboard: Keyboard::default(),
            keyboard_settings: saved.keyboard,
            pressed: None,
            last_note: None,
            midi_sounding: Vec::new(),
            live_for: None,
            midi_in: MidiIn::new(),
            midi_ports: Vec::new(),
            fit_view: false,
        };
        for source in saved.lanes.into_iter().take(MAX_LANES) {
            app.add_lane(source);
        }
        if app.lanes.is_empty() {
            app.add_lane(Source::sf2synth(Sound::same_as_a()));
        }
        app
    }

    fn t(&self, english: &'static str, japanese: &'static str) -> &'static str {
        if self.japanese { japanese } else { english }
    }

    fn add_lane(&mut self, source: Source) {
        let id = self.next_id;
        self.next_id += 1;
        self.lanes.push(Lane::new(id, source));
        self.outputs.push(true);
        let index = self.lanes.len() - 1;
        self.render_lane(index);
    }

    /// The lane's source with "same as A" replaced by lane A's sound.
    fn effective_source(&self, index: usize) -> Source {
        match &self.lanes[index].source {
            Source::Synth { engine, sound, tuning } if sound.file == SoundFile::SameAsA => {
                let sound = match self.lanes.first().map(|l| &l.source) {
                    Some(Source::Synth { sound: a, .. }) if index > 0 && a.file != SoundFile::SameAsA => a.clone(),
                    _ => sound.clone(),
                };
                Source::Synth { engine: *engine, sound, tuning: tuning.clone() }
            }
            source => source.clone(),
        }
    }

    fn render_lane(&mut self, index: usize) {
        let source = self.effective_source(index);
        let lane = &mut self.lanes[index];
        lane.generation += 1;
        lane.error = None;
        if source.path().is_none() {
            lane.rendering = false;
            lane.audio = None;
            lane.analysis = None;
            lane.texture = None;
            self.sync_player();
            return;
        }
        lane.rendering = true;
        self.renderer.submit(Job {
            lane: lane.id,
            generation: lane.generation,
            source,
            program: self.program.clone(),
            sample_rate: self.sample_rate,
        });
    }

    fn render_all(&mut self) {
        for i in 0..self.lanes.len() {
            self.render_lane(i);
        }
    }

    fn poll_renders(&mut self) {
        let mut changed = false;
        for done in self.renderer.poll() {
            let Some(lane) = self.lanes.iter_mut().find(|l| l.id == done.lane) else { continue };
            if lane.generation != done.generation {
                continue;
            }
            lane.rendering = false;
            match done.result {
                Ok((audio, analysis)) => {
                    lane.audio = Some(audio);
                    lane.analysis = Some(analysis);
                    lane.texture = None;
                    lane.error = None;
                }
                Err(error) => {
                    lane.audio = None;
                    lane.analysis = None;
                    lane.texture = None;
                    lane.error = Some(error);
                }
            }
            changed = true;
        }
        if changed {
            self.sync_player();
            if self.fit_view && !self.lanes.iter().any(|l| l.rendering) {
                self.fit_view = false;
                self.view.start = 0.0;
                self.view.span = self.duration().max(0.05);
                self.view.follow = false;
            }
        }
        if let Some((id, at)) = self.pending_render {
            if at.elapsed() >= RENDER_DELAY {
                self.pending_render = None;
                if let Some(index) = self.lanes.iter().position(|l| l.id == id) {
                    self.render_lane(index);
                }
            }
        }
    }

    /// Level-matching gains: every lane brought down to the quietest.
    fn gains(&self) -> Vec<f32> {
        let target = self
            .lanes
            .iter()
            .filter_map(|l| l.analysis.as_ref().map(|a| a.loudness))
            .filter(|&l| l > 0.0)
            .fold(f32::INFINITY, f32::min);
        self.lanes
            .iter()
            .map(|l| match (&l.analysis, self.level_match && target.is_finite()) {
                (Some(a), true) if a.loudness > 0.0 => target / a.loudness,
                _ => 1.0,
            })
            .collect()
    }

    fn sync_player(&mut self) {
        let gains = self.gains();
        let heard = self.heard();
        let Some(player) = &self.player else { return };
        let mut transport = player.transport.lock().unwrap();
        transport.set_lanes(
            self.lanes
                .iter()
                .zip(&gains)
                .map(|(l, &gain)| l.audio.as_ref().map(|audio| LaneAudio { audio: Arc::clone(audio), gain }))
                .collect(),
        );
        transport.set_outputs(&heard);
        let focus_gain = gains.get(self.focus).copied().unwrap_or(1.0);
        if let Some(live) = &mut transport.live {
            live.gain = focus_gain;
        }
    }

    /// The lanes heard: the focused one when tuning, the checked ones when
    /// comparing.
    fn heard(&self) -> Vec<bool> {
        match self.mode {
            Mode::Compare => self.outputs.clone(),
            _ => (0..self.lanes.len()).map(|i| i == self.focus).collect(),
        }
    }

    fn push_outputs(&self) {
        let heard = self.heard();
        self.with_transport(|t| t.set_outputs(&heard));
    }

    // MARK: - Transport

    fn with_transport<R>(&self, f: impl FnOnce(&mut crate::audio::Transport) -> R) -> Option<R> {
        self.player.as_ref().map(|p| f(&mut p.transport.lock().unwrap()))
    }

    fn position_seconds(&self) -> f64 {
        self.with_transport(|t| t.position).unwrap_or(0) as f64 / self.sample_rate as f64
    }

    fn is_playing(&self) -> bool {
        self.with_transport(|t| t.playing).unwrap_or(false)
    }

    fn seek(&self, seconds: f64) {
        let frame = (seconds.max(0.0) * self.sample_rate as f64) as usize;
        self.with_transport(|t| t.position = frame.min(t.length().saturating_sub(1)));
    }

    fn toggle_play(&self) {
        self.with_transport(|t| {
            if t.length() > 0 {
                t.playing = !t.playing;
            }
        });
    }

    /// Focuses a lane (and, when tuning, hears only it).
    fn focus_lane(&mut self, lane: usize) {
        if lane < self.lanes.len() {
            self.focus = lane;
            self.push_outputs();
        }
    }

    /// Turns a lane's output on or off (comparing).
    fn toggle_output(&mut self, lane: usize) {
        if let Some(on) = self.outputs.get_mut(lane) {
            *on = !*on;
            self.push_outputs();
        }
    }

    /// Hears only `lane` (comparing).
    fn solo(&mut self, lane: usize) {
        if lane < self.lanes.len() {
            self.outputs = (0..self.lanes.len()).map(|i| i == lane).collect();
            self.focus = lane;
            self.push_outputs();
        }
    }

    fn set_mode(&mut self, mode: Mode) {
        if self.mode != mode {
            self.mode = mode;
            self.push_outputs();
        }
    }

    fn duration(&self) -> f64 {
        self.lanes.iter().filter_map(|l| l.audio.as_ref()).map(|a| a.duration()).fold(0.0, f64::max)
    }

    fn handle_keys(&mut self, ctx: &egui::Context) {
        if ctx.egui_wants_keyboard_input() {
            return;
        }
        let keys = [Key::Num1, Key::Num2, Key::Num3, Key::Num4, Key::Num5, Key::Num6, Key::Num7, Key::Num8, Key::Num9];
        let (space, tab, home, left, right, clear_loop, pressed, shift) = ctx.input(|i| {
            (
                i.key_pressed(Key::Space),
                i.key_pressed(Key::Tab),
                i.key_pressed(Key::Home),
                i.key_pressed(Key::ArrowLeft),
                i.key_pressed(Key::ArrowRight),
                i.key_pressed(Key::L),
                keys.iter().position(|&k| i.key_pressed(k)),
                i.modifiers.shift,
            )
        });
        if space {
            self.toggle_play();
        }
        if let Some(lane) = pressed {
            match (self.mode, shift) {
                (Mode::Compare, false) => self.toggle_output(lane),
                (Mode::Compare, true) => self.solo(lane),
                _ => self.focus_lane(lane),
            }
        }
        if tab {
            // A ⇄ B, or the next lane when there are more.
            let next = (self.focus + 1) % self.lanes.len().max(1);
            match self.mode {
                Mode::Compare => self.solo(next),
                _ => self.focus_lane(next),
            }
        }
        if home {
            self.seek(0.0);
        }
        if left || right {
            let step = if left { -5.0 } else { 5.0 };
            self.seek(self.position_seconds() + step);
        }
        if clear_loop {
            self.with_transport(|t| t.loop_range = None);
        }
    }

    // MARK: - Panels

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("sf2studio");
            ui.add_space(16.0);
            for (mode, english, japanese) in
                [(Mode::Create, "Create", "作成"), (Mode::Tune, "Tune", "調整再生"), (Mode::Compare, "Compare", "比較")]
            {
                let label = egui::RichText::new(if self.japanese { japanese } else { english }).size(15.0);
                if ui.add(egui::Button::selectable(self.mode == mode, label).min_size(Vec2::new(96.0, 28.0))).clicked()
                {
                    self.set_mode(mode);
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.selectable_label(self.japanese, "日本語").clicked() {
                    self.japanese = true;
                }
                if ui.selectable_label(!self.japanese, "English").clicked() {
                    self.japanese = false;
                }
                if let Some(error) = &self.player_error {
                    ui.colored_label(Color32::LIGHT_RED, format!("audio: {error}"));
                }
            });
        });
    }

    fn transport_bar(&mut self, ui: &mut egui::Ui) {
        let playing = self.is_playing();
        let position = self.position_seconds();
        let duration = self.duration();
        ui.horizontal(|ui| {
            let play = if playing { self.t("⏸ Pause", "⏸ 一時停止") } else { self.t("▶ Play", "▶ 再生") };
            if ui.add(egui::Button::new(play).min_size(Vec2::new(96.0, 28.0))).clicked() {
                self.toggle_play();
            }
            if ui.button("⏮").on_hover_text(self.t("To the start (Home)", "先頭へ（Home）")).clicked() {
                self.seek(0.0);
            }
            ui.monospace(format!("{} / {}", clock(position), clock(duration)));
            ui.separator();
            if self.mode == Mode::Compare {
                ui.label(self.t("Output:", "出力:"));
                for (i, &color) in LANE_COLORS.iter().enumerate().take(self.lanes.len()) {
                    let mut on = self.outputs[i];
                    let label = egui::RichText::new(lane_letter(i).to_string()).color(color).strong().size(16.0);
                    let hint = self.t(
                        "Hear this lane (1–9) · Shift-click or Shift+1–9: only this lane",
                        "このレーンを鳴らす（1–9）· Shift+クリック / Shift+1–9: このレーンだけ",
                    );
                    let response = ui.checkbox(&mut on, label).on_hover_text(hint);
                    if response.changed() {
                        if ui.input(|i| i.modifiers.shift) {
                            self.solo(i);
                        } else {
                            self.toggle_output(i);
                        }
                    }
                }
                if ui.small_button(self.t("All", "全部")).clicked() {
                    self.outputs = vec![true; self.lanes.len()];
                    self.push_outputs();
                }
            } else {
                ui.label(self.t("Hear:", "再生中:"));
                for (i, &color) in LANE_COLORS.iter().enumerate().take(self.lanes.len()) {
                    let label = egui::RichText::new(format!(" {} ", lane_letter(i))).color(color).strong().size(16.0);
                    let button = ui.add(egui::Button::selectable(self.focus == i, label));
                    if button
                        .on_hover_text(format!("{} ({})", self.t("Switch to this lane", "このレーンに切り替え"), i + 1))
                        .clicked()
                    {
                        self.focus_lane(i);
                    }
                }
            }
            ui.separator();
            let match_label = self.t("Match levels", "音量を揃える");
            if ui.checkbox(&mut self.level_match, match_label).changed() {
                self.sync_player();
            }
            let looping = self.with_transport(|t| t.loop_range).flatten();
            if let Some((start, end)) = looping {
                let rate = self.sample_rate as f64;
                ui.label(format!("🔁 {}–{}", clock(start as f64 / rate), clock(end as f64 / rate)));
                if ui.small_button("×").clicked() {
                    self.with_transport(|t| t.loop_range = None);
                }
            }
            let meter = self.with_transport(|t| t.meter).unwrap_or(0.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let (rect, _) = ui.allocate_exact_size(Vec2::new(120.0, 10.0), Sense::hover());
                let level = ((to_db(meter) + 60.0) / 60.0).clamp(0.0, 1.0);
                ui.painter().rect_filled(rect, 2.0, Color32::from_gray(40));
                let mut filled = rect;
                filled.set_width(rect.width() * level);
                let color = if meter > 0.99 { Color32::RED } else { Color32::from_rgb(90, 200, 120) };
                ui.painter().rect_filled(filled, 2.0, color);
                ui.weak(self.t(
                    "Space play · 1–9 / Tab switch lane · Shift-drag loop · L clear loop",
                    "Space 再生 · 1–9 / Tab レーン切替 · Shift+ドラッグ ループ · L ループ解除",
                ));
            });
        });
    }

    fn program_ui(&mut self, ui: &mut egui::Ui) {
        ui.strong(self.t("Play", "再生する内容"));
        let mut changed = false;
        let japanese = self.japanese;
        let current_pattern = match &self.program {
            Program::Pattern(p) => Some(*p),
            Program::Midi(_) | Program::Note { .. } => None,
        };
        egui::ComboBox::from_id_salt("pattern")
            .width(220.0)
            .selected_text(match &self.program {
                Program::Pattern(p) => p.name(japanese).to_string(),
                Program::Midi(path) => file_name(path),
                Program::Note { key, velocity, length } => {
                    format!("{} · vel {velocity} · {length:.2} s", key_name(*key))
                }
            })
            .show_ui(ui, |ui| {
                for pattern in TestPattern::ALL {
                    if ui.selectable_label(current_pattern == Some(pattern), pattern.name(japanese)).clicked() {
                        self.program = Program::Pattern(pattern);
                        changed = true;
                    }
                }
            });
        if ui.button(self.t("Open MIDI file…", "MIDI ファイルを開く…")).clicked() {
            if let Some(path) = rfd::FileDialog::new().add_filter("MIDI", &["mid", "midi", "kar", "rmi"]).pick_file() {
                self.program = Program::Midi(path);
                changed = true;
            }
        }
        if changed {
            self.with_transport(|t| {
                t.position = 0;
                t.loop_range = None;
            });
            self.render_all();
        }
    }

    fn lanes_ui(&mut self, ui: &mut egui::Ui) {
        ui.strong(self.t("Lanes", "レーン"));
        let mut remove = None;
        let mut rerender = Vec::new();
        let gains = self.gains();
        let japanese = self.japanese;
        let t = |en: &'static str, ja: &'static str| if japanese { ja } else { en };
        let shared = self.lanes.first().and_then(|l| match &l.source {
            Source::Synth { sound, .. } => sound.file.path(),
            Source::Wav { .. } => None,
        });
        for i in 0..self.lanes.len() {
            // The presets of the lane's file, for the preset menu.
            let effective = self.effective_source(i);
            let presets: Vec<(u16, u8, String)> = match &effective {
                Source::Synth { engine: Engine::Sf2synth, sound, .. } => sound
                    .file
                    .path()
                    .filter(|p| !is_dls(p))
                    .and_then(|p| self.renderer.font(&p).ok())
                    .map(|font| {
                        let mut list: Vec<(u16, u8, String)> = font
                            .presets()
                            .iter()
                            .map(|p| (p.bank(), p.program() as u8, p.name().to_string()))
                            .collect();
                        list.sort();
                        list
                    })
                    .unwrap_or_default(),
                Source::Synth { .. } => {
                    let mut list: Vec<(u16, u8, String)> =
                        gm::PROGRAMS.iter().enumerate().map(|(p, n)| (0, p as u8, n.to_string())).collect();
                    list.push((128, 0, "Drums".to_string()));
                    list
                }
                Source::Wav { .. } => Vec::new(),
            };
            let lane = &mut self.lanes[i];
            egui::Frame::group(ui.style()).stroke(Stroke::new(1.0, LANE_COLORS[i].gamma_multiply(0.6))).show(
                ui,
                |ui| {
                    // Synthesizer.
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(lane_letter(i)).color(LANE_COLORS[i]).strong().size(18.0));
                        let mut kinds = vec![(0, "sf2synth")];
                        if cfg!(target_os = "macos") {
                            kinds.push((1, t("macOS sampler", "Mac 標準")));
                        }
                        kinds.push((2, t("WAV recording", "WAV（録音）")));
                        let kind = match &lane.source {
                            Source::Synth { engine: Engine::Sf2synth, .. } => 0,
                            Source::Synth { engine: Engine::MacSampler, .. } => 1,
                            Source::Wav { .. } => 2,
                        };
                        ui.label(t("Synth", "シンセ"));
                        egui::ComboBox::from_id_salt(("engine", lane.id))
                            .width(120.0)
                            .selected_text(kinds.iter().find(|(k, _)| *k == kind).map_or("", |(_, n)| *n))
                            .show_ui(ui, |ui| {
                                for &(k, name) in &kinds {
                                    if ui.selectable_label(k == kind, name).clicked() && k != kind {
                                        let sound = match &lane.source {
                                            Source::Synth { sound, .. } => sound.clone(),
                                            Source::Wav { .. } => Sound::same_as_a(),
                                        };
                                        lane.source = match k {
                                            0 => Source::sf2synth(sound),
                                            1 => Source::mac_sampler(sound),
                                            _ => Source::Wav { path: None },
                                        };
                                        rerender.push(i);
                                    }
                                }
                            });
                        if ui.small_button("×").on_hover_text(t("Remove the lane", "レーンを削除")).clicked() {
                            remove = Some(i);
                        }
                    });

                    match &mut lane.source {
                        Source::Synth { engine, sound, .. } => {
                            // Instrument file.
                            ui.horizontal(|ui| {
                                ui.label(t("Sound", "音源"));
                                let current = match &sound.file {
                                    SoundFile::SameAsA if i == 0 => t("Choose…", "選んでください").to_string(),
                                    SoundFile::SameAsA => {
                                        let name = shared.as_deref().map(file_name).unwrap_or_default();
                                        if japanese {
                                            format!("A と同じ（{name}）")
                                        } else {
                                            format!("Same as A ({name})")
                                        }
                                    }
                                    SoundFile::File(path) => file_name(path),
                                    SoundFile::MacBuiltIn => t("macOS built-in GS", "Mac 内蔵 GS 音源").to_string(),
                                };
                                egui::ComboBox::from_id_salt(("sound", lane.id))
                                    .width(170.0)
                                    .selected_text(current)
                                    .show_ui(ui, |ui| {
                                        if i > 0
                                            && ui
                                                .selectable_label(
                                                    sound.file == SoundFile::SameAsA,
                                                    t("Same as lane A", "A と同じ"),
                                                )
                                                .clicked()
                                        {
                                            sound.file = SoundFile::SameAsA;
                                            rerender.push(i);
                                        }
                                        if ui
                                            .selectable_label(
                                                false,
                                                t("Choose a file (SF2 / DLS)…", "ファイルを選ぶ（SF2 / DLS）…"),
                                            )
                                            .clicked()
                                        {
                                            if let Some(path) = rfd::FileDialog::new()
                                                .add_filter("SF2 / DLS", &["sf2", "dls"])
                                                .pick_file()
                                            {
                                                self.renderer.forget_font(&path);
                                                sound.file = SoundFile::File(path);
                                                sound.bank = 0;
                                                sound.program = 0;
                                                rerender.push(i);
                                            }
                                        }
                                        if cfg!(target_os = "macos")
                                            && ui
                                                .selectable_label(
                                                    sound.file == SoundFile::MacBuiltIn,
                                                    t("macOS built-in GS (DLS)", "Mac 内蔵 GS 音源（DLS）"),
                                                )
                                                .clicked()
                                        {
                                            sound.file = SoundFile::MacBuiltIn;
                                            sound.bank = 0;
                                            sound.program = 0;
                                            rerender.push(i);
                                        }
                                    });
                            });
                            // Preset in the file.
                            if sound.file != SoundFile::SameAsA && !presets.is_empty() {
                                ui.horizontal(|ui| {
                                    ui.label(t("Preset", "音色"));
                                    let name = presets
                                        .iter()
                                        .find(|(b, p, _)| *b == sound.bank && *p == sound.program)
                                        .map(|(b, p, n)| format!("{b}:{p} {n}"))
                                        .unwrap_or_else(|| format!("{}:{}", sound.bank, sound.program));
                                    egui::ComboBox::from_id_salt(("preset", lane.id))
                                        .width(190.0)
                                        .selected_text(name)
                                        .show_ui(ui, |ui| {
                                            for (b, p, n) in &presets {
                                                let selected = *b == sound.bank && *p == sound.program;
                                                if ui.selectable_label(selected, format!("{b}:{p} {n}")).clicked()
                                                    && !selected
                                                {
                                                    sound.bank = *b;
                                                    sound.program = *p;
                                                    rerender.push(i);
                                                }
                                            }
                                        });
                                });
                            }
                            if *engine == Engine::Sf2synth && effective.path().is_some_and(|p| is_dls(&p)) {
                                ui.colored_label(
                                    Color32::from_rgb(240, 190, 90),
                                    t(
                                        "sf2synth can't read DLS yet: choose the macOS sampler.",
                                        "sf2synth はまだ DLS を読めません。シンセを Mac 標準にしてください。",
                                    ),
                                );
                            }
                        }
                        Source::Wav { path } => {
                            let label = path
                                .as_deref()
                                .map(file_name)
                                .unwrap_or_else(|| t("Choose a WAV…", "WAV を選ぶ…").to_string());
                            if ui.button(label).clicked() {
                                if let Some(chosen) = rfd::FileDialog::new().add_filter("WAV", &["wav"]).pick_file() {
                                    *path = Some(chosen);
                                    rerender.push(i);
                                }
                            }
                        }
                    }

                    if lane.rendering {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(t("Rendering…", "書き出し中…"));
                        });
                    } else if let Some(error) = &lane.error {
                        ui.colored_label(Color32::LIGHT_RED, error);
                    } else if let Some(analysis) = &lane.analysis {
                        ui.weak(format!(
                            "{} {:.1} dB · {} {:.1} dB · {} {:+.1} dB · {} {:.0} Hz",
                            t("level", "音量"),
                            to_db(analysis.loudness),
                            t("peak", "ピーク"),
                            to_db(analysis.peak),
                            t("gain", "補正"),
                            to_db(gains[i]),
                            t("brightness", "明るさ"),
                            analysis.brightness_hz(),
                        ));
                    }
                },
            );
        }
        if let Some(i) = remove {
            if self.lanes.len() > 1 {
                self.lanes.remove(i);
                self.outputs.remove(i);
                if self.focus >= self.lanes.len() || self.focus > i {
                    self.focus = self.focus.saturating_sub(1);
                }
                self.sync_player();
            }
        }
        // Lanes playing lane A's sound follow its changes.
        if rerender.contains(&0) {
            for (i, lane) in self.lanes.iter().enumerate().skip(1) {
                let follows = matches!(&lane.source, Source::Synth { sound, .. } if sound.file == SoundFile::SameAsA);
                if follows && !rerender.contains(&i) {
                    rerender.push(i);
                }
            }
        }
        for i in rerender {
            self.render_lane(i);
        }
        if self.lanes.len() < MAX_LANES && ui.button(self.t("+ Add lane", "+ レーンを追加")).clicked() {
            self.add_lane(Source::sf2synth(Sound::same_as_a()));
        }
    }

    fn tuning_ui(&mut self, ui: &mut egui::Ui) {
        let current = self.focus;
        let japanese = self.japanese;
        ui.strong(self.t("sf2synth settings", "sf2synth の設定"));
        let Some(lane) = self.lanes.get_mut(current) else { return };
        let id = lane.id;
        let Source::Synth { engine: Engine::Sf2synth, tuning, .. } = &mut lane.source else {
            ui.label(if japanese {
                "いま再生中のレーンは sf2synth ではありません。sf2synth のレーンに切り替えると設定できます。"
            } else {
                "The lane you hear isn't sf2synth. Switch to an sf2synth lane to tune it."
            });
            return;
        };
        ui.label(format!("{} {}", if japanese { "レーン" } else { "Lane" }, lane_letter(current)));
        let before = tuning.clone();
        let t = |en: &'static str, ja: &'static str| if japanese { ja } else { en };
        ui.add(
            egui::Slider::new(&mut tuning.master_gain_db, -24.0..=12.0)
                .suffix(" dB")
                .text(t("Master gain", "全体の音量")),
        );
        ui.separator();
        ui.label(t("Velocity response", "ベロシティの反応"));
        ui.add(egui::Slider::new(&mut tuning.velocity_range_cb, 0.0..=1440.0).text(t("Range (cB)", "幅 (cB)")));
        egui::ComboBox::from_id_salt("curve").selected_text(tuning.velocity_curve.name(japanese)).show_ui(ui, |ui| {
            for curve in VelocityCurve::ALL {
                ui.selectable_value(&mut tuning.velocity_curve, curve, curve.name(japanese));
            }
        });
        ui.add(
            egui::Slider::new(&mut tuning.velocity_filter_cents, -4800.0..=0.0)
                .suffix(" cents")
                .text(t("Soft notes darker", "弱音をこもらせる")),
        );
        ui.add(
            egui::Slider::new(&mut tuning.initial_attenuation_scale, 0.0..=1.0)
                .text(t("Attenuation scale", "減衰の係数")),
        );
        ui.separator();
        ui.checkbox(&mut tuning.cubic_interpolation, t("Cubic interpolation", "キュービック補間"));
        ui.checkbox(&mut tuning.release_same_key, t("Re-strike releases the key", "同じ鍵盤の再打鍵で前の音を離す"));
        ui.separator();
        ui.label(t("Reverb", "リバーブ"));
        ui.add(egui::Slider::new(&mut tuning.reverb_send, 0..=127).text(t("Send (CC91)", "送り (CC91)")));
        ui.add(egui::Slider::new(&mut tuning.reverb_room, 0.0..=1.0).text(t("Room size", "部屋の大きさ")));
        ui.add(egui::Slider::new(&mut tuning.reverb_damping, 0.0..=1.0).text(t("Damping", "高域の減衰")));
        ui.add(egui::Slider::new(&mut tuning.reverb_level, 0.0..=1.0).text(t("Level", "量")));
        ui.label(t("Chorus", "コーラス"));
        ui.add(egui::Slider::new(&mut tuning.chorus_send, 0..=127).text(t("Send (CC93)", "送り (CC93)")));
        ui.add(egui::Slider::new(&mut tuning.chorus_level, 0.0..=1.0).text(t("Level", "量")));
        ui.separator();
        let mut message = None;
        ui.horizontal(|ui| {
            if ui.button(t("Reset", "初期値に戻す")).clicked() {
                *tuning = SynthTuning::default();
            }
            if ui.button(t("Save…", "保存…")).clicked() {
                if let Some(path) =
                    rfd::FileDialog::new().add_filter("TOML", &["toml"]).set_file_name("sf2synth.toml").save_file()
                {
                    message = Some(match std::fs::write(&path, tuning.to_toml()) {
                        Ok(()) => format!("{} {}", t("Saved", "保存しました:"), path.display()),
                        Err(e) => e.to_string(),
                    });
                }
            }
            if ui.button(t("Load…", "読み込む…")).clicked() {
                if let Some(path) = rfd::FileDialog::new().add_filter("TOML", &["toml"]).pick_file() {
                    match std::fs::read_to_string(&path)
                        .map_err(|e| e.to_string())
                        .and_then(|s| SynthTuning::from_toml(&s))
                    {
                        Ok(loaded) => *tuning = loaded,
                        Err(e) => message = Some(e),
                    }
                }
            }
        });
        let changed = *tuning != before;
        if message.is_some() {
            self.message = message;
        }
        if changed {
            self.pending_render = Some((id, Instant::now()));
        }
        if let Some(message) = &self.message {
            ui.weak(message);
        }
    }

    fn create_ui(&mut self, ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space(80.0);
            ui.heading(self.t("Create an SF2", "SF2 を作る"));
            ui.add_space(12.0);
            ui.label(self.t(
                "Coming next: start from an SF2, a recording or synthesis, shape brightness, hardness, sustain, \
                 velocity layers and tuning, and write a new SF2 file.",
                "次の段階で追加します: 既存の SF2・録音・合成を元に、明るさ・硬さ・余韻・ベロシティレイヤー・\
                 調律を整えて、新しい SF2 として書き出します。",
            ));
        });
    }

    // MARK: - Keyboard

    /// Keeps the live synthesizer in step with the focused lane (Tune mode,
    /// sf2synth lanes only).
    fn ensure_live(&mut self) {
        let wanted = match (self.mode, self.focus < self.lanes.len()) {
            (Mode::Tune, true) => match self.effective_source(self.focus) {
                source @ Source::Synth { engine: Engine::Sf2synth, .. }
                    if source.path().is_some_and(|p| !is_dls(&p)) =>
                {
                    Some(source)
                }
                _ => None,
            },
            _ => None,
        };
        if wanted == self.live_for {
            return;
        }
        self.live_for = wanted.clone();
        let synth = wanted.and_then(|source| {
            let Source::Synth { sound, tuning, .. } = &source else { return None };
            let font = self.renderer.font(&source.path()?).ok()?;
            let mut synth = Synthesizer::new(font, &tuning.settings(self.sample_rate)).ok()?;
            crate::render::prepare_synth(&mut synth, tuning, sound);
            Some(synth)
        });
        let gain = self.gains().get(self.focus).copied().unwrap_or(1.0);
        self.with_transport(|t| t.live = synth.map(|s| Live::new(s, gain)));
    }

    fn live_message(&self, message: &[u8]) {
        self.with_transport(|t| {
            if let Some(live) = &mut t.live {
                live.synth.process_midi_message(message);
            }
        });
    }

    fn key_down(&mut self, key: u8, velocity: u8, from_midi: bool) {
        if let Some(press) = self.pressed.take() {
            if !from_midi {
                self.live_message(&[0x80, press.key, 0]);
            }
            self.finish_note(press.key, press.velocity, press.at.elapsed().as_secs_f32());
        }
        // The lanes stop so the key is heard alone.
        self.with_transport(|t| t.playing = false);
        if !from_midi {
            self.live_message(&[0x90, key, velocity]);
        }
        self.last_note = Some((key, velocity));
        self.pressed = Some(Press { key, velocity, at: Instant::now(), from_midi });
    }

    fn key_up(&mut self, key: u8, from_midi: bool) {
        let fixed = self.keyboard_settings.fixed_length && !from_midi;
        if fixed || self.pressed.as_ref().is_none_or(|p| p.key != key) {
            return;
        }
        let press = self.pressed.take().unwrap();
        if !from_midi {
            self.live_message(&[0x80, key, 0]);
        }
        self.finish_note(key, press.velocity, press.at.elapsed().as_secs_f32());
    }

    /// Releases a fixed-length note when its time is up.
    fn tick_note(&mut self) {
        let length = self.keyboard_settings.length;
        let due = self.pressed.as_ref().is_some_and(|p| {
            self.keyboard_settings.fixed_length && !p.from_midi && p.at.elapsed().as_secs_f32() >= length
        });
        if due {
            let press = self.pressed.take().unwrap();
            self.live_message(&[0x80, press.key, 0]);
            self.finish_note(press.key, press.velocity, length);
        }
    }

    /// Renders the note just played in every lane, to look at and compare.
    fn finish_note(&mut self, key: u8, velocity: u8, length: f32) {
        self.program = Program::Note { key, velocity, length: length.max(0.05) };
        self.with_transport(|t| {
            t.position = 0;
            t.loop_range = None;
        });
        self.fit_view = true;
        self.render_all();
    }

    fn poll_midi(&mut self) {
        let messages: Vec<Vec<u8>> = self.midi_in.receiver.try_iter().collect();
        for message in messages {
            let (status, key, velocity) =
                (message[0] & 0xf0, message.get(1).copied().unwrap_or(0), message.get(2).copied().unwrap_or(0));
            match status {
                0x90 if velocity > 0 => {
                    self.midi_sounding.push(key);
                    if self.mode == Mode::Tune {
                        self.key_down(key, velocity, true);
                    }
                }
                0x80 | 0x90 => {
                    self.midi_sounding.retain(|&k| k != key);
                    if self.mode == Mode::Tune {
                        self.key_up(key, true);
                    }
                }
                _ => {}
            }
        }
    }

    fn keyboard_ui(&mut self, ui: &mut egui::Ui) {
        let japanese = self.japanese;
        let t = |en: &'static str, ja: &'static str| if japanese { ja } else { en };
        let live = self.live_for.is_some();
        ui.horizontal(|ui| {
            let settings = &mut self.keyboard_settings;
            ui.checkbox(&mut settings.fixed_velocity, t("Fixed velocity", "強さを固定"));
            ui.add_enabled(settings.fixed_velocity, egui::Slider::new(&mut settings.velocity, 1..=127));
            ui.separator();
            ui.checkbox(&mut settings.fixed_length, t("Fixed length", "長さを固定"));
            ui.add_enabled(
                settings.fixed_length,
                egui::Slider::new(&mut settings.length, 0.1..=10.0).suffix(" s").logarithmic(true),
            );
            ui.separator();
            let connected = self.midi_in.connected().map(str::to_string);
            let label = connected.clone().unwrap_or_else(|| t("No MIDI keyboard", "MIDI キーボードなし").to_string());
            let mut choice: Option<Option<String>> = None;
            let combo = egui::ComboBox::from_id_salt("midi").width(200.0).selected_text(label).show_ui(ui, |ui| {
                if ui.selectable_label(connected.is_none(), t("None", "なし")).clicked() {
                    choice = Some(None);
                }
                for port in &self.midi_ports {
                    if ui.selectable_label(connected.as_deref() == Some(port), port).clicked() {
                        choice = Some(Some(port.clone()));
                    }
                }
            });
            if combo.response.clicked() {
                self.midi_ports = MidiIn::ports();
            }
            match choice {
                Some(None) => self.midi_in.disconnect(),
                Some(Some(port)) => {
                    if let Some(player) = &self.player {
                        let ctx = ui.ctx().clone();
                        if let Err(e) =
                            self.midi_in.connect(&port, Arc::clone(&player.transport), move || ctx.request_repaint())
                        {
                            self.message = Some(e);
                        }
                    }
                }
                None => {}
            }
            ui.separator();
            if !live {
                ui.colored_label(
                    Color32::from_rgb(240, 190, 90),
                    t(
                        "Focus an sf2synth lane with an SF2 to play the keys.",
                        "鍵盤を鳴らすには、SF2 を選んだ sf2synth のレーンを調整対象にしてください。",
                    ),
                );
            } else {
                ui.weak(t(
                    "Press higher on a key for soft, lower for loud. Release to see the note in every lane.",
                    "鍵盤の上の方ほど弱く、下の方ほど強く鳴ります。離すとその音を全レーンで表示します。",
                ));
            }
        });
        ui.add_space(4.0);
        let fixed = self.keyboard_settings.fixed_velocity.then_some(self.keyboard_settings.velocity);
        let accent = LANE_COLORS[self.focus.min(MAX_LANES - 1)];
        let events = self.keyboard.show(ui, fixed, &self.midi_sounding, self.last_note, accent);
        for event in events {
            match event {
                KeyEvent::On { key, velocity } => self.key_down(key, velocity, false),
                KeyEvent::Off { key } => self.key_up(key, false),
            }
        }
    }

    // MARK: - Timeline

    /// Toggles for what the timeline shows.
    fn layers_bar(&mut self, ui: &mut egui::Ui) {
        let japanese = self.japanese;
        ui.horizontal(|ui| {
            ui.label(if japanese { "表示:" } else { "Show:" });
            ui.toggle_value(&mut self.layers.spectrogram, if japanese { "スペクトログラム" } else { "Spectrogram" });
            ui.toggle_value(&mut self.layers.envelope, if japanese { "音量推移 (dB)" } else { "Level (dB)" });
            ui.toggle_value(&mut self.layers.closeup, if japanese { "拡大波形" } else { "Waveform close-up" });
        });
    }

    fn timeline(&mut self, ui: &mut egui::Ui) {
        let duration = self.duration();
        let position = self.position_seconds();
        let playing = self.is_playing();
        let heard = self.heard();
        if duration <= 0.0 {
            ui.centered_and_justified(|ui| {
                ui.label(self.t(
                    "Choose an SF2 (or WAV) for a lane on the left to start.",
                    "左のレーンで SF2（または WAV）を選ぶと始まります。",
                ));
            });
            return;
        }
        self.view.span = self.view.span.clamp(0.05, duration.max(0.05));
        if playing
            && self.view.follow
            && (position < self.view.start || position > self.view.start + self.view.span * 0.9)
        {
            self.view.start = position - self.view.span * 0.1;
        }
        self.view.start = self.view.start.clamp(0.0, (duration - self.view.span).max(0.0));

        self.layers_bar(ui);
        let available = ui.available_size();
        let (rect, response) = ui.allocate_exact_size(available, Sense::click_and_drag());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, Color32::from_gray(18));

        let ruler_height = 20.0;
        let closeup_height = if self.layers.closeup { (rect.height() * 0.22).clamp(80.0, 180.0) } else { 0.0 };
        let lanes_rect = Rect::from_min_max(
            Pos2::new(rect.left(), rect.top() + ruler_height),
            Pos2::new(rect.right(), rect.bottom() - closeup_height - 6.0),
        );
        let lane_height = lanes_rect.height() / self.lanes.len().max(1) as f32;
        let (view_start, view_span) = (self.view.start, self.view.span);
        let to_x = |t: f64| rect.left() + ((t - view_start) / view_span) as f32 * rect.width();
        let to_t = |x: f32| view_start + ((x - rect.left()) / rect.width()) as f64 * view_span;

        // Ruler.
        let step = nice_step(self.view.span / (rect.width() as f64 / 90.0));
        let mut tick = (self.view.start / step).ceil() * step;
        while tick < self.view.start + self.view.span {
            let x = to_x(tick);
            painter.line_segment(
                [Pos2::new(x, rect.top() + 12.0), Pos2::new(x, rect.top() + ruler_height)],
                Stroke::new(1.0, Color32::GRAY),
            );
            painter.text(
                Pos2::new(x + 3.0, rect.top() + 1.0),
                Align2::LEFT_TOP,
                clock(tick),
                FontId::monospace(10.0),
                Color32::GRAY,
            );
            tick += step;
        }

        // Lanes.
        // Drawn level-matched to the loudest lane (playback matches to the
        // quietest, to stay clear of clipping).
        let gains = self.gains();
        let lowest = gains.iter().map(|&g| to_db(g)).fold(0.0f32, f32::min);
        for (i, &gain) in gains.iter().enumerate() {
            let lane_rect = Rect::from_min_size(
                Pos2::new(lanes_rect.left(), lanes_rect.top() + lane_height * i as f32),
                Vec2::new(lanes_rect.width(), lane_height - 2.0),
            );
            self.draw_lane(ui.ctx(), &painter, i, lane_rect, heard[i], to_db(gain) - lowest);
        }

        // The keyboard note: a band as long as it was held, as strong as
        // its velocity.
        if let Program::Note { key, velocity, length } = self.program {
            let (x0, x1) = (to_x(NOTE_START), to_x(NOTE_START + length as f64));
            let alpha = (40.0 + velocity as f32 / 127.0 * 160.0) as u8;
            let band = Rect::from_x_y_ranges(x0..=x1.max(x0 + 2.0), rect.top() + 2.0..=rect.top() + ruler_height - 2.0);
            painter.rect_filled(band, 2.0, Color32::from_rgba_unmultiplied(255, 200, 60, alpha));
            painter.text(
                band.left_center() + Vec2::new(4.0, 0.0),
                Align2::LEFT_CENTER,
                format!("{} · vel {velocity} · {length:.2} s", key_name(key)),
                FontId::proportional(11.0),
                Color32::BLACK,
            );
            for x in [x0, x1] {
                painter.line_segment(
                    [Pos2::new(x, lanes_rect.top()), Pos2::new(x, lanes_rect.bottom())],
                    Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 200, 60, 120)),
                );
            }
        }

        // Loop and playhead.
        if let Some(Some((start, end))) = self.with_transport(|t| t.loop_range) {
            let rate = self.sample_rate as f64;
            let r = Rect::from_x_y_ranges(to_x(start as f64 / rate)..=to_x(end as f64 / rate), lanes_rect.y_range());
            painter.rect_filled(r, 0.0, Color32::from_rgba_unmultiplied(255, 255, 255, 24));
        }
        let x = to_x(position);
        painter.line_segment(
            [Pos2::new(x, rect.top()), Pos2::new(x, lanes_rect.bottom())],
            Stroke::new(1.5, Color32::WHITE),
        );

        // Close-up of the waveforms at the playhead.
        let closeup = Rect::from_min_max(Pos2::new(rect.left(), rect.bottom() - closeup_height), rect.right_bottom());
        if self.layers.closeup {
            self.draw_closeup(&painter, closeup, position, &heard);
        }

        // Interaction.
        let shift = ui.input(|i| i.modifiers.shift);
        if let Some(pointer) = response.interact_pointer_pos() {
            if lanes_rect.contains(pointer) || pointer.y < lanes_rect.top() {
                let t = to_t(pointer.x).clamp(0.0, duration);
                if response.drag_started() && shift {
                    self.loop_drag = Some(t);
                }
                if let Some(start) = self.loop_drag {
                    let rate = self.sample_rate as f64;
                    let (a, b) = if t < start { (t, start) } else { (start, t) };
                    if b - a > 0.05 {
                        self.with_transport(|tr| tr.loop_range = Some(((a * rate) as usize, (b * rate) as usize)));
                    }
                } else if response.clicked() || response.dragged() {
                    self.seek(t);
                    // Clicking a lane's header focuses it.
                    let lane = ((pointer.y - lanes_rect.top()) / lane_height) as usize;
                    let in_header = (pointer.y - lanes_rect.top()) % lane_height < 20.0;
                    if response.clicked() && in_header && lane < self.lanes.len() {
                        self.focus_lane(lane);
                    }
                }
            }
        }
        if response.drag_stopped() {
            if let Some(start) = self.loop_drag.take() {
                let _ = start;
                if let Some(Some((a, _))) = self.with_transport(|t| t.loop_range) {
                    self.seek(a as f64 / self.sample_rate as f64);
                }
            }
        }
        if response.hovered() {
            let (zoom, scroll, pointer) = ui.input(|i| (i.zoom_delta(), i.smooth_scroll_delta, i.pointer.hover_pos()));
            if let Some(pointer) = pointer {
                if closeup.contains(pointer) {
                    if scroll.y != 0.0 || zoom != 1.0 {
                        let factor = if zoom != 1.0 { zoom } else { (1.0 + scroll.y / 200.0).max(0.1) };
                        self.view.zoom_ms = (self.view.zoom_ms / factor).clamp(1.0, 500.0);
                    }
                } else {
                    if zoom != 1.0 {
                        let anchor = to_t(pointer.x);
                        let span = (self.view.span / zoom as f64).clamp(0.05, duration.max(0.05));
                        self.view.start = anchor - (anchor - self.view.start) * span / self.view.span;
                        self.view.span = span;
                        self.view.follow = false;
                    }
                    let pan = if scroll.x != 0.0 { scroll.x } else { scroll.y };
                    if pan != 0.0 {
                        self.view.start -= pan as f64 / rect.width() as f64 * self.view.span;
                        self.view.follow = false;
                    }
                }
            }
        }
        if response.double_clicked() {
            self.view.start = 0.0;
            self.view.span = duration;
            self.view.follow = true;
        }
    }

    /// Draws a lane, its levels shifted by `gain_db` (level matching).
    fn draw_lane(
        &mut self,
        ctx: &egui::Context,
        painter: &egui::Painter,
        index: usize,
        rect: Rect,
        heard: bool,
        gain_db: f32,
    ) {
        let color = LANE_COLORS[index];
        let focused = index == self.focus;
        let layers = self.layers;
        let header = Rect::from_min_size(rect.min, Vec2::new(rect.width(), 18.0));
        let body = Rect::from_min_max(Pos2::new(rect.left(), header.bottom()), rect.max);
        painter.rect_filled(header, 0.0, if heard { color.gamma_multiply(0.35) } else { Color32::from_gray(30) });
        let effective = self.effective_source(index);
        let source = match &effective {
            Source::Synth { engine: Engine::Sf2synth, .. } => "sf2synth",
            Source::Synth { engine: Engine::MacSampler, .. } => {
                if self.japanese {
                    "Mac 標準"
                } else {
                    "macOS"
                }
            }
            Source::Wav { .. } => "WAV",
        };
        let name = match &effective {
            Source::Synth { sound, .. } => {
                let file = match &sound.file {
                    SoundFile::MacBuiltIn => "GS (built-in)".to_string(),
                    file => file.path().as_deref().map(file_name).unwrap_or_default(),
                };
                format!("{file} · {}:{}", sound.bank, sound.program)
            }
            Source::Wav { path } => path.as_deref().map(file_name).unwrap_or_default(),
        };
        let lane = &mut self.lanes[index];
        let speaker = if heard { "🔊" } else { "🔈" };
        let marker = if focused { "▶" } else { " " };
        painter.text(
            header.left_center() + Vec2::new(6.0, 0.0),
            Align2::LEFT_CENTER,
            format!("{marker} {}  {speaker}  {source} · {name}", lane_letter(index)),
            FontId::proportional(13.0),
            if heard { Color32::WHITE } else { color },
        );

        let (Some(audio), Some(analysis)) = (&lane.audio, &lane.analysis) else {
            let text = if lane.rendering {
                if self.japanese { "書き出し中…" } else { "Rendering…" }.to_string()
            } else if let Some(e) = &lane.error {
                e.clone()
            } else {
                String::new()
            };
            painter.text(body.center(), Align2::CENTER_CENTER, text, FontId::proportional(13.0), Color32::GRAY);
            return;
        };

        // Spectrogram.
        if layers.spectrogram && lane.texture.as_ref().is_none_or(|(_, made_for)| (made_for - gain_db).abs() > 0.05) {
            lane.texture = Some((spectrogram_texture(ctx, lane.id, analysis, gain_db), gain_db));
        }
        let texture = &lane.texture.as_ref().unwrap().0;
        let lane_duration = audio.duration();
        let view_end = self.view.start + self.view.span;
        let visible_end = view_end.min(lane_duration);
        if layers.spectrogram && visible_end > self.view.start {
            let x0 = body.left();
            let x1 = body.left() + ((visible_end - self.view.start) / self.view.span) as f32 * body.width();
            let uv = Rect::from_min_max(
                Pos2::new((self.view.start / lane_duration) as f32, 0.0),
                Pos2::new((visible_end / lane_duration) as f32, 1.0),
            );
            painter.image(texture.id(), Rect::from_x_y_ranges(x0..=x1, body.y_range()), uv, Color32::WHITE);
        }

        // The note's fundamental and harmonics.
        if let (true, Program::Note { key, .. }) = (layers.spectrogram, &self.program) {
            let fundamental = key_hz(*key);
            let span = (SPECTRUM_HIGH_HZ / SPECTRUM_LOW_HZ).ln();
            for n in 1..=16 {
                let f = fundamental * n as f32;
                if !(SPECTRUM_LOW_HZ..SPECTRUM_HIGH_HZ).contains(&f) {
                    continue;
                }
                let y = body.bottom() - (f / SPECTRUM_LOW_HZ).ln() / span * body.height();
                let alpha = if n == 1 { 110 } else { 45 };
                painter.line_segment(
                    [Pos2::new(body.left(), y), Pos2::new(body.right() - 30.0, y)],
                    Stroke::new(1.0, Color32::from_rgba_unmultiplied(120, 255, 200, alpha)),
                );
                if n <= 8 {
                    painter.text(
                        Pos2::new(body.left() + 3.0, y),
                        Align2::LEFT_BOTTOM,
                        if n == 1 { format!("{:.0} Hz", f) } else { format!("×{n}") },
                        FontId::monospace(9.0),
                        Color32::from_rgba_unmultiplied(120, 255, 200, 160),
                    );
                }
            }
        }

        if !layers.envelope {
            return;
        }
        // Level envelope (dB) on top.
        let mut points = Vec::with_capacity(body.width() as usize);
        let rate = audio.sample_rate;
        let width = body.width().max(1.0) as usize;
        for px in 0..width {
            let t0 = self.view.start + px as f64 / width as f64 * self.view.span;
            let t1 = self.view.start + (px + 1) as f64 / width as f64 * self.view.span;
            if t0 >= lane_duration {
                break;
            }
            let c0 = analysis.column_at(t0, rate);
            let c1 = analysis.column_at(t1, rate).max(c0);
            let level = analysis.envelope_db[c0..=c1].iter().fold(FLOOR_DB, |m, &v| m.max(v)) + gain_db;
            let y = body.bottom() - ((level + 72.0) / 72.0).clamp(0.0, 1.0) * body.height();
            points.push(Pos2::new(body.left() + px as f32, y));
        }
        painter.add(Shape::line(points, Stroke::new(1.5, color)));
        for db in [-12.0, -24.0, -48.0] {
            let y = body.bottom() - ((db + 72.0) / 72.0) * body.height();
            painter.text(
                Pos2::new(body.right() - 4.0, y),
                Align2::RIGHT_CENTER,
                format!("{db:.0}"),
                FontId::monospace(9.0),
                Color32::from_gray(110),
            );
        }
    }

    fn draw_closeup(&self, painter: &egui::Painter, rect: Rect, position: f64, heard: &[bool]) {
        painter.rect_filled(rect, 0.0, Color32::from_gray(12));
        let mid = rect.center().y;
        painter.line_segment(
            [Pos2::new(rect.left(), mid), Pos2::new(rect.right(), mid)],
            Stroke::new(1.0, Color32::from_gray(50)),
        );
        let half = self.view.zoom_ms as f64 / 1000.0;
        let gains = self.gains();
        // Lanes not heard first, faint; the heard ones on top.
        let mut order: Vec<usize> = (0..self.lanes.len()).filter(|&i| !heard[i]).collect();
        order.extend((0..self.lanes.len()).filter(|&i| heard[i]));
        let width = rect.width().max(1.0) as usize;
        for i in order {
            let Some(audio) = &self.lanes[i].audio else { continue };
            let rate = audio.sample_rate as f64;
            let gain = gains[i];
            let mut points = Vec::with_capacity(width);
            for px in 0..width {
                let t = position - half + px as f64 / width as f64 * 2.0 * half;
                let frame = (t * rate) as isize;
                let value = if frame >= 0 && (frame as usize) < audio.frames() {
                    0.5 * (audio.left[frame as usize] + audio.right[frame as usize]) * gain
                } else {
                    0.0
                };
                points.push(Pos2::new(
                    rect.left() + px as f32,
                    mid - value.clamp(-1.0, 1.0) * rect.height() * 0.48 * 2.0,
                ));
            }
            let stroke = if heard[i] {
                Stroke::new(1.6, LANE_COLORS[i])
            } else {
                Stroke::new(1.0, LANE_COLORS[i].gamma_multiply(0.5))
            };
            painter.add(Shape::line(points, stroke));
        }
        painter.line_segment(
            [Pos2::new(rect.center().x, rect.top()), Pos2::new(rect.center().x, rect.bottom())],
            Stroke::new(1.0, Color32::from_gray(90)),
        );
        painter.text(
            rect.left_top() + Vec2::new(6.0, 4.0),
            Align2::LEFT_TOP,
            format!(
                "±{:.0} ms  {}",
                self.view.zoom_ms,
                self.t("(scroll here to zoom)", "（ここでスクロールして拡大縮小）")
            ),
            FontId::proportional(11.0),
            Color32::GRAY,
        );
    }
}

impl eframe::App for StudioApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.poll_renders();
        self.ensure_live();
        self.poll_midi();
        self.tick_note();
        self.handle_keys(ui.ctx());
        egui::Panel::top("top").show(ui, |ui| {
            ui.add_space(4.0);
            self.top_bar(ui);
            ui.add_space(4.0);
        });
        egui::Panel::bottom("transport").show(ui, |ui| {
            ui.add_space(4.0);
            self.transport_bar(ui);
            ui.add_space(4.0);
        });
        if self.mode == Mode::Tune {
            egui::Panel::bottom("keyboard").resizable(true).default_size(210.0).min_size(120.0).show(ui, |ui| {
                ui.add_space(4.0);
                self.keyboard_ui(ui);
            });
        }
        if self.mode != Mode::Create {
            egui::Panel::left("lanes").resizable(true).default_size(280.0).show(ui, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    self.program_ui(ui);
                    ui.separator();
                    self.lanes_ui(ui);
                });
            });
        }
        if self.mode == Mode::Tune {
            egui::Panel::right("tuning").resizable(true).default_size(300.0).show(ui, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| self.tuning_ui(ui));
            });
        }
        egui::CentralPanel::default().show(ui, |ui| match self.mode {
            Mode::Create => self.create_ui(ui),
            Mode::Tune | Mode::Compare => self.timeline(ui),
        });
        if self.is_playing()
            || self.pending_render.is_some()
            || self.pressed.is_some()
            || self.lanes.iter().any(|l| l.rendering)
        {
            ui.ctx().request_repaint();
        }
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        let saved = Saved {
            japanese: self.japanese,
            mode: self.mode,
            program: self.program.clone(),
            lanes: self.lanes.iter().map(|l| l.source.clone()).collect(),
            level_match: self.level_match,
            layers: self.layers,
            keyboard: self.keyboard_settings,
        };
        eframe::set_value(storage, eframe::APP_KEY, &saved);
    }
}

/// Command-line arguments replace the saved lanes and program:
/// `--sf2 <file>`, `--mac <file>`, `--mac-builtin`, `--wav <file>` (one lane
/// each, in order), `--midi <file>`, `--mode create|tune|compare`.
fn apply_arguments(saved: &mut Saved, arguments: impl Iterator<Item = String>) {
    let arguments: Vec<String> = arguments.collect();
    let mut lanes = Vec::new();
    let mut i = 0;
    while i < arguments.len() {
        let value = arguments.get(i + 1).map(PathBuf::from);
        let file = |value: &Option<PathBuf>| value.clone().map(SoundFile::File);
        let used = match (arguments[i].as_str(), &value) {
            ("--sf2", Some(_)) => {
                lanes.push(Source::sf2synth(Sound { file: file(&value).unwrap(), bank: 0, program: 0 }));
                2
            }
            ("--mac", Some(_)) => {
                lanes.push(Source::mac_sampler(Sound { file: file(&value).unwrap(), bank: 0, program: 0 }));
                2
            }
            ("--mac-builtin", _) => {
                lanes.push(Source::mac_sampler(Sound { file: SoundFile::MacBuiltIn, bank: 0, program: 0 }));
                1
            }
            ("--wav", Some(path)) => {
                lanes.push(Source::Wav { path: Some(path.clone()) });
                2
            }
            ("--midi", Some(path)) => {
                saved.program = Program::Midi(path.clone());
                2
            }
            ("--mode", Some(mode)) => {
                saved.mode = match mode.to_str() {
                    Some("create") => Mode::Create,
                    Some("tune") => Mode::Tune,
                    _ => Mode::Compare,
                };
                2
            }
            _ => 1,
        };
        i += used;
    }
    if !lanes.is_empty() {
        saved.lanes = lanes;
    }
}

fn is_dls(path: &std::path::Path) -> bool {
    path.extension().is_some_and(|e| e.eq_ignore_ascii_case("dls"))
}

fn lane_letter(index: usize) -> char {
    (b'A' + index as u8) as char
}

fn file_name(path: &std::path::Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| path.display().to_string())
}

fn clock(seconds: f64) -> String {
    let seconds = seconds.max(0.0);
    format!("{}:{:04.1}", (seconds / 60.0) as u32, seconds % 60.0)
}

fn nice_step(raw: f64) -> f64 {
    let steps = [0.01, 0.02, 0.05, 0.1, 0.2, 0.5, 1.0, 2.0, 5.0, 10.0, 15.0, 30.0, 60.0, 120.0, 300.0];
    steps.iter().copied().find(|&s| s >= raw).unwrap_or(600.0)
}

/// The spectrogram as an image: time across, frequency up, colored by level.
fn spectrogram_texture(ctx: &egui::Context, id: u64, analysis: &Analysis, gain_db: f32) -> TextureHandle {
    let (width, height) = (analysis.columns, SPECTRUM_ROWS);
    let mut pixels = vec![Color32::BLACK; width * height];
    for column in 0..width {
        for row in 0..height {
            let db = analysis.spectrogram_db[column * SPECTRUM_ROWS + row] + gain_db;
            pixels[(height - 1 - row) * width + column] = heat((db + 90.0) / 80.0);
        }
    }
    let image = ColorImage::new([width, height], pixels);
    ctx.load_texture(format!("spectrogram-{id}"), image, TextureOptions::LINEAR)
}

/// A dark-to-bright color ramp (black, purple, orange, pale yellow).
fn heat(t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let stops =
        [(0.0, [0, 0, 4]), (0.3, [60, 15, 110]), (0.55, [180, 50, 100]), (0.8, [250, 140, 40]), (1.0, [252, 250, 190])];
    for pair in stops.windows(2) {
        let ((a, ca), (b, cb)) = (pair[0], pair[1]);
        if t <= b {
            let f = (t - a) / (b - a);
            let mix = |i: usize| (ca[i] as f32 + (cb[i] as f32 - ca[i] as f32) * f) as u8;
            return Color32::from_rgb(mix(0), mix(1), mix(2));
        }
    }
    Color32::from_rgb(252, 250, 190)
}

/// Adds a system Japanese font as a fallback so the Japanese UI shows.
fn install_fonts(ctx: &egui::Context) {
    let candidates: &[(&str, u32)] = &[
        ("/System/Library/Fonts/ヒラギノ角ゴシック W3.ttc", 0),
        ("/System/Library/Fonts/Hiragino Sans GB.ttc", 0),
        ("/Library/Fonts/Arial Unicode.ttf", 0),
        ("C:\\Windows\\Fonts\\YuGothM.ttc", 0),
        ("C:\\Windows\\Fonts\\meiryo.ttc", 0),
        ("C:\\Windows\\Fonts\\msgothic.ttc", 0),
        ("/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc", 0),
    ];
    for (path, index) in candidates {
        if let Ok(bytes) = std::fs::read(PathBuf::from(path)) {
            let mut data = FontData::from_owned(bytes);
            data.index = *index;
            ctx.add_font(FontInsert {
                name: "japanese".into(),
                data,
                families: vec![
                    InsertFontFamily { family: FontFamily::Proportional, priority: FontPriority::Lowest },
                    InsertFontFamily { family: FontFamily::Monospace, priority: FontPriority::Lowest },
                ],
            });
            return;
        }
    }
}
