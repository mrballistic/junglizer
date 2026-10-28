<img src="assets/junglizer-wordmark.png" width="420" alt="Junglizer">

**Junglizer 0.9.1 beta 2** is a four-slot break instrument for macOS, Windows and Linux. It turns four breaks into an editable phrase, with AUv2 for Logic Pro on macOS and VST3 and CLAP builds for compatible hosts on all three systems. Junglizer now ships its own amen, played on the Salamander Drumkit and embedded in the plugin, so a downloaded build plays without this source checkout.

You can also bring your own breaks. Click an empty slot to choose a WAV, AIFF or MP3; on macOS and Windows you can also drag one in. A break runs 4–32 beats, whole bars, at 80–195 BPM. Junglizer reads the tempo from the filename when it contains one (`AJ_170_Drum_Loop.wav`, `loop 174 BPM.aif`); otherwise it picks the fastest tempo that fits. The ½ and ×2 buttons correct a tempo that landed at half or double speed. Need breaks? KAN Samples’ free [Amen Break Tribute](https://kansamples.com/blogs/free-drum-and-bass-sample-packs/amen-break-tribute-pack-free-drum-bass-sample-pack-free-download) and [Think Break Tribute](https://kansamples.com/blogs/free-drum-and-bass-sample-packs/think-break-tribute-pack-free-drum-bass-sample-pack) packs work well. You download them from KAN yourself; Junglizer doesn’t bundle them, and KAN Samples isn’t affiliated with this project.

![The native editor playing a phrase](docs/screenshots/editor-playing.png)

## Make a phrase

1. Choose a source break and its slice count (1–32). Every break has its own count and its own movable, sound-aligned cuts, however long the break is. Click a slice to audition it; right-click to mute it or use the Bass hit checkbox to correct the 808 trigger.
2. Choose a break for each of four bars from the four slots. The complete break plays as a ghost in empty player slots.
3. Choose 1–32 evenly spaced player slots across the phrase. Drag a source slice onto the player, or select one and click an empty slot. A placement replaces the ghost over its span. Set its length in slots, Original or Fit speed, bit depth, retriggers, and lock, using the Placed slice panel under the player.
4. Click **Fill** on a bar header to suggest a fill in that bar, and the lock beside it to keep the bar as it is. The phrase **Mode** control switches between **Edit**, **Live** (a new variation each four-bar pass) and **Frozen** (the phrase you like, kept as is). Mode and individual placement locks are separate; choosing **Edit** again recovers editing. **Fill** and **Remove** can be undone with Cmd+Z (macOS) or Ctrl+Z (Windows and Linux).
5. Shape the entire player with the Thin / Dry / Thicc crossfader. Thin increasingly high-passes. Thicc reinforces tagged bass hits with a decaying 808 at the chosen root; the far end adds a lower octave. The Output knob adjusts gain.

The player follows host transport and tempo. When the host tempo differs from a break’s own tempo, breaks change speed and pitch to stay on the beat grid. Fit to length also changes pitch; pitch-preserving stretch is planned. Markers, placements, locks, and phrase settings are saved in plugin project state. Your own audio is not: the project stores the path to each file you loaded and its slices and placements. If a file moves, its slot shows as missing; **Relink…** opens a file dialog, and the slices and placements come back with it. **Unload** empties a slot after asking you to confirm. Beta 1 projects open as fresh instances.

## Download and install

Get the latest build from the [releases page](https://github.com/mrballistic/junglizer/releases/latest). Archive names carry no version: `Junglizer-macos-arm64-<format>.zip` (Apple Silicon) and `Junglizer-macos-x86_64-<format>.zip` (Intel), where `<format>` is `component` (AUv2), `vst3` or `clap`; `Junglizer-windows-x86_64-<format>.zip` and `Junglizer-linux-x86_64-<format>.zip` for `vst3` or `clap`. Each zip holds one plugin, plus the license and third-party notices. Unzip it and copy the bundle to:

| System | Format | Copy the unzipped bundle to |
| --- | --- | --- |
| macOS | AUv2 / Logic Pro | `~/Library/Audio/Plug-Ins/Components/` |
| macOS | VST3 | `~/Library/Audio/Plug-Ins/VST3/` |
| macOS | CLAP | `~/Library/Audio/Plug-Ins/CLAP/` |
| Windows | VST3 | `C:\Program Files\Common Files\VST3` |
| Windows | CLAP | `C:\Program Files\Common Files\CLAP` |
| Linux | VST3 | `~/.vst3` |
| Linux | CLAP | `~/.clap` |

Restart the host to rescan. The six macOS archives are signed with a Developer ID Application certificate, notarized by Apple, and include stapled tickets. The Windows and Linux archives are unsigned, and Linux support is beta. The release also provides SHA-256 checksums for every archive (`SHA256-macos-arm64.txt`, `SHA256-macos-x86_64.txt`, `SHA256-windows.txt`, `SHA256-linux.txt`). No separate sample installation is needed.

## MIDI export and automation

**Export MIDI** writes a four-bar `.mid` file to `Junglizer/Exports` inside your operating system’s Music folder (`~/Music` on macOS and Linux, `Music` in your user folder on Windows) and, on macOS, reveals it in Finder. On Windows and Linux a **Show in Explorer** or **Show in Files** button appears after the export. In Logic, drag the file onto the Junglizer track and choose **MIDI region** playback in the plugin. The file carries slice notes, bar-source choices, and control changes for speed, bit depth, retriggers, length, slot count, tone, and 808 root. The ghost break continues under empty slots. To match an exported phrase, freeze it and leave source markers unchanged; marker edits change how its notes sound. Direct dragging from the plugin into Logic is not yet supported.

The plugin exposes its controls as host automation parameters, including source and player selection, marker position, slice properties, bass tags, tone, output, playback mode, live variations, and momentary actions. Map CS12 controls using Logic's controller assignment workflow. For a momentary action, send a low-to-high transition and return it low before triggering again. Exported MIDI uses channel 1 notes `0..127` for break `0..3` × slice `0..31`; CC `20` is speed, `21` bit depth, `22` retriggers, `23` length, `24` slot count, `25` tone, `26` root, and `30` the bar's ghost break.

## Credits

The built-in amen is played on the Salamander Drumkit by Alexander Holm (CC BY-SA 3.0). Nick Collins’s *Algorithmic Composition Methods for Breakbeat Science* ([local PDF](docs/acmethodsforbbsci.pdf)) informed the separation of source cuts, placements and playback. [Livecut](https://github.com/mdsp/Livecut) inspired the first editable fill suggestion; Junglizer uses its own code and deterministic rendering of accepted fills. See [THIRD-PARTY-NOTICES.md](THIRD-PARTY-NOTICES.md), which also ships in every release zip.

## More

- [CHANGELOG.md](CHANGELOG.md): what changed in each release.
- [docs/DEVELOPING.md](docs/DEVELOPING.md): building, testing, architecture and releasing.

## License

Junglizer · Copyright © 2026 Todd Greco.

Junglizer is free software under the [GNU General Public License v3.0 or later](LICENSE): you can use, study, share and modify it, and if you distribute a modified version you must share its source under the same license. The built-in amen breaks are separately licensed CC BY-SA 3.0 (see [`sample/LICENSE.md`](sample/LICENSE.md)), and the vendored truce-egui keeps its Apache-2.0/MIT terms. Third-party credits are in [`THIRD-PARTY-NOTICES.md`](THIRD-PARTY-NOTICES.md).
