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
6. Play the pattern in sync with Logic. You can ask Junglizer to vary each
   four-bar pass, freeze the one you like, and export its triggering events as
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
editable fill in the selected bar while preserving locked slices. “Evolve every
four bars” makes new fills while Logic plays; “Freeze this version” keeps the
exact currently displayed phrase and its bar choices. Freezing pauses editing
without changing individual slice locks. If an older export left slices locked,
choose **Unfreeze for editing**, then **Unlock all slices** beside the player.
The player shows a moving
playhead and highlights the active bar during playback. Markers and placements
are saved in plugin project state.

The global **Thin ↔ Thicc** control starts dry in the center. The left side
progressively high-passes the whole player. The right side reinforces bass-hit
markers with a synthesized, decaying 808 at the selected root in octave 1;
the far right also adds octave 0. Junglizer guesses bass-hit markers from the
break's low-frequency onsets. Select a source slice and use **Bass hit → tuned
808** to correct those guesses.

## MIDI in Logic

Click **Export four-bar MIDI to Finder**. Junglizer writes a `.mid` file under
`~/Music/Junglizer/Exports` and reveals it in Finder. Drag that file onto the
Junglizer track in Logic, then choose **Playback: MIDI region** in the plugin.
The MIDI region contains the four-bar slice notes, bar-source choices, slice
speed, bit depth, retriggers, length, player-slot count, Thin ↔ Thicc position,
and 808 root. The ghost break continues underneath empty slots. Editing source
markers after export can change the sound of the MIDI region, so freeze and
leave markers as they are when matching an exported phrase matters.

Junglizer exposes its performance and edit controls as Logic automation
parameters. Assign those parameters to CS12 controls using Logic's MIDI
controller assignment workflow. The available parameters include the four bar
sources, Thin ↔ Thicc, root, playback mode, source-slice count, player-slot
count, selected slot/break/slice, fill bar, slice length/speed/bit
depth/retriggers/lock, source mute, bass-hit tag, marker position, live
variations, and momentary Place Slice, Clear Slice, Suggest Fill, Lock Bar,
Freeze Variation, Unfreeze, and Unlock All Slices actions. For a momentary action,
send a low-to-high transition and then return it low before triggering again.

The exported file uses channel 1 notes `0..127` for break `0..3` × slice
`0..31`. CC `20` is speed, `21` bit depth, `22` retriggers, `23` length,
`24` player-slot count, `25` Thin ↔ Thicc, `26` root, and `30` the ghost
break for the current bar. These CCs are for reproducible MIDI playback;
CS12 mapping should target Logic's Junglizer parameters.

At host tempos other than 174 BPM, ghost playback changes speed and pitch to
stay on the host grid. The source loops are read from this project's `sample/`
directory at plugin startup. The included breaks are local development inputs;
`.gitignore` excludes them from source control. Anyone cloning the project must
supply authorized WAV files with the expected filenames before building. A
portable sample-loading flow is still needed. The current **Fit to length**
mode changes pitch with speed; pitch-preserving stretch is planned. MIDI export
uses Finder because the current editor does not support direct file dragging
from the plugin into Logic.

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
