# UI refresh and 0.9 beta validation — 2026-10-26

The refresh preserves the approved graffiti wordmark, including the J/I cleanup. The supplied Opus comps informed compact source controls, full-width slice pads, integrated player bar selectors, a beat ruler, placement envelopes, and paired Placed slice/Phrase panels. The native olive/ivory treatment remains intentionally flatter than those references.

The final header evenly distributes playback, a Thin/Dry/Thicc crossfader, the 808 root selector, and the Output knob. Its duplicate state label and Reset button were removed. The four authorized WAVs are now embedded at build time, so release binaries no longer depend on a build-machine sample path. The package version is `0.9.0-beta.1`.

## Verified

- `cargo test --locked`: 23 passed, zero failed; one explicitly invoked rendering test is ignored in the normal suite.
- `cargo test render_editor_states -- --ignored --nocapture`: generated 13 actual egui fixture renders at 1100 × 720. These cover ghost/stopped, selected, muted, maximum density, collision recovery, playing, two live phrases, frozen MIDI, legacy locks, missing samples, and export success/failure.
- Independent design review inspected all 13 renders and both supplied comps: ship, no required design fixes.
- `CLANG_MODULE_CACHE_PATH=/tmp/junglizer-clang-cache cargo truce build --au2 --vst3 --clap`: all three release bundles built and signed in `target/bundles/`.
- The 0.9 beta AU with embedded breaks was installed in `~/Library/Audio/Plug-Ins/Components/Junglizer.component` and passed `cargo truce validate --auval`. Cross-architecture GitHub CI builds remain a separate check.
- Local Apple Silicon AUv2, VST3, and CLAP ZIP archives passed `unzip -t`; CI will package both Mac architectures from the release tag.
- Original installed AU backup retained at `target/validation-backup/Junglizer.component`.

## Evidence and remaining checks

Screenshots are under [`.impeccable/review/`](../.impeccable/review/) and the final [README preview](screenshots/editor-playing.png). They are real native renders with deterministic fixtures, not Logic screenshots. The missing-samples render is a deliberate failure fixture; release binaries embed all four breaks. Export messages are fixtures, not proof of a Finder export round trip. Waveform previews use cached peak envelopes and placement timing; they do not represent post-effect audio.

Manual checks still needed in Logic: reload the AU, audition and drag slices, move source markers, use keyboard selection, check CS12 automation, listen through live/frozen transitions, and export/drop MIDI from Finder. AU validation and unit tests do not establish these end-to-end interactions.

Implementation conventions and reference provenance are documented in `DESIGN.md` and `assets/README.md`. No DSP algorithm or parameter ID changes were made for this refresh.
