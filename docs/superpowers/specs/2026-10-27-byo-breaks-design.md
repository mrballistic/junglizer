# Junglizer: bring your own breaks, a built-in amen we own, and Windows and Linux

Status: design, for Todd's approval. Date: 2026-10-27. Ships as **0.9.1 beta 2** (Todd).

## Why

Junglizer 0.9 beta 1 embeds four Amen cuts from KAN Samples' *Amen Break Tribute*, a
subscriber-exclusive pack. Its license allows using the samples in your own music, but forbids
distributing the files or sharing them with anyone who didn't get the pack (clauses 3 and 5),
except with KAN's written consent. A free plugin that ships those four files to everyone falls
outside that, and so does a public repo with them in it.

This update removes the KAN audio from Junglizer entirely. In its place:

1. **A built-in amen we own.** It's programmed from scratch to match the KAN cuts hit for hit,
   rendered from a freely licensed acoustic kit, so we can redistribute it.
2. **Bring your own break.** Producers load their own WAV, AIFF or MP3, which is also what most
   of them want.
3. **Windows and Linux,** as VST3 and CLAP, alongside macOS.

Asking KAN for consent stays possible (a draft email exists), but this release must not depend
on it.

## Goals

- No KAN audio in the plugin, its releases or the repository's working tree.
- A built-in four-bar amen that feels like the original: natural, not reinforced, with the
  snare crack that carries the chop, and a lean low end because Junglizer's 808 supplies the bass.
- Loading your own break is obvious on first launch and easy afterwards.
- The same features on macOS, Windows and Linux.

## Non-goals

- Changing the slicing, placement, fills, Thin/Thicc, MIDI export or automation behavior, beyond
  what longer sources require (see Engine).
- Time-stretching or pitch-preserving playback. That's already on the roadmap and separate.
- Copying loaded breaks into the project. They are linked, the way Logic links audio.
- Making the repository public. That needs its own step, including removing the KAN WAVs from
  git history. Note it, but don't do it here.
- A Linux or Windows installer. The release ships zips, as today.
- A user-facing standalone app (tempo and play controls, audio export, packaged apps). Todd: that's
  beta 3. Leave the existing developer standalone build as it is.
- An automatic “rate us” or onboarding tour. The splash below is the only first-run guidance.

## Anti-goals (failure even if it works)

- Any KAN audio, processed or not, in a shipped file. The spike's `LOCAL_ONLY_…` comparison file
  contains reference audio and must never be committed or shipped.
- A built-in break that sounds compressed, layered or sub-heavy.
- A first-run screen that blocks the instrument or nags.
- Losing a user's slices and placements because a linked file moved.

## The built-in amen

**Decided (Todd):** program the amen ourselves (option 2 of three looked at), from a freely
licensed kit, matched hit for hit to the KAN cuts. Analyze their timing and dynamics only.

- **Kit:** Salamander Drumkit by Alexander Holm, CC BY-SA 3.0, overhead-mic one-shots. Todd
  judged it very close to the original's sound.
- **Source of truth:** the spike in `tools/amen/clone_amen.py`, its native-tempo transcription
  `target/amen-spike/transcription_native.json`, and render **out4b**: kick, not tom, on the hit
  after beat 3 of bar 4 (Todd chose b). Rendering:
  - **Kit choices:** each hit's Salamander dynamic layer and gain come from the measured level,
    and round-robin sample choice uses a fixed seed.
  - **One take:** a single continuous four-bar render, so tails ring across the bar lines, then
    cut into four bars.
  - **Bar 3's crash:** cut as in the original. 0 dB to step 5.80, −13 dB by 5.89, held to 6.22,
    then a fade to −30 dB by step 7.0.
  - **EQ, with no compression or limiting:**
    - high-pass at 85 Hz, 24 dB/oct
    - low shelf at 60 Hz, −20 dB
    - peaks at 100 Hz +10.8 dB (Q 1), 300 Hz +7 dB and 4.5 kHz +9.3 dB
    - high shelf at 14 kHz, −3 dB
    - +6 dB at 4 kHz on each snare hit
  - **Output:** −1 dBFS peak; stereo, 24-bit, 44.1 kHz; 60828 frames per bar (one bar at 174 BPM).
- **What ships (my call):** commit the approved out4/b WAVs from `target/amen-spike/out4/b/` exactly,
  since those are the ones Todd listened to and chose. The renderer only has to reproduce them closely.
  Bake the final per-hit timing corrections into the committed transcription so a rerun lands
  within a fraction of a millisecond. Any fresh render that replaces the shipped WAVs needs a new
  listen from Todd first.
- **Productionizing the spike** (builder's call on the details):
  - Keep the renderer in the repo, deterministic, and rerunnable from a pinned Salamander download.
  - The KAN cuts are only an input to transcription, which is already done. The committed
    transcription JSON is what the renderer reads, so neither the KAN files nor the Salamander
    archive need to be in the repo.
  - Commit the four rendered WAVs as `sample/174_JunglizerAmen_Bar_0{1..4}.wav`, with a short
    `sample/LICENSE.md`: CC BY-SA 3.0, the Salamander credit, and the fact that the pattern was
    programmed from scratch.
  - Delete the KAN WAVs from the working tree.
- **Credit:** “Built-in breaks played on the Salamander Drumkit by Alexander Holm (CC BY-SA 3.0)”
  - in the plugin, wherever an about or credits line fits (builder's call)
  - in the README
  - on getjunglizer.com, next to the existing credit for Nick Collins's paper
- **Licensing note:** the four WAVs are CC BY-SA 3.0, so share-alike covers that sample set, not
  Junglizer's code. Say so in `sample/LICENSE.md`.

## Loading your own break

**Slots.** Junglizer keeps four break slots. A slot holds a break or is blank.

**Fresh instance.** A new instance always starts with the built-in amen in all four slots, so
there's never an empty instrument.

**First launch, once per machine** (Todd: “only the first time”). This is layout C from the
mockups. There's no modal: the source waveform area turns into a dashed, lime-outlined drop
target, with the player dimmed below. It reads “Drop your own break here (4 to 32 beats, 80–195 BPM)” (Todd: state the BPM range in the drop UI) and
has two buttons, **Choose…** and **Keep the amen**.

- **Keep the amen** dismisses the prompt.
- **Choose…** or a drop loads the file into slot 1 and clears slots 2 to 4 (Todd: “it will clear
  the others, too”).
- Either way the prompt is remembered per machine, outside any project, and never shows again.
- **Only on a fresh instance (my call):** the first-launch prompt shows only when the instance
  opening has no saved state. If the first time a machine runs this version is an existing
  project, the prompt waits for the next fresh instance, so **Choose…** can never clear slots
  that hold someone's work.
- The same drop target also appears whenever no slot has a break: after unloading everything, or
  a project that opens empty. It adds a “Use the Junglizer amen” link to restore the four
  built-in breaks. The link is my call.

**Afterwards:**
- Clicking a blank slot opens the system file dialog, filtered to WAV, AIFF and MP3.
- Drag and drop onto the waveform or a slot works too. My call: it teaches a gesture that keeps
  working after the first launch.

**Unload** (Todd: an unload button with a modal):
- Each loaded slot has **Unload**, which asks: “Unload this break? Its slices and placements in
  this project will be removed.” [Unload] [Cancel].
- Unloading leaves the slot blank.
- Replacing a break is unload, then load.

**Blank slots during playback** (Todd: “loop 1 if 2, 3 and 4 are empty”):
- Any player bar whose slot is blank plays slot 1's break instead.
- **Generalization (my call):** a bar whose slot is blank uses the lowest-numbered loaded slot.

## Accepting a file

**Formats:** WAV, AIFF, MP3, mono or stereo, any common sample rate. Decode with Symphonia
(MPL-2.0) and resample to the host rate with Rubato (MIT/Apache). Both were chosen for license
compatibility. Don't use aubio, which is GPL-3 and would force Junglizer's license.

**Length** (Todd: refuse anything outside the range):
- **Accepted range (Todd):** a whole number of bars from 4 to 32 beats (1 to 8 bars), at
  **80 to 195 BPM**. Todd lowered the minimum from 8 beats to 4 after the loop test (one-bar
  cuts were being refused). Todd first said 199 as the ceiling, then wrote 80–195 for the drop UI; the
  design uses 195 throughout so the rule matches what the UI promises.
- **Filename hint first (Todd, after the loop test):** sample packs put the tempo in the
  filename (`KTB1_114_ThinkBreak…`, `AJ_170_Drum_Loop…`). If the name carries a BPM in 80–195
  and that BPM makes the file a whole number of bars (within the trim tolerance), use it:
  beats = BPM × duration / 60.
  - **More than 32 beats at the named tempo refuses the file** (Todd: “refuse longer than
    32 beats”), rather than misreading it at half speed.
  - A named BPM that doesn't fit whole bars, or falls outside 80–195, is ignored, and the
    fastest-fit rule below decides.
  - The parser should find a 2–3 digit number, optionally with one decimal (`119.9`), standing
    alone between separators or followed by “bpm”. The builder picks the exact pattern; it must
    not read bar or take numbers such as `Bar_01` or `Break 2`.
- **Fastest fit, when there's no usable hint (Todd):** always assume the fastest tempo that
  fits. For each whole-bar beat count from 4 to 32 (4, 8, …, 32), compute BPM = beats × 60 /
  duration, and take the largest BPM that's 195 or below. Accept it if it's 80 or above.
  - Known limit: with no BPM in the name, a loop longer than 32 beats can't be told apart from a
    half-speed loop of 32 beats or fewer, so it's misread rather than refused. Half/double can't
    fix that either, so it's accepted as-is.
- **Untrimmed files (Todd):** a file that isn't really a whole number of bars (an untrimmed vinyl
  rip, say) is the uploader's problem. If the rule accepts it, it plays as read; no extra
  detection.
- **Loop test:** `tools/tempo/check_tempo_rule.py` runs this rule over folders of named loops
  (`--no-hint` for fastest fit only). Against 397 of Todd's loops: 378 read correctly, the 7
  loops longer than 32 beats are refused, 1 untrimmed vinyl rip is misread, and 11 have no BPM in
  the name to check against.
- **Refusal:** if no whole-bar count lands in 80–195 BPM, or the named tempo puts the file past
  32 beats, refuse the file with a short, specific message (too short, too long, or not a whole
  number of bars), and leave the slot unchanged.
- **BPM display:** non-integer tempos are fine (a loop can be 172.4 BPM); show one decimal.
- **Tolerance:** allow a small trim tolerance on the length, about 1% or a few milliseconds.
  The builder chooses the exact figure.
- My call: no tempo-detection library at launch. Add stratum-dsp (MIT/Apache) only if testing
  shows real breaks trimmed off the bar lines.

**Half/double** (Todd: a half/double control when the guess is wrong):
- A small control on each loaded slot shows the reading, e.g. “16 beats · 174 BPM”.
- Halve or double toggles between 8 beats at 87 and 16 beats at 174, and so on. A choice that
  would leave 4–32 beats or 80–195 BPM is unavailable.
- It changes only how the break is counted, never the audio.
- The choice is saved in the project.

## Projects and linked files

Todd: link loaded files, like Logic does.

**Saved in the project, per slot:**
- whether the slot is built-in, user or blank
- for a user break: the file path, the beat count, and the slot's slices, markers, placements
  and locks, as today

The file's audio isn't saved in the project.

**Missing files:**
- A user break whose file is missing shows as **missing** on its slot, with **Relink…**, which
  opens the file dialog. Its slices and placements are kept and restored.
- A relinked file that reads differently, by length or beat count, is still accepted if it
  passes the length rule. The markers are kept and clamped. The builder decides how to clamp.

**Existing 0.9 beta 1 projects (Todd: not worried about backwards compatibility):** no
migration is required. Beta 2 may open beta 1 projects as fresh instances; it must not crash on
old state.

## Engine changes longer breaks need

Today each source is one bar. A user break is 1 to 8 bars.

- **Slicing, per slot (Todd):** each slot has its own slice count (1 to 32), and its markers span
  the whole break, however long it is. The existing Source Slices control sets the selected
  slot's count (my call on the control); its host-automation and MIDI export (CC 24) behavior may
  change to suit, since backwards compatibility isn't required. Placement, Original/Fit, bit depth, retriggers, locks, bass tags and MIDI export
  work as today on those slices.
- **Ghost playback** (my call; Todd only specified looping slot 1):
  - Until slices are placed, a bar plays its source break continuously, aligned to the
    phrase start and looping at the break's length.
  - So a 4-beat break plays in every bar, and an 8-beat break plays twice across the four bars.
  - A break of 16 beats or more plays its first 16 beats, then repeats each phrase.
- Built-in amen slots stay one bar each, with no behavior change.

## Windows and Linux

Todd: in this update.

**Formats:**
- VST3 and CLAP on macOS (Apple Silicon and Intel), Windows (x86_64) and Linux (x86_64).
- AUv2 stays macOS-only.
- truce 6.3 supports VST3 and CLAP on all three.

**Platform code:**
- **MIDI export folder:** today it's `~/Music/Junglizer/Exports`. Use the OS's music folder,
  e.g. via the `dirs` crate: `~/Music` on macOS and Linux, `Music` in the user folder on Windows.
- **Revealing exported files:** “Reveal in Finder” uses `open -R` on macOS, `explorer /select,`
  on Windows, and opens the folder with `xdg-open` on Linux.
- **Copy:** UI text that says “Finder” or assumes Logic needs a per-platform or neutral wording:
  “Show in Finder”, “Show in Explorer”, “Show in Files”.
- **Native file dialog:** use `rfd` (MIT), which is cross-platform. Drag and drop uses each
  host's drop events, through truce's egui editor where available.
- **Paths:** linked paths work on all three platforms. Use forward and backslashes correctly,
  and don't assume case sensitivity.

**CI** (Todd watches GitHub Actions cost):
- Add Windows and Linux build jobs that run only on release tags.
- Keep the `site/**` paths-ignore.
- Pull-request runs stay as today: macOS tests only.

**Signing:** macOS stays signed and notarized. Windows and Linux ship unsigned zips. A Windows
code-signing certificate is optional and out of scope.

**Testing:** VST3 and CLAP in Reaper on Windows and Linux at minimum, and Bitwig if available.
Testing needs an x86_64 Windows machine or VM and a Linux one. A Windows ARM VM on Apple Silicon
runs the x86 build only under emulation.

**Release order** (my call): Windows as supported, Linux labelled beta.

**Website:** add Windows and Linux download cards and install paths to getjunglizer.com, and
update its platform text, JSON-LD and llms.txt. This is part of the release, not a separate
project.

## Retiring 0.9 beta 1

Todd: remove the beta 1 downloads when beta 2 ships, not before. On release day, replace the
zips in `site/downloads/` and the download links, and take down (or replace the assets of) the
`v0.9.0-beta.1` GitHub release, so no copy of the KAN audio stays downloadable.

## Copy rules

Smart punctuation, US English, em dashes only where they earn their place, and exact names:
Junglizer, AUv2, VST3, CLAP, macOS, Windows, Linux.

## Left to the builder

- Exact UI placement and styling of Unload, Relink, the half/double control and the missing
  state, within `DESIGN.md`.
- The length tolerance, how markers clamp on relink, and the on-disk format of the new state
  fields.
- How the renderer is structured and pinned, and where the credit line sits in the plugin.
- The version number for this release.
