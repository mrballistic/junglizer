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
5. Choose original-speed or pitch-changing varispeed per placement, with
   optional bit crushing and retriggers. Pitch-preserving stretch is planned.
6. Play the pattern in sync with the host, or export its triggering events as
   MIDI for the Logic timeline.

## Current state

The AU plays the four supplied 174 BPM, one-bar stereo breaks as a host-synced,
four-bar ghost arrangement. `Bar 1` through `Bar 4` choose the source loop for
each bar. The editor shows a source waveform, hand-adjustable slice markers,
and a four-bar player with the full ghost waveform behind its evenly spaced
slots. Click a source slice to audition it, then click an empty player slot or
drag the slice into the player. Right-click a source slice to mark it quiet or
active. Placed slices replace the ghost for their span and can play at their
original speed or fit the span by changing speed and pitch. Each placement has
bit depth, retrigger, and lock controls. “Suggest / try another” makes an
editable fill in the selected bar while preserving locked slices. Markers and
placements are saved in plugin project state.

At host tempos other than 174 BPM, ghost playback changes speed and pitch to
stay on the host grid. The source loops are read from this project's `sample/`
directory at plugin startup. The included breaks are local development inputs;
`.gitignore` excludes them from source control. Anyone cloning the project must
supply authorized WAV files with the expected filenames before building. A
portable sample-loading flow is still needed. MIDI control and drag-to-Logic
MIDI export, automatic live variations, pitch-preserving stretch, and the
global Thin to Thicc control are planned, not yet implemented.

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
influenced by BBCut. Its phrase, block, and fill structure inspired the first
editable fill suggestion. Junglizer uses its own code and renders each accepted
suggestion deterministically.

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
