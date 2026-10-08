<p align="center"><img src="assets/icon-256.png" width="128" alt="sf2studio"></p>

# sf2studio

Make, tune and compare SF2 instruments — a desktop app for macOS and
Windows built on [sf2synth](https://github.com/albos-inc/sf2synth).

| Mode | What it does |
|---|---|
| **Create** | Make a new SF2 from a preset of an existing SF2, one recording per note, or the built-in piano-like synthesis. Shape volume, brightness and warmth, touch (velocity range, curve, synthesized velocity layers), attack, fade and release, stretch tuning, stereo width and reverb; play it on the keyboard as it is made, save it, or send it to Compare. |
| **Tune** | Play the 88-key keyboard (or a MIDI keyboard) with sf2synth and an SF2: a key sounds at once, and when you release it the note is rendered in every lane with its spectrogram, harmonics and level. Press high on a key for soft, low for loud, or fix the velocity and length. Adjust the synthesizer as you listen, play a song or a test pattern, and save the settings as TOML. |
| **Compare** | Put up to nine lanes side by side — sf2synth with any SF2, the macOS sampler, or a WAV recording. Every lane is heard by default; turn each one's output on and off while playing (sample-aligned, level-matched, click-free), or solo one. |

Every lane shows a spectrogram, its level in dB and a close-up of the
waveform at the playhead, with a piano roll of the notes above; each view
can be turned on and off. Compare adds measurement charts (velocity against
level and brightness, decay per key) and an ABX blind test.

Light, dark or system theme; English and Japanese; Help, update check and
About in the Help menu.

## Guide

An illustrated guide to every screen and setting, how to read the views and
step-by-step recipes: **[English](docs/en/README.md)** ·
**[日本語](docs/ja/README.md)**. It also opens from Help → Open the
illustrated guide.

## Keys

| Key | |
|---|---|
| Space | Play / pause |
| 1–9 | Tune: hear lane 1–9 · Compare: turn lane 1–9's output on/off |
| Shift+1–9 | Compare: hear only lane 1–9 |
| Tab | Hear the next lane alone (A ⇄ B) |
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
cargo run --release -- --sf2 piano.sf2 --mac piano.sf2 --mac-builtin --midi song.mid --mode compare
```

`--sf2`, `--mac` (macOS sampler), `--mac-builtin` (macOS's GS set) and
`--wav` add lanes in order; `--create <sf2>` opens Create on a file;
`--theme light|dark|system`.

`docs/tools/capture.sh` takes the guide's screenshots again (macOS:
`cargo run --features capture -- --capture docs/images`), and
`python3 docs/tools/diagrams.py` redraws its diagrams.

## Icon

The icon is drawn in `assets/icon.svg`. `cargo run --example make_icon`
renders it into the PNGs, the window icon, `sf2studio.ico` (embedded in the
Windows executable) and, on macOS, `sf2studio.icns`.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.

"SoundFont" is a registered trademark of Creative Technology Ltd.
