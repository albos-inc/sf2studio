//! The null test: one lane's audio subtracted from another's, to see (and
//! hear) whether and where they differ.

use std::sync::Arc;

use crate::analysis::Rendered;

/// Levels below this are drawn as silence.
pub const FLOOR_DB: f32 = -150.0;

pub struct NullTest {
    /// The difference, a × gain a − b × gain b, as audio.
    pub difference: Arc<Rendered>,
    /// Samples (of both channels) that differ at all.
    pub differing: usize,
    pub samples: usize,
    /// Largest difference, dBFS (None when there is none).
    pub peak_db: Option<f32>,
    /// The difference's RMS level relative to a's, dB: negative when it is
    /// quieter (None when there is none).
    pub depth_db: Option<f32>,
    /// When the difference first appears, seconds.
    pub first: Option<f64>,
    /// Frames per point of `envelope_db`.
    pub hop: usize,
    /// The difference's peak level over time, dBFS.
    pub envelope_db: Vec<f32>,
    /// Frames of a and of b (the shorter is taken as silent after its end).
    pub lengths: (usize, usize),
}

impl NullTest {
    /// Whether a and b are the same, sample for sample.
    pub fn identical(&self) -> bool {
        self.differing == 0
    }
}

fn db(x: f32) -> f32 {
    if x > 0.0 { (20.0 * x.log10()).max(FLOOR_DB) } else { FLOOR_DB }
}

/// Subtracts `b` from `a`, each scaled by its gain first (level matching).
pub fn compare(a: &Rendered, b: &Rendered, gain_a: f32, gain_b: f32) -> NullTest {
    let frames = a.frames().max(b.frames());
    let at = |x: &[f32], i: usize| x.get(i).copied().unwrap_or(0.0);
    let mut left = Vec::with_capacity(frames);
    let mut right = Vec::with_capacity(frames);
    let mut differing = 0;
    let mut first = None;
    let mut peak = 0f32;
    let (mut difference_energy, mut signal_energy) = (0f64, 0f64);
    for i in 0..frames {
        let (al, ar) = (at(&a.left, i) * gain_a, at(&a.right, i) * gain_a);
        let (dl, dr) = (al - at(&b.left, i) * gain_b, ar - at(&b.right, i) * gain_b);
        if dl != 0.0 || dr != 0.0 {
            differing += (dl != 0.0) as usize + (dr != 0.0) as usize;
            first.get_or_insert(i);
            peak = peak.max(dl.abs()).max(dr.abs());
        }
        difference_energy += (dl * dl + dr * dr) as f64;
        signal_energy += (al * al + ar * ar) as f64;
        left.push(dl);
        right.push(dr);
    }
    let hop = (a.sample_rate as usize / 100).max(1);
    let envelope_db = left
        .chunks(hop)
        .zip(right.chunks(hop))
        .map(|(l, r)| db(l.iter().chain(r).fold(0f32, |m, x| m.max(x.abs()))))
        .collect();
    let any = differing > 0;
    NullTest {
        difference: Arc::new(Rendered { sample_rate: a.sample_rate, left, right }),
        differing,
        samples: frames * 2,
        peak_db: any.then(|| db(peak)),
        depth_db: (any && signal_energy > 0.0).then(|| (10.0 * (difference_energy / signal_energy).log10()) as f32),
        first: first.map(|i| i as f64 / a.sample_rate as f64),
        hop,
        envelope_db,
        lengths: (a.frames(), b.frames()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(frames: usize, amplitude: f32) -> Rendered {
        let left: Vec<f32> = (0..frames).map(|i| (i as f32 * 0.05).sin() * amplitude).collect();
        Rendered { sample_rate: 48000, right: left.clone(), left }
    }

    #[test]
    fn the_same_audio_nulls() {
        let a = tone(48000, 0.5);
        let test = compare(&a, &tone(48000, 0.5), 1.0, 1.0);
        assert!(test.identical());
        assert_eq!((test.peak_db, test.depth_db, test.first), (None, None, None));
        assert!(test.envelope_db.iter().all(|&d| d == FLOOR_DB));
        assert_eq!(test.envelope_db.len(), 100);
    }

    #[test]
    fn a_small_difference_shows_where_and_how_deep() {
        let a = tone(48000, 0.5);
        let mut b = tone(48000, 0.5);
        // -60 dB of the signal from 0.5 s on.
        for i in 24000..48000 {
            b.left[i] *= 1.001;
            b.right[i] *= 1.001;
        }
        let test = compare(&a, &b, 1.0, 1.0);
        assert!(!test.identical());
        let first = test.first.unwrap();
        assert!((0.5..0.51).contains(&first), "{first}");
        let depth = test.depth_db.unwrap();
        // Half the time at -60 dB: -63 dB overall.
        assert!((depth + 63.0).abs() < 0.5, "{depth}");
        assert!(test.envelope_db[10] == FLOOR_DB && test.envelope_db[60] > -70.0);
    }

    #[test]
    fn level_matching_and_lengths() {
        let a = tone(1000, 0.5);
        let b = tone(1200, 0.25);
        // Matched, the overlap nulls; b's longer tail is what's left.
        let test = compare(&a, &b, 1.0, 2.0);
        assert_eq!(test.lengths, (1000, 1200));
        assert_eq!(test.first.map(|t| (t * 48000.0).round() as usize), Some(1000));
    }
}
