# Junglizer 0.9.3 beta 4

A fix release for Logic Pro on macOS. If breaks you keep on the Desktop, or in Documents or Downloads, wouldn’t load, update.

## Fixed

- **Breaks in Desktop, Documents or Downloads wouldn’t load in Logic Pro.** Junglizer said it couldn’t read the file, even one you had just picked or dropped. Logic was running the plug-in in a sandbox that keeps it out of those folders. Junglizer now asks to run unsandboxed, as most plug-ins do, so files there load. Breaks on other drives or folders were never affected.
- **Logic showed the AU as version 1.0.0.** It now shows the real version, 0.9.3.

## Installing

Download the archive for your system and format from the latest release (names look like `Junglizer-macos-arm64-component.zip`), unzip it, and replace the bundle in the plugin folder:

- macOS: `~/Library/Audio/Plug-Ins/` (`Components`, `VST3` or `CLAP`)
- Windows: `C:\Program Files\Common Files\VST3` or `C:\Program Files\Common Files\CLAP`
- Linux: `~/.vst3` or `~/.clap`

Quit and reopen Logic so it picks up the change. Beta 3 projects open unchanged.
