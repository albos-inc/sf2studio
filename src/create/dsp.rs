//! Signal processing for building instruments: filters baked into samples,
//! pitch detection, note names and a simple piano-like synthesis.

/// A biquad filter (RBJ cookbook), run over a sample once.
#[derive(Clone, Copy)]
pub struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
}

impl Biquad {
    fn normalized(b0: f32, b1: f32, b2: f32, a0: f32, a1: f32, a2: f32) -> Biquad {
        Biquad { b0: b0 / a0, b1: b1 / a0, b2: b2 / a0, a1: a1 / a0, a2: a2 / a0 }
    }

    /// A shelf boosting or cutting by `gain_db` above (`high`) or below `frequency`.
    pub fn shelf(high: bool, frequency: f32, gain_db: f32, sample_rate: f32) -> Biquad {
        let a = 10f32.powf(gain_db / 40.0);
        let w0 = 2.0 * std::f32::consts::PI * (frequency / sample_rate).min(0.45);
        let (sin, cos) = w0.sin_cos();
        let alpha = sin / 2.0 * std::f32::consts::SQRT_2;
        let k = 2.0 * a.sqrt() * alpha;
        if high {
            Biquad::normalized(
                a * ((a + 1.0) + (a - 1.0) * cos + k),
                -2.0 * a * ((a - 1.0) + (a + 1.0) * cos),
                a * ((a + 1.0) + (a - 1.0) * cos - k),
                (a + 1.0) - (a - 1.0) * cos + k,
                2.0 * ((a - 1.0) - (a + 1.0) * cos),
                (a + 1.0) - (a - 1.0) * cos - k,
            )
        } else {
            Biquad::normalized(
                a * ((a + 1.0) - (a - 1.0) * cos + k),
                2.0 * a * ((a - 1.0) - (a + 1.0) * cos),
                a * ((a + 1.0) - (a - 1.0) * cos - k),
                (a + 1.0) + (a - 1.0) * cos + k,
                -2.0 * ((a - 1.0) + (a + 1.0) * cos),
                (a + 1.0) + (a - 1.0) * cos - k,
            )
        }
    }

    /// A 2-pole low-pass at `frequency`.
    pub fn low_pass(frequency: f32, sample_rate: f32) -> Biquad {
        let w0 = 2.0 * std::f32::consts::PI * (frequency / sample_rate).min(0.45);
        let (sin, cos) = w0.sin_cos();
        let alpha = sin / 2.0 * std::f32::consts::SQRT_2;
        Biquad::normalized((1.0 - cos) / 2.0, 1.0 - cos, (1.0 - cos) / 2.0, 1.0 + alpha, -2.0 * cos, 1.0 - alpha)
    }

    pub fn run(&self, samples: &mut [f32]) {
        let (mut z1, mut z2) = (0.0f32, 0.0f32);
        for x in samples.iter_mut() {
            let input = *x;
            let y = self.b0 * input + z1;
            z1 = self.b1 * input - self.a1 * y + z2;
            z2 = self.b2 * input - self.a2 * y;
            *x = y;
        }
    }
}

/// The fundamental of a tone (YIN), searched between 25 Hz and 4.5 kHz, as
/// a fractional MIDI key. Looks at up to 0.3 s after `start`.
pub fn detect_key(samples: &[f32], sample_rate: u32, start: usize) -> Option<f32> {
    let rate = sample_rate as f32;
    let window = ((rate * 0.05) as usize).max(256);
    let max_lag = ((rate / 25.0) as usize).min(window);
    let min_lag = ((rate / 4500.0) as usize).max(2);
    // Skip the attack: analyse from 60 ms in.
    let from = start + (rate * 0.06) as usize;
    if from + window + max_lag >= samples.len() {
        return None;
    }
    let frame = &samples[from..from + window + max_lag];
    let mut difference = vec![0.0f32; max_lag + 1];
    for (lag, d) in difference.iter_mut().enumerate().skip(1) {
        let mut sum = 0.0;
        for i in 0..window {
            let delta = frame[i] - frame[i + lag];
            sum += delta * delta;
        }
        *d = sum;
    }
    // Cumulative mean normalized difference.
    let mut running = 0.0;
    let mut normalized = vec![1.0f32; max_lag + 1];
    for lag in 1..=max_lag {
        running += difference[lag];
        normalized[lag] = if running > 0.0 { difference[lag] * lag as f32 / running } else { 1.0 };
    }
    let mut lag = (min_lag..max_lag).find(|&l| normalized[l] < 0.15)?;
    while lag + 1 < max_lag && normalized[lag + 1] < normalized[lag] {
        lag += 1;
    }
    // Parabolic interpolation around the minimum.
    let (a, b, c) = (normalized[lag - 1], normalized[lag], normalized[lag + 1]);
    let shift = if (a - 2.0 * b + c).abs() > 1e-9 { 0.5 * (a - c) / (a - 2.0 * b + c) } else { 0.0 };
    let period = lag as f32 + shift;
    let hz = rate / period;
    Some(69.0 + 12.0 * (hz / 440.0).log2())
}

/// A MIDI key from a file name such as `C4.wav`, `F#2-loud.wav`, `Bb5`,
/// `piano_060.wav` (C4 = 60).
pub fn key_from_name(name: &str) -> Option<u8> {
    let stem = name.rsplit_once('.').map_or(name, |(s, _)| s);
    let bytes: Vec<char> = stem.chars().collect();
    // Note names: a letter, an optional accidental, an octave (may be -1).
    for i in 0..bytes.len() {
        let letter = bytes[i].to_ascii_uppercase();
        let base = match letter {
            'C' => 0,
            'D' => 2,
            'E' => 4,
            'F' => 5,
            'G' => 7,
            'A' => 9,
            'B' => 11,
            _ => continue,
        };
        if i > 0 && bytes[i - 1].is_ascii_alphabetic() {
            continue;
        }
        let mut j = i + 1;
        let mut offset = 0i32;
        match bytes.get(j) {
            Some('#') | Some('♯') | Some('s') => {
                offset = 1;
                j += 1;
            }
            Some('b') | Some('♭') => {
                offset = -1;
                j += 1;
            }
            _ => {}
        }
        let negative = bytes.get(j) == Some(&'-');
        if negative {
            j += 1;
        }
        let digit = bytes.get(j).and_then(|c| c.to_digit(10));
        let after = bytes.get(j + 1);
        if let Some(octave) = digit
            && !after.is_some_and(|c| c.is_ascii_digit())
        {
            let octave = if negative { -(octave as i32) } else { octave as i32 };
            let key = (octave + 1) * 12 + base + offset;
            if (0..=127).contains(&key) {
                return Some(key as u8);
            }
        }
    }
    // A plain MIDI number.
    let digits: String =
        stem.chars().rev().skip_while(|c| !c.is_ascii_digit()).take_while(|c| c.is_ascii_digit()).collect();
    let number: String = digits.chars().rev().collect();
    number.parse::<u8>().ok().filter(|&k| (12..=120).contains(&k))
}

/// Parameters of the built-in piano-like synthesis.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct SynthesisParams {
    /// Harmonic roll-off: 0 dull … 1 bright.
    pub brightness: f32,
    /// String stiffness: partials pulled sharp (0 none … 1 strong).
    pub inharmonicity: f32,
    /// Seconds for middle C to fall 60 dB.
    pub decay: f32,
    /// Detuning between the strings of a note, cents.
    pub detune: f32,
    /// Hammer noise level, 0…1.
    pub hammer: f32,
}

impl Default for SynthesisParams {
    fn default() -> Self {
        SynthesisParams { brightness: 0.55, inharmonicity: 0.4, decay: 9.0, detune: 1.2, hammer: 0.25 }
    }
}

/// A piano-like tone for `key`: three slightly detuned strings of decaying,
/// stretched partials and a hammer thump. `seconds` long at `sample_rate`.
pub fn synthesize(key: u8, params: &SynthesisParams, sample_rate: u32, seconds: f32) -> Vec<f32> {
    let rate = sample_rate as f32;
    let length = (seconds * rate) as usize;
    let mut out = vec![0.0f32; length];
    let f0 = 440.0 * ((key as f32 - 69.0) / 12.0).exp2();
    // Stiffness grows towards the treble.
    let b = params.inharmonicity * 0.0004 * (1.0 + (key as f32 - 21.0) / 40.0);
    // Higher notes decay faster.
    let decay_c = params.decay * (2f32).powf(-(key as f32 - 60.0) / 18.0);
    let roll_off = 2.2 - 1.6 * params.brightness;
    let strings: &[f32] = if key < 34 {
        &[0.0]
    } else if key < 48 {
        &[-0.5, 0.5]
    } else {
        &[-1.0, 0.0, 1.0]
    };
    for n in 1..=48u32 {
        let n_f = n as f32;
        let frequency = n_f * f0 * (1.0 + b * n_f * n_f).sqrt();
        if frequency > rate * 0.45 {
            break;
        }
        let amplitude = 1.0 / n_f.powf(roll_off);
        // Upper partials die away sooner.
        let tau = decay_c / 6.9 / (1.0 + 0.15 * (n_f - 1.0));
        let attack_rise = (rate * 0.002) as usize;
        for &s in strings {
            let f = frequency * (s * params.detune / 1200.0).exp2();
            let step = 2.0 * std::f32::consts::PI * f / rate;
            let phase0 = n_f * 1.7 + s;
            let decay_step = (-1.0 / (tau * rate)).exp();
            let mut envelope = amplitude / strings.len() as f32;
            for (i, sample) in out.iter_mut().enumerate() {
                let rise = if i < attack_rise { i as f32 / attack_rise as f32 } else { 1.0 };
                *sample += (phase0 + step * i as f32).sin() * envelope * rise;
                envelope *= decay_step;
                if envelope < 1e-6 {
                    break;
                }
            }
        }
    }
    // Hammer thump: a short burst of low-passed noise.
    if params.hammer > 0.0 {
        let mut seed = 0x1234_5678u32 ^ key as u32;
        let burst = (rate * 0.03) as usize;
        let mut noise: Vec<f32> = (0..burst.min(length))
            .map(|i| {
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                let white = (seed as f32 / u32::MAX as f32) * 2.0 - 1.0;
                white * (-(i as f32) / (rate * 0.006)).exp()
            })
            .collect();
        Biquad::low_pass((f0 * 6.0).clamp(400.0, 6000.0), rate).run(&mut noise);
        for (o, n) in out.iter_mut().zip(noise) {
            *o += n * params.hammer * 0.6;
        }
    }
    let peak = out.iter().fold(0.0f32, |m, &x| m.max(x.abs())).max(1e-6);
    out.iter_mut().for_each(|x| *x *= 0.8 / peak);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_keys_from_file_names() {
        assert_eq!(key_from_name("C4.wav"), Some(60));
        assert_eq!(key_from_name("A0.wav"), Some(21));
        assert_eq!(key_from_name("F#2-loud.wav"), Some(42));
        assert_eq!(key_from_name("Bb5.wav"), Some(82));
        assert_eq!(key_from_name("piano_060.wav"), Some(60));
        assert_eq!(key_from_name("Piano C-1.wav"), Some(0));
        assert_eq!(key_from_name("take.wav"), None);
    }

    #[test]
    fn detects_the_pitch_of_a_synthesized_note() {
        for key in [33u8, 45, 60, 72, 84] {
            let tone = synthesize(key, &SynthesisParams::default(), 44100, 1.0);
            let detected = detect_key(&tone, 44100, 0).unwrap();
            // Stretched partials pull the treble a little sharp, as on a
            // real piano; still well within the key.
            assert!((detected - key as f32).abs() < 0.2, "key {key}: detected {detected}");
        }
    }

    #[test]
    fn a_high_shelf_brightens_highs_only() {
        let rate = 44100.0;
        let tone = |f: f32| -> Vec<f32> {
            (0..44100).map(|i| (2.0 * std::f32::consts::PI * f * i as f32 / rate).sin()).collect()
        };
        let level = |s: &[f32]| (s[22050..].iter().map(|x| x * x).sum::<f32>() / 22050.0).sqrt();
        let shelf = Biquad::shelf(true, 2500.0, 6.0, rate);
        let mut low = tone(200.0);
        shelf.run(&mut low);
        let mut high = tone(10000.0);
        shelf.run(&mut high);
        assert!((20.0 * (level(&low) / std::f32::consts::FRAC_1_SQRT_2).log10()).abs() < 0.5);
        assert!((20.0 * (level(&high) / std::f32::consts::FRAC_1_SQRT_2).log10() - 6.0).abs() < 0.7);
    }
}
