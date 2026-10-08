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
    message: Option<String>,
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
        }
    }

    pub fn created_generation(&self) -> Option<u64> {
        self.created.as_ref().map(|(_, g)| *g)
    }

    pub fn created_font(&self) -> Option<Arc<SoundFont>> {
        self.created.as_ref().map(|(c, _)| Arc::clone(&c.font))
    }

    pub fn busy(&self) -> bool {
        self.pending.is_some() || self.progress.is_some()
    }

    fn changed(&mut self) {
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
        ui.strong(t("Start from", "元にする音"));
        let kind = match &self.create.origin {
            Origin::Sf2 { .. } => Kind::Sf2,
            Origin::Recordings { .. } => Kind::Recordings,
            Origin::Synthesis(_) => Kind::Synthesis,
        };
        let mut chosen = kind;
        ui.horizontal(|ui| {
            ui.selectable_value(&mut chosen, Kind::Sf2, t("An SF2", "既存の SF2"));
            ui.selectable_value(&mut chosen, Kind::Recordings, t("Recordings", "録音"));
            ui.selectable_value(&mut chosen, Kind::Synthesis, t("Synthesis", "合成"));
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
                    "Re-voice a preset of an SF2: its samples are kept and shaped by the settings on the right.",
                    "SF2 のプリセットを作り直します。サンプルはそのまま使い、右の設定で整えます。",
                ));
                let label =
                    path.as_deref().map(file_name).unwrap_or_else(|| t("Choose an SF2…", "SF2 を選ぶ…").to_string());
                if ui.button(label).clicked()
                    && let Some(chosen) = rfd::FileDialog::new().add_filter("SF2", &["sf2"]).pick_file()
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
                if ui.button(t("Add WAV files…", "WAV を追加…")).clicked()
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
                        if ui.small_button("×").clicked() {
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
                ui.strong(t("Level and tone", "音量と音色"));
                ui.add(egui::Slider::new(&mut v.volume_db, -12.0..=12.0).suffix(" dB").text(t("Volume", "音量")));
                ui.add(egui::Slider::new(&mut v.brightness_db, -12.0..=12.0).suffix(" dB").text(t("Brightness (highs)", "明るさ（高域）")));
                ui.add(egui::Slider::new(&mut v.warmth_db, -12.0..=12.0).suffix(" dB").text(t("Warmth (lows)", "温かみ（低域）")));
                ui.add_space(8.0);
                ui.strong(t("Touch", "タッチ（強さ）"));
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
                ui.strong(t("Sustain", "余韻"));
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
                ui.strong(t("Tuning and space", "調律と広がり"));
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
                if ui.add_enabled(ready, egui::Button::new(t("Save SF2…", "SF2 を保存…"))).clicked() {
                    self.save_created();
                }
                if ui
                    .add_enabled(ready, egui::Button::new(t("Compare with others", "比較に追加")))
                    .on_hover_text(t("Adds (or updates) a lane playing this instrument.", "この音源を鳴らすレーンを追加（または更新）します。"))
                    .clicked()
                {
                    self.send_to_compare();
                    if let Some(message) = &mut self.create.message {
                        message.push_str(if japanese { " 比較モードで見られます。" } else { " See it in Compare." });
                    }
                }
                if ui.button(t("Reset settings", "設定を初期値に戻す")).clicked() {
                    let name = self.create.voicing.name.clone();
                    self.create.voicing = Voicing { name, ..Voicing::default() };
                    self.create.changed();
                }
            });
            if let Some(message) = &self.create.message {
                ui.weak(message);
            }
            ui.add_space(8.0);
            ui.label(RichText::new(t(
                "Brightness and warmth are baked into the samples; touch, sustain and tuning are written as SF2 parameters, so other SF2 players follow them too.",
                "明るさと温かみはサンプルに焼き込みます。タッチ・余韻・調律は SF2 のパラメータとして書くので、ほかの SF2 プレイヤーでも同じように反映されます。",
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

    /// Writes the instrument to a scratch file and shows it in a lane.
    fn send_to_compare(&mut self) {
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

/// A file name from an instrument name.
fn safe_name(name: &str) -> String {
    let cleaned: String =
        name.trim().chars().map(|c| if c.is_alphanumeric() || " -_".contains(c) { c } else { '_' }).collect();
    if cleaned.is_empty() { "instrument".into() } else { cleaned }
}
