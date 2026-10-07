//! Rendered audio and what is drawn from it.

use realfft::RealFftPlanner;

/// Stereo audio at the output sample rate.
pub struct Rendered {
    pub sample_rate: u32,
    pub left: Vec<f32>,
    pub right: Vec<f32>,
}

impl Rendered {
    pub fn frames(&self) -> usize {
        self.left.len()
    }

    pub fn duration(&self) -> f64 {
        self.frames() as f64 / self.sample_rate as f64
    }
}

/// Frequencies the spectrogram spans (log-spaced rows).
pub const SPECTRUM_LOW_HZ: f32 = 27.5;
pub const SPECTRUM_HIGH_HZ: f32 = 18_000.0;
pub const SPECTRUM_ROWS: usize = 200;
const FFT_SIZE: usize = 4096;
/// The most spectrogram columns (textures can't be arbitrarily wide).
const MAX_COLUMNS: usize = 8000;
/// Levels below this are silence.
pub const FLOOR_DB: f32 = -100.0;

pub struct Analysis {
    /// Frames per column.
    pub hop: usize,
    pub columns: usize,
    /// RMS level of each column, dB.
    pub envelope_db: Vec<f32>,
    /// `SPECTRUM_ROWS` levels per column (low to high), dB.
    pub spectrogram_db: Vec<f32>,
    /// Spectral centroid of each column, Hz (0 when silent).
    pub centroid_hz: Vec<f32>,
    /// RMS over the sounding parts, linear.
    pub loudness: f32,
    pub peak: f32,
}

impl Analysis {
    pub fn new(audio: &Rendered) -> Analysis {
        let frames = audio.frames();
        let rate = audio.sample_rate as f32;
        let hop = (audio.sample_rate as usize / 100).max(frames.div_ceil(MAX_COLUMNS)).max(1);
        let columns = frames.div_ceil(hop).max(1);

        let mono: Vec<f32> = audio.left.iter().zip(&audio.right).map(|(l, r)| 0.5 * (l + r)).collect();
        let peak = audio.left.iter().chain(&audio.right).fold(0.0f32, |m, &x| m.max(x.abs()));

        let mut planner = RealFftPlanner::<f32>::new();
        let fft = planner.plan_fft_forward(FFT_SIZE);
        let window: Vec<f32> = (0..FFT_SIZE)
            .map(|i| 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / FFT_SIZE as f32).cos())
            .collect();
        let window_gain: f32 = window.iter().sum::<f32>() / 2.0;
        let mut input = fft.make_input_vec();
        let mut spectrum = fft.make_output_vec();
        let bin_hz = rate / FFT_SIZE as f32;

        // Row edges in FFT bins.
        let edges: Vec<f32> = (0..=SPECTRUM_ROWS)
            .map(|r| {
                let t = r as f32 / SPECTRUM_ROWS as f32;
                SPECTRUM_LOW_HZ * (SPECTRUM_HIGH_HZ / SPECTRUM_LOW_HZ).powf(t) / bin_hz
            })
            .collect();

        let mut envelope_db = Vec::with_capacity(columns);
        let mut centroid_hz = Vec::with_capacity(columns);
        let mut spectrogram_db = Vec::with_capacity(columns * SPECTRUM_ROWS);
        let mut magnitudes = vec![0.0f32; FFT_SIZE / 2 + 1];
        let mut loud_sum = 0.0f64;
        let mut loud_count = 0usize;

        for column in 0..columns {
            let start = column * hop;
            let end = (start + hop).min(frames);
            let sum: f64 = mono[start..end].iter().map(|&x| (x as f64) * (x as f64)).sum();
            let rms = (sum / (end - start).max(1) as f64).sqrt() as f32;
            let level = to_db(rms);
            envelope_db.push(level);
            if level > -60.0 {
                loud_sum += sum;
                loud_count += end - start;
            }

            // FFT window centred on the column.
            let centre = start + hop / 2;
            for (i, value) in input.iter_mut().enumerate() {
                let at = centre as isize + i as isize - FFT_SIZE as isize / 2;
                *value = if at >= 0 && (at as usize) < frames { mono[at as usize] * window[i] } else { 0.0 };
            }
            if fft.process(&mut input, &mut spectrum).is_err() {
                spectrum.iter_mut().for_each(|c| *c = Default::default());
            }
            let mut weighted = 0.0f32;
            let mut total = 0.0f32;
            for (bin, c) in spectrum.iter().enumerate() {
                let m = c.norm() / window_gain;
                magnitudes[bin] = m;
                weighted += m * bin as f32 * bin_hz;
                total += m;
            }
            centroid_hz.push(if level > -70.0 && total > 0.0 { weighted / total } else { 0.0 });
            for row in 0..SPECTRUM_ROWS {
                let (low, high) = (edges[row], edges[row + 1]);
                let value = if high - low < 1.0 {
                    // Narrower than a bin: interpolate.
                    let position = (low + high) / 2.0;
                    let i = position.floor() as usize;
                    let f = position - i as f32;
                    let a = magnitudes.get(i).copied().unwrap_or(0.0);
                    let b = magnitudes.get(i + 1).copied().unwrap_or(0.0);
                    a + (b - a) * f
                } else {
                    let from = low.ceil() as usize;
                    let to = (high.floor() as usize).min(magnitudes.len() - 1);
                    magnitudes[from.min(to)..=to].iter().fold(0.0f32, |m, &x| m.max(x))
                };
                spectrogram_db.push(to_db(value));
            }
        }

        let loudness = if loud_count > 0 { (loud_sum / loud_count as f64).sqrt() as f32 } else { 0.0 };
        Analysis { hop, columns, envelope_db, spectrogram_db, centroid_hz, loudness, peak }
    }

    /// The median spectral centroid of the sounding columns: how bright the
    /// lane sounds overall.
    pub fn brightness_hz(&self) -> f32 {
        let mut sounding: Vec<f32> = self.centroid_hz.iter().copied().filter(|&c| c > 0.0).collect();
        if sounding.is_empty() {
            return 0.0;
        }
        sounding.sort_by(f32::total_cmp);
        sounding[sounding.len() / 2]
    }

    /// The column at `seconds`.
    pub fn column_at(&self, seconds: f64, sample_rate: u32) -> usize {
        ((seconds * sample_rate as f64) as usize / self.hop).min(self.columns - 1)
    }
}

pub fn to_db(linear: f32) -> f32 {
    if linear <= 1e-5 { FLOOR_DB } else { (20.0 * linear.log10()).max(FLOOR_DB) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sine_lights_its_row_and_has_its_level() {
        let rate = 48000;
        let samples: Vec<f32> =
            (0..rate).map(|i| 0.5 * (2.0 * std::f32::consts::PI * 1000.0 * i as f32 / rate as f32).sin()).collect();
        let audio = Rendered { sample_rate: rate, left: samples.clone(), right: samples };
        let analysis = Analysis::new(&audio);
        let column = analysis.columns / 2;
        let level = analysis.envelope_db[column];
        assert!((level - to_db(0.5 / 2f32.sqrt())).abs() < 0.2, "{level}");
        let rows = &analysis.spectrogram_db[column * SPECTRUM_ROWS..(column + 1) * SPECTRUM_ROWS];
        let loudest = rows.iter().enumerate().fold((0, f32::MIN), |m, (i, &v)| if v > m.1 { (i, v) } else { m }).0;
        let t = loudest as f32 / SPECTRUM_ROWS as f32;
        let frequency = SPECTRUM_LOW_HZ * (SPECTRUM_HIGH_HZ / SPECTRUM_LOW_HZ).powf(t);
        assert!((frequency / 1000.0 - 1.0).abs() < 0.05, "{frequency}");
        assert!((analysis.centroid_hz[column] - 1000.0).abs() < 50.0);
    }
}
