# Junglizer 0.9.2 beta 3

A fix release for beta 2. If you use Junglizer in Logic Pro, or load breaks by dragging them in on a Mac, update.

## Fixed

- **Choosing a break crashed the plugin in Logic Pro.** Logic runs plugins in a separate process, and the file panel Junglizer opened there brought the plugin down. The panel now opens without blocking, so **Add…**, **Choose…** and **Relink…** work in Logic.
- **Drag and drop did nothing on macOS.** A dropped file was placed at the wrong height and missed every slot. Dropping a WAV, AIFF or MP3 on a slot, the waveform or the first-run prompt now loads it.

## Changed

- **Unload** also clears every slot that still holds the Junglizer amen, so unloading leaves you with only your own breaks. Your other breaks stay. The confirmation names the breaks it removes.
- A blank slot’s button now reads **Add…** instead of “Empty”, and a selected blank break shows a **Choose…** button.

## Installing

Download the archive for your system and format from the latest release (names look like `Junglizer-macos-arm64-vst3.zip`), unzip it, and replace the bundle in the plugin folder:

- Windows: `C:\Program Files\Common Files\VST3` or `C:\Program Files\Common Files\CLAP`
- Linux: `~/.vst3` or `~/.clap`
- macOS: `~/Library/Audio/Plug-Ins/` (`Components`, `VST3` or `CLAP`)

Restart the host so it scans the plugin. Beta 2 projects open unchanged.
