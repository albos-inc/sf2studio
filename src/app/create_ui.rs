//! Create mode: choose what to start from, shape it, play it on the
//! keyboard, save it or compare it.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};

use eframe::egui::{self, Color32, RichText};
use sf2synth::SoundFont;

use super::{StudioApp, file_name};
use crate::create::dsp::SynthesisParams;
use crate::create::{self, Created, Origin, Region, Update, Voicing};
use crate::render::{Sound, SoundFile, Source};
use crate::theme::{icon, labeled};
use crate::tuning::VelocityCurve;

/// How long changes settle before the instrument is made again.
const MAKE_DELAY: Duration = Duration::from_millis(400);

pub(super) struct CreateState {
    pub origin: Origin,
    pub voicing: Voicing,
    generation: u64,
    pending: Option<Instant>,
    progress: Option<f32>,
    created: Option<(Created, u64)>,
    error: Option<String>,
    material: Option<(Origin, Arc<Vec<Region>>)>,
    sender: Sender<Update>,
    receiver: Receiver<Update>,
    /// The Compare lane showing the made instrument.
    lane: Option<u64>,
    pub(super) message: Option<String>,
    /// An SFZ is being written.
    saving: bool,
}

impl CreateState {
    pub fn new(origin: Origin, voicing: Voicing) -> CreateState {
        let (sender, receiver) = channel();
        let ready = !matches!(&origin, Origin::Sf2 { path: None, .. } | Origin::Recordings { .. });
        CreateState {
            origin,
            voicing,
            generation: 0,
            pending: ready.then(Instant::now),
            progress: None,
            created: None,
            error: None,
            material: None,
            sender,
            receiver,
            lane: None,
            message: None,
            saving: false,
        }
    }

    pub fn created_generation(&self) -> Option<u64> {
        self.created.as_ref().map(|(_, g)| *g)
    }

    pub fn created_font(&self) -> Option<Arc<SoundFont>> {
        self.created.as_ref().map(|(c, _)| Arc::clone(&c.font))
    }

    pub fn busy(&self) -> bool {
        self.pending.is_some() || self.progress.is_some() || self.saving
    }

    pub(super) fn changed(&mut self) {
        self.pending = Some(Instant::now());
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Sf2,
    Recordings,
    Synthesis,
}

impl StudioApp {
    /// Runs the instrument maker when changes have settled, and takes its results.
    pub(super) fn poll_create(&mut self, ctx: &egui::Context) {
        let updates: Vec<Update> = self.create.receiver.try_iter().collect();
        for update in updates {
            match update {
                Update::Progress { generation, fraction } if generation == self.create.generation => {
                    self.create.progress = Some(fraction);
                }
                Update::Progress { .. } => {}
                Update::Material { origin, regions } => self.create.material = Some((origin, regions)),
                Update::Done { generation, result } if generation == self.create.generation => {
                    self.create.progress = None;
                    match result {
                        Ok(created) => {
                            self.create.error = None;
                            self.create.created = Some((created, generation));
                            if self.create.lane.is_some() {
                                self.send_to_compare();
                            }
                        }
                        Err(error) => self.create.error = Some(error),
                    }
                }
                Update::Done { .. } => {}
                Update::Saved(result) => {
                    self.create.saving = false;
                    self.create.message = Some(result.unwrap_or_else(|error| error));
                }
            }
        }
        if self.create.pending.is_some_and(|at| at.elapsed() >= MAKE_DELAY) {
            self.create.pending = None;
            self.start_create(ctx);
        }
    }

    fn start_create(&mut self, ctx: &egui::Context) {
        let state = &mut self.create;
        state.generation += 1;
        state.progress = Some(0.0);
        state.message = None;
        let regions = state.material.as_ref().filter(|(o, _)| *o == state.origin).map(|(_, r)| Arc::clone(r));
        let font = match &state.origin {
            Origin::Sf2 { path: Some(path), .. } if regions.is_none() => self.renderer.font(path).ok(),
            _ => None,
        };
        let ctx = ctx.clone();
        create::spawn(
            create::Job {
                generation: state.generation,
                origin: state.origin.clone(),
                regions,
                font,
                voicing: state.voicing.clone(),
            },
            state.sender.clone(),
            move || ctx.request_repaint(),
        );
    }

    /// The left panel: what the instrument starts from.
    pub(super) fn origin_ui(&mut self, ui: &mut egui::Ui) {
        let japanese = self.japanese;
        let t = |en: &'static str, ja: &'static str| if japanese { ja } else { en };
        ui.label(crate::theme::section(t("Start from", "元にする音")));
        let kind = match &self.create.origin {
            Origin::Sf2 { .. } => Kind::Sf2,
            Origin::Recordings { .. } => Kind::Recordings,
            Origin::Synthesis(_) => Kind::Synthesis,
        };
        let mut chosen = kind;
        ui.horizontal(|ui| {
            ui.selectable_value(
                &mut chosen,
                Kind::Sf2,
                labeled(icon::FILE_AUDIO, t("An SF2 / SFZ", "既存の SF2 / SFZ")),
            );
            ui.selectable_value(&mut chosen, Kind::Recordings, labeled(icon::MICROPHONE, t("Recordings", "録音")));
            ui.selectable_value(&mut chosen, Kind::Synthesis, labeled(icon::WAVE_SINE, t("Synthesis", "合成")));
        });
        if chosen != kind {
            self.create.origin = match chosen {
                Kind::Sf2 => Origin::Sf2 { path: None, bank: 0, program: 0 },
                Kind::Recordings => Origin::Recordings { files: Vec::new() },
                Kind::Synthesis => Origin::Synthesis(SynthesisParams::default()),
            };
            self.create.changed();
        }
        ui.add_space(6.0);
        let before = self.create.origin.clone();
        match &mut self.create.origin {
            Origin::Sf2 { path, bank, program } => {
                ui.label(t(
                    "Re-voice a preset of an SF2, DLS or SFZ: its samples are kept and shaped by the settings on the right. Of an SFZ, the regions a plain note plays are used (no release sounds, the first of a round robin).",
                    "SF2・DLS・SFZ のプリセットを作り直します。サンプルはそのまま使い、右の設定で整えます。SFZ からは、普通に鍵盤を弾いて鳴る音を使います（離鍵時の音は使わず、ラウンドロビンは最初の 1 つだけ）。",
                ));
                let label = path
                    .as_deref()
                    .map(file_name)
                    .unwrap_or_else(|| t("Choose an SF2, DLS or SFZ…", "SF2 / DLS / SFZ を選ぶ…").to_string());
                if ui.button(label).clicked()
                    && let Some(chosen) =
                        rfd::FileDialog::new().add_filter("SF2 / DLS / SFZ", &["sf2", "dls", "sfz"]).pick_file()
                {
                    *path = Some(chosen);
                    *bank = 0;
                    *program = 0;
                }
                if let Some(font) = path.as_ref().and_then(|p| self.renderer.font(p).ok()) {
                    let mut presets: Vec<(u16, u8, String)> =
                        font.presets().iter().map(|p| (p.bank(), p.program() as u8, p.name().to_string())).collect();
                    presets.sort();
                    let current = presets
                        .iter()
                        .find(|(b, p, _)| b == bank && p == program)
                        .map(|(b, p, n)| format!("{b}:{p} {n}"))
                        .unwrap_or_else(|| format!("{bank}:{program}"));
                    egui::ComboBox::from_id_salt("create-preset").width(240.0).selected_text(current).show_ui(
                        ui,
                        |ui| {
                            for (b, p, n) in &presets {
                                if ui.selectable_label(b == bank && p == program, format!("{b}:{p} {n}")).clicked() {
                                    *bank = *b;
                                    *program = *p;
                                }
                            }
                        },
                    );
                }
            }
            Origin::Recordings { files } => {
                ui.label(t(
                    "One WAV per note. The key is read from the file name (C4, F#2, 60…) or detected from the pitch.",
                    "1 音につき 1 つの WAV です。鍵盤はファイル名（C4、F#2、60 など）から読み取り、なければ音程から検出します。",
                ));
                if ui.button(labeled(icon::PLUS, t("Add WAV files…", "WAV を追加…"))).clicked()
                    && let Some(chosen) = rfd::FileDialog::new().add_filter("WAV", &["wav"]).pick_files()
                {
                    for file in chosen {
                        if !files.contains(&file) {
                            files.push(file);
                        }
                    }
                }
                let mut remove = None;
                for (i, file) in files.iter().enumerate() {
                    ui.horizontal(|ui| {
                        if ui.small_button(icon::X).clicked() {
                            remove = Some(i);
                        }
                        let key = crate::create::dsp::key_from_name(&file_name(file));
                        ui.label(file_name(file));
                        if let Some(key) = key {
                            ui.weak(crate::patterns::key_name(key));
                        }
                    });
                }
                if let Some(i) = remove {
                    files.remove(i);
                }
                if !files.is_empty() && ui.small_button(t("Remove all", "すべて外す")).clicked() {
                    files.clear();
                }
            }
            Origin::Synthesis(params) => {
                ui.label(t(
                    "A piano-like tone made from scratch: stiff strings of decaying partials and a hammer thump.",
                    "ピアノ風の音を一から作ります。減衰する倍音でできた弦と、ハンマーの打音です。",
                ));
                ui.add(egui::Slider::new(&mut params.brightness, 0.0..=1.0).text(t("Brightness", "明るさ")));
                ui.add(
                    egui::Slider::new(&mut params.inharmonicity, 0.0..=1.0)
                        .text(t("String stiffness", "弦の硬さ（倍音のずれ）")),
                );
                ui.add(
                    egui::Slider::new(&mut params.decay, 1.0..=30.0)
                        .suffix(" s")
                        .text(t("Decay (middle C)", "減衰（中央の C）")),
                );
                ui.add(
                    egui::Slider::new(&mut params.detune, 0.0..=5.0)
                        .suffix(" cents")
                        .text(t("String detune", "弦のずれ（うなり）")),
                );
                ui.add(egui::Slider::new(&mut params.hammer, 0.0..=1.0).text(t("Hammer noise", "ハンマーの打音")));
            }
        }
        if self.create.origin != before {
            self.create.changed();
        }
    }

    /// The centre: how the instrument should sound, and what to do with it.
    pub(super) fn create_ui(&mut self, ui: &mut egui::Ui) {
        let japanese = self.japanese;
        let t = |en: &'static str, ja: &'static str| if japanese { ja } else { en };
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.heading(t("Shape the instrument", "音源を好みに整える"));
            ui.add_space(4.0);
            let before = self.create.voicing.clone();
            let v = &mut self.create.voicing;
            ui.horizontal(|ui| {
                ui.label(t("Name", "名前"));
                ui.text_edit_singleline(&mut v.name);
            });
            ui.add_space(6.0);
            ui.columns(2, |columns| {
                let ui = &mut columns[0];
                ui.label(crate::theme::section(t("Level and tone", "音量と音色")));
                ui.add(egui::Slider::new(&mut v.volume_db, -12.0..=12.0).suffix(" dB").text(t("Volume", "音量")));
                ui.add(egui::Slider::new(&mut v.brightness_db, -12.0..=12.0).suffix(" dB").text(t("Brightness (highs)", "明るさ（高域）")));
                ui.add(egui::Slider::new(&mut v.warmth_db, -12.0..=12.0).suffix(" dB").text(t("Warmth (lows)", "温かみ（低域）")));
                ui.add_space(8.0);
                ui.label(crate::theme::section(t("Touch", "タッチ（強さ）")));
                let mut range_db = v.velocity_range_cb / 10.0;
                if ui.add(egui::Slider::new(&mut range_db, 0.0..=96.0).suffix(" dB").text(t("Soft-to-loud range", "弱音と強音の差"))).changed() {
                    v.velocity_range_cb = range_db * 10.0;
                }
                egui::ComboBox::from_id_salt("create-curve").selected_text(v.velocity_curve.name(japanese)).show_ui(ui, |ui| {
                    for curve in VelocityCurve::ALL {
                        ui.selectable_value(&mut v.velocity_curve, curve, curve.name(japanese));
                    }
                });
                ui.add(
                    egui::Slider::new(&mut v.velocity_brightness_cents, -4800.0..=0.0)
                        .suffix(" cents")
                        .text(t("Soft notes darker", "弱音をこもらせる")),
                );
                ui.add(egui::Slider::new(&mut v.layers, 1..=4).text(t("Velocity layers", "強さのレイヤー数")));
                ui.add_enabled(
                    v.layers > 1,
                    egui::Slider::new(&mut v.softest_cutoff_hz, 500.0..=8000.0)
                        .logarithmic(true)
                        .suffix(" Hz")
                        .text(t("Softest layer cutoff", "一番弱いレイヤーの高域")),
                );

                let ui = &mut columns[1];
                ui.label(crate::theme::section(t("Sustain", "余韻")));
                ui.add(egui::Slider::new(&mut v.attack_ms, 0.0..=100.0).suffix(" ms").text(t("Attack (0 = as is)", "立ち上がり（0 = 元のまま）")));
                ui.add(
                    egui::Slider::new(&mut v.decay_s, 0.0..=30.0)
                        .suffix(" s")
                        .text(t("Fade while held (0 = as is)", "押したままの減衰（0 = 元のまま）")),
                );
                ui.add(
                    egui::Slider::new(&mut v.release_s, 0.0..=5.0)
                        .suffix(" s")
                        .text(t("Release (0 = as is)", "離したあとの余韻（0 = 元のまま）")),
                );
                ui.add_space(8.0);
                ui.label(crate::theme::section(t("Tuning and space", "調律と広がり")));
                ui.add(egui::Slider::new(&mut v.stretch_cents, -30.0..=30.0).suffix(" cents").text(t("Stretch tuning", "ストレッチ調律")));
                ui.add(egui::Slider::new(&mut v.width, 0.0..=1.5).text(t("Stereo width", "ステレオの広がり")));
                ui.add(egui::Slider::new(&mut v.reverb_percent, 0.0..=100.0).suffix(" %").text(t("Reverb", "リバーブ")));
            });
            if self.create.voicing != before {
                self.create.changed();
            }
            ui.add_space(10.0);
            ui.separator();

            // Status and actions.
            if let Some(fraction) = self.create.progress {
                ui.add(egui::ProgressBar::new(fraction).text(t("Making…", "作成中…")).show_percentage());
            } else if let Some(error) = &self.create.error {
                ui.colored_label(Color32::LIGHT_RED, error);
            } else if let Some((created, _)) = &self.create.created {
                ui.label(format!(
                    "{} · {} {} · {:.1} MB · {:.1} s",
                    t("Ready — play it on the keyboard below", "完成 — 下の鍵盤で弾けます"),
                    created.zones,
                    t("zones", "ゾーン"),
                    created.bytes.len() as f64 / 1_048_576.0,
                    created.seconds,
                ));
            } else {
                ui.weak(t("Choose what to start from on the left.", "左で元にする音を選んでください。"));
            }
            let ready = self.create.created.is_some();
            ui.horizontal(|ui| {
                if ui.add_enabled(ready, egui::Button::new(labeled(icon::FLOPPY_DISK, t("Save SF2…", "SF2 を保存…")))).clicked() {
                    self.save_created();
                }
                if ui
                    .add_enabled(ready && !self.create.saving, egui::Button::new(labeled(icon::EXPORT, t("Save SFZ…", "SFZ を保存…"))))
                    .on_hover_text(t(
                        "An SFZ file and its samples as lossless FLAC, in a folder beside it: plays the same as the SF2, a fraction of the size.",
                        "SFZ ファイルと、そのサンプル（可逆圧縮の FLAC）を隣のフォルダに保存します。SF2 とまったく同じ音のまま、サイズは数分の一になります。",
                    ))
                    .clicked()
                {
                    self.save_created_sfz(ui.ctx());
                }
                if ui
                    .add_enabled(ready, egui::Button::new(labeled(icon::SCALES, t("Compare with others", "比較に追加"))))
                    .on_hover_text(t("Adds (or updates) a lane playing this instrument.", "この音源を鳴らすレーンを追加（または更新）します。"))
                    .clicked()
                {
                    self.send_to_compare();
                    if let Some(message) = &mut self.create.message {
                        message.push_str(if japanese { " 比較モードで見られます。" } else { " See it in Compare." });
                    }
                }
                if ui.button(labeled(icon::ARROW_COUNTER_CLOCKWISE, t("Reset settings", "設定を初期値に戻す"))).clicked() {
                    let name = self.create.voicing.name.clone();
                    self.create.voicing = Voicing { name, ..Voicing::default() };
                    self.create.changed();
                }
            });
            if self.create.saving {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(t("Writing the SFZ and its FLAC samples…", "SFZ と FLAC サンプルを書き出し中…"));
                });
            }
            if let Some(message) = &self.create.message {
                ui.weak(message);
            }
            ui.add_space(8.0);
            ui.label(RichText::new(t(
                "Brightness and warmth are baked into the samples; touch, sustain and tuning are written as SF2 (or SFZ) parameters, so other players follow them too.",
                "明るさと温かみはサンプルに焼き込みます。タッチ・余韻・調律は SF2（または SFZ）のパラメータとして書くので、ほかのプレイヤーでも同じように反映されます。",
            )).small().weak());
        });
    }

    fn save_created(&mut self) {
        let Some((created, _)) = &self.create.created else { return };
        let name = format!("{}.sf2", safe_name(&self.create.voicing.name));
        if let Some(path) = rfd::FileDialog::new().add_filter("SF2", &["sf2"]).set_file_name(name).save_file() {
            self.create.message = Some(match std::fs::write(&path, created.bytes.as_slice()) {
                Ok(()) => format!("{} {}", if self.japanese { "保存しました:" } else { "Saved" }, path.display()),
                Err(e) => e.to_string(),
            });
        }
    }

    /// Writes the instrument as SFZ with FLAC samples (in the background:
    /// encoding a large instrument takes a while).
    fn save_created_sfz(&mut self, ctx: &egui::Context) {
        let Some((created, _)) = &self.create.created else { return };
        let stem = safe_name(&self.create.voicing.name);
        let Some(path) =
            rfd::FileDialog::new().add_filter("SFZ", &["sfz"]).set_file_name(format!("{stem}.sfz")).save_file()
        else {
            return;
        };
        let font = Arc::clone(&created.font);
        let sender = self.create.sender.clone();
        let japanese = self.japanese;
        let ctx = ctx.clone();
        self.create.saving = true;
        self.create.message = None;
        std::thread::spawn(move || {
            let result = save_sfz(&font, &path, japanese);
            let _ = sender.send(Update::Saved(result));
            ctx.request_repaint();
        });
    }

    /// Writes the instrument to a scratch file and shows it in a lane.
    pub(super) fn send_to_compare(&mut self) {
        let Some((created, _)) = &self.create.created else { return };
        let directory = std::env::temp_dir().join("sf2studio");
        let path: PathBuf = directory.join(format!("{}.sf2", safe_name(&self.create.voicing.name)));
        if let Err(e) =
            std::fs::create_dir_all(&directory).and_then(|_| std::fs::write(&path, created.bytes.as_slice()))
        {
            self.create.message = Some(e.to_string());
            return;
        }
        self.renderer.forget_font(&path);
        let sound = Sound { file: SoundFile::File(path), bank: 0, program: 0 };
        let existing = self.create.lane.and_then(|id| self.lanes.iter().position(|l| l.id == id));
        let index = match existing {
            Some(index) => {
                self.lanes[index].source = Source::sf2synth(sound);
                self.render_lane(index);
                index
            }
            None => {
                if self.lanes.len() >= super::MAX_LANES {
                    self.create.message = Some(
                        if self.japanese { "レーンがいっぱいです。" } else { "No room for another lane." }.into(),
                    );
                    return;
                }
                self.add_lane(Source::sf2synth(sound));
                let index = self.lanes.len() - 1;
                self.create.lane = Some(self.lanes[index].id);
                index
            }
        };
        let letter = super::lane_letter(index);
        self.create.message = Some(if self.japanese {
            format!("レーン {letter} に入れました。")
        } else {
            format!("In lane {letter}.")
        });
    }
}

/// Writes `font`'s first preset at `path` as SFZ, its FLAC samples in a
/// folder named after it; a message saying what was written.
fn save_sfz(font: &SoundFont, path: &std::path::Path, japanese: bool) -> Result<String, String> {
    let stem = path.file_stem().map_or("instrument".into(), |s| s.to_string_lossy().into_owned());
    let files = sf2synth::writer::SfzFiles::from_preset(font, 0, &format!("{stem} samples/"));
    files.write(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let megabytes = files.samples.iter().map(|(_, bytes)| bytes.len()).sum::<usize>() as f64 / 1_048_576.0;
    let mut message = if japanese {
        format!("保存しました: {}（FLAC {} 個、{megabytes:.1} MB）", path.display(), files.samples.len())
    } else {
        format!("Saved {} ({} FLAC files, {megabytes:.1} MB)", path.display(), files.samples.len())
    };
    if !files.warnings.is_empty() {
        message.push_str(if japanese {
            " SFZ で表せず省いたもの: "
        } else {
            " Left out (SFZ can't say it): "
        });
        message.push_str(&files.warnings.join(", "));
    }
    Ok(message)
}

/// A file name from an instrument name.
fn safe_name(name: &str) -> String {
    let cleaned: String =
        name.trim().chars().map(|c| if c.is_alphanumeric() || " -_".contains(c) { c } else { '_' }).collect();
    if cleaned.is_empty() { "instrument".into() } else { cleaned }
}
