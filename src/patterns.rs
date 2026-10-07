//! Test patterns: MIDI files generated to probe an instrument.

use serde::{Deserialize, Serialize};

/// Ticks per quarter note of the generated files; at 120 BPM a tick is
/// 1/960 s.
const TICKS_PER_QUARTER: u32 = 480;
const TICKS_PER_SECOND: f64 = 960.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TestPattern {
    /// Middle C, then C2 and C6, at velocities 16…127.
    VelocitySweep,
    /// Every key of the piano, A0 to C8, at velocity 80.
    Chromatic,
    /// Chords across the keyboard, soft to loud.
    Chords,
    /// Repeated notes with the sustain pedal held, then released.
    SustainPedal,
    /// Long notes, to hear the decay and release.
    Decay,
}

impl TestPattern {
    pub const ALL: [TestPattern; 5] = [
        TestPattern::VelocitySweep,
        TestPattern::Chromatic,
        TestPattern::Chords,
        TestPattern::SustainPedal,
        TestPattern::Decay,
    ];

    pub fn name(self, japanese: bool) -> &'static str {
        match (self, japanese) {
            (TestPattern::VelocitySweep, false) => "Velocity sweep",
            (TestPattern::VelocitySweep, true) => "ベロシティの段階",
            (TestPattern::Chromatic, false) => "Chromatic scale (88 keys)",
            (TestPattern::Chromatic, true) => "半音階（88 鍵）",
            (TestPattern::Chords, false) => "Chords, soft to loud",
            (TestPattern::Chords, true) => "和音（弱 → 強）",
            (TestPattern::SustainPedal, false) => "Sustain pedal",
            (TestPattern::SustainPedal, true) => "サステインペダル",
            (TestPattern::Decay, false) => "Long notes (decay)",
            (TestPattern::Decay, true) => "長い音（減衰）",
        }
    }

    /// The pattern as a Standard MIDI File.
    pub fn midi(self) -> Vec<u8> {
        let mut events = Events::default();
        match self {
            TestPattern::VelocitySweep => {
                let mut t = 0.2;
                for key in [60, 36, 84] {
                    for velocity in [16, 32, 48, 64, 80, 96, 112, 127] {
                        events.note(t, 1.2, key, velocity);
                        t += 1.6;
                    }
                    t += 0.6;
                }
            }
            TestPattern::Chromatic => {
                let mut t = 0.2;
                for key in 21..=108 {
                    events.note(t, 0.25, key, 80);
                    t += 0.3;
                }
            }
            TestPattern::Chords => {
                let mut t = 0.2;
                for (root, velocity) in [(36, 30), (48, 50), (60, 70), (72, 90), (84, 110), (60, 127)] {
                    for interval in [0, 4, 7, 12] {
                        events.note(t, 1.5, root + interval, velocity);
                    }
                    t += 2.2;
                }
            }
            TestPattern::SustainPedal => {
                events.control(0.1, 64, 127);
                let mut t = 0.2;
                for _ in 0..3 {
                    for key in [48, 55, 60, 64, 67, 72, 76, 79] {
                        events.note(t, 0.15, key, 72);
                        t += 0.25;
                    }
                }
                events.control(t + 2.0, 64, 0);
                t += 4.0;
                // The same chord without the pedal, for comparison.
                for key in [48, 55, 60, 64, 67, 72, 76, 79] {
                    events.note(t, 0.15, key, 72);
                    t += 0.25;
                }
            }
            TestPattern::Decay => {
                let mut t = 0.2;
                for key in [24, 36, 48, 60, 72, 84, 96] {
                    events.note(t, 6.0, key, 96);
                    t += 7.5;
                }
            }
        }
        events.into_midi()
    }
}

/// When a keyboard note starts in its rendering.
pub const NOTE_START: f64 = 0.05;

/// One note: `key` at `velocity`, held `length` seconds.
pub fn single_note(key: u8, velocity: u8, length: f32) -> Vec<u8> {
    let mut events = Events::default();
    events.note(NOTE_START, length.max(0.01) as f64, key, velocity.max(1));
    events.into_midi()
}

/// The name of a MIDI key: C4 is 60.
pub fn key_name(key: u8) -> String {
    const NAMES: [&str; 12] = ["C", "C♯", "D", "D♯", "E", "F", "F♯", "G", "G♯", "A", "A♯", "B"];
    format!("{}{}", NAMES[key as usize % 12], key as i32 / 12 - 1)
}

/// The frequency of a MIDI key (A4 = 440 Hz).
pub fn key_hz(key: u8) -> f32 {
    440.0 * ((key as f32 - 69.0) / 12.0).exp2()
}

#[derive(Default)]
struct Events {
    /// (tick, order, bytes)
    list: Vec<(u32, u32, Vec<u8>)>,
}

impl Events {
    fn tick(seconds: f64) -> u32 {
        (seconds * TICKS_PER_SECOND).round() as u32
    }

    fn push(&mut self, seconds: f64, bytes: Vec<u8>) {
        let order = self.list.len() as u32;
        self.list.push((Self::tick(seconds), order, bytes));
    }

    fn note(&mut self, start: f64, length: f64, key: u8, velocity: u8) {
        self.push(start, vec![0x90, key, velocity]);
        self.push(start + length, vec![0x80, key, 0]);
    }

    fn control(&mut self, at: f64, controller: u8, value: u8) {
        self.push(at, vec![0xb0, controller, value]);
    }

    fn into_midi(mut self) -> Vec<u8> {
        // Note-offs before note-ons at the same tick.
        self.list.sort_by_key(|(tick, order, bytes)| (*tick, bytes[0] & 0xf0 != 0x80, *order));
        let mut track = Vec::new();
        let mut last = 0;
        for (tick, _, bytes) in &self.list {
            write_variable_length(&mut track, tick - last);
            last = *tick;
            track.extend_from_slice(bytes);
        }
        write_variable_length(&mut track, Self::tick(0.5));
        track.extend_from_slice(&[0xff, 0x2f, 0x00]);

        let mut out = b"MThd".to_vec();
        out.extend_from_slice(&6u32.to_be_bytes());
        out.extend_from_slice(&0u16.to_be_bytes());
        out.extend_from_slice(&1u16.to_be_bytes());
        out.extend_from_slice(&(TICKS_PER_QUARTER as u16).to_be_bytes());
        out.extend_from_slice(b"MTrk");
        out.extend_from_slice(&(track.len() as u32).to_be_bytes());
        out.extend_from_slice(&track);
        out
    }
}

fn write_variable_length(out: &mut Vec<u8>, mut value: u32) {
    let mut bytes = vec![(value & 0x7f) as u8];
    value >>= 7;
    while value > 0 {
        bytes.push((value & 0x7f) as u8 | 0x80);
        value >>= 7;
    }
    bytes.reverse();
    out.extend_from_slice(&bytes);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_pattern_is_a_valid_midi_file() {
        for pattern in TestPattern::ALL {
            let midi = sf2synth::MidiFile::from_bytes(&pattern.midi()).unwrap();
            assert!(!midi.events().is_empty());
            assert!(midi.duration() > 1.0);
        }
    }
}
