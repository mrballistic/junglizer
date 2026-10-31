# Junglizer 0.9.3 beta 4

A fix release for Logic Pro on macOS, plus quicker editing in the player. If breaks you keep on the Desktop, or in Documents or Downloads, wouldn’t load, update.

## Fixed

- **Breaks in Desktop, Documents or Downloads wouldn’t load in Logic Pro.** Junglizer said it couldn’t read the file, even one you had just picked or dropped. Logic was running the plug-in in a sandbox that keeps it out of those folders. Junglizer now asks to run unsandboxed, as most plug-ins do, so files there load. Breaks on other drives or folders were never affected.
- **Logic showed the AU as version 1.0.0.** It now shows the real version, 0.9.3.

## New

- **Delete a placed slice.** Select it in the player and press Delete (or Backspace). Cmd+Z (Ctrl+Z on Windows and Linux) puts it back. Locked slices stay put.
- **Swap a placed slice.** Click it in the player, then click any source slice above. The new slice takes its place and keeps its length, speed, bits and retrigger, so you can try different slices in the same spot. Cmd+Z undoes the swap. Placing a new slice or pressing Esc ends swapping, so auditioning slices never changes what you’ve placed.

## Installing

Download the archive for your system and format from the latest release (names look like `Junglizer-macos-arm64-component.zip`), unzip it, and replace the bundle in the plugin folder:

- macOS: `~/Library/Audio/Plug-Ins/` (`Components`, `VST3` or `CLAP`)
- Windows: `C:\Program Files\Common Files\VST3` or `C:\Program Files\Common Files\CLAP`
- Linux: `~/.vst3` or `~/.clap`

Quit and reopen Logic so it picks up the change. Beta 3 projects open unchanged.
