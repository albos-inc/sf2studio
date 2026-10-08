//! macOS's own sampler (AVAudioUnitSampler + AVAudioSequencer), the engine
//! iOS and macOS apps play SF2 files with, rendered offline.

use std::path::Path;

use crate::analysis::Rendered;

/// Seconds rendered after the program's end, for the releases.
#[cfg(target_os = "macos")]
const TAIL_SECONDS: f64 = 3.0;

/// Renders `midi` with the SF2 at `font` through AVAudioUnitSampler.
#[cfg(target_os = "macos")]
pub fn render(font: &Path, midi: &[u8], sample_rate: u32) -> Result<Rendered, String> {
    use objc2::rc::Retained;
    use objc2::{AnyThread, msg_send};
    use objc2_avf_audio::{
        AVAudioEngine, AVAudioEngineManualRenderingMode, AVAudioFormat, AVAudioPCMBuffer, AVAudioSequencer,
        AVAudioUnit, AVAudioUnitSampler, AVMusicSequenceLoadOptions,
    };
    use objc2_foundation::{NSData, NSError, NSInteger, NSURL};

    fn describe(error: Retained<NSError>) -> String {
        error.localizedDescription().to_string()
    }

    let duration = sf2synth::MidiFile::from_bytes(midi).map_err(|e| e.to_string())?.duration();
    let total = ((duration + TAIL_SECONDS) * sample_rate as f64) as usize;
    let url = NSURL::from_file_path(font).ok_or("bad SF2 path")?;
    const BLOCK: u32 = 4096;

    // SAFETY: plain AVFoundation calls on objects this function owns; the
    // buffer's channel pointers are read only within its frame length.
    unsafe {
        let engine = AVAudioEngine::new();
        let format =
            AVAudioFormat::initStandardFormatWithSampleRate_channels(AVAudioFormat::alloc(), sample_rate as f64, 2)
                .ok_or("no audio format")?;
        engine
            .enableManualRenderingMode_format_maximumFrameCount_error(
                AVAudioEngineManualRenderingMode::Offline,
                &format,
                BLOCK,
            )
            .map_err(describe)?;
        let sampler = AVAudioUnitSampler::new();
        engine.attachNode(&sampler);
        engine.connect_to_format(&sampler, &engine.mainMixerNode(), None);
        sampler.loadSoundBankInstrumentAtURL_program_bankMSB_bankLSB_error(&url, 0, 0x79, 0).map_err(describe)?;

        let sequencer = AVAudioSequencer::initWithAudioEngine(AVAudioSequencer::alloc(), &engine);
        sequencer
            .loadFromData_options_error(&NSData::with_bytes(midi), AVMusicSequenceLoadOptions::SMF_PreserveTracks)
            .map_err(describe)?;
        let unit: &AVAudioUnit = &sampler;
        for track in sequencer.tracks().iter() {
            track.setDestinationAudioUnit(Some(unit));
        }
        engine.startAndReturnError().map_err(describe)?;
        sequencer.prepareToPlay();
        sequencer.startAndReturnError().map_err(describe)?;

        let buffer = AVAudioPCMBuffer::initWithPCMFormat_frameCapacity(AVAudioPCMBuffer::alloc(), &format, BLOCK)
            .ok_or("no buffer")?;
        let mut left = Vec::with_capacity(total);
        let mut right = Vec::with_capacity(total);
        while left.len() < total {
            let frames = BLOCK.min((total - left.len()) as u32);
            let mut error: Option<Retained<NSError>> = None;
            let status: NSInteger = msg_send![&engine, renderOffline: frames, toBuffer: &*buffer, error: &mut error];
            if status != 0 {
                sequencer.stop();
                engine.stop();
                return Err(error.map(describe).unwrap_or_else(|| format!("render status {status}")));
            }
            let length = buffer.frameLength() as usize;
            if length == 0 {
                break;
            }
            let channels = buffer.floatChannelData();
            left.extend_from_slice(std::slice::from_raw_parts((*channels).as_ptr(), length));
            right.extend_from_slice(std::slice::from_raw_parts((*channels.add(1)).as_ptr(), length));
        }
        sequencer.stop();
        engine.stop();
        Ok(Rendered { sample_rate, left, right })
    }
}

#[cfg(not(target_os = "macos"))]
pub fn render(_font: &Path, _midi: &[u8], _sample_rate: u32) -> Result<Rendered, String> {
    Err("The macOS sampler is only available on macOS.".into())
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    /// Needs an SF2: `SF2STUDIO_TEST_SF2=piano.sf2 cargo test`.
    #[test]
    fn renders_a_note_through_the_macos_sampler() {
        let Ok(font) = std::env::var("SF2STUDIO_TEST_SF2") else { return };
        let midi = crate::patterns::single_note(60, 100, 1.0);
        let audio = super::render(std::path::Path::new(&font), &midi, 48_000).unwrap();
        assert!(audio.duration() > 3.5);
        let peak = audio.left.iter().fold(0.0f32, |m, &x| m.max(x.abs()));
        assert!(peak > 0.01, "silent: peak {peak}");
    }
}
