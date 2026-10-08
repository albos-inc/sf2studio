//! Per-note measurements of a rendered lane: how loud, how bright and how
//! long each note of the program is.

use sf2synth::MidiFile;

use crate::analysis::{Analysis, FLOOR_DB};

/// A note of the program.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Note {
    pub start: f64,
    pub end: f64,
    pub key: u8,
    pub velocity: u8,
}

/// The notes of a MIDI file (note-ons paired with their note-offs).
pub fn notes(midi: &MidiFile) -> Vec<Note> {
    let mut open: Vec<(u8, u8, f64, u8)> = Vec::new(); // channel, key, start, velocity
    let mut notes = Vec::new();
    for event in midi.events() {
        let (status, key, velocity) = (event.status() & 0xf0, event.bytes[1], event.bytes[2]);
        let channel = event.channel();
        if status == 0x90 && velocity > 0 {
            open.push((channel, key, event.time, velocity));
        } else if (status == 0x80 || status == 0x90)
            && let Some(i) = open.iter().position(|&(c, k, _, _)| c == channel && k == key)
        {
            let (_, _, start, velocity) = open.remove(i);
            notes.push(Note { start, end: event.time, key, velocity });
        }
    }
    for (_, key, start, velocity) in open {
        notes.push(Note { start, end: midi.duration(), key, velocity });
    }
    notes.sort_by(|a, b| a.start.total_cmp(&b.start));
    notes
}

/// What was measured of one note.
#[derive(Clone, Copy, Debug)]
pub struct NoteMeasure {
    pub key: u8,
    pub velocity: u8,
    /// Loudest level in the first 0.3 s, dB.
    pub peak_db: f32,
    /// Spectral centroid just after the attack, Hz.
    pub brightness_hz: f32,
    /// Seconds from the peak until 40 dB quieter, if that happens before the
    /// next note.
    pub decay_s: Option<f32>,
}

/// How far below its peak a note has decayed.
pub const DECAY_DB: f32 = 40.0;

/// Measures every note that starts alone (no other note within 0.3 s).
pub fn measure(analysis: &Analysis, sample_rate: u32, notes: &[Note]) -> Vec<NoteMeasure> {
    let seconds_per_column = analysis.hop as f64 / sample_rate as f64;
    let mut result = Vec::new();
    for (i, note) in notes.iter().enumerate() {
        let next_start = notes[i + 1..].iter().map(|n| n.start).find(|&s| s > note.start);
        if next_start.is_some_and(|s| s - note.start < 0.3)
            || (i > 0 && note.start - notes[i - 1].start < 0.3 && notes[i - 1].start != note.start)
            || notes.iter().filter(|n| n.start == note.start).count() > 1
        {
            continue;
        }
        let first = analysis.column_at(note.start, sample_rate);
        let attack_end = analysis.column_at(note.start + 0.3, sample_rate);
        let (peak_column, peak_db) = (first..=attack_end)
            .map(|c| (c, analysis.envelope_db[c]))
            .fold((first, FLOOR_DB), |best, x| if x.1 > best.1 { x } else { best });
        if peak_db <= FLOOR_DB + 1.0 {
            continue;
        }

        let bright_from = analysis.column_at(note.start + 0.02, sample_rate);
        let bright_to = analysis.column_at(note.start + 0.25, sample_rate);
        let mut centroids: Vec<f32> =
            analysis.centroid_hz[bright_from..=bright_to].iter().copied().filter(|&c| c > 0.0).collect();
        centroids.sort_by(f32::total_cmp);
        let brightness_hz = centroids.get(centroids.len() / 2).copied().unwrap_or(0.0);

        let limit = next_start.map_or(analysis.columns - 1, |s| analysis.column_at(s, sample_rate));
        let decay_s = (peak_column..=limit)
            .find(|&c| analysis.envelope_db[c] < peak_db - DECAY_DB)
            .map(|c| ((c - peak_column) as f64 * seconds_per_column) as f32);

        result.push(NoteMeasure { key: note.key, velocity: note.velocity, peak_db, brightness_hz, decay_s });
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::patterns::TestPattern;

    #[test]
    fn pairs_the_notes_of_a_pattern() {
        let midi = MidiFile::from_bytes(&TestPattern::VelocitySweep.midi()).unwrap();
        let notes = notes(&midi);
        assert_eq!(notes.len(), 24);
        assert!(notes.iter().all(|n| n.end > n.start));
        assert_eq!(notes[0].key, 60);
        assert_eq!(notes[0].velocity, 16);
    }
}
