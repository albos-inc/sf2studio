//! Playing the rendered lanes, switching between them without a click.

use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample};

use crate::analysis::Rendered;

/// Frames a lane switch crossfades over (about 10 ms).
const FADE_FRAMES: u32 = 480;

pub struct LaneAudio {
    pub audio: Arc<Rendered>,
    /// Level-matching gain.
    pub gain: f32,
}

#[derive(Default)]
pub struct Transport {
    pub lanes: Vec<Option<LaneAudio>>,
    pub playing: bool,
    /// Frame being played.
    pub position: usize,
    /// Lane heard.
    pub current: usize,
    previous: usize,
    fade: u32,
    /// Frames played in a loop, if any.
    pub loop_range: Option<(usize, usize)>,
    /// Output level of the last buffer (peak), for the meter.
    pub meter: f32,
}

impl Transport {
    pub fn select(&mut self, lane: usize) {
        if lane != self.current {
            self.previous = self.current;
            self.current = lane;
            self.fade = FADE_FRAMES;
        }
    }

    /// Frames of the longest lane.
    pub fn length(&self) -> usize {
        self.lanes.iter().flatten().map(|l| l.audio.frames()).max().unwrap_or(0)
    }

    fn frame(&self, lane: usize, position: usize) -> (f32, f32) {
        match self.lanes.get(lane).and_then(|l| l.as_ref()) {
            Some(l) if position < l.audio.frames() => {
                (l.audio.left[position] * l.gain, l.audio.right[position] * l.gain)
            }
            _ => (0.0, 0.0),
        }
    }

    fn next(&mut self) -> (f32, f32) {
        if !self.playing {
            return (0.0, 0.0);
        }
        let (mut left, mut right) = self.frame(self.current, self.position);
        if self.fade > 0 {
            let w = self.fade as f32 / FADE_FRAMES as f32;
            let (pl, pr) = self.frame(self.previous, self.position);
            left = left * (1.0 - w) + pl * w;
            right = right * (1.0 - w) + pr * w;
            self.fade -= 1;
        }
        self.position += 1;
        match self.loop_range {
            Some((start, end)) if self.position >= end && end > start => self.position = start,
            _ => {
                if self.position >= self.length() {
                    self.playing = false;
                    self.position = 0;
                }
            }
        }
        (left, right)
    }
}

/// The output stream.
pub struct Player {
    pub transport: Arc<Mutex<Transport>>,
    pub sample_rate: u32,
    _stream: cpal::Stream,
}

impl Player {
    pub fn new() -> Result<Player, String> {
        let host = cpal::default_host();
        let device = host.default_output_device().ok_or("no audio output device")?;
        let config = device.default_output_config().map_err(|e| e.to_string())?;
        let sample_rate = config.sample_rate();
        let transport = Arc::new(Mutex::new(Transport::default()));
        let stream = match config.sample_format() {
            SampleFormat::F32 => build::<f32>(&device, config.into(), Arc::clone(&transport)),
            SampleFormat::I16 => build::<i16>(&device, config.into(), Arc::clone(&transport)),
            SampleFormat::I32 => build::<i32>(&device, config.into(), Arc::clone(&transport)),
            SampleFormat::U16 => build::<u16>(&device, config.into(), Arc::clone(&transport)),
            format => Err(format!("unsupported sample format {format}")),
        }?;
        stream.play().map_err(|e| e.to_string())?;
        Ok(Player { transport, sample_rate, _stream: stream })
    }
}

fn build<T>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    transport: Arc<Mutex<Transport>>,
) -> Result<cpal::Stream, String>
where
    T: SizedSample + FromSample<f32>,
{
    let channels = config.channels as usize;
    device
        .build_output_stream(
            config,
            move |data: &mut [T], _| {
                // Never wait for the UI: if it holds the lock, play silence.
                let Ok(mut transport) = transport.try_lock() else {
                    data.iter_mut().for_each(|s| *s = T::from_sample(0.0));
                    return;
                };
                let mut meter = 0.0f32;
                for frame in data.chunks_mut(channels) {
                    let (left, right) = transport.next();
                    meter = meter.max(left.abs()).max(right.abs());
                    for (i, sample) in frame.iter_mut().enumerate() {
                        let value = match i {
                            0 => left,
                            1 => right,
                            _ => 0.0,
                        };
                        *sample = T::from_sample(value.clamp(-1.0, 1.0));
                    }
                }
                transport.meter = meter;
            },
            |error| eprintln!("audio stream error: {error}"),
            None,
        )
        .map_err(|e| e.to_string())
}
