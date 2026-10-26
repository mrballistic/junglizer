# Junglizer

Junglizer is an early Truce audio-plugin prototype for rearranging four-beat
break loops into a four-bar, 16-beat pattern. It is being built as an instrument
for Logic Pro, with VST3 and CLAP builds from the same Rust project.

## Intended workflow

1. Add several four-beat loops to the source bank.
2. Choose one shared source slice count from 1 to 32. Each loop has its own
   hand-placed marker set for that count. Empty or quiet source slots are valid.
3. Pick a source loop for each of the four bars. This is the ghost arrangement:
   it plays the full loops in their original order unless replaced.
4. Choose 1 to 32 equally spaced player slots across the four bars. Drag source
   slices over one or more slots to replace the ghost for that span.
5. Choose original-speed, pitch-changing varispeed, or pitch-preserving stretch
   per placement, with optional bit crushing.
6. Play the pattern in sync with the host, or export its triggering events as
   MIDI for the Logic timeline.

## Current state

The first audible AU prototype plays the four supplied 174 BPM, one-bar stereo
breaks as a host-synced, four-bar ghost arrangement. The `Bar 1` through `Bar 4`
controls select a loop for each bar; each can choose any of the four loops. The
default order is 1, 2, 3, 4. The source loops are read from this project's
`sample/` directory when the plugin starts. At host tempos other than 174 BPM,
this prototype changes playback speed and pitch to keep the loops on the host
grid. The player is silent while host transport is stopped.

The pure pattern model also handles independent source/player counts, per-loop
manual marker sets, overlapping slice replacements, and ghost resumption.
Those editing features are not connected to the audio engine or editor yet.
The waveform/player editor, MIDI control and export, pitch-preserving stretch,
bitcrushing, and pattern state persistence remain to be built. The included
breaks are local development inputs; `.gitignore` excludes them from source
control. A portable sample-loading and project-state flow is still needed.

## Planned global Thin to Thicc control

A single left-to-right control will affect the entire four-bar player and be
available to host automation and MIDI control:

- **Thin side:** progressively high-pass the player's output.
- **Center:** dry player, with neither high-pass nor added 808.
- **Thicc side:** layer an 808 on bass hits, with the 808 tuned to the track's
  chosen root note (for example, C1 in a C-key track).
- **Extra Thicc end:** add the note one octave below those same hits (C0 in
  that example).

Bass-hit positions should be user-editable source metadata. The trigger plan
will follow the selected ghost loop or a placed source slice, so rearranging a
bar rearranges its 808 accents too. Exact trigger detection, 808 sound design,
and the control curve are still design work; this section records the intended
behavior, not a working feature.

## Design references

Nick Collins's *Algorithmic Composition Methods for Breakbeat Science*
(`docs/acmethodsforbbsci.pdf`) separates decisions about cut positions and
repetitions from audio rendering. It also discusses manually constrained source
offsets, short repeated cuts, and phrase-ending rolls. This informs the model's
separation of source slices, player placements, and audio playback. Livecut
([repository](https://github.com/mdsp/Livecut)) is a related live beat slicer
influenced by BBCut. Its phrase, block, and fill structure is useful inspiration
for a later optional suggestion engine. Junglizer currently prioritizes hand
placement and deterministic playback; it does not use Livecut code.

## Local development

Requires Rust 1.92 or newer, the `cargo-truce` CLI, and Xcode command-line tools
on macOS. Full Xcode is needed later if an AUv3 build is desired.

```sh
cargo test
cargo truce build --au2
```

Build from this project folder because the current AU opens the WAVs from its
local `sample/` directory. `cargo truce build` stages bundles in
`target/bundles/` without installing them.
The initial Logic target is AUv2. Truce metadata and format IDs are in
`truce.toml`; they are prototype identities and should be finalized before a
public release.
