//! The guide's screenshots: `cargo run --features capture -- --capture
//! docs/images` sets the app up scene by scene, in English and Japanese,
//! circles and numbers what the guide refers to and saves each view as
//! `docs/images/<en|ja>/<name>.png`. The real mouse and keyboard are ignored
//! while it runs.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use eframe::egui::{
    self, Align2, Color32, ColorImage, CornerRadius, Event, FontId, Id, LayerId, Order, PointerButton, Pos2, Rect,
    Stroke, StrokeKind, Vec2,
};

use super::{KeyboardSettings, Layers, Mode, NullState, StudioApp, View};
use crate::create::dsp::SynthesisParams;
use crate::create::{Origin, Voicing};
use crate::patterns::TestPattern;
use crate::render::{MAC_BUILT_IN_DLS, Program, Sound, SoundFile, Source};
use crate::theme::{ThemeChoice, icon};

const CALLOUT: Color32 = Color32::from_rgb(232, 52, 92);

type Marks = Vec<(&'static str, usize, Rect)>;

/// Records a named area of this frame.
pub fn mark(ctx: &egui::Context, name: &'static str, index: usize, rect: Rect) {
    ctx.data_mut(|d| d.get_temp_mut_or_default::<Marks>(Id::new("capture-marks")).push((name, index, rect)));
}

/// What a callout or a crop points at.
#[derive(Clone)]
enum Target {
    /// A widget whose text (icons left out) is this, in English and Japanese.
    Text(&'static str, &'static str),
    /// A widget whose text contains this.
    Contains(&'static str, &'static str),
    /// A widget whose text contains this icon or symbol.
    Glyph(&'static str),
    /// An area the app marked.
    Mark(&'static str, usize),
    /// The `n`th match (top to bottom, left to right).
    Nth(usize, Box<Target>),
    /// Matches inside a marked area.
    In(&'static str, usize, Box<Target>),
    /// The row (the parent `Ui`) of the match.
    Row(Box<Target>),
    /// Everything together.
    Union(Vec<Target>),
    /// The open menu or drop-down list.
    Popup,
    /// The whole window.
    Window,
    /// Grown by (left, top, right, bottom) points.
    Pad(Box<Target>, [f32; 4]),
}

use Target::{Glyph, Popup, Window};

fn text(en: &'static str, ja: &'static str) -> Target {
    Target::Text(en, ja)
}

fn contains(en: &'static str, ja: &'static str) -> Target {
    Target::Contains(en, ja)
}

fn mark_of(name: &'static str) -> Target {
    Target::Mark(name, 0)
}

fn nth(n: usize, target: Target) -> Target {
    Target::Nth(n, Box::new(target))
}

fn inside(name: &'static str, index: usize, target: Target) -> Target {
    Target::In(name, index, Box::new(target))
}

fn row(target: Target) -> Target {
    Target::Row(Box::new(target))
}

fn union(targets: impl IntoIterator<Item = Target>) -> Target {
    Target::Union(targets.into_iter().collect())
}

fn pad(target: Target, by: [f32; 4]) -> Target {
    Target::Pad(Box::new(target), by)
}

/// Where a callout's number goes.
#[derive(Clone, Copy)]
enum Side {
    Left,
    Right,
    Top,
    Bottom,
    /// Inside, at the top left corner (for large areas).
    Inside,
    /// At this point of the area (fractions of its size).
    Over(f32, f32),
}

struct Callout {
    number: u8,
    target: Target,
    side: Side,
}

fn callout(number: u8, target: Target, side: Side) -> Callout {
    Callout { number, target, side }
}

struct Shot {
    name: &'static str,
    crop: Target,
    callouts: Vec<Callout>,
}

fn shot(name: &'static str, crop: Target, callouts: Vec<Callout>) -> Shot {
    Shot { name, crop, callouts }
}

type Change = Box<dyn Fn(&mut StudioApp, &egui::Context)>;

enum Step {
    Set(Change),
    /// Waits until nothing is rendering or being made.
    Ready,
    /// Rests the pointer at a point of a target (fractions of its size).
    Hover(Target, f32, f32),
    Click(Target),
    /// Scrolls what is under the target by this many points (negative: down).
    Scroll(Target, f32),
    Leave,
    Shoot(Vec<Shot>),
}

fn set(change: impl Fn(&mut StudioApp, &egui::Context) + 'static) -> Step {
    Step::Set(Box::new(change))
}

#[derive(Clone)]
struct Widget {
    parent: Id,
    order: Order,
    rect: Rect,
    /// The text with icons left out, trimmed.
    text: String,
    raw: String,
}

#[derive(PartialEq)]
enum Phase {
    /// Widget texts are being recorded.
    Measure,
    /// The callouts are drawn and the screenshot asked for.
    Paint,
    Receive,
}

pub struct Capture {
    directory: PathBuf,
    steps: Vec<(bool, Step)>,
    at: usize,
    frames: u32,
    shot: usize,
    phase: Phase,
    widgets: Vec<Widget>,
    marks: Marks,
    window: Rect,
    pointer: Option<Pos2>,
    button: Option<bool>,
    wheel: Option<f32>,
    saved: usize,
}

impl Capture {
    /// `--capture <directory>` starts a capture.
    pub fn from_arguments() -> Option<Capture> {
        let arguments: Vec<String> = std::env::args().collect();
        let i = arguments.iter().position(|a| a == "--capture")?;
        let directory = PathBuf::from(arguments.get(i + 1).map_or("docs/images", String::as_str));
        let mut steps = Vec::new();
        for japanese in [false, true] {
            for step in scenes() {
                steps.push((japanese, step));
            }
        }
        Some(Capture {
            directory,
            steps,
            at: 0,
            frames: 0,
            shot: 0,
            phase: Phase::Measure,
            widgets: Vec::new(),
            marks: Vec::new(),
            window: Rect::NOTHING,
            pointer: None,
            button: None,
            wheel: None,
            saved: 0,
        })
    }

    /// Replaces the real pointer and keys with the scripted ones.
    pub fn input(&mut self, raw: &mut egui::RawInput) {
        raw.events.retain(|e| {
            !matches!(
                e,
                Event::PointerMoved(_)
                    | Event::MouseMoved(_)
                    | Event::PointerButton { .. }
                    | Event::PointerGone
                    | Event::MouseWheel { .. }
                    | Event::Zoom(_)
                    | Event::Key { .. }
                    | Event::Text(_)
                    | Event::Touch { .. }
            )
        });
        match self.pointer {
            Some(pos) => raw.events.push(Event::PointerMoved(pos)),
            None => raw.events.push(Event::PointerGone),
        }
        if let Some(dy) = self.wheel.take() {
            raw.events.push(Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: Vec2::new(0.0, dy),
                phase: egui::TouchPhase::Move,
                modifiers: egui::Modifiers::default(),
            });
        }
        if let (Some(pressed), Some(pos)) = (self.button.take(), self.pointer) {
            raw.events.push(Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::default(),
            });
        }
    }

    fn japanese(&self) -> bool {
        self.steps.get(self.at).is_some_and(|(japanese, _)| *japanese)
    }

    fn advance(&mut self) {
        self.at += 1;
        self.frames = 0;
        self.shot = 0;
        self.phase = Phase::Measure;
    }

    fn measuring(&self) -> bool {
        !matches!(self.steps.get(self.at), Some((_, Step::Shoot(_)))) || self.phase == Phase::Measure
    }

    // MARK: - Finding targets

    fn matches(&self, target: &Target) -> Vec<(Rect, Id)> {
        let japanese = self.japanese();
        let pick = |en: &'static str, ja: &'static str| if japanese { ja } else { en };
        let mut found: Vec<(Rect, Id)> = match target {
            Target::Text(en, ja) => {
                self.widgets.iter().filter(|w| w.text == pick(en, ja)).map(|w| (w.rect, w.parent)).collect()
            }
            Target::Contains(en, ja) => {
                self.widgets.iter().filter(|w| w.text.contains(pick(en, ja))).map(|w| (w.rect, w.parent)).collect()
            }
            Target::Glyph(glyph) => {
                self.widgets.iter().filter(|w| w.raw.contains(glyph)).map(|w| (w.rect, w.parent)).collect()
            }
            Target::Mark(name, index) => self
                .marks
                .iter()
                .filter(|(n, i, _)| n == name && i == index)
                .map(|&(_, _, rect)| (rect, Id::NULL))
                .collect(),
            Target::Nth(n, inner) => self.matches(inner).into_iter().skip(*n).take(1).collect(),
            Target::In(name, index, inner) => match self.resolve(&Target::Mark(name, *index)) {
                Some(area) => self.matches(inner).into_iter().filter(|(r, _)| area.contains(r.center())).collect(),
                None => Vec::new(),
            },
            // The widgets beside the match on its line, up to a wide gap.
            Target::Row(inner) => self
                .matches(inner)
                .into_iter()
                .take(1)
                .map(|(first, parent)| {
                    let line: Vec<Rect> = self
                        .widgets
                        .iter()
                        .map(|w| w.rect)
                        .filter(|r| {
                            r.is_positive()
                                && r.is_finite()
                                && r.height() <= first.height().max(16.0) * 2.0 + 8.0
                                && (r.center().y - first.center().y).abs() <= first.height() / 2.0 + 3.0
                        })
                        .collect();
                    let mut row = first;
                    loop {
                        let grown = line
                            .iter()
                            .filter(|r| r.left() <= row.right() + 20.0 && r.right() >= row.left() - 20.0)
                            .fold(row, |a, r| a.union(*r));
                        if grown == row {
                            break;
                        }
                        row = grown;
                    }
                    (row, parent)
                })
                .collect(),
            _ => self.resolve(target).map(|r| (r, Id::NULL)).into_iter().collect(),
        };
        found.retain(|(r, _)| r.is_positive());
        found.sort_by(|a, b| a.0.top().total_cmp(&b.0.top()).then(a.0.left().total_cmp(&b.0.left())));
        found
    }

    fn resolve(&self, target: &Target) -> Option<Rect> {
        match target {
            Target::Union(targets) => {
                let rects: Vec<Rect> = targets.iter().filter_map(|t| self.resolve(t)).collect();
                (!rects.is_empty()).then(|| rects.iter().fold(Rect::NOTHING, |a, b| a.union(*b)))
            }
            Target::Popup => {
                let rects: Vec<Rect> = self
                    .widgets
                    .iter()
                    .filter(|w| w.order == Order::Foreground && w.rect.is_positive())
                    .map(|w| w.rect)
                    .collect();
                (!rects.is_empty()).then(|| rects.iter().fold(Rect::NOTHING, |a, b| a.union(*b)))
            }
            Target::Window => Some(self.window),
            Target::Pad(inner, [left, top, right, bottom]) => self
                .resolve(inner)
                .map(|r| Rect::from_min_max(r.min - Vec2::new(*left, *top), r.max + Vec2::new(*right, *bottom))),
            _ => self.matches(target).first().map(|(r, _)| *r),
        }
    }

    fn describe(&self, target: &Target) -> String {
        match target {
            Target::Text(en, ja) | Target::Contains(en, ja) => (if self.japanese() { ja } else { en }).to_string(),
            Target::Glyph(glyph) => format!("glyph {glyph:?}"),
            Target::Mark(name, index) => format!("mark {name}#{index}"),
            Target::Nth(n, inner) => format!("#{n} of {}", self.describe(inner)),
            Target::In(name, index, inner) => format!("{} in {name}#{index}", self.describe(inner)),
            Target::Row(inner) => format!("row of {}", self.describe(inner)),
            Target::Union(targets) => targets.iter().map(|t| self.describe(t)).collect::<Vec<_>>().join(" + "),
            Target::Popup => "popup".into(),
            Target::Window => "window".into(),
            Target::Pad(inner, _) => self.describe(inner),
        }
    }

    fn missing(&self, target: &Target) {
        let (_, step) = &self.steps[self.at];
        let name = match step {
            Step::Shoot(shots) => shots.get(self.shot).map_or("", |s| s.name),
            _ => "",
        };
        eprintln!("capture: {name}: nothing matches {}", self.describe(target));
    }

    // MARK: - Drawing and saving

    fn paint(&self, ctx: &egui::Context, shot: &Shot) -> Option<Rect> {
        let Some(crop) = self.resolve(&shot.crop).map(|c| c.intersect(self.window)) else {
            self.missing(&shot.crop);
            return None;
        };
        let painter = ctx.layer_painter(LayerId::new(Order::Debug, Id::new("capture-callouts")));
        let inner = crop.shrink(2.0);
        let mut badges = Vec::new();
        for c in &shot.callouts {
            let Some(rect) = self.resolve(&c.target) else {
                self.missing(&c.target);
                continue;
            };
            let outline = rect.expand(3.0).intersect(inner);
            painter.rect_stroke(outline, CornerRadius::same(6), Stroke::new(2.5, CALLOUT), StrokeKind::Middle);
            let radius = 11.0;
            let gap = radius + 5.0;
            let centre = match c.side {
                Side::Left => Pos2::new(outline.left() - gap, outline.center().y),
                Side::Right => Pos2::new(outline.right() + gap, outline.center().y),
                Side::Top => Pos2::new(outline.left() + radius + 2.0, outline.top() - gap),
                Side::Bottom => Pos2::new(outline.left() + radius + 2.0, outline.bottom() + gap),
                Side::Inside => outline.left_top() + Vec2::splat(radius + 6.0),
                Side::Over(fx, fy) => outline.min + Vec2::new(outline.width() * fx, outline.height() * fy),
            };
            let room = crop.shrink(radius + 2.0);
            let centre =
                Pos2::new(centre.x.clamp(room.left(), room.right()), centre.y.clamp(room.top(), room.bottom()));
            badges.push((centre, c.number));
        }
        // Numbers on top of every outline.
        for (centre, number) in badges {
            painter.circle(centre, 11.0, CALLOUT, Stroke::new(2.0, Color32::WHITE));
            painter.text(
                centre + Vec2::new(0.0, 0.5),
                Align2::CENTER_CENTER,
                number.to_string(),
                FontId::new(13.0, crate::theme::bold()),
                Color32::WHITE,
            );
        }
        Some(crop)
    }

    fn save(&mut self, name: &str, image: &ColorImage, crop: Rect, pixels_per_point: f32) {
        let directory = self.directory.join(if self.japanese() { "ja" } else { "en" });
        let path = directory.join(format!("{name}.png"));
        let region = image.region(&crop, Some(pixels_per_point));
        match std::fs::create_dir_all(&directory).map_err(|e| e.to_string()).and_then(|_| write_png(&path, &region)) {
            Ok(()) => {
                self.saved += 1;
                eprintln!("capture: {}", path.display());
            }
            Err(e) => eprintln!("capture: {}: {e}", path.display()),
        }
    }
}

fn write_png(path: &Path, image: &ColorImage) -> Result<(), String> {
    let file = std::fs::File::create(path).map_err(|e| e.to_string())?;
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), image.width() as u32, image.height() as u32);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
    let rgb: Vec<u8> = image.pixels.iter().flat_map(|p| [p.r(), p.g(), p.b()]).collect();
    writer.write_image_data(&rgb).map_err(|e| e.to_string())
}

fn widgets(ctx: &egui::Context) -> Vec<Widget> {
    ctx.viewport(|viewport| {
        let all = &viewport.this_pass.widgets;
        let mut list = Vec::new();
        for (layer, rects) in all.layers() {
            for r in rects {
                let raw = all
                    .info(r.id)
                    .and_then(|i| i.label.clone().or_else(|| i.current_text_value.clone()))
                    .unwrap_or_default();
                // Phosphor icons live in the private use area.
                let text: String = raw.chars().filter(|c| !('\u{e000}'..='\u{f8ff}').contains(c)).collect();
                list.push(Widget {
                    parent: r.parent_id,
                    order: layer.order,
                    rect: r.rect,
                    text: text.split_whitespace().collect::<Vec<_>>().join(" "),
                    raw,
                });
            }
        }
        list
    })
}

impl StudioApp {
    /// Runs the capture's step for this frame (before the panels are drawn).
    pub(super) fn capture_before(&mut self, ctx: &egui::Context) {
        let Some(mut capture) = self.capture.take() else { return };
        ctx.request_repaint();
        if capture.at >= capture.steps.len() {
            self.capture = Some(capture);
            return;
        }
        let screenshot = ctx.input(|i| {
            i.events.iter().find_map(|e| match e {
                Event::Screenshot { image, .. } => Some(Arc::clone(image)),
                _ => None,
            })
        });
        let japanese = capture.japanese();
        let step = std::mem::replace(&mut capture.steps[capture.at].1, Step::Ready);
        let mut done = false;
        match &step {
            Step::Set(change) => {
                self.japanese = japanese;
                change(self, ctx);
                done = true;
            }
            Step::Ready => done = self.settled() && capture.frames >= 20,
            Step::Hover(target, fx, fy) => {
                if capture.frames == 1 {
                    match capture.resolve(target) {
                        Some(r) => capture.pointer = Some(r.min + Vec2::new(r.width() * fx, r.height() * fy)),
                        None => capture.missing(target),
                    }
                }
                done = capture.frames >= 8;
            }
            Step::Click(target) => {
                match capture.frames {
                    1 => match capture.resolve(target) {
                        Some(r) => capture.pointer = Some(r.center()),
                        None => capture.missing(target),
                    },
                    3 => capture.button = Some(true),
                    5 => capture.button = Some(false),
                    _ => {}
                }
                // Menus fade in.
                done = capture.frames >= 45;
            }
            Step::Scroll(target, dy) => {
                match capture.frames {
                    1 => match capture.resolve(target) {
                        Some(r) => capture.pointer = Some(r.center()),
                        None => capture.missing(target),
                    },
                    3 => capture.wheel = Some(*dy),
                    _ => {}
                }
                done = capture.frames >= 30;
            }
            Step::Leave => {
                capture.pointer = None;
                done = true;
            }
            Step::Shoot(shots) => {
                if capture.phase == Phase::Receive
                    && let Some(image) = screenshot
                {
                    let shot = &shots[capture.shot];
                    if let Some(crop) = capture.resolve(&shot.crop).map(|c| c.intersect(capture.window)) {
                        capture.save(shot.name, &image, crop, ctx.pixels_per_point());
                    }
                    capture.shot += 1;
                    capture.phase = Phase::Paint;
                    done = capture.shot >= shots.len();
                }
            }
        }
        capture.steps[capture.at].1 = step;
        if done {
            capture.advance();
        } else {
            capture.frames += 1;
        }
        let measuring = capture.measuring();
        ctx.all_styles_mut(|s| s.debug.show_interactive_widgets = measuring);
        if capture.at >= capture.steps.len() {
            eprintln!("capture: {} screenshots in {}", capture.saved, capture.directory.display());
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        self.capture = Some(capture);
    }

    /// Records what was drawn, and draws the callouts when a shot is due.
    pub(super) fn capture_after(&mut self, ctx: &egui::Context) {
        let Some(capture) = self.capture.as_mut() else { return };
        capture.marks = ctx.data_mut(|d| std::mem::take(d.get_temp_mut_or_default::<Marks>(Id::new("capture-marks"))));
        capture.window = ctx.content_rect();
        if capture.measuring() {
            capture.widgets = widgets(ctx);
        }
        let Some((_, Step::Shoot(shots))) = capture.steps.get(capture.at) else { return };
        match capture.phase {
            Phase::Measure => capture.phase = Phase::Paint,
            // The screenshot may be of a later frame: the callouts stay until it comes.
            Phase::Paint | Phase::Receive => {
                if let Some(shot) = shots.get(capture.shot) {
                    capture.paint(ctx, shot);
                    if capture.phase == Phase::Paint {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
                        capture.phase = Phase::Receive;
                    }
                }
            }
        }
    }

    /// Nothing is rendering or being made.
    fn settled(&self) -> bool {
        !self.lanes.iter().any(|l| l.rendering)
            && self.pending_render.is_none()
            && !self.create.busy()
            && !self.fit_view
    }
}

// MARK: - Scenes

fn lane_a() -> Source {
    Source::sf2synth(Sound { file: SoundFile::MacBuiltIn, bank: 0, program: 0 })
}

fn lane_b() -> Source {
    Source::mac_sampler(Sound::same_as_a())
}

fn set_lanes(app: &mut StudioApp, sources: Vec<Source>) {
    let same = app.lanes.len() == sources.len() && app.lanes.iter().zip(&sources).all(|(l, s)| l.source == *s);
    if !same {
        app.lanes.clear();
        app.outputs.clear();
        for source in sources {
            app.add_lane(source);
        }
    }
}

fn set_program(app: &mut StudioApp, program: Program) {
    if app.program != program {
        app.program = program;
        app.render_all();
    }
}

/// A scene's starting point: Compare, lanes A (sf2synth) and B (the macOS
/// sampler), the velocity sweep, every view on.
fn reset(app: &mut StudioApp, ctx: &egui::Context) {
    app.theme = ThemeChoice::Light;
    app.theme.apply(ctx);
    app.set_mode(Mode::Compare);
    app.layers = Layers::default();
    app.level_match = true;
    app.keyboard_settings = KeyboardSettings::default();
    app.abx.open = false;
    app.chrome.help = false;
    app.chrome.about = false;
    app.chrome.updates = false;
    app.message = None;
    app.last_note = None;
    app.null = NullState::default();
    set_lanes(app, vec![lane_a(), lane_b()]);
    set_program(app, Program::Pattern(TestPattern::VelocitySweep));
    app.focus = 0;
    app.outputs = vec![true; app.lanes.len()];
    app.push_outputs();
    app.with_transport(|t| {
        t.playing = false;
        t.loop_range = None;
        t.position = 0;
    });
    app.view = View { start: 0.0, span: 30.0, follow: true, zoom_ms: 20.0 };
}

/// The whole program in view, the playhead at `seconds`.
fn fit(app: &mut StudioApp, seconds: f64) {
    app.view.start = 0.0;
    app.view.span = app.duration().max(0.05);
    app.view.follow = false;
    app.seek(seconds);
}

fn create_from(app: &mut StudioApp, origin: Origin) {
    app.set_mode(Mode::Create);
    app.create.origin = origin;
    app.create.voicing = Voicing::default();
    app.create.changed();
}

/// A few piano-like notes as WAV files named by their keys.
fn recordings() -> Vec<PathBuf> {
    let directory = std::env::temp_dir().join("sf2studio-capture");
    let _ = std::fs::create_dir_all(&directory);
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 44100,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    [(36, "C2"), (48, "C3"), (60, "C4"), (72, "C5"), (84, "C6")]
        .into_iter()
        .map(|(key, name)| {
            let path = directory.join(format!("piano-{name}.wav"));
            if !path.exists()
                && let Ok(mut writer) = hound::WavWriter::create(&path, spec)
            {
                for x in crate::create::dsp::synthesize(key, &SynthesisParams::default(), 44100, 2.5) {
                    let _ = writer.write_sample((x.clamp(-1.0, 1.0) * 32767.0) as i16);
                }
                let _ = writer.finalize();
            }
            path
        })
        .collect()
}

/// Lane B's rendering saved as a WAV, standing in for a recording.
fn recording_of_lane_b(app: &StudioApp) -> Option<PathBuf> {
    let audio = app.lanes.get(1)?.audio.as_ref()?;
    let path = std::env::temp_dir().join("sf2studio-capture").join("piano-recording.wav");
    std::fs::create_dir_all(path.parent()?).ok()?;
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: audio.sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(&path, spec).ok()?;
    for (l, r) in audio.left.iter().zip(&audio.right) {
        writer.write_sample((l.clamp(-1.0, 1.0) * 32767.0) as i16).ok()?;
        writer.write_sample((r.clamp(-1.0, 1.0) * 32767.0) as i16).ok()?;
    }
    writer.finalize().ok()?;
    Some(path)
}

fn scenes() -> Vec<Step> {
    use Side::{Bottom, Inside, Left, Over, Right, Top};
    let mode_switch = || union([text("Create", "作成"), text("Compare", "比較")]);
    let lane = |i: usize, target: Target| inside("lane", i, target);
    let left = |target: Target| inside("left", 0, target);
    let transport = |target: Target| inside("transport", 0, target);
    let central = |target: Target| inside("central", 0, target);
    let tuning = |target: Target| inside("right", 0, target);
    let creating = |target: Target| inside("left", 0, target);
    let bar = |target: Target| pad(target, [6.0, 30.0, 6.0, 6.0]);

    vec![
        // Compare: the main view.
        set(reset),
        Step::Ready,
        set(|app, _| fit(app, 8.6)),
        Step::Ready,
        Step::Shoot(vec![
            shot(
                "overview-compare",
                Window,
                vec![
                    callout(1, mode_switch(), Bottom),
                    callout(2, union([text("English", "日本語"), text("Help", "ヘルプ")]), Bottom),
                    callout(3, mark_of("left"), Over(0.85, 0.05)),
                    callout(4, mark_of("layers"), Right),
                    callout(5, mark_of("measure"), Over(0.75, 0.05)),
                    callout(6, mark_of("timeline"), Over(0.6, 0.88)),
                    callout(7, mark_of("transport"), Over(0.75, 0.5)),
                ],
            ),
            shot(
                "topbar",
                mark_of("top"),
                vec![
                    callout(1, mode_switch(), Left),
                    callout(2, text("English", "日本語"), Left),
                    callout(3, mark_of("theme"), Left),
                    callout(4, text("Help", "ヘルプ"), Left),
                ],
            ),
            shot(
                "compare-lanes",
                pad(mark_of("left"), [0.0, 0.0, 0.0, -170.0]),
                vec![
                    callout(1, left(text("Velocity sweep", "ベロシティの段階")), Right),
                    callout(2, left(text("Open MIDI file…", "MIDI ファイルを開く…")), Right),
                    callout(3, union([lane(0, text("Synth", "シンセ")), lane(0, text("sf2synth", "sf2synth"))]), Top),
                    callout(4, row(lane(0, text("Sound", "音源"))), Right),
                    callout(5, row(lane(0, text("Preset", "音色"))), Right),
                    callout(6, lane(0, contains("peak", "ピーク")), Right),
                    callout(7, lane(0, Glyph(icon::X)), Top),
                    callout(8, row(lane(1, text("Sound", "音源"))), Right),
                    callout(9, left(text("Add lane", "レーンを追加")), Right),
                ],
            ),
            shot(
                "transport-compare",
                bar(mark_of("transport")),
                vec![
                    callout(1, transport(text("Play", "再生")), Top),
                    callout(2, transport(Glyph(icon::SKIP_BACK)), Top),
                    callout(3, mark_of("clock"), Top),
                    callout(4, text("Blind test", "ブラインドテスト"), Top),
                    callout(5, union([text("Output:", "出力:"), text("All", "全部")]), Top),
                    callout(6, text("Match levels", "音量を揃える"), Top),
                    callout(7, mark_of("meter"), Top),
                ],
            ),
            shot(
                "layers-bar",
                pad(mark_of("layers"), [4.0, 4.0, 4.0, 30.0]),
                vec![
                    callout(1, text("Spectrogram", "スペクトログラム"), Bottom),
                    callout(2, text("Level (dB)", "音量推移 (dB)"), Bottom),
                    callout(3, text("Waveform close-up", "拡大波形"), Bottom),
                    callout(4, text("Piano roll", "ピアノロール"), Bottom),
                    callout(5, text("Measurements", "計測グラフ"), Bottom),
                    callout(6, text("Null test", "ヌルテスト（差分）"), Bottom),
                ],
            ),
            shot(
                "measurements",
                pad(mark_of("measure"), [4.0, 4.0, 4.0, 4.0]),
                vec![
                    callout(1, text("Relative to each key's maximum", "鍵ごとの最大を基準にする"), Right),
                    callout(2, Target::Mark("chart", 0), Inside),
                    callout(3, Target::Mark("chart", 1), Inside),
                    callout(4, Target::Mark("chart", 2), Inside),
                ],
            ),
            shot(
                "timeline-compare",
                mark_of("timeline"),
                vec![
                    callout(1, mark_of("ruler"), Over(0.56, 0.5)),
                    callout(2, mark_of("roll"), Over(0.6, 0.5)),
                    callout(3, Target::Mark("lane-header", 0), Over(0.5, 0.5)),
                    callout(4, Target::Mark("lane-body", 0), Over(0.75, 0.25)),
                    callout(5, Target::Mark("level-scale", 0), Left),
                    callout(6, mark_of("playhead"), Right),
                    callout(7, mark_of("closeup"), Over(0.35, 0.3)),
                ],
            ),
        ]),
        // The null test: lane A (sf2synth) minus lane B (the macOS sampler).
        set(|app, _| {
            app.layers.measure = false;
            app.layers.null_test = true;
            app.sync_player();
            fit(app, 8.6);
        }),
        Step::Ready,
        Step::Shoot(vec![shot(
            "null-test",
            pad(union([mark_of("null"), mark_of("null-band")]), [4.0, 34.0, 4.0, 4.0]),
            vec![
                callout(1, inside("null", 0, text("−", "−")), Top),
                callout(2, text("Match levels first", "先に音量を揃える"), Top),
                callout(3, contains("Hear the difference", "差分を聞く"), Top),
                callout(4, inside("null", 0, contains("difference peaks", "差のピーク")), Bottom),
                callout(5, mark_of("null-band"), Over(0.4, 0.35)),
            ],
        )]),
        set(|app, _| {
            app.layers.null_test = false;
            app.sync_player();
        }),
        // A loop, zoomed in.
        set(|app, _| {
            app.layers.measure = false;
            let rate = app.sample_rate as f64;
            app.with_transport(|t| t.loop_range = Some(((8.0 * rate) as usize, (9.8 * rate) as usize)));
            app.view = View { start: 6.0, span: 6.0, follow: false, zoom_ms: 12.0 };
            app.seek(8.62);
        }),
        Step::Ready,
        Step::Shoot(vec![shot(
            "timeline-loop",
            union([mark_of("timeline"), mark_of("transport")]),
            vec![
                callout(1, mark_of("loop"), Over(0.85, 0.12)),
                callout(2, mark_of("loop-label"), Top),
                callout(3, mark_of("playhead"), Right),
                callout(4, mark_of("closeup"), Over(0.35, 0.3)),
            ],
        )]),
        // Three lanes, one heard.
        set(|app, _| {
            app.with_transport(|t| t.loop_range = None);
            if let Some(path) = recording_of_lane_b(app) {
                app.add_lane(Source::Wav { path: Some(path) });
            }
        }),
        Step::Ready,
        set(|app, _| {
            app.solo(1);
            fit(app, 8.6);
        }),
        Step::Ready,
        Step::Shoot(vec![shot(
            "compare-three-lanes",
            Window,
            vec![
                callout(1, Target::Mark("lane", 2), Right),
                callout(2, union([text("Output:", "出力:"), text("All", "全部")]), Top),
                callout(3, Target::Mark("lane-header", 0), Over(0.6, 0.5)),
                callout(4, Target::Mark("lane-header", 1), Over(0.6, 0.5)),
                callout(5, mark_of("closeup"), Over(0.35, 0.3)),
            ],
        )]),
        // The blind test.
        set(|app, _| {
            app.abx.open = true;
            app.abx.a = 0;
            app.abx.b = 1;
            app.abx.demo();
        }),
        Step::Ready,
        Step::Shoot(vec![
            shot(
                "abx-window",
                pad(mark_of("abx-window"), [30.0, 8.0, 30.0, 8.0]),
                vec![
                    callout(1, row(text("Compare lanes", "比べるレーン")), Left),
                    callout(2, union([nth(0, Glyph("🔊")), nth(2, Glyph("🔊"))]), Left),
                    callout(3, row(text("X is", "X は")), Left),
                    callout(4, contains("next X is ready", "次の X を用意しました"), Left),
                    callout(5, contains("chance of guessing", "当てずっぽう"), Left),
                    callout(6, text("You can tell them apart.", "聞き分けられています。"), Left),
                    callout(7, text("Start over", "最初から"), Left),
                ],
            ),
            shot(
                "abx-full",
                Window,
                vec![
                    callout(1, text("Blind test", "ブラインドテスト"), Top),
                    callout(2, contains("Which lane plays is hidden", "どのレーンが鳴っているかを隠して"), Top),
                    callout(3, mark_of("abx-window"), Right),
                    callout(4, Target::Mark("lane-header", 0), Over(0.75, 0.5)),
                ],
            ),
        ]),
        set(|app, _| app.abx.open = false),
        // Drop-down lists and menus.
        Step::Ready,
        Step::Click(lane(0, text("sf2synth", "sf2synth"))),
        Step::Shoot(vec![shot(
            "combo-synth",
            pad(union([Target::Mark("lane", 0), Popup]), [4.0, 4.0, 12.0, 8.0]),
            vec![callout(1, Popup, Right)],
        )]),
        Step::Click(lane(0, text("sf2synth", "sf2synth"))),
        Step::Click(lane(1, contains("Same as A", "A と同じ"))),
        Step::Shoot(vec![shot(
            "combo-sound",
            pad(union([Target::Mark("lane", 1), Popup]), [4.0, 4.0, 12.0, 8.0]),
            vec![callout(1, Popup, Right)],
        )]),
        Step::Click(lane(1, contains("Same as A", "A と同じ"))),
        Step::Click(left(text("Velocity sweep", "ベロシティの段階"))),
        Step::Shoot(vec![shot(
            "combo-pattern",
            pad(union([left(text("Velocity sweep", "ベロシティの段階")), Popup]), [12.0, 34.0, 12.0, 8.0]),
            vec![callout(1, Popup, Right)],
        )]),
        Step::Click(left(text("Velocity sweep", "ベロシティの段階"))),
        Step::Click(text("Help", "ヘルプ")),
        Step::Shoot(vec![shot(
            "menu-help",
            pad(union([text("Help", "ヘルプ"), Popup]), [12.0, 8.0, 12.0, 12.0]),
            vec![],
        )]),
        Step::Click(text("Help", "ヘルプ")),
        Step::Click(mark_of("theme")),
        Step::Shoot(vec![shot("menu-theme", pad(union([mark_of("theme"), Popup]), [12.0, 8.0, 12.0, 12.0]), vec![])]),
        Step::Click(mark_of("theme")),
        Step::Click(text("English", "日本語")),
        Step::Shoot(vec![shot(
            "menu-language",
            pad(union([text("English", "日本語"), Popup]), [12.0, 8.0, 12.0, 12.0]),
            vec![],
        )]),
        Step::Click(text("English", "日本語")),
        Step::Leave,
        // Tune: a note from the keyboard.
        set(|app, ctx| {
            reset(app, ctx);
            app.set_mode(Mode::Tune);
            set_program(app, Program::Note { key: 60, velocity: 96, length: 1.5 });
            app.last_note = Some((60, 96));
        }),
        Step::Ready,
        set(|app, _| fit(app, 0.42)),
        Step::Ready,
        Step::Hover(mark_of("keyboard"), 25.5 / 52.0, 0.8),
        Step::Shoot(vec![
            shot(
                "overview-tune",
                Window,
                vec![
                    callout(1, mark_of("left"), Over(0.85, 0.05)),
                    callout(2, mark_of("timeline"), Over(0.6, 0.85)),
                    callout(3, mark_of("right"), Over(0.85, 0.05)),
                    callout(4, mark_of("keyboard-panel"), Over(0.97, 0.12)),
                    callout(5, mark_of("transport"), Over(0.5, 0.5)),
                ],
            ),
            shot(
                "tune-panel",
                mark_of("right"),
                vec![
                    callout(1, row(text("Master gain", "全体の音量")), Right),
                    callout(
                        2,
                        union([text("Velocity response", "ベロシティの反応"), row(text("Range (cB)", "幅 (cB)"))]),
                        Right,
                    ),
                    callout(3, contains("Concave", "凹型"), Right),
                    callout(4, row(tuning(text("Soft notes darker", "弱音をこもらせる"))), Right),
                    callout(5, row(text("Attenuation scale", "減衰の係数")), Right),
                    callout(6, text("Cubic interpolation", "キュービック補間"), Right),
                    callout(7, text("Re-strike releases the key", "同じ鍵盤の再打鍵で前の音を離す"), Right),
                ],
            ),
            shot(
                "keyboard",
                pad(mark_of("keyboard-panel"), [0.0, 26.0, 0.0, 0.0]),
                vec![
                    callout(1, text("Fixed velocity", "強さを固定"), Top),
                    callout(2, text("Fixed length", "長さを固定"), Top),
                    callout(3, contains("No MIDI keyboard", "MIDI キーボードなし"), Top),
                    callout(4, contains("Press higher on a key", "鍵盤の上の方ほど"), Top),
                    callout(5, mark_of("hover-tip"), Right),
                    callout(6, mark_of("last-note"), Left),
                ],
            ),
            shot(
                "transport-tune",
                bar(mark_of("transport")),
                vec![
                    callout(1, union([text("Hear:", "再生中:"), transport(text("B", "B"))]), Top),
                    callout(2, text("Match levels", "音量を揃える"), Top),
                ],
            ),
        ]),
        // The rest of the settings, scrolled into view.
        Step::Scroll(mark_of("right"), -800.0),
        Step::Shoot(vec![shot(
            "tune-panel-effects",
            mark_of("right"),
            vec![
                callout(8, union([tuning(text("Reverb", "リバーブ")), row(nth(0, text("Level", "量")))]), Right),
                // Each slider's track and label both carry its text.
                callout(9, union([text("Chorus", "コーラス"), row(nth(2, text("Level", "量")))]), Right),
                callout(10, union([text("Reset", "初期値に戻す"), text("Load…", "読み込む…")]), Right),
            ],
        )]),
        Step::Scroll(mark_of("right"), 800.0),
        Step::Leave,
        // The note up close: only the lanes shown.
        set(|app, _| {
            app.layers.closeup = false;
            app.layers.piano_roll = false;
            app.view = View { start: 0.0, span: 2.4, follow: false, zoom_ms: 20.0 };
        }),
        Step::Ready,
        Step::Shoot(vec![shot(
            "timeline-note",
            mark_of("timeline"),
            vec![
                callout(1, mark_of("note-band"), Right),
                callout(2, Target::Mark("harmonics", 0), Right),
                callout(3, Target::Mark("lane-header", 0), Over(0.75, 0.5)),
                callout(4, Target::Mark("lane-header", 1), Over(0.75, 0.5)),
                callout(5, Target::Mark("level-scale", 0), Left),
            ],
        )]),
        set(|app, _| app.layers = Layers::default()),
        set(|app, _| app.focus_lane(1)),
        Step::Ready,
        Step::Shoot(vec![shot(
            "tune-not-sf2synth",
            Window,
            vec![
                callout(1, transport(text("B", "B")), Top),
                callout(2, contains("isn't sf2synth", "sf2synth ではありません"), Left),
                callout(3, contains("Focus an sf2synth lane", "SF2 を選んだ sf2synth のレーン"), Top),
            ],
        )]),
        // Create from the synthesis.
        set(|app, ctx| {
            reset(app, ctx);
            create_from(app, Origin::Synthesis(SynthesisParams::default()));
        }),
        Step::Ready,
        Step::Shoot(vec![
            shot(
                "overview-create",
                Window,
                vec![
                    callout(1, mark_of("left"), Over(0.9, 0.04)),
                    callout(2, mark_of("central"), Over(0.95, 0.04)),
                    callout(3, union([contains("Ready", "完成"), text("Reset settings", "設定を初期値に戻す")]), Right),
                    callout(4, mark_of("keyboard-panel"), Over(0.97, 0.12)),
                ],
            ),
            shot(
                "create-origin-synthesis",
                pad(mark_of("left"), [0.0, 0.0, 0.0, -150.0]),
                vec![
                    callout(1, union([text("An SF2 / SFZ", "既存の SF2 / SFZ"), text("Synthesis", "合成")]), Bottom),
                    callout(2, row(creating(text("Brightness", "明るさ"))), Right),
                    callout(3, row(text("String stiffness", "弦の硬さ（倍音のずれ）")), Right),
                    callout(4, row(text("Decay (middle C)", "減衰（中央の C）")), Right),
                    callout(5, row(text("String detune", "弦のずれ（うなり）")), Right),
                    callout(6, row(text("Hammer noise", "ハンマーの打音")), Right),
                ],
            ),
            shot(
                "create-voicing",
                mark_of("central"),
                vec![
                    callout(1, row(text("Name", "名前")), Right),
                    callout(
                        2,
                        union([text("Level and tone", "音量と音色"), row(text("Warmth (lows)", "温かみ（低域）"))]),
                        Over(0.94, 0.12),
                    ),
                    callout(
                        3,
                        union([
                            text("Touch", "タッチ（強さ）"),
                            row(central(text("Soft notes darker", "弱音をこもらせる"))),
                        ]),
                        Over(0.94, 0.12),
                    ),
                    callout(
                        4,
                        union([
                            row(text("Velocity layers", "強さのレイヤー数")),
                            row(text("Softest layer cutoff", "一番弱いレイヤーの高域")),
                        ]),
                        Right,
                    ),
                    callout(
                        5,
                        union([
                            text("Sustain", "余韻"),
                            row(text("Release (0 = as is)", "離したあとの余韻（0 = 元のまま）")),
                        ]),
                        Over(0.94, 0.12),
                    ),
                    callout(
                        6,
                        union([text("Tuning and space", "調律と広がり"), row(central(text("Reverb", "リバーブ")))]),
                        Over(0.94, 0.12),
                    ),
                    callout(7, contains("Ready", "完成"), Right),
                    callout(8, union([text("Save SF2…", "SF2 を保存…"), text("Save SFZ…", "SFZ を保存…")]), Bottom),
                    callout(9, text("Compare with others", "比較に追加"), Bottom),
                    callout(10, text("Reset settings", "設定を初期値に戻す"), Bottom),
                ],
            ),
        ]),
        // …sent to Compare.
        set(|app, _| {
            app.send_to_compare();
            if let Some(message) = &mut app.create.message {
                message.push_str(if app.japanese {
                    " 比較モードで見られます。"
                } else {
                    " See it in Compare."
                });
            }
        }),
        Step::Ready,
        // The message is below the fold.
        Step::Scroll(mark_of("central"), -600.0),
        Step::Shoot(vec![shot(
            "create-sent",
            pad(union([contains("Ready", "完成"), contains("In lane", "に入れました")]), [12.0, 10.0, 160.0, 10.0]),
            vec![
                callout(1, text("Compare with others", "比較に追加"), Right),
                callout(2, contains("In lane", "に入れました"), Right),
            ],
        )]),
        set(|app, _| {
            app.set_mode(Mode::Compare);
            app.layers.measure = false;
        }),
        Step::Ready,
        set(|app, _| fit(app, 8.6)),
        Step::Ready,
        Step::Shoot(vec![shot(
            "compare-created",
            Window,
            vec![
                callout(1, Target::Mark("lane", 2), Right),
                callout(2, Target::Mark("lane-header", 2), Over(0.6, 0.5)),
            ],
        )]),
        // Create from an SF2 (here macOS's own DLS) and from recordings.
        set(|app, ctx| {
            reset(app, ctx);
            create_from(app, Origin::Sf2 { path: Some(PathBuf::from(MAC_BUILT_IN_DLS)), bank: 0, program: 0 });
        }),
        Step::Ready,
        Step::Shoot(vec![shot(
            "create-origin-sf2",
            pad(mark_of("left"), [0.0, 0.0, 0.0, -300.0]),
            vec![
                callout(1, union([text("An SF2 / SFZ", "既存の SF2 / SFZ"), text("Synthesis", "合成")]), Right),
                callout(2, text("gs_instruments.dls", "gs_instruments.dls"), Right),
                callout(3, contains("0:0", "0:0"), Right),
            ],
        )]),
        set(|app, _| create_from(app, Origin::Recordings { files: recordings() })),
        Step::Ready,
        Step::Shoot(vec![shot(
            "create-origin-recordings",
            pad(mark_of("left"), [0.0, 0.0, 0.0, -120.0]),
            vec![
                callout(1, text("Add WAV files…", "WAV を追加…"), Right),
                callout(
                    2,
                    union([text("piano-C2.wav", "piano-C2.wav"), text("piano-C6.wav", "piano-C6.wav")]),
                    Over(0.55, 0.5),
                ),
                callout(3, union([text("C2", "C2"), text("C6", "C6")]), Right),
                callout(4, creating(nth(0, Glyph(icon::X))), Left),
                callout(5, text("Remove all", "すべて外す"), Right),
            ],
        )]),
        // Help.
        set(|app, ctx| {
            reset(app, ctx);
            app.chrome.help = true;
        }),
        Step::Ready,
        Step::Shoot(vec![shot(
            "help-window",
            pad(mark_of("help-window"), [8.0, 8.0, 8.0, 8.0]),
            vec![
                callout(1, contains("Open the illustrated guide", "図解ガイドを開く"), Right),
                callout(2, contains("Recipes", "やりたいこと別の手順"), Right),
            ],
        )]),
        set(|app, _| app.chrome.help = false),
    ]
}
