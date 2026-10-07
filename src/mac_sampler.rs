//! macOS's own sampler (AVAudioUnitSampler + AVAudioSequencer), the engine
//! iOS and macOS apps play SF2 files with.

use std::path::Path;

use crate::analysis::Rendered;

/// Renders `midi` with the SF2 at `font` through AVAudioUnitSampler.
pub fn render(_font: &Path, _midi: &[u8], _sample_rate: u32) -> Result<Rendered, String> {
    Err("The macOS sampler lane is not available yet.".into())
}
