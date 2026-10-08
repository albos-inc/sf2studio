//! Making an SF2: material from an SF2 preset, recordings or synthesis,
//! shaped by a [`Voicing`] and written with sf2synth's writer.

pub mod dsp;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use sf2synth::generator as g;
use sf2synth::modulator::{controller, source};
use sf2synth::writer::{InstrumentDef, PresetDef, SampleDef, Sf2Builder, ZoneDef};
use sf2synth::{Modulator, SoundFont};

use dsp::{Biquad, SynthesisParams};

use crate::tuning::VelocityCurve;

/// What a new instrument is made from.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Origin {
    /// A preset of an SF2 file.
    Sf2 { path: Option<PathBuf>, bank: u16, program: u8 },
    /// One WAV file per note (the key read from the file name, or detected).
    Recordings { files: Vec<PathBuf> },
    /// The built-in piano-like synthesis.
    Synthesis(SynthesisParams),
}

impl Default for Origin {
    fn default() -> Self {
        Origin::Sf2 { path: None, bank: 0, program: 0 }
    }
}

/// How the new instrument should sound.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Voicing {
    pub name: String,
    /// Gain baked into the samples, dB.
    pub volume_db: f32,
    /// High shelf at 2.5 kHz, dB.
    pub brightness_db: f32,
    /// Low shelf at 200 Hz, dB.
    pub warmth_db: f32,
    /// Attack time, ms (0 keeps the original).
    pub attack_ms: f32,
    /// Seconds a held note takes to fade away (0 keeps the sample's own decay).
    pub decay_s: f32,
    /// Release time after the key is let go, s (0 keeps the original).
    pub release_s: f32,
    /// How much quieter soft notes are (velocity → attenuation), centibels.
    pub velocity_range_cb: f32,
    pub velocity_curve: VelocityCurve,
    /// How much darker soft notes are (velocity → filter cutoff), cents.
    pub velocity_brightness_cents: f32,
    /// Velocity layers made by darkening copies of the samples (1 = none).
    pub layers: u8,
    /// Cutoff of the softest layer, Hz.
    pub softest_cutoff_hz: f32,
    /// Stretch tuning at the ends of the keyboard, cents.
    pub stretch_cents: f32,
    /// Stereo width (pan scaled), 0…1.5.
    pub width: f32,
    /// Reverb send, 0…100 %.
    pub reverb_percent: f32,
}

impl Default for Voicing {
    fn default() -> Self {
        Voicing {
            name: "My Piano".into(),
            volume_db: 0.0,
            brightness_db: 0.0,
            warmth_db: 0.0,
            attack_ms: 0.0,
            decay_s: 0.0,
            release_s: 0.0,
            velocity_range_cb: 960.0,
            velocity_curve: VelocityCurve::Concave,
            velocity_brightness_cents: -2400.0,
            layers: 1,
            softest_cutoff_hz: 2500.0,
            stretch_cents: 0.0,
            width: 1.0,
            reverb_percent: 0.0,
        }
    }
}

/// A sample and where it plays.
#[derive(Clone, Debug)]
pub struct Region {
    pub key_range: (u8, u8),
    pub velocity_range: (u8, u8),
    pub root: u8,
    /// Cents.
    pub correction: i8,
    pub sample_rate: u32,
    pub points: Arc<Vec<i16>>,
    pub loop_range: Option<(u32, u32)>,
    pub pan: i16,
    /// Other generators carried over from the source.
    pub generators: Vec<(u16, i16)>,
    pub name: String,
}

/// Generators a region carries over from an SF2 (everything but addresses,
/// ranges, links, loop mode, root key and pan, which a region holds itself).
fn carried(generator: u16) -> bool {
    !matches!(
        generator,
        g::START_ADDRS_OFFSET
            | g::END_ADDRS_OFFSET
            | g::STARTLOOP_ADDRS_OFFSET
            | g::ENDLOOP_ADDRS_OFFSET
            | g::START_ADDRS_COARSE_OFFSET
            | g::END_ADDRS_COARSE_OFFSET
            | g::STARTLOOP_ADDRS_COARSE_OFFSET
            | g::ENDLOOP_ADDRS_COARSE_OFFSET
            | g::KEY_RANGE
            | g::VEL_RANGE
            | g::INSTRUMENT
            | g::SAMPLE_ID
            | g::SAMPLE_MODES
            | g::OVERRIDING_ROOT_KEY
            | g::PAN
            | g::KEYNUM
            | g::VELOCITY
            | g::EXCLUSIVE_CLASS
    )
}

fn intersect(a: (u8, u8), b: (u8, u8)) -> Option<(u8, u8)> {
    let (low, high) = (a.0.max(b.0), a.1.min(b.1));
    (low <= high).then_some((low, high))
}

fn to_i16(x: f32) -> i16 {
    (x * 32768.0).round().clamp(-32768.0, 32767.0) as i16
}

/// The regions of an SF2 preset.
pub fn from_sf2(font: &SoundFont, bank: u16, program: u8) -> Result<Vec<Region>, String> {
    let index = font.find_preset(bank, program as u16).ok_or_else(|| format!("no preset {bank}:{program}"))?;
    let preset = &font.presets()[index];
    let mut regions = Vec::new();
    // Zones playing the same sample points share them.
    let mut shared: std::collections::HashMap<(i64, i64), Arc<Vec<i16>>> = std::collections::HashMap::new();
    for preset_zone in preset.zones() {
        let Some(instrument) = font.instruments().get(preset_zone.link()) else { continue };
        for zone in instrument.zones() {
            let Some(sample) = font.samples().get(zone.link()) else { continue };
            if sample.is_rom() {
                continue;
            }
            let (Some(keys), Some(velocities)) = (
                intersect(preset_zone.key_range(), zone.key_range()),
                intersect(preset_zone.velocity_range(), zone.velocity_range()),
            ) else {
                continue;
            };
            let offset = |fine: u16, coarse: u16| {
                zone.generator(fine).unwrap_or(0) as i64 + 32768 * zone.generator(coarse).unwrap_or(0) as i64
            };
            let total = font.sample_point_count() as i64;
            let start =
                (sample.start as i64 + offset(g::START_ADDRS_OFFSET, g::START_ADDRS_COARSE_OFFSET)).clamp(0, total);
            let end = (sample.end as i64 + offset(g::END_ADDRS_OFFSET, g::END_ADDRS_COARSE_OFFSET)).clamp(start, total);
            if end - start < 16 {
                continue;
            }
            let loop_start =
                sample.loop_start as i64 + offset(g::STARTLOOP_ADDRS_OFFSET, g::STARTLOOP_ADDRS_COARSE_OFFSET);
            let loop_end = sample.loop_end as i64 + offset(g::ENDLOOP_ADDRS_OFFSET, g::ENDLOOP_ADDRS_COARSE_OFFSET);
            let loops = matches!(zone.generator(g::SAMPLE_MODES).unwrap_or(0) & 3, 1 | 3)
                && loop_start >= start
                && loop_end <= end
                && loop_end - loop_start >= 2;
            let points = Arc::clone(shared.entry((start, end)).or_insert_with(|| {
                Arc::new(font.sample_points(start as usize..end as usize).into_iter().map(to_i16).collect())
            }));
            let root = match zone.generator(g::OVERRIDING_ROOT_KEY) {
                Some(k) if (0..=127).contains(&k) => k as u8,
                _ if sample.original_pitch <= 127 => sample.original_pitch,
                _ => 60,
            };
            // Instrument generators, plus the preset's relative ones.
            let mut generators: Vec<(u16, i16)> = zone.generators().filter(|&(op, _)| carried(op)).collect();
            for (op, value) in preset_zone.generators().filter(|&(op, _)| carried(op)) {
                match generators.iter_mut().find(|(o, _)| *o == op) {
                    Some(entry) => entry.1 = entry.1.saturating_add(value),
                    None => generators.push((op, value)),
                }
            }
            let pan = zone.generator(g::PAN).unwrap_or(0).saturating_add(preset_zone.generator(g::PAN).unwrap_or(0));
            regions.push(Region {
                key_range: keys,
                velocity_range: velocities,
                root,
                correction: sample.pitch_correction,
                sample_rate: sample.sample_rate.max(1),
                points,
                loop_range: loops.then(|| ((loop_start - start) as u32, (loop_end - start) as u32)),
                pan,
                generators,
                name: sample.name.clone(),
            });
        }
    }
    if regions.is_empty() {
        return Err("the preset plays no samples".into());
    }
    Ok(regions)
}

/// One WAV per note.
pub fn from_recordings(files: &[PathBuf]) -> Result<Vec<Region>, String> {
    let mut found: Vec<(f32, Region)> = Vec::new();
    for path in files {
        let (mono, rate) = read_mono(path)?;
        let peak = mono.iter().fold(0.0f32, |m, &x| m.max(x.abs()));
        if peak < 1e-4 {
            return Err(format!("{} is silent", path.display()));
        }
        // From 5 ms before the sound starts.
        let onset = mono.iter().position(|&x| x.abs() > peak * 0.003).unwrap_or(0);
        let start = onset.saturating_sub((rate as f32 * 0.005) as usize);
        let trimmed = &mono[start..];
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let detected = dsp::detect_key(trimmed, rate, onset - start);
        let key = match (dsp::key_from_name(&name), detected) {
            (Some(named), _) => named as f32,
            (None, Some(detected)) => detected,
            (None, None) => return Err(format!("{name}: no key in the name and no clear pitch")),
        };
        let root = key.round().clamp(0.0, 127.0) as u8;
        // A detected pitch between keys is corrected in the sample header.
        let correction = detected.filter(|d| (d - root as f32).abs() < 0.5).map_or(0.0, |d| (root as f32 - d) * 100.0);
        found.push((
            key,
            Region {
                key_range: (root, root),
                velocity_range: (0, 127),
                root,
                correction: correction.round().clamp(-50.0, 50.0) as i8,
                sample_rate: rate,
                points: Arc::new(trimmed.iter().map(|&x| to_i16(x)).collect()),
                loop_range: None,
                pan: 0,
                generators: Vec::new(),
                name: name.rsplit_once('.').map_or(name.clone(), |(s, _)| s.to_string()),
            },
        ));
    }
    if found.is_empty() {
        return Err("choose recordings".into());
    }
    found.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut regions: Vec<Region> = found.into_iter().map(|(_, r)| r).collect();
    spread_key_ranges(&mut regions);
    Ok(regions)
}

/// Samples synthesized every three semitones.
pub fn from_synthesis(params: &SynthesisParams) -> Vec<Region> {
    let rate = 44100;
    let mut regions: Vec<Region> = (21..=108)
        .step_by(3)
        .map(|key: u8| {
            let seconds = (6.0 * (2f32).powf(-(key as f32 - 60.0) / 24.0)).clamp(2.0, 8.0);
            let tone = dsp::synthesize(key, params, rate, seconds);
            Region {
                key_range: (key, key),
                velocity_range: (0, 127),
                root: key,
                correction: 0,
                sample_rate: rate,
                points: Arc::new(tone.into_iter().map(to_i16).collect()),
                loop_range: None,
                pan: 0,
                generators: Vec::new(),
                name: format!("synth {}", crate::patterns::key_name(key)),
            }
        })
        .collect();
    spread_key_ranges(&mut regions);
    regions
}

/// Key ranges meeting halfway between neighbouring roots, the ends reaching
/// the whole keyboard.
fn spread_key_ranges(regions: &mut [Region]) {
    let n = regions.len();
    for i in 0..n {
        let low = if i == 0 { 0 } else { (regions[i - 1].root as u16 + regions[i].root as u16).div_ceil(2) as u8 };
        let high = if i + 1 == n { 127 } else { ((regions[i].root as u16 + regions[i + 1].root as u16) / 2) as u8 };
        regions[i].key_range = (low.min(regions[i].root), high.max(regions[i].root));
    }
}

fn read_mono(path: &Path) -> Result<(Vec<f32>, u32), String> {
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
    let mono = samples.chunks(channels).map(|frame| frame.iter().sum::<f32>() / channels as f32).collect();
    Ok((mono, spec.sample_rate))
}

fn timecents(seconds: f32) -> i16 {
    (1200.0 * seconds.max(0.001).log2()).round().clamp(-12000.0, 8000.0) as i16
}

/// The new instrument as an SF2. `progress` gets 0…1.
pub fn build(regions: &[Region], voicing: &Voicing, mut progress: impl FnMut(f32)) -> Sf2Builder {
    let name = if voicing.name.trim().is_empty() { "Untitled".to_string() } else { voicing.name.trim().to_string() };
    let mut sf2 = Sf2Builder::new(name.clone());
    sf2.comment = "Made with sf2studio".into();

    // Layers only split regions that cover every velocity.
    let layers = if regions.iter().all(|r| r.velocity_range.0 <= 1 && r.velocity_range.1 == 127) {
        voicing.layers.clamp(1, 4)
    } else {
        1
    };
    let gain = 10f32.powf(voicing.volume_db / 20.0);

    let mut global = ZoneDef::global();
    global.modulators.push(Modulator {
        source: controller::NOTE_ON_VELOCITY | source::NEGATIVE | velocity_curve_flag(voicing.velocity_curve),
        destination: g::INITIAL_ATTENUATION,
        amount: voicing.velocity_range_cb.round().clamp(0.0, 1440.0) as i16,
        amount_source: 0,
        transform: 0,
    });
    global.modulators.push(Modulator {
        source: controller::NOTE_ON_VELOCITY | source::NEGATIVE,
        destination: g::INITIAL_FILTER_FC,
        amount: voicing.velocity_brightness_cents.round().clamp(-9600.0, 0.0) as i16,
        amount_source: 0,
        transform: 0,
    });
    let mut instrument = InstrumentDef::new(name.clone());
    instrument.global = Some(global);

    let total = (regions.len() * layers as usize).max(1);
    let mut done = 0;
    // Zones that play the same sample share it in the new file too.
    type SampleKey = (usize, u8, u8, i8, Option<(u32, u32)>);
    let mut made: std::collections::HashMap<SampleKey, u16> = std::collections::HashMap::new();
    for region in regions {
        let rate = region.sample_rate as f32;
        // EQ and gain, once per region and only if a sample is still to make.
        let mut base: Option<Vec<f32>> = None;
        for layer in 0..layers {
            let velocities = if layers == 1 {
                region.velocity_range
            } else {
                let low = 1 + (126 * layer as u32 / layers as u32) as u8;
                let high = if layer + 1 == layers { 127 } else { (126 * (layer as u32 + 1) / layers as u32) as u8 };
                (low, high)
            };
            let key = (Arc::as_ptr(&region.points) as usize, layer, region.root, region.correction, region.loop_range);
            let index = match made.get(&key) {
                Some(&index) => index,
                None => {
                    let base = base.get_or_insert_with(|| {
                        let mut points: Vec<f32> = region.points.iter().map(|&p| p as f32 / 32768.0 * gain).collect();
                        if voicing.warmth_db.abs() > 0.05 {
                            Biquad::shelf(false, 200.0, voicing.warmth_db, rate).run(&mut points);
                        }
                        if voicing.brightness_db.abs() > 0.05 {
                            Biquad::shelf(true, 2500.0, voicing.brightness_db, rate).run(&mut points);
                        }
                        points
                    });
                    let mut points = base.clone();
                    if layer + 1 < layers {
                        // The softest layer is darkest; each louder one less so.
                        let t = layer as f32 / (layers - 1) as f32;
                        let cutoff =
                            voicing.softest_cutoff_hz * (18000.0 / voicing.softest_cutoff_hz.max(100.0)).powf(t);
                        let filter = Biquad::low_pass(cutoff, rate);
                        filter.run(&mut points);
                        filter.run(&mut points);
                    }
                    let suffix = if layers > 1 { format!(" v{}", layer + 1) } else { String::new() };
                    let mut sample = SampleDef::mono(
                        format!("{}{suffix}", region.name),
                        points.into_iter().map(to_i16).collect(),
                        region.sample_rate,
                        region.root,
                    );
                    sample.pitch_correction = region.correction;
                    if let Some((start, end)) = region.loop_range {
                        sample = sample.looped(start, end);
                    }
                    let index = sf2.add_sample(sample);
                    made.insert(key, index);
                    index
                }
            };

            let mut zone = ZoneDef::sample(index).keys(region.key_range.0, region.key_range.1);
            if velocities != (0, 127) {
                zone = zone.velocities(velocities.0, velocities.1);
            }
            for &(op, value) in &region.generators {
                zone.set(op, value);
            }
            zone.set(g::SAMPLE_MODES, if region.loop_range.is_some() { 1 } else { 0 });
            let pan = (region.pan as f32 * voicing.width).round().clamp(-500.0, 500.0) as i16;
            if pan != 0 {
                zone.set(g::PAN, pan);
            }
            if voicing.attack_ms > 0.0 {
                zone.set(g::ATTACK_VOL_ENV, timecents(voicing.attack_ms / 1000.0));
            }
            if voicing.decay_s > 0.0 {
                zone.set(g::DECAY_VOL_ENV, timecents(voicing.decay_s));
                zone.set(g::SUSTAIN_VOL_ENV, 1440);
            }
            if voicing.release_s > 0.0 {
                zone.set(g::RELEASE_VOL_ENV, timecents(voicing.release_s));
            }
            if voicing.reverb_percent > 0.0 {
                zone.set(g::REVERB_EFFECTS_SEND, (voicing.reverb_percent * 10.0).round().clamp(0.0, 1000.0) as i16);
            }
            if voicing.stretch_cents.abs() > 0.05 {
                // More stretch the further from middle C, sharp above, flat below.
                let centre = (region.key_range.0 as f32 + region.key_range.1 as f32) / 2.0;
                let distance = ((centre - 60.0) / 48.0).clamp(-1.0, 1.0);
                let cents = voicing.stretch_cents * distance * distance.abs();
                let total = zone.get(g::COARSE_TUNE).unwrap_or(0) as f32 * 100.0
                    + zone.get(g::FINE_TUNE).unwrap_or(0) as f32
                    + cents;
                let coarse = (total / 100.0).trunc();
                zone.set(g::COARSE_TUNE, coarse as i16);
                zone.set(g::FINE_TUNE, (total - coarse * 100.0).round() as i16);
            }
            instrument.zones.push(zone);
            done += 1;
            progress(done as f32 / total as f32);
        }
    }
    let instrument = sf2.add_instrument(instrument);
    sf2.add_preset(PresetDef::new(name, 0, 0).zone(ZoneDef::instrument(instrument)));
    sf2
}

fn velocity_curve_flag(curve: VelocityCurve) -> u16 {
    match curve {
        VelocityCurve::Concave => source::CONCAVE,
        VelocityCurve::Linear => source::LINEAR,
        VelocityCurve::Convex => source::CONVEX,
    }
}

// MARK: - Running in the background

/// A finished instrument.
pub struct Created {
    pub bytes: Arc<Vec<u8>>,
    pub font: Arc<SoundFont>,
    pub zones: usize,
    pub seconds: f32,
}

pub enum Update {
    Progress {
        generation: u64,
        fraction: f32,
    },
    /// The origin's regions, read once and kept for the next voicings.
    Material {
        origin: Origin,
        regions: Arc<Vec<Region>>,
    },
    Done {
        generation: u64,
        result: Result<Created, String>,
    },
}

pub struct Job {
    pub generation: u64,
    pub origin: Origin,
    /// The origin's regions when already read.
    pub regions: Option<Arc<Vec<Region>>>,
    /// The origin's SF2, when it is one and already loaded.
    pub font: Option<Arc<SoundFont>>,
    pub voicing: Voicing,
}

pub fn spawn(job: Job, sender: std::sync::mpsc::Sender<Update>, repaint: impl Fn() + Send + 'static) {
    std::thread::spawn(move || {
        let started = std::time::Instant::now();
        let result = (|| -> Result<Created, String> {
            let regions = match job.regions {
                Some(regions) => regions,
                None => {
                    let regions = Arc::new(match &job.origin {
                        Origin::Sf2 { path, bank, program } => {
                            let font = match job.font {
                                Some(font) => font,
                                None => {
                                    let path = path.as_ref().ok_or("choose an SF2")?;
                                    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
                                    Arc::new(SoundFont::from_bytes(bytes).map_err(|e| e.to_string())?)
                                }
                            };
                            from_sf2(&font, *bank, *program)?
                        }
                        Origin::Recordings { files } => from_recordings(files)?,
                        Origin::Synthesis(params) => from_synthesis(params),
                    });
                    let _ = sender.send(Update::Material { origin: job.origin.clone(), regions: Arc::clone(&regions) });
                    regions
                }
            };
            let mut last = 0.0;
            let builder = build(&regions, &job.voicing, |fraction| {
                if fraction - last >= 0.02 {
                    last = fraction;
                    let _ = sender.send(Update::Progress { generation: job.generation, fraction });
                    repaint();
                }
            });
            let bytes = Arc::new(builder.to_bytes());
            let font = Arc::new(SoundFont::from_shared(bytes.clone()).map_err(|e| e.to_string())?);
            let zones = font.instruments().first().map_or(0, |i| i.zones().len());
            Ok(Created { bytes, font, zones, seconds: started.elapsed().as_secs_f32() })
        })();
        let _ = sender.send(Update::Done { generation: job.generation, result });
        repaint();
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synthesis_builds_a_playable_sf2() {
        let params = SynthesisParams { decay: 2.0, ..Default::default() };
        let mut regions = from_synthesis(&params);
        // Keep the test quick: a few regions.
        regions.truncate(3);
        let voicing = Voicing { layers: 2, brightness_db: 3.0, stretch_cents: 10.0, ..Default::default() };
        let bytes = build(&regions, &voicing, |_| {}).to_bytes();
        let font = SoundFont::from_bytes(bytes).unwrap();
        assert_eq!(font.presets().len(), 1);
        assert_eq!(font.samples().len(), 6, "3 regions × 2 layers");
        let zones = font.instruments()[0].zones();
        assert_eq!(zones[0].velocity_range(), (1, 63));
        assert_eq!(zones[1].velocity_range(), (64, 127));
        assert!(!zones[0].modulators().is_empty());
    }

    #[test]
    fn an_sf2_survives_a_round_trip() {
        let params = SynthesisParams { decay: 1.0, ..Default::default() };
        let mut regions = from_synthesis(&params);
        regions.truncate(2);
        let first = SoundFont::from_bytes(build(&regions, &Voicing::default(), |_| {}).to_bytes()).unwrap();
        let again = from_sf2(&first, 0, 0).unwrap();
        assert_eq!(again.len(), 2);
        assert_eq!(again[0].root, regions[0].root);
        assert_eq!(again[0].points.len(), regions[0].points.len());
        assert_eq!(again[0].points[1000], regions[0].points[1000]);
    }
}
