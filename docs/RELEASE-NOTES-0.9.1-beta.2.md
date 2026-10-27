# Junglizer 0.9.1 beta 2

Junglizer now ships its own amen, played on the Salamander Drumkit and embedded in every plugin bundle. Beta 2 also lets you bring your own breaks, and it adds Windows and Linux builds.

## What changed

- **Your own breaks.** Click an empty slot to choose a WAV, AIFF or MP3; on macOS and Windows you can also drag one in. Four linked slots hold the breaks the four bars pick from. A fresh instance starts with the built-in amen in all four.
- **What loads.** Breaks run 4–32 beats, whole bars, at 80–195 BPM. The tempo comes from the filename when it holds one, such as `AJ_170_Drum_Loop.wav` or `loop 174 BPM.aif`. Otherwise Junglizer picks the fastest tempo that fits. The slot shows the result, for example “16 beats · 174.0 BPM”, and ½ and ×2 correct a tempo that landed at half or double speed. Files outside the rule are refused with a message that says why.
- **Per-break slicing.** Each break has its own slice count (1–32) and its own markers across its whole length.
- **Missing files.** The project stores the path to each break, not its audio. A moved file shows as missing, and **Relink…** brings its slices and placements back. **Unload** empties a slot after a confirmation.
- **Windows and Linux.** VST3 and CLAP for Windows x86_64 and Linux x86_64. AUv2 remains macOS-only.
- **MIDI export** now writes to the Music folder of your operating system: `~/Music/Junglizer/Exports` on macOS and Linux, `Music\Junglizer\Exports` in your user folder on Windows.

Need breaks? KAN Samples’ free Amen Break Tribute and Think Break Tribute packs work well. You download them from KAN yourself; Junglizer doesn’t bundle them, and KAN Samples isn’t affiliated with this project.

## Installing

Unzip the archive for your system and format, then copy the bundle to the plugin folder:

- Windows: `C:\Program Files\Common Files\VST3` or `C:\Program Files\Common Files\CLAP`
- Linux: `~/.vst3` or `~/.clap`
- macOS: `~/Library/Audio/Plug-Ins/` (`Components`, `VST3` or `CLAP`)

Restart the host so it scans the plugin.

## Upgrading

Beta 1 projects open as fresh instances: the four amen slots, with no markers or placements carried over. Plugin IDs are unchanged, so hosts still find the plugin.

## Known limits

- Tempo is read from the filename, or taken from the fastest tempo that fits. A name with no BPM can guess wrong; use ½ and ×2.
- Loops over 32 beats without a BPM in the name read at half speed.
- There is no time-stretch. Breaks change speed and pitch to follow the host tempo, and Fit to length changes pitch.
- The Windows and Linux zips are unsigned, and Linux is beta. The macOS archives are signed and notarized.
- Direct dragging from the plugin into a host is not supported.
