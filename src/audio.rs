//! Playing the rendered lanes, turning each on and off without a click.

use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample};

use crate::analysis::Rendered;

/// Frames a lane fades in or out over when turned on or off (about 10 ms).
const FADE_FRAMES: f32 = 480.0;

pub struct LaneAudio {
    pub audio: Arc<Rendered>,
    /// Level-matching gain.
    pub gain: f32,
}

#[derive(Default)]
pub struct Transport {
    lanes: Vec<Option<LaneAudio>>,
    /// Which lanes are heard.
    outputs: Vec<bool>,
    /// Each lane's fade level, moving towards 1 (heard) or 0.
    levels: Vec<f32>,
    length: usize,
    pub playing: bool,
    /// Frame being played.
    pub position: usize,
    /// Frames played in a loop, if any.
    pub loop_range: Option<(usize, usize)>,
    /// Output level of the last buffer (peak), for the meter.
    pub meter: f32,
}

impl Transport {
    pub fn set_lanes(&mut self, lanes: Vec<Option<LaneAudio>>) {
        self.length = lanes.iter().flatten().map(|l| l.audio.frames()).max().unwrap_or(0);
        self.levels.resize(lanes.len(), 0.0);
        self.outputs.resize(lanes.len(), false);
        self.lanes = lanes;
        if self.position >= self.length {
            self.position = 0;
            self.playing = false;
        }
    }

    /// Sets which lanes are heard (they fade in and out).
    pub fn set_outputs(&mut self, outputs: &[bool]) {
        self.outputs.clear();
        self.outputs.extend_from_slice(outputs);
        self.outputs.resize(self.lanes.len(), false);
        self.levels.resize(self.lanes.len(), 0.0);
        if !self.playing {
            // Nothing to fade while stopped.
            for (level, &on) in self.levels.iter_mut().zip(&self.outputs) {
                *level = if on { 1.0 } else { 0.0 };
            }
        }
    }

    /// Frames of the longest lane.
    pub fn length(&self) -> usize {
        self.length
    }

    fn next(&mut self) -> (f32, f32) {
        if !self.playing {
            return (0.0, 0.0);
        }
        let (mut left, mut right) = (0.0, 0.0);
        let position = self.position;
        for (i, lane) in self.lanes.iter().enumerate() {
            let target = if self.outputs[i] { 1.0 } else { 0.0 };
            let level = &mut self.levels[i];
            if *level != target {
                *level = if target > *level {
                    (*level + 1.0 / FADE_FRAMES).min(1.0)
                } else {
                    (*level - 1.0 / FADE_FRAMES).max(0.0)
                };
            }
            if *level > 0.0 {
                if let Some(lane) = lane {
                    if position < lane.audio.frames() {
                        let g = lane.gain * *level;
                        left += lane.audio.left[position] * g;
                        right += lane.audio.right[position] * g;
                    }
                }
            }
        }
        self.position += 1;
        match self.loop_range {
            Some((start, end)) if self.position >= end && end > start => self.position = start,
            _ => {
                if self.position >= self.length {
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
