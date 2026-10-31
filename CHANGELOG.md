# Changelog

All notable changes to Junglizer are documented here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Fixed
- Logic Pro: choosing a break (Add…, Choose…, Relink…) crashed the plugin. The open panel no longer blocks, so it works when Logic hosts the plugin out of process.
- macOS: dropped files landed at the wrong height and were ignored, so drag and drop never loaded a break.

### Changed
- **Unload** also clears every slot still holding the Junglizer amen; other user breaks stay. The confirmation names the breaks it removes.
- A blank slot’s button reads **Add…** instead of “Empty”, and a selected blank break shows a **Choose…** button.

## [0.9.1-beta.2] - 2026-10-27

### Added
- Junglizer’s own amen, played on the Salamander Drumkit and embedded in every plugin bundle.
- Bring your own breaks: load a WAV, AIFF or MP3 into any of four slots (4–32 beats, whole bars, 80–195 BPM). Tempo comes from the filename when it holds a BPM, otherwise the fastest tempo that fits; ½ and ×2 correct a half- or double-speed result.
- Per-break slicing: each break has its own slice count (1–32) and its own markers across its whole length.
- Linked files: the project stores each break’s path, not its audio. **Relink…** restores a moved file with its slices and placements, **Unload** empties a slot after a confirmation, and a blocked location (such as a network share) shows “Not opened”.
- Drag and drop of breaks onto slots on macOS and Windows.
- Reworked editor: a Fill button and a lock on each bar header, an Edit / Live / Frozen mode control, and undo (Cmd/Ctrl+Z) for Fill and Remove.
- VST3 and CLAP builds for Windows x86_64 and Linux x86_64 (Linux is beta; both are unsigned).

### Changed
- MIDI export now writes to `Junglizer/Exports` in the operating system’s Music folder.
- Junglizer is licensed under GPL-3.0-or-later. Every release zip includes `LICENSE` and the third-party notices.
- Release archives are named without a version, and downloads come from GitHub releases.

### Upgrading
- Beta 1 projects open as fresh instances (the four amen slots, no markers or placements). Plugin IDs are unchanged, so hosts still find the plugin.

## [0.9.0-beta.1] - 2026-10-26

The first beta, macOS only.

### Added
- AUv2, VST3 and CLAP for Apple Silicon and Intel Macs, signed and notarized.
- Four built-in breaks, with source slicing and movable markers, and a four-bar player with placements.
- Fill suggestions, live phrase variations and freeze.
- A Thin / Dry / Thicc crossfader with root-tuned 808 reinforcement, and an Output knob.
- MIDI export of the phrase.

[0.9.1-beta.2]: https://github.com/mrballistic/junglizer/releases/tag/v0.9.1-beta.2
[0.9.0-beta.1]: https://github.com/mrballistic/junglizer/releases/tag/v0.9.0-beta.1
