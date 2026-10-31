# Developing Junglizer

Junglizer is a Rust plugin built on [truce](https://crates.io/crates/cargo-truce) with an egui editor. For what it does and how to install it, see the [README](../README.md).

## Prerequisites

Rust 1.92 or newer and `cargo-truce` 6.3. Per platform:

- **macOS:** the Xcode command-line tools. AUv2 builds only here.
- **Windows:** the MSVC build tools are enough. VST3 and CLAP only.
- **Linux:** `pkg-config` and the X11, XCB, OpenGL and D-Bus development packages (the CI workflow lists them). VST3 and CLAP only.

The four built-in amen WAV files in `sample/` are embedded at build time; their license terms are in `sample/LICENSE.md`.

## Build and verify

```sh
cargo fmt --all -- --check
cargo test --locked
cargo truce build --au2 --vst3 --clap   # macOS
cargo truce build --vst3 --clap         # Windows and Linux
cargo truce validate --auval            # macOS only
```

`cargo truce build` stages the format bundles in `target/bundles/`. `cargo truce install --au2 --user --no-build` installs the staged AU for the current user, then run `tools/release/patch_au_plist.sh ~/Library/Audio/Plug-Ins/Components/Junglizer.component` on the installed copy (install rewrites the Info.plist, so patch after it). cargo-truce marks the AU sandbox-safe, so Logic runs it in a sandbox where breaks in Desktop, Documents and Downloads won’t open, and versions it 1.0.0; the script fixes both, and CI runs it before packing. This project keeps the plugin IDs in `truce.toml` unchanged; changing them would break project recall in hosts.

To trace drops, the file dialog and loads, build with `--features debug-log`; it appends to `~/Library/Logs/Junglizer/debug.log`. Never ship that build.

## Architecture

A tour of `src/`:

- **`lib.rs`**: the plugin, its parameters, the engine and the DSP. The audio thread never allocates, locks or blocks; `truce::enable_rt_paranoid!()` checks that under the `rt-paranoid` feature.
- **`library.rs`**: the four break slots. A loader thread (`junglizer-loader`) decodes requests off the audio thread and publishes one result per slot and purpose behind an `Arc`. The DSP adopts results with `try_lock` and retires the audio it lets go of to a fixed queue that the loader and the editor drain, so the handoff is lock-free from the audio side. Restore-path hardening: a path loaded from saved project state is checked first (`restore_allowed`), and a blocked location, such as a network share, is not opened. The slot shows “Not opened” instead.
- **`decode.rs`**: WAV, AIFF and MP3 decoding with Symphonia. UI and init threads only, never the audio callback.
- **`tempo.rs`**: the tempo rule for user breaks (filename hint, otherwise fastest fit; 4–32 beats, 80–195 BPM). It mirrors `tools/tempo/check_tempo_rule.py`; change both together.
- **`edit_state.rs`**: the editable pattern shared by the editor, project state and the engine, in fixed-size storage so state loads and audio reads allocate nothing. It serializes as JZP3 project state. Beta 1 state is not migrated: those projects open fresh.
- **`ui.rs`, `ui_slots.rs`**: the egui editor, and the slot controls (load, unload, relink, half/double). `ui_tests.rs` renders the editor for fixtures.
- **`platform.rs`**: per-machine locations (the OS Music folder for MIDI export) and handing files to the operating system.
- **`midi.rs`**: Standard MIDI File export of a four-bar phrase.
- **`suggest.rs`**: the small, repeatable fill generator. It produces editable placements and never alters locked cuts.
- **`live.rs`**: a lock-free copy of the exact live phrase the DSP is rendering, which the UI uses when you freeze a variation.
- **`pattern.rs`, `sample.rs`, `tone.rs`**: pattern data, the sample type, and the Thin/Thicc processing.
- **`vendor/truce-egui`**: a vendored copy of truce-egui, patched to forward file drag-and-drop events to egui (`junglizer patch: file drop` in `editor.rs`). Drag and drop works on macOS and Windows. It keeps its Apache-2.0/MIT terms.

## Tests

`cargo test --locked` is the gate. Two extras:

```sh
cargo test --features rt-paranoid                  # real-time allocation checker
cargo test render_editor_states -- --ignored --nocapture   # GPU fixture renders
```

The fixture renders draw the real editor without a DAW and need normal desktop GPU access (macOS). Environment variables: `JUNGLIZER_RENDER_DIR` (output folder), `JUNGLIZER_RENDER_SCALE` (for example `2`) and `JUNGLIZER_RENDER_ONLY` (one state, such as `playing`). The review fixtures are described in the [validation record](UI-REFRESH-VALIDATION.md).

## Tools

- **`tools/amen/`**: renders the built-in amen from Salamander Drumkit one-shots (`clone_amen.py render-final`) and checks the result against `sample/` (`check_render.py`). See its README. The renders in `sample/` are canonical.
- **`tools/tempo/check_tempo_rule.py`**: runs the tempo rule over a folder of loops whose names carry a BPM. Mirrors `src/tempo.rs`.
- **`tools/fixtures/make_fixtures.sh`**: regenerates `tests/fixtures`, the synthetic click loops and deliberately bad files. Needs ffmpeg and lame.
- **`tools/screenshots/render_site_screenshot.sh`**: renders the playing editor at 2x and encodes the site hero images and README screenshot. Needs a GPU, ffmpeg and cwebp.
- **`tools/site/waveform_svg.py`**: writes `site/img/break-waveform.svg` from the built-in amen.
- **`tools/release/sign_macos.sh`**: signs, notarizes, staples and repackages the macOS archives for one architecture and can replace them in the draft release. See [release signing](RELEASE-SIGNING.md).

## CI and releases

[`.github/workflows/ci-release.yml`](../.github/workflows/ci-release.yml):

- **Pull requests and pushes to `main`** (ignoring `site/**`): `cargo fmt --check` and `cargo test --locked` on macOS.
- **Tags `v*`:** after the tests, builds macOS arm64 and x86_64 (AUv2, VST3, CLAP), Windows x86_64 and Linux x86_64 (VST3, CLAP), then creates a **draft** release.
- **Manual dispatch:** the tests plus the Windows and Linux builds, so they can be tried without a tag.

Asset names carry no version, so the site can link to `releases/latest/download/<name>`: `Junglizer-macos-{arm64,x86_64}-{component,vst3,clap}.zip`, `Junglizer-{windows,linux}-x86_64-{vst3,clap}.zip`, and `SHA256-macos-arm64.txt`, `SHA256-macos-x86_64.txt`, `SHA256-windows.txt`, `SHA256-linux.txt`. Every zip carries `LICENSE` and `THIRD-PARTY-NOTICES.md`.

To release: tag and push, sign and notarize the six macOS archives with `tools/release/sign_macos.sh` (see [RELEASE-SIGNING.md](RELEASE-SIGNING.md)), then publish the draft as the **latest** release, not a pre-release, or `releases/latest` will not serve it. Windows and Linux zips stay unsigned. The website ships separately with `site/deploy.sh`, which only syncs the site; downloads come from GitHub.

## Design docs

- [UI design brief](UI-DESIGN-BRIEF.md)
- [Implemented design system](../DESIGN.md)
- [UI refresh validation record](UI-REFRESH-VALIDATION.md)
- [Release notes, 0.9.1 beta 2](RELEASE-NOTES-0.9.1-beta.2.md)
