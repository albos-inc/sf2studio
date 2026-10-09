//! The synthesizer settings a lane can tune, saved as TOML.

use serde::{Deserialize, Serialize};
use sf2synth::modulator::source;
use sf2synth::{Interpolation, Modulator, SynthesizerSettings, generator};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum VelocityCurve {
    Concave,
    Linear,
    Convex,
}

impl VelocityCurve {
    pub const ALL: [VelocityCurve; 3] = [VelocityCurve::Concave, VelocityCurve::Linear, VelocityCurve::Convex];

    pub fn name(self, japanese: bool) -> &'static str {
        match (self, japanese) {
            (VelocityCurve::Concave, false) => "Concave (SF2 default)",
            (VelocityCurve::Concave, true) => "凹型（SF2 既定）",
            (VelocityCurve::Linear, false) => "Linear",
            (VelocityCurve::Linear, true) => "直線",
            (VelocityCurve::Convex, false) => "Convex",
            (VelocityCurve::Convex, true) => "凸型",
        }
    }

    fn flag(self) -> u16 {
        match self {
            VelocityCurve::Concave => source::CONCAVE,
            VelocityCurve::Linear => source::LINEAR,
            VelocityCurve::Convex => source::CONVEX,
        }
    }
}

/// The output gain to start from: sf2synth 0.1's default master gain (0.5),
/// 6 dB below its current one (1.0), which earlier comparisons and saved
/// levels were made at.
pub const DEFAULT_MASTER_GAIN_DB: f32 = -6.0206;

/// What the tuning screen changes.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SynthTuning {
    /// Output gain, dB.
    pub master_gain_db: f32,
    /// How much quieter velocity 1 is than 127, in centibels of the
    /// velocity → attenuation modulator (SF2 default 960 = 96 dB at the
    /// curve's end).
    pub velocity_range_cb: f32,
    pub velocity_curve: VelocityCurve,
    /// Velocity → filter cutoff, cents at velocity 0 (SF2 default -2400;
    /// 0 turns the velocity filter off).
    pub velocity_filter_cents: f32,
    pub initial_attenuation_scale: f32,
    pub cubic_interpolation: bool,
    pub release_same_key: bool,
    /// Reverb send (CC91) given to every channel before playing.
    pub reverb_send: u8,
    pub reverb_room: f32,
    pub reverb_damping: f32,
    pub reverb_width: f32,
    pub reverb_level: f32,
    /// Chorus send (CC93) given to every channel before playing.
    pub chorus_send: u8,
    pub chorus_level: f32,
    pub max_voices: usize,
}

impl Default for SynthTuning {
    fn default() -> Self {
        let defaults = SynthesizerSettings::default();
        SynthTuning {
            master_gain_db: DEFAULT_MASTER_GAIN_DB,
            velocity_range_cb: 960.0,
            velocity_curve: VelocityCurve::Concave,
            velocity_filter_cents: -2400.0,
            initial_attenuation_scale: defaults.initial_attenuation_scale,
            cubic_interpolation: true,
            release_same_key: defaults.release_same_key,
            reverb_send: 0,
            reverb_room: defaults.reverb.room_size,
            reverb_damping: defaults.reverb.damping,
            reverb_width: defaults.reverb.width,
            reverb_level: defaults.reverb.level,
            chorus_send: 0,
            chorus_level: defaults.chorus.level,
            max_voices: defaults.max_voices,
        }
    }
}

impl SynthTuning {
    pub fn settings(&self, sample_rate: u32) -> SynthesizerSettings {
        let mut settings = SynthesizerSettings::new(sample_rate);
        settings.master_gain = 10f32.powf(self.master_gain_db / 20.0);
        settings.initial_attenuation_scale = self.initial_attenuation_scale;
        settings.interpolation = if self.cubic_interpolation { Interpolation::Cubic } else { Interpolation::Linear };
        settings.release_same_key = self.release_same_key;
        settings.max_voices = self.max_voices.clamp(16, 4096);
        settings.reverb.room_size = self.reverb_room;
        settings.reverb.damping = self.reverb_damping;
        settings.reverb.width = self.reverb_width;
        settings.reverb.level = self.reverb_level;
        settings.reverb.enabled = self.reverb_level > 0.0;
        settings.chorus.level = self.chorus_level;
        settings.chorus.enabled = self.chorus_level > 0.0;
        // Measurements and A/B comparisons need the synthesizer's own output:
        // no limiter turning loud notes down (nor its 1 ms of delay).
        settings.limiter = false;
        for modulator in settings.default_modulators.iter_mut() {
            let velocity = sf2synth::modulator::controller::NOTE_ON_VELOCITY;
            let is_velocity = modulator.source & 0x7f == velocity && modulator.source & source::CC == 0;
            if is_velocity && modulator.destination == generator::INITIAL_ATTENUATION {
                *modulator = Modulator {
                    source: velocity | source::NEGATIVE | self.velocity_curve.flag(),
                    amount: self.velocity_range_cb.round() as i16,
                    ..*modulator
                };
            } else if is_velocity && modulator.destination == generator::INITIAL_FILTER_FC {
                modulator.amount = self.velocity_filter_cents.round() as i16;
            }
        }
        settings
    }

    pub fn to_toml(&self) -> String {
        toml::to_string_pretty(self).unwrap_or_default()
    }

    pub fn from_toml(text: &str) -> Result<SynthTuning, String> {
        toml::from_str(text).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_tuning_gives_default_settings() {
        let tuned = SynthTuning::default().settings(48000);
        let defaults = SynthesizerSettings::new(48000);
        assert_eq!(tuned.default_modulators, defaults.default_modulators);
        assert!((tuned.master_gain - 0.5).abs() < 1e-5);
        assert!(!tuned.limiter);
    }

    #[test]
    fn round_trips_through_toml() {
        let tuning =
            SynthTuning { velocity_range_cb: 600.0, velocity_curve: VelocityCurve::Linear, ..Default::default() };
        assert_eq!(SynthTuning::from_toml(&tuning.to_toml()).unwrap(), tuning);
    }
}
