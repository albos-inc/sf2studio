# Reference

[Guide](README.md) › Reference

## Keys

The keys work while no text field is being edited.

| Key | Tune | Compare |
|---|---|---|
| Space | Play / pause | Play / pause |
| 1 – 9 | Hear lane A – I | Turn lane A – I's output on / off |
| Shift + 1 – 9 | — | Hear only that lane |
| Tab | Hear the next lane | Hear the next lane alone |
| ← / → | Back / forward 5 s | Back / forward 5 s |
| Home | To the start | To the start |
| L | Clear the loop | Clear the loop |

During the blind test only Space works, so the keys can't reveal the lanes.

## Mouse and trackpad

| Where | Do this | To |
|---|---|---|
| Timeline | Click / drag | Move the playhead |
| Timeline | Shift-drag | Loop a range |
| Timeline | Pinch, Ctrl + scroll | Zoom around the pointer |
| Timeline | Scroll | Move along |
| Timeline | Double-click | Show everything, follow the playhead |
| Lane header | Click | Select the lane (▶) |
| Piano roll | Click a note | Loop that note and play |
| Waveform close-up | Scroll | Zoom ±1 – 500 ms |
| Output box | Shift-click | Hear only that lane |
| Keyboard | Press high / low on a key | Soft / loud |
| Keyboard | Drag across keys | Glissando |
| Panel edges | Drag | Resize the panels and the keyboard |

## Command line

```sh
sf2studio --sf2 piano.sf2 --mac piano.sf2 --mac-builtin --midi song.mid --mode compare
```

| Option | Does |
|---|---|
| `--sf2 <file>` | Adds an sf2synth lane playing the file (SF2, DLS or SFZ) |
| `--mac <file>` | Adds a macOS-sampler lane playing the file |
| `--mac-builtin` | Adds a macOS-sampler lane playing macOS's GS set |
| `--wav <file>` | Adds a WAV lane |
| `--midi <file>` | Plays the MIDI file |
| `--mode create\|tune\|compare` | Opens in that mode |
| `--create <file>` | Opens Create starting from that file (SF2, DLS or SFZ) |
| `--null` | Shows the null test (Compare) |
| `--theme light\|dark\|system` | Sets the theme |
| `--show help\|about` | Opens that window at launch |

Lane options add lanes in the order given and replace the saved lanes.

## Files

| Kind | Used for |
|---|---|
| SF2 (`.sf2`) | Instruments in lanes; the source and the result of Create |
| DLS (`.dls`) | Instruments in lanes (sf2synth reads DLS too); a source for Create. macOS and Windows each have a GS set built in. |
| SFZ (`.sfz`) with WAV or FLAC samples | Instruments in sf2synth lanes; a source for Create; what **Save SFZ…** writes (with FLAC samples). The samples are read into memory. |
| WAV (`.wav`) | Recording lanes; recordings for Create. 16/24/32-bit or float, mono or stereo, any sample rate (converted). |
| MIDI (`.mid`, `.midi`, `.kar`, `.rmi`) | What the lanes play |
| TOML (`.toml`) | sf2synth settings saved from Tune |

### The settings file (TOML)

**Save…** in Tune writes, for example:

```toml
master_gain_db = -6.0206
velocity_range_cb = 960.0
velocity_curve = "Concave"
velocity_filter_cents = -2400.0
initial_attenuation_scale = 0.4
cubic_interpolation = true
release_same_key = true
reverb_send = 0
reverb_room = 0.6
reverb_damping = 0.4
reverb_width = 1.0
reverb_level = 0.8
chorus_send = 0
chorus_level = 0.6
max_voices = 256
```

| Key | Slider | Meaning |
|---|---|---|
| `master_gain_db` | Master gain | Output gain, dB |
| `velocity_range_cb` | Range (cB) | Velocity → attenuation amount, centibels (10 cB = 1 dB) |
| `velocity_curve` | Curve | `Concave`, `Linear` or `Convex` |
| `velocity_filter_cents` | Soft notes darker | Velocity → filter cutoff at velocity 0, cents (0 = off) |
| `initial_attenuation_scale` | Attenuation scale | How the file's attenuation counts (0.4 E-mu, 1.0 literal) |
| `cubic_interpolation` | Cubic interpolation | `false` = linear |
| `release_same_key` | Re-strike releases the key | |
| `reverb_send`, `chorus_send` | Send (CC91 / CC93) | Sent to every channel before playing, 0–127 |
| `reverb_room`, `reverb_damping`, `reverb_level`, `chorus_level` | the sliders | 0–1 |
| `reverb_width` | — | Stereo width of the reverb, 0–1 |
| `max_voices` | — | Voices sounding at once (16–4096) |

Missing keys take their default, so a file may list only what it changes.

## What is saved between launches

Language, theme, mode, what is played, every lane (synth, sound, preset,
sf2synth settings), Match levels, the Show toggles, the keyboard settings
and Create's source and shaping settings. Not saved: the made instrument
(use Save SF2…), the playhead, loop and zoom, the blind-test score.

## Troubleshooting

| What you see | Why, and what to do |
|---|---|
| A red speaker icon with a message in the top bar | The audio output couldn't be opened. Check the output device, then restart sf2studio. The views still work. |
| “Choose an SF2 (or WAV) for a lane on the left to start.” | No lane has anything to play yet. Choose a Sound (or a WAV) for lane A. |
| A lane's figures are replaced by red text | The file couldn't be read or played; the text says why. Choose another file or preset. |
| Tune: the keyboard is silent, a yellow hint asks for an sf2synth lane | The lane you hear isn't sf2synth, or has no file. Select an sf2synth lane with a Sound. |
| Tune: “The lane you hear isn't sf2synth” | The settings are for sf2synth lanes only. |
| Compare: no charts, a hint about test patterns | Measurements need Velocity sweep, Long notes or the Chromatic scale. |
| Compare: the Output boxes are gone | The blind test is on. Close its window. |
| The output meter turns red | The output clips. Lower Master gain (Tune) or Volume (Create), or turn on Match levels. |
| A WAV lane doesn't line up | WAV lanes play from the file's start; trim the recording so its first note starts with the MIDI file's. |
| The MIDI keyboard isn't listed | Connect it, then open the menu again: the list is read each time it opens. |
| “Layers” has no effect in Create | Layers are only made when one sample covers every velocity; the source has velocity layers of its own. |
| The instrument from Create is gone after restarting | It isn't kept; Save SF2… before quitting. |
| macOS sampler isn't offered | It is part of macOS; on Windows, compare with recordings instead. |
| A lane says “N left out on loading” | The SFZ uses opcodes sf2synth doesn't follow, or some of its samples couldn't be read; hover over it for the list. |
| A macOS-sampler lane can't play an SFZ | The macOS sampler reads SF2 and DLS only; use sf2synth for SFZ. |
| The null test says the lanes differ though they should be the same | Make sure **Match levels first** is off and both lanes use the same synthesizer and settings. **First difference at** shows where they part. |

## Glossary

| Term | Meaning |
|---|---|
| SF2 (SoundFont 2) | A file of samples and the rules for playing them as instruments |
| DLS | Downloadable Sounds, a similar format; the systems' GS sets are DLS |
| SFZ | A text file of regions (keys, velocities, settings) playing WAV or FLAC samples beside it |
| FLAC | Lossless audio compression: the samples exactly, at about a half to a sixth of the size |
| Null test | Subtracting one rendering from another: nothing is left when they are identical |
| GS set | The General MIDI instruments built into macOS and Windows (Roland GS) |
| Preset · bank · program | An instrument in an SF2, numbered `bank:program` |
| Velocity | How hard a note is played, 1–127 |
| cent · cB | A hundredth of a semitone · a tenth of a decibel |
| Partial · harmonic | The frequencies a note is made of; harmonics are whole multiples of the fundamental |
| Inharmonicity | Partials sharper than whole multiples — the stiffness of a piano string |
| Spectral centroid | The average frequency weighted by level: a measure of brightness |
| RMS level | The average power of the sound, in dB |
| Lane | One synth + sound + preset (or a recording), drawn and heard as a row |
| ABX | A blind test: is the unknown X the same as A or as B? |
