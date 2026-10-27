<img src="assets/junglizer-wordmark.png" width="420" alt="Junglizer">

**Junglizer 0.9.1 beta 2** is a four-slot break instrument for macOS, Windows and Linux. It turns four breaks into an editable phrase, with AUv2 for Logic Pro on macOS and VST3 and CLAP builds for compatible hosts on all three systems. Junglizer now ships its own amen, played on the Salamander Drumkit and embedded in the plugin, so a downloaded build plays without this source checkout.

You can also bring your own breaks. Click an empty slot to choose a WAV, AIFF or MP3; on macOS and Windows you can also drag one in. A break runs 4–32 beats, whole bars, at 80–195 BPM. Junglizer reads the tempo from the filename when it contains one (`AJ_170_Drum_Loop.wav`, `loop 174 BPM.aif`); otherwise it picks the fastest tempo that fits. The ½ and ×2 buttons correct a tempo that landed at half or double speed. Need breaks? KAN Samples’ free [Amen Break Tribute](https://kansamples.com/blogs/free-drum-and-bass-sample-packs/amen-break-tribute-pack-free-drum-bass-sample-pack-free-download) and [Think Break Tribute](https://kansamples.com/blogs/free-drum-and-bass-sample-packs/think-break-tribute-pack-free-drum-bass-sample-pack) packs work well. You download them from KAN yourself; Junglizer doesn’t bundle them, and KAN Samples isn’t affiliated with this project.

![The native editor playing a phrase](docs/screenshots/editor-playing.png)

## Make a phrase

1. Choose a source break and its slice count (1–32). Every break has its own count and its own movable, sound-aligned cuts, however long the break is. Click a slice to audition it; right-click to mute it or use the Bass hit checkbox to correct the 808 trigger.
2. Choose a break for each of four bars from the four slots. The complete break plays as a ghost in empty player slots.
3. Choose 1–32 evenly spaced player slots across the phrase. Drag a source slice onto the player, or select one and click an empty slot. A placement replaces the ghost over its span. Set its length in slots, Original or Fit speed, bit depth, retriggers, and lock.
4. Suggest a fill in one bar, or enable Live variations for a new phrase each four-bar pass. Freeze a phrase to keep it. Freeze and individual placement locks are separate; Unfreeze and Unlock all slices recover editing when needed.
5. Shape the entire player with the Thin / Dry / Thicc crossfader. Thin increasingly high-passes. Thicc reinforces tagged bass hits with a decaying 808 at the chosen root; the far end adds a lower octave. The Output knob adjusts gain.

The player follows host transport and tempo. At tempos other than 174 BPM, the source breaks change speed and pitch to stay on the beat grid. Fit to length also changes pitch; pitch-preserving stretch is planned. Markers, placements, locks, and phrase settings are saved in plugin project state. Your own audio is not: the project stores the path to each file you loaded and its slices and placements. If a file moves, its slot shows as missing; **Relink…** opens a file dialog, and the slices and placements come back with it. **Unload** empties a slot after asking you to confirm. Beta 1 projects open as fresh instances.

## Download and install

Use the [0.9.1 beta 2 release](https://github.com/mrballistic/junglizer/releases/tag/v0.9.1-beta.2). On a Mac, pick the archive for your machine (`arm64` for Apple Silicon, `x86_64` for Intel) and plugin format. On Windows and Linux, pick the `windows-x86_64` or `linux-x86_64` VST3 or CLAP archive. Unzip it and copy the bundle to:

| System | Format | Copy the unzipped bundle to |
| --- | --- | --- |
| macOS | AUv2 / Logic Pro | `~/Library/Audio/Plug-Ins/Components/` |
| macOS | VST3 | `~/Library/Audio/Plug-Ins/VST3/` |
| macOS | CLAP | `~/Library/Audio/Plug-Ins/CLAP/` |
| Windows | VST3 | `C:\Program Files\Common Files\VST3` |
| Windows | CLAP | `C:\Program Files\Common Files\CLAP` |
| Linux | VST3 | `~/.vst3` |
| Linux | CLAP | `~/.clap` |

Restart the host to rescan. The six macOS archives are signed with a Developer ID Application certificate, notarized by Apple, and include stapled tickets. The Windows and Linux archives are unsigned, and Linux support is beta. The release also provides SHA-256 checksums for every archive. No separate sample installation is needed.

## MIDI export and automation

**Export MIDI to Finder** writes a four-bar `.mid` file to `Junglizer/Exports` inside your operating system’s Music folder (`~/Music` on macOS and Linux, `Music` in your user folder on Windows) and reveals it. In Logic, drag the file onto the Junglizer track and choose **MIDI region** playback in the plugin. The file carries slice notes, bar-source choices, and control changes for speed, bit depth, retriggers, length, slot count, tone, and 808 root. The ghost break continues under empty slots. To match an exported phrase, freeze it and leave source markers unchanged; marker edits change how its notes sound. Direct dragging from the plugin into Logic is not yet supported.

The plugin exposes its controls as host automation parameters, including source and player selection, marker position, slice properties, bass tags, tone, output, playback mode, live variations, and momentary actions. Map CS12 controls using Logic's controller assignment workflow. For a momentary action, send a low-to-high transition and return it low before triggering again. Exported MIDI uses channel 1 notes `0..127` for break `0..3` × slice `0..31`; CC `20` is speed, `21` bit depth, `22` retriggers, `23` length, `24` slot count, `25` tone, `26` root, and `30` the bar's ghost break.

## Build and verify

Requires Rust 1.92 or newer and `cargo-truce` 6.3. On macOS you also need the Xcode command-line tools. On Windows, the MSVC build tools are enough. On Linux, install `pkg-config` and the X11, XCB, OpenGL and D-Bus development packages; the CI workflow lists them. AUv2 builds only on macOS; Windows and Linux build VST3 and CLAP. The four built-in amen WAV files in `sample/` are embedded at build time, and their license terms are in `sample/LICENSE.md`.

```sh
cargo fmt --all -- --check
cargo test --locked
cargo truce build --au2 --vst3 --clap   # macOS
cargo truce build --vst3 --clap         # Windows and Linux
cargo truce validate --auval            # macOS only
```

`cargo truce build` stages the format bundles in `target/bundles/`. `cargo truce install --au2 --user --no-build` installs the staged AU for the current user. The [CI and release workflow](.github/workflows/ci-release.yml) tests pushes and pull requests on macOS. Release tags also build both Mac architectures, Windows x86_64 and Linux x86_64. A release maintainer signs, notarizes, and staples each macOS format before publishing the downloads; see [release signing](docs/RELEASE-SIGNING.md). This beta retains the current plugin IDs in `truce.toml`; changing them would affect project recall.

The [UI design brief](docs/UI-DESIGN-BRIEF.md), [implemented design system](DESIGN.md), [validation record](docs/UI-REFRESH-VALIDATION.md), and [0.9.1 beta 2 release notes](docs/RELEASE-NOTES-0.9.1-beta.2.md) document the current scope and its limits. Nick Collins's *Algorithmic Composition Methods for Breakbeat Science* ([local PDF](docs/acmethodsforbbsci.pdf)) informed the separation of source cuts, placements, and playback. [Livecut](https://github.com/mdsp/Livecut) inspired the first editable fill suggestion; Junglizer uses its own code and deterministic rendering of accepted fills. The built-in amen is played on the Salamander Drumkit by Alexander Holm (CC BY-SA 3.0).
