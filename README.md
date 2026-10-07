# sf2studio

Make, tune and compare SF2 instruments — a desktop app for macOS and
Windows built on [sf2synth](https://github.com/albos-inc/sf2synth).

| Mode | What it does |
|---|---|
| **Create** | *(in progress)* Build a new SF2 from an existing SF2, recordings or synthesis: brightness, hardness, sustain, velocity layers, tuning. |
| **Tune** | Play a song or a test pattern with sf2synth and an SF2, watch its spectrogram and levels, and adjust the synthesizer as you listen. Save the settings as TOML. |
| **Compare** | Put up to nine lanes side by side — sf2synth with any SF2, the macOS sampler, or a WAV recording — and switch between them while playing, sample-aligned and level-matched. |

Every lane shows a spectrogram, its level in dB and a close-up of the
waveform at the playhead.

## Keys

| Key | |
|---|---|
| Space | Play / pause |
| 1–9 | Hear lane 1–9 |
| Tab | Hear the next lane (A ⇄ B) |
| ← / → | Back / forward 5 s |
| Home | To the start |
| Shift-drag | Loop a range |
| L | Clear the loop |
| Pinch or Ctrl-scroll | Zoom the timeline; scroll to pan; double-click to see it all |

## Build

```sh
cargo run --release
```

Lanes can also be given on the command line:

```sh
cargo run --release -- --sf2 piano.sf2 --wav reference.wav --midi song.mid --mode compare
```

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.

"SoundFont" is a registered trademark of Creative Technology Ltd.
