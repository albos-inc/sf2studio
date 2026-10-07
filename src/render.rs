//! Rendering lanes in the background.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use sf2synth::{MidiFile, Sequencer, SoundFont, Synthesizer};

use crate::analysis::{Analysis, Rendered};
use crate::patterns::TestPattern;
use crate::tuning::SynthTuning;

/// Where a lane's sound comes from.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Source {
    /// sf2synth playing an SF2 file.
    Sf2 { path: Option<PathBuf>, tuning: SynthTuning },
    /// A recording, played as it is.
    Wav { path: Option<PathBuf> },
    /// macOS's own sampler (AVAudioUnitSampler) playing an SF2 file.
    MacSampler { path: Option<PathBuf> },
}

impl Source {
    pub fn path(&self) -> Option<&Path> {
        match self {
            Source::Sf2 { path, .. } | Source::Wav { path } | Source::MacSampler { path } => path.as_deref(),
        }
    }
}

/// What the lanes play.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Program {
    Pattern(TestPattern),
    Midi(PathBuf),
    /// One note, played from the keyboard.
    Note {
        key: u8,
        velocity: u8,
        length: f32,
    },
}

impl Program {
    fn midi_bytes(&self) -> Result<Vec<u8>, String> {
        match self {
            Program::Pattern(pattern) => Ok(pattern.midi()),
            Program::Midi(path) => std::fs::read(path).map_err(|e| format!("{}: {e}", path.display())),
            Program::Note { key, velocity, length } => Ok(crate::patterns::single_note(*key, *velocity, *length)),
        }
    }
}

pub struct Job {
    pub lane: u64,
    pub generation: u64,
    pub source: Source,
    pub program: Program,
    pub sample_rate: u32,
}

pub struct Done {
    pub lane: u64,
    pub generation: u64,
    pub result: Result<(Arc<Rendered>, Arc<Analysis>), String>,
}

/// Runs jobs on background threads and hands back their results.
pub struct Renderer {
    sender: Sender<Done>,
    receiver: Receiver<Done>,
    fonts: Arc<Mutex<HashMap<PathBuf, Arc<SoundFont>>>>,
    repaint: Arc<dyn Fn() + Send + Sync>,
}

impl Renderer {
    pub fn new(repaint: impl Fn() + Send + Sync + 'static) -> Renderer {
        let (sender, receiver) = channel();
        Renderer { sender, receiver, fonts: Arc::default(), repaint: Arc::new(repaint) }
    }

    pub fn submit(&self, job: Job) {
        let sender = self.sender.clone();
        let fonts = Arc::clone(&self.fonts);
        let repaint = Arc::clone(&self.repaint);
        std::thread::spawn(move || {
            let result = run(&job, &fonts);
            let _ = sender.send(Done { lane: job.lane, generation: job.generation, result });
            repaint();
        });
    }

    pub fn poll(&self) -> Vec<Done> {
        self.receiver.try_iter().collect()
    }

    /// The SF2 at `path`, from the cache or read now.
    pub fn font(&self, path: &Path) -> Result<Arc<SoundFont>, String> {
        load_font(path, &self.fonts)
    }

    /// Forgets a cached SF2 so it is read again (it changed on disk).
    pub fn forget_font(&self, path: &Path) {
        if let Ok(mut fonts) = self.fonts.lock() {
            fonts.remove(path);
        }
    }
}

fn run(job: &Job, fonts: &Mutex<HashMap<PathBuf, Arc<SoundFont>>>) -> Result<(Arc<Rendered>, Arc<Analysis>), String> {
    let rendered = match &job.source {
        Source::Sf2 { path, tuning } => {
            let path = path.as_ref().ok_or("no SF2 file")?;
            let font = load_font(path, fonts)?;
            render_sf2(font, tuning, &job.program, job.sample_rate)?
        }
        Source::Wav { path } => load_wav(path.as_ref().ok_or("no WAV file")?, job.sample_rate)?,
        Source::MacSampler { path } => {
            let path = path.as_ref().ok_or("no SF2 file")?;
            crate::mac_sampler::render(path, &job.program.midi_bytes()?, job.sample_rate)?
        }
    };
    let analysis = Analysis::new(&rendered);
    Ok((Arc::new(rendered), Arc::new(analysis)))
}

fn load_font(path: &Path, fonts: &Mutex<HashMap<PathBuf, Arc<SoundFont>>>) -> Result<Arc<SoundFont>, String> {
    if let Some(font) = fonts.lock().ok().and_then(|f| f.get(path).cloned()) {
        return Ok(font);
    }
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let font = Arc::new(SoundFont::from_bytes(bytes).map_err(|e| e.to_string())?);
    if let Ok(mut cache) = fonts.lock() {
        cache.insert(path.to_path_buf(), Arc::clone(&font));
    }
    Ok(font)
}

/// Seconds rendered after the program's end, for releases and reverb.
const TAIL_SECONDS: f64 = 3.0;

pub fn render_sf2(
    font: Arc<SoundFont>,
    tuning: &SynthTuning,
    program: &Program,
    sample_rate: u32,
) -> Result<Rendered, String> {
    let midi = Arc::new(MidiFile::from_bytes(&program.midi_bytes()?).map_err(|e| e.to_string())?);
    let mut synth = Synthesizer::new(font, &tuning.settings(sample_rate)).map_err(|e| e.to_string())?;
    for channel in 0..16 {
        synth.control_change(channel, 91, tuning.reverb_send);
        synth.control_change(channel, 93, tuning.chorus_send);
    }
    let mut sequencer = Sequencer::new(sample_rate);
    sequencer.load(Arc::clone(&midi));
    sequencer.play();
    let total = ((midi.duration() + TAIL_SECONDS) * sample_rate as f64) as usize;
    let mut left = vec![0.0f32; total];
    let mut right = vec![0.0f32; total];
    let block = 512;
    let mut position = 0;
    while position < total {
        let end = (position + block).min(total);
        sequencer.render(&mut synth, &mut left[position..end], &mut right[position..end]);
        position = end;
    }
    Ok(Rendered { sample_rate, left, right })
}

/// A WAV file as stereo at `sample_rate` (cubic resampling).
pub fn load_wav(path: &Path, sample_rate: u32) -> Result<Rendered, String> {
    let mut reader = hound::WavReader::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let spec = reader.spec();
    let channels = spec.channels.max(1) as usize;
    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader.samples::<f32>().map(|s| s.unwrap_or(0.0)).collect(),
        hound::SampleFormat::Int => {
            let scale = 1.0 / (1u64 << (spec.bits_per_sample.max(1) - 1)) as f32;
            reader.samples::<i32>().map(|s| s.unwrap_or(0) as f32 * scale).collect()
        }
    };
    let frames = samples.len() / channels;
    let left: Vec<f32> = (0..frames).map(|i| samples[i * channels]).collect();
    let right: Vec<f32> = (0..frames).map(|i| samples[i * channels + if channels > 1 { 1 } else { 0 }]).collect();
    if spec.sample_rate == sample_rate {
        return Ok(Rendered { sample_rate, left, right });
    }
    Ok(Rendered {
        sample_rate,
        left: resample(&left, spec.sample_rate, sample_rate),
        right: resample(&right, spec.sample_rate, sample_rate),
    })
}

fn resample(input: &[f32], from: u32, to: u32) -> Vec<f32> {
    let ratio = from as f64 / to as f64;
    let length = (input.len() as f64 / ratio) as usize;
    let at = |i: isize| -> f32 { input.get(i.max(0) as usize).copied().unwrap_or(0.0) };
    (0..length)
        .map(|n| {
            let position = n as f64 * ratio;
            let i = position.floor() as isize;
            let t = (position - i as f64) as f32;
            let (xm1, x0, x1, x2) = (at(i - 1), at(i), at(i + 1), at(i + 2));
            let c1 = 0.5 * (x1 - xm1);
            let c2 = xm1 - 2.5 * x0 + 2.0 * x1 - 0.5 * x2;
            let c3 = 0.5 * (x2 - xm1) + 1.5 * (x0 - x1);
            ((c3 * t + c2) * t + c1) * t + x0
        })
        .collect()
}
