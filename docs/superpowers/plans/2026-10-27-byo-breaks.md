# Junglizer 0.9.1 beta 2 (bring your own breaks) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the KAN amen with one we own, let producers load their own WAV/AIFF/MP3 breaks of 4–32 beats into four linked slots, and ship VST3/CLAP on Windows and Linux.

**Architecture:**
- **Decoding:** a new decode module (Symphonia) turns any file into the existing `StereoSample`.
- **Tempo:** a pure tempo module reads beats and BPM from the filename first, then falls back to the fastest tempo that fits.
- **Project state:** `EditState` gains four `BreakSlot`s (built-in, user or blank; path, beats, slice count).
- **Audio handoff:** truce runs `load_state` on the audio thread, so files are never decoded there. A shared `Library` in the params owns a loader thread. The editor and the DSP post requests to it. The DSP adopts finished `Arc<LoadedBreak>`s with `try_lock`, and retires old ones to a queue, so the audio thread never allocates or frees.
- **Engine:** plays every source at its native rate. Ghost bars loop at the break's length, aligned to the phrase start.

**Departures from the spec, pending Todd's OK:**
- **No Rubato.** The engine already interpolates at each file's own rate, and ghosts map by beats, so resampling to the host rate adds a dependency for nothing. The Review Focus item 3 tests prove it.
- **Click to choose rather than drop.** truce-egui 6.3 doesn't pass file drops to egui, so the first-run copy reads “Choose your own break…”. See Task 13.

**Tech Stack:**
- Rust 2024 and truce 6.3 (egui 0.34)
- symphonia 0.6.1, rfd 0.17.2, dirs 7.0.0 and crossbeam-queue 0.3.14, all current stable as of 2026-10-27
- hound 3.5, now a dev-dependency only
- Python 3 for `tools/`

**Spec:** `docs/superpowers/specs/2026-10-27-byo-breaks-design.md` (read it with this plan)

## Global Constraints

- **No KAN audio** in any committed or shipped file. Never commit anything from `target/amen-spike/original/` or `target/amen-spike/LOCAL_ONLY_contains_reference_audio/`. Never copy Todd's loop folders into the repo.
- **Accepted breaks:** 4 to 32 beats, whole bars, 80–195 BPM. The trim tolerance is 1%, applied to the BPM bounds, exactly as `tools/tempo/check_tempo_rule.py` does.
- **Display:** BPM shows one decimal, e.g. “16 beats · 174.0 BPM”.
- **UI copy:** smart punctuation (’ “ ” …), US English, and em dashes only where they earn their place, closed up. Exact names: Junglizer, AUv2, VST3, CLAP, macOS, Windows, Linux.
- **Licenses:** dependencies must be MIT, Apache-2.0 or MPL-2.0. No aubio or any GPL crate.
- **Real time:** the audio thread (`process`, `load_state`, `state_changed`) never allocates, frees, blocks or does file I/O. `cargo test --features rt-paranoid` must pass.
- **Platforms:**
  - VST3 and CLAP on macOS (arm64 and x86_64), Windows x86_64 and Linux x86_64. AUv2 stays macOS-only.
  - Keep the plugin IDs in `truce.toml` unchanged.
- **Git:**
  - Commit by explicit path (`git commit -- <paths>`), never `-a`. Work on a branch, not `main`.
  - The repo-local email is `mrballistic@gmail.com`.
- **Release jobs:** Windows and Linux CI jobs run only on `v*` tags (plus manual dispatch). Pull requests stay macOS tests only. Keep the `site/**` paths-ignore.
- **Site:** don't run `site/deploy.sh`. Todd deploys.
- **Each task's gate:** `cargo fmt --all -- --check` and `cargo test --locked`. Add `cargo test --features rt-paranoid` from Task 7 on.

## Review Focus

1. **A project whose user break sits on an unplugged drive or a moved folder.** It must open without a crash, and the slot shows **missing** with **Relink…**. Its slices and placements survive, playback falls back to the lowest loaded slot, and relinking restores everything. Tests: Task 5 `relink_scales_markers_and_keeps_placements`, Task 6 `blank_or_missing_bar_plays_lowest_loaded_slot`, Task 7 `missing_file_is_reported_and_dsp_plays_fallback`.
2. **Loading, unloading or relinking while the host transport is running.** No allocation or free may happen on the audio thread. Test: Task 7 `adopting_and_retiring_breaks_never_allocates_on_audio_thread` (rt-paranoid).
3. **A file whose sample rate differs from the host's** (a 48 kHz or 96 kHz file in a 44.1 kHz session). It must play at the right tempo and pitch. Tests: Task 6 `original_speed_slice_uses_file_rate_not_host_rate` and `ghost_follows_beats_not_sample_rate`.
4. **Filenames with numbers that aren't tempos** (`Bar_01`, `Break 2`, `808`, `2024`, `Cut_02`). They must not be read as a BPM. Test: Task 4 `name_numbers_that_are_not_tempos_are_ignored`.
5. **Bad files:** zero bytes, truncated, a text file renamed `.wav`, a path that isn't UTF-8, or a path over 1024 bytes. Each must give a clear message and leave the slot unchanged, never panic. Tests: Task 3 `bad_files_return_errors_not_panics`, and Task 5 `overlong_path_is_refused`.

---

## File map

| File | Responsibility |
|---|---|
| `sample/174_JunglizerAmen_Bar_0{1..4}.wav`, `sample/LICENSE.md` | Built-in amen and its license (Task 1) |
| `tools/amen/` | Deterministic renderer, committed transcription, render check (Task 2) |
| `src/decode.rs` (new) | Symphonia: file or bytes → `StereoSample` (Task 3) |
| `tests/fixtures/`, `tools/fixtures/make_fixtures.sh` | Our own synthetic test audio (Task 3) |
| `src/tempo.rs` (new) | Filename hint, fastest fit, half/double (Task 4) |
| `src/edit_state.rs` | `BreakSlot`, per-slot slice counts, load/unload/relink, `JZP3` format (Task 5) |
| `src/suggest.rs`, `src/live.rs`, `src/midi.rs` | Per-slot slice counts (Task 5) |
| `src/library.rs` (new) | `LoadedBreak` (Task 6); requests, results, retire queue, loader thread (Task 7) |
| `src/lib.rs` | Engine: native-rate multi-bar sources, ghost alignment, fallback, reconcile (Tasks 6–7) |
| `src/ui.rs`, `src/ui_slots.rs` (new) | Slot controls, file dialog, modal, half/double (Task 8); first run, empty state, credit (Task 9) |
| `src/platform.rs` (new) | Export folder, reveal, per-OS labels, first-run flag path (Task 10, flag path used in Task 9) |
| `.github/workflows/ci-release.yml` | Windows and Linux bundles (Task 11) |
| `README.md`, `docs/RELEASE-NOTES-0.9.1-beta.2.md`, `Cargo.toml`, `site/` | Docs, version, site copy (Task 12) |

Task 9 uses `platform::first_run_flag()`. To keep Task 9 independent, create `src/platform.rs` with just that function in Task 9, and Task 10 adds the rest.

---

### Task 1: Ship the built-in amen we own

**Files:**
- Create: `sample/174_JunglizerAmen_Bar_01.wav` … `_04.wav` (copied byte for byte from `target/amen-spike/out4/b/`), and `sample/LICENSE.md`
- Delete: `sample/174_AmenBreak_Cut_01_NoStab.wav`, `_02`, `_03`, `_04`
- Modify: `src/lib.rs:280-291` (`SOURCE_FILES`, `SOURCE_AUDIO`), `src/sample.rs` tests, `README.md` (credit line and the sample sentence)

**Interfaces:**
- Produces: `SOURCE_FILES` / `SOURCE_AUDIO` naming the four new files. Later tasks keep the constants and only change their consumers.

- [ ] **Step 1: Copy the renders and verify they are the approved bytes**

```bash
cp target/amen-spike/out4/b/174_JunglizerAmen_Bar_0{1,2,3,4}.wav sample/
shasum -a 256 sample/174_JunglizerAmen_Bar_0*.wav
```
Expected:
```
495c1da389d7e6cc6975647c1248d1a218b89c3113f0bec37aa01d28dfc2e7cb  sample/174_JunglizerAmen_Bar_01.wav
540fcd0b7444fb196ae64c9f231ac0e629e16c9ff55a315cfebb019f51c6a02c  sample/174_JunglizerAmen_Bar_02.wav
0ba9a8103a0c4404045c60905ee8fd3a076418bce1d52541572d85650f019fef  sample/174_JunglizerAmen_Bar_03.wav
43549219c4806fd0797931b705ba10e38f09057cb5727ca48ba1b83628e2afd2  sample/174_JunglizerAmen_Bar_04.wav
```

- [ ] **Step 2: Point `SOURCE_FILES` and `SOURCE_AUDIO` at the new names, and `git rm` the four KAN WAVs**

- [ ] **Step 3: Run the existing sample tests against the new audio**

Run: `cargo test --locked sample::`
Expected: PASS for all three: one-bar length 60,820–60,840 frames, ordered markers, and a bass hit at 8 slices in every bar.
If the bass-hit test fails on a bar, stop and report it. Don't loosen the threshold.

- [ ] **Step 4: Write `sample/LICENSE.md`**

It must state:
- The four WAVs are licensed CC BY-SA 3.0.
- They were played on the Salamander Drumkit by Alexander Holm.
- The pattern was programmed from scratch.
- Share-alike covers this sample set, not Junglizer's code.

- [ ] **Step 5: Update `README.md`**

- Replace “The four WAV files in `sample/` are embedded” wording so it no longer implies KAN audio.
- Add, next to the Nick Collins credit: “Built-in breaks played on the Salamander Drumkit by Alexander Holm (CC BY-SA 3.0).”

- [ ] **Step 6: Gate and commit**

```bash
cargo fmt --all -- --check && cargo test --locked
git add sample/174_JunglizerAmen_Bar_0*.wav sample/LICENSE.md
git commit -m "Built-in amen we own: Salamander render replaces the KAN cuts" -- sample/ src/lib.rs src/sample.rs README.md
```

---

### Task 2: Make the amen renderer reproducible

The shipped WAVs from Task 1 are canonical. This task makes the renderer rerunnable and proves it reproduces them closely. It is Python-only, with no Rust changes.

**Files:**
- Create:
  - `tools/amen/transcription.json`: copy of `target/amen-spike/transcription_native.json`, with the final per-hit nudges from the out4 run baked into each hit's time
  - `tools/amen/render_settings.json`: from `target/amen-spike/out4/out4_settings.json`, with `choice: "b"` and the EQ, snare EQ and crash cut recorded in the design
  - `tools/amen/README.md`
  - `tools/amen/check_render.py`
- Modify: `tools/amen/clone_amen.py` (add a `render-final` mode that reads only those two JSON files and a kit directory), and `.gitignore` (add `tools/**/__pycache__/`)
- Delete: `tools/amen/render_amen.py`, the superseded first spike (its docstring says throwaway)

**Interfaces:**
- Produces: `python3 tools/amen/clone_amen.py render-final <kit OH dir> <out dir>`, which writes `174_JunglizerAmen_Bar_0{1..4}.wav`. Also `python3 tools/amen/check_render.py <out dir> sample/`, which exits 0 on a match.

- [ ] **Step 1: Write `check_render.py`**

For each bar, it compares the candidate to the shipped WAV:
- **Length:** identical frame counts (60828).
- **Onsets:** every onset is within 0.5 ms. Use `onsets.py`.
- **Difference:** the residual RMS (candidate − shipped) is at least 30 dB below the shipped RMS.

It prints per-bar figures and exits 1 on any failure.

- [ ] **Step 2: Implement `render-final`**

- Read `transcription.json` and `render_settings.json`. It must not read any reference-audio directory.
- Use a fixed seed of 174 and the existing `render()` with the out4b options.

- [ ] **Step 3: Run it against the pinned kit and check it**

```bash
target/amen-spike/venv/bin/python tools/amen/clone_amen.py render-final target/amen-spike/salamander_drumkit_v1/OH target/amen-check
target/amen-spike/venv/bin/python tools/amen/check_render.py target/amen-check sample
```
Expected: `4/4 bars match`, exit 0. Adjust the kit OH path to the real folder inside `salamander_drumkit_v1`.

- [ ] **Step 4: Write `tools/amen/README.md`**

It covers:
- **Purpose:** the built-in amen, and that the renders in `sample/` are canonical.
- **Kit:** the Salamander Drumkit v1, with archive SHA-256 `a576ff79fcc33b53d8a373f459277e6727bd4e68d5383b9d69b1d9e0ec585329`. Find and record its public download URL.
- **Setup:** the venv (numpy and scipy).
- **Commands:** the two commands above.
- **The rule:** any new render that would replace `sample/` needs Todd's listen first.
- **History:** the `transcribe`/`native`/`out3`/`out4` modes need reference audio that is not in the repo and never will be.

- [ ] **Step 5: Commit**

```bash
git add tools/amen/transcription.json tools/amen/render_settings.json tools/amen/README.md tools/amen/check_render.py
git rm tools/amen/render_amen.py
git commit -m "Amen renderer: committed transcription, render-final, render check" -- tools/amen .gitignore
```

---

### Task 3: Decode WAV, AIFF and MP3 with Symphonia

**Files:**
- Create: `src/decode.rs`, `tools/fixtures/make_fixtures.sh`, and `tests/fixtures/`:
  - `loop_160_4beats_mono16_48k.wav`
  - `loop_160_4beats_stereo24_44k.aif`
  - `loop_160_4beats_float32_48k.wav`
  - `loop_160_4beats.mp3`
  - `truncated.wav`
  - `not_audio.wav`
  - `empty.wav`
- Modify: `Cargo.toml`, `src/sample.rs` (`load` / `from_bytes` delegate to `decode`), `src/lib.rs` (`mod decode;`)

**Interfaces:**
- Produces:
  - `pub enum DecodeError { Missing, Unreadable, Empty }`, deriving `Debug, Clone, Copy, PartialEq, Eq`
  - `pub fn decode_file(path: &Path) -> Result<StereoSample, DecodeError>`
  - `pub fn decode_bytes(bytes: &[u8], extension: Option<&str>) -> Result<StereoSample, DecodeError>`
  - `impl StereoSample { pub fn duration_secs(&self) -> f64 }`
  - `StereoSample::load` and `from_bytes` keep their names and now return `Result<Self, DecodeError>`

- [ ] **Step 1: Add the dependencies**

```toml
symphonia = { version = "0.6", default-features = false, features = ["wav", "aiff", "pcm", "mp3"] }
[dev-dependencies]
hound = "3.5"
```
- Move `hound` out of `[dependencies]`.
- Confirm the 0.6 feature names with `cargo add symphonia@0.6 --no-default-features -F ...` before writing code. 0.6 changed APIs from 0.5, so follow docs.rs for 0.6.1.

- [ ] **Step 2: Write `make_fixtures.sh` and generate the fixtures**

It uses ffmpeg (and lame for the MP3) to synthesize a click every beat at 160 BPM for 4 beats (1.5 s). This is our own audio. Then it writes:
- the four format variants named above
- `truncated.wav`: the first 100 bytes of the mono WAV
- `not_audio.wav`: the text “not audio”
- `empty.wav`: zero bytes

Run it and commit the outputs; together they should be under 1.5 MB.

- [ ] **Step 3: Write the failing tests in `src/decode.rs`**

```rust
fn fixture(name: &str) -> PathBuf { Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name) }

#[test]
fn every_supported_format_decodes_to_stereo_at_its_own_rate() {
    for (name, rate) in [
        ("loop_160_4beats_mono16_48k.wav", 48_000),
        ("loop_160_4beats_stereo24_44k.aif", 44_100),
        ("loop_160_4beats_float32_48k.wav", 48_000),
        ("loop_160_4beats.mp3", 44_100),
    ] {
        let s = decode_file(&fixture(name)).unwrap();
        assert_eq!(s.sample_rate, rate, "{name}");
        assert!((s.duration_secs() - 1.5).abs() <= 0.015, "{name}: {}", s.duration_secs());
        assert!(s.frames.iter().any(|f| f[0].abs() > 0.1 && f[1].abs() > 0.1), "{name}");
    }
}

#[test]
fn mono_is_copied_to_both_channels() {
    let s = decode_file(&fixture("loop_160_4beats_mono16_48k.wav")).unwrap();
    assert!(s.frames.iter().all(|f| f[0] == f[1]));
}

#[test]
fn bad_files_return_errors_not_panics() {
    assert_eq!(decode_file(&fixture("no_such_file.wav")).err(), Some(DecodeError::Missing));
    assert_eq!(decode_file(&fixture("not_audio.wav")).err(), Some(DecodeError::Unreadable));
    assert!(matches!(decode_file(&fixture("empty.wav")), Err(DecodeError::Unreadable | DecodeError::Empty)));
    assert!(decode_file(&fixture("truncated.wav")).is_err());
}
```

- [ ] **Step 4: Run them to confirm they fail**

Run: `cargo test --locked decode::`
Expected: FAIL, because `decode_file` is not defined yet.

- [ ] **Step 5: Implement `decode_file` and `decode_bytes`**

- Probe with the extension as a hint, and enable gapless MP3 so encoder padding doesn't skew the length.
- Convert any sample format to f32. Mono becomes `[x, x]`. Use the first two channels of anything wider.
- Return `Empty` when zero frames decode, and `Missing` for `io::ErrorKind::NotFound`.
- Rewire `StereoSample::load` / `from_bytes` to use these. Built-in WAVs now go through Symphonia too.

- [ ] **Step 6: Gate and commit**

Run: `cargo fmt --all -- --check && cargo test --locked`
Expected: PASS, including Task 1's sample tests through the new decoder.
```bash
git add src/decode.rs tools/fixtures/make_fixtures.sh tests/fixtures
git commit -m "Decode WAV, AIFF and MP3 with Symphonia" -- src/decode.rs src/sample.rs src/lib.rs Cargo.toml Cargo.lock tools/fixtures tests/fixtures
```

---

### Task 4: The tempo rule

**Files:**
- Create: `src/tempo.rs`
- Modify: `src/lib.rs` (`pub mod tempo;`), and the `tools/tempo/check_tempo_rule.py` docstring (add “Mirrors src/tempo.rs; keep them in step.”)

**Interfaces:**
- Produces:
```rust
pub const MIN_BEATS: u8 = 4;
pub const MAX_BEATS: u8 = 32;
pub const MIN_BPM: f64 = 80.0;
pub const MAX_BPM: f64 = 195.0;
pub const TOLERANCE: f64 = 0.01;
#[derive(Clone, Copy, Debug, PartialEq)] pub struct Reading { pub beats: u8, pub bpm: f64 }
#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum Refusal { TooShort, TooLong }
pub fn bpm_from_name(file_name: &str) -> Option<f64>;
pub fn read_length(duration_secs: f64, file_name: &str) -> Result<Reading, Refusal>;
pub fn reading_for(beats: u8, duration_secs: f64) -> Reading; // bpm = beats × 60 / duration
pub fn halved(r: Reading) -> Option<Reading>;
pub fn doubled(r: Reading) -> Option<Reading>;
impl Reading { pub fn label(&self) -> String } // "16 beats · 174.0 BPM"
impl Refusal { pub fn message(&self) -> &'static str }
```

Notes on the rule:
- “Not a whole number of bars” can't happen in the fastest-fit path, because the 4–32-beat ranges overlap across 1.23–24 s. So `Refusal` has only two variants. That keeps the spec's intent, and the plan records why.
- `Refusal::message`:
  - `TooShort`: “That file is too short. Breaks need at least 4 beats at 195 BPM or slower.”
  - `TooLong`: “That file is too long. Breaks can be up to 32 beats.”

- [ ] **Step 1: Write the failing tests**

```rust
fn ok(name: &str, secs: f64) -> (u8, f64) { let r = read_length(secs, name).unwrap(); (r.beats, (r.bpm * 10.0).round() / 10.0) }

#[test]
fn filename_tempo_wins_over_fastest_fit() {
    assert_eq!(ok("KTB1_114_ThinkBreak_Original_NoVocal.wav", 4.211), (8, 114.0));
    assert_eq!(ok("KSB1_119.9_SportBreak_Original_Vinyl_HD.wav", 8.007), (16, 119.9));
    assert_eq!(ok("loop 174 BPM.aif", 60_828.0 / 44_100.0), (4, 174.0));
    assert_eq!(ok("loop_174bpm.mp3", 60_828.0 / 44_100.0), (4, 174.0));
}

#[test]
fn longer_than_32_beats_at_the_named_tempo_is_refused() {
    assert_eq!(read_length(22.588, "AJ_170_Drum_Loop_Continuum.wav"), Err(Refusal::TooLong));
    assert_eq!(read_length(14.458, "AJ_166_Drum_Loop_Boxing.wav"), Err(Refusal::TooLong));
}

#[test]
fn without_a_usable_hint_the_fastest_fit_decides() {
    assert_eq!(ok("174_JunglizerAmen_Bar_01.wav", 60_828.0 / 44_100.0), (4, 174.0));
    assert_eq!(ok("Break 2.wav", 2.759), (8, 174.0));
    assert_eq!(ok("loop.wav", 22.588), (32, 85.0)); // known limit, documented in the design
    // A named tempo that doesn't make whole bars is ignored.
    assert_eq!(ok("KSCB1_114_ScorpioBreak_00_Original_Vinyl.wav", 6.665), (20, 180.0));
}

#[test]
fn outside_the_range_is_refused() {
    assert_eq!(read_length(1.0, "x.wav"), Err(Refusal::TooShort));
    assert_eq!(read_length(30.0, "x.wav"), Err(Refusal::TooLong));
}

#[test]
fn name_numbers_that_are_not_tempos_are_ignored() {
    for name in ["Bar_01.wav", "Break 2.wav", "808 Kick.wav", "Session 2024.wav", "AmenBreak_Cut_02.wav", "KAPB1.wav"] {
        assert_eq!(bpm_from_name(name), None, "{name}");
    }
    assert_eq!(bpm_from_name("AJ_166_Drum_Loop_Baracks.wav"), Some(166.0));
    assert_eq!(bpm_from_name("KSB1_119.9_SportBreak.wav"), Some(119.9));
    assert_eq!(bpm_from_name("70_slow_loop.wav"), None); // outside 80–195
}

#[test]
fn half_and_double_stay_inside_the_rule() {
    let r = reading_for(16, 16.0 * 60.0 / 174.0);
    assert_eq!(halved(r).map(|h| h.beats), Some(8));
    assert_eq!(doubled(r), None);           // 348 BPM
    assert_eq!(halved(reading_for(4, 1.38)), None); // 2 beats isn't whole bars
    assert_eq!(halved(reading_for(12, 4.14)), None); // 6 beats isn't whole bars
    assert_eq!(reading_for(16, 16.0 * 60.0 / 174.0).label(), "16 beats · 174.0 BPM");
}
```

- [ ] **Step 2: Run them to confirm they fail**

Run: `cargo test --locked tempo::`
Expected: FAIL (unresolved items).

- [ ] **Step 3: Implement it**

`bpm_from_name`:
- Take the file stem and split it on `_`, `-`, space and `.`, except a `.` between digits.
- A candidate is a token matching `^\d{2,3}(\.\d)?$`, or such a number followed by `bpm` (case-insensitive) in the same token, or a number token followed by a `bpm` token.
- Return the first candidate in 80–195.
- Don't use the regex crate; scan by hand.

`read_length`:
- With a hint `h`: `exact = h × duration / 60`, `beats = round(exact / 4) × 4`.
- If `beats > 0` and `|exact − beats| ≤ beats × TOLERANCE`, then `beats > 32` returns `TooLong` and 4–32 returns the reading.
- Otherwise use fastest fit, keeping the largest beat count whose BPM lies in `[80 × 0.99, 195 × 1.01]`.
- When nothing fits, return `TooShort` if `duration < 4 × 60 / 195`, else `TooLong`.

- [ ] **Step 4: Run them to confirm they pass**

Run: `cargo test --locked tempo::`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/tempo.rs
git commit -m "Tempo rule: filename hint, fastest fit, half/double" -- src/tempo.rs src/lib.rs tools/tempo/check_tempo_rule.py
```

---

### Task 5: Project state with four break slots

This replaces the shared `source_count` with a per-slot slice count, and adds the slot kind, path, beats and frame count. It changes the format to `JZP3`. Beta 1 blobs (`JZP1`/`JZP2`) return `None`, so `load_state` reports a malformed blob and the instance stays fresh. The spec allows that.

**Files:**
- Modify:
  - `src/edit_state.rs`
  - `src/suggest.rs`: use `state.slice_count(source_loop)`
  - `src/live.rs`: `counts` becomes an `AtomicU64` packing `player_slots` and four slice counts, and `CapturedPhrase.source_count` becomes `slice_counts: [u8; 4]`
  - `src/lib.rs`: `sync_controls`, `load_sources`, `load_state` and the tests, for the new names only
  - `src/ui.rs`: compile-only renames. Its real UI changes are Task 8.
  - `src/ui_tests.rs`

**Interfaces:**
- Produces:
```rust
pub const MAX_PATH_BYTES: usize = 1024;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)] #[repr(u8)]
pub enum SlotKind { #[default] Builtin = 0, User = 1, Blank = 2 }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BreakSlot { pub kind: SlotKind, pub beats: u8, pub slice_count: u8, pub frames: u32, pub path_len: u16, pub path: [u8; MAX_PATH_BYTES] }
impl BreakSlot {
    pub fn builtin() -> Self;                 // Builtin, beats 4, slice_count 8, frames 0, no path
    pub fn path(&self) -> Option<&str>;       // None unless User
    pub fn path_hash(&self) -> u64;           // FNV-1a 64 of the path bytes; 0 when no path
}
pub fn path_hash(bytes: &[u8]) -> u64;        // shared with library.rs
// EditState: `slots: [BreakSlot; LOOP_COUNT]` replaces `source_count`.
impl EditState {
    pub fn slice_count(&self, source_loop: usize) -> u8;
    pub fn set_slice_count(&mut self, source_loop: usize, count: u8) -> bool; // deactivates that loop's placements with slice ≥ count
    pub fn load_user(&mut self, slot: usize, path: &str, frames: u32, beats: u8, markers: &[SliceMarker]) -> Result<(), SlotError>;
    pub fn unload(&mut self, slot: usize) -> bool;
    pub fn relink(&mut self, slot: usize, path: &str, frames: u32, beats: u8) -> Result<(), SlotError>;
    pub fn set_beats(&mut self, slot: usize, beats: u8) -> bool; // User slots only; beats ∈ 4..=32, multiple of 4
    pub fn restore_builtin(&mut self);         // all four Builtin, marker sets invalid, placements cleared
    pub fn is_pristine(&self) -> bool;         // all Builtin at 8 slices, no active placements, not frozen, not live
    pub fn freeze_phrase(&mut self, placements: [StepPlacement; MAX_STEPS], slice_counts: [u8; LOOP_COUNT], player_slots: u8, bars: [u8; 4]);
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum SlotError { PathTooLong, BadSlot, BadBeats }
impl SlotError { pub fn message(&self) -> &'static str }
```

Rules the tests pin down:
- **`load_user`:** sets `kind = User` and copies the path. It invalidates every marker set of that slot, sets `slice_count = markers.len()`, stores the markers as that count's valid set, and deactivates every placement whose `source_loop == slot`, locked ones included.
- **`unload`:** sets `kind = Blank`, clears the path and beats, invalidates the marker sets and deactivates the slot's placements.
- **`relink`:** keeps the placements. It scales every valid marker of the slot by `new_frames / old_frames`, clamps it to `new_frames`, and re-establishes `start < end` and contiguity. Then it sets the path, frames and beats.
- **`place`:** rejects a `Blank` source loop.
- **Built-in markers:** `frames` stays 0 for Builtin. The DSP knows the length.
- **`SlotError::PathTooLong` message:** “That file’s path is too long. Move it to a shorter folder.”

- [ ] **Step 1: Write the failing tests in `src/edit_state.rs`**

```rust
fn marks(n: u8, frames: u32) -> Vec<SliceMarker> {
    (0..n as u32).map(|i| SliceMarker { start: frames * i / n as u32, end: frames * (i + 1) / n as u32, active: true, bass: i == 0 }).collect()
}

#[test]
fn loading_a_user_break_replaces_only_that_slot() {
    let mut s = EditState::default();
    for slot in [0_u8, 1] {
        assert!(s.place(StepPlacement { active: true, start_slot: slot * 4, span_slots: 1, source_loop: slot, source_slice: 0, mode: 0, bit_depth: 16, repeats: 1, locked: slot == 0 }));
    }
    s.load_user(0, "/breaks/Think 114.wav", 200_000, 8, &marks(16, 200_000)).unwrap();
    assert_eq!(s.slots[0].kind, SlotKind::User);
    assert_eq!(s.slots[0].path(), Some("/breaks/Think 114.wav"));
    assert_eq!(s.slice_count(0), 16);
    assert_eq!(s.slice_count(1), 8);
    assert!(s.placements.iter().all(|p| !p.active || p.source_loop != 0)); // even the locked one
    assert!(s.placements.iter().any(|p| p.active && p.source_loop == 1));
}

#[test]
fn unload_blanks_the_slot_and_refuses_placements_on_it() {
    let mut s = EditState::default();
    assert!(s.unload(2));
    assert_eq!(s.slots[2].kind, SlotKind::Blank);
    assert!(!s.place(StepPlacement { active: true, start_slot: 0, span_slots: 1, source_loop: 2, source_slice: 0, mode: 0, bit_depth: 16, repeats: 1, locked: false }));
}

#[test]
fn relink_scales_markers_and_keeps_placements() {
    let mut s = EditState::default();
    s.load_user(1, "/old/a.wav", 100_000, 8, &marks(4, 100_000)).unwrap();
    assert!(s.place(StepPlacement { active: true, start_slot: 0, span_slots: 2, source_loop: 1, source_slice: 3, mode: 0, bit_depth: 16, repeats: 1, locked: true }));
    s.relink(1, "/new/a.wav", 50_000, 8).unwrap();
    let set = s.marker_set(1).unwrap();
    assert_eq!(set.slices[1].start, 12_500);
    assert_eq!(set.slices[3].end, 50_000);
    assert!(s.placements.iter().any(|p| p.active && p.source_loop == 1 && p.locked));
}

#[test]
fn per_slot_slice_counts_are_independent() {
    let mut s = EditState::default();
    assert!(s.set_slice_count(3, 24));
    assert_eq!((s.slice_count(0), s.slice_count(3)), (8, 24));
}

#[test]
fn half_double_only_on_user_slots_and_whole_bars() {
    let mut s = EditState::default();
    assert!(!s.set_beats(0, 8));
    s.load_user(0, "/a.wav", 1000, 16, &marks(8, 1000)).unwrap();
    assert!(s.set_beats(0, 8));
    assert!(!s.set_beats(0, 6));
    assert!(!s.set_beats(0, 36));
}

#[test]
fn overlong_path_is_refused() {
    let mut s = EditState::default();
    let long = format!("/{}", "a".repeat(MAX_PATH_BYTES));
    assert_eq!(s.load_user(0, &long, 1000, 4, &marks(8, 1000)), Err(SlotError::PathTooLong));
    assert_eq!(s.slots[0].kind, SlotKind::Builtin);
}

#[test]
fn v3_roundtrips_slots_and_beta1_blobs_are_rejected() {
    let mut s = EditState::default();
    s.load_user(2, "C:\\Breaks\\Amen 172.wav", 99_000, 16, &marks(12, 99_000)).unwrap();
    s.unload(3);
    let bytes = s.serialize();
    assert_eq!(&bytes[..4], b"JZP3");
    assert_eq!(EditState::deserialize(&bytes), Some(s));
    let mut v2 = bytes.clone();
    v2[..4].copy_from_slice(b"JZP2");
    assert_eq!(EditState::deserialize(&v2), None);
    assert_eq!(EditState::deserialize(&bytes[..bytes.len() - 1]), None);
}

#[test]
fn pristine_means_untouched_built_ins() {
    let mut s = EditState::default();
    assert!(s.is_pristine());
    s.set_slice_count(0, 16);
    assert!(!s.is_pristine());
}
```
Delete `legacy_pattern_loads_with_new_controls_off`, and update the other existing tests to the new names.

- [ ] **Step 2: Run them to confirm they fail**

Run: `cargo test --locked edit_state::`
Expected: FAIL (missing items).

- [ ] **Step 3: Implement the new state and serialization**

Serialization:
- Magic `JZP3`.
- After the header, write each slot as `kind u8, beats u8, slice_count u8, frames u32 LE, path_len u16 LE, path [u8; 1024]`.
- `SERIALIZED_LEN` stays a fixed constant, so state loads remain allocation-free.

`deserialize` must reject:
- an unknown kind
- `path_len > 1024`
- a User slot with an empty path
- beats outside 4–32 or not a multiple of 4 on a User slot
- a slice count outside 1–32

- [ ] **Step 4: Ripple the rename through the other modules**

- `suggest.rs`, `live.rs` and `lib.rs` use `slice_count(loop)`.
- In `sync_controls`, a `source_slices` change calls `set_slice_count(current.source_loop, value)`. If that count's marker set is invalid, the DSP fills it with even, active slices over the source's frames, without allocating.
- In `load_state`, the existing merge of invalid marker sets from built-in suggestions applies only to `Builtin` slots.

- [ ] **Step 5: Gate**

Run: `cargo fmt --all -- --check && cargo test --locked`
Expected: PASS (all crates, including `suggest`, `live`, `midi` and the audio tests).

- [ ] **Step 6: Commit**

```bash
git commit -m "State v3: four break slots with per-slot slice counts" -- src/edit_state.rs src/suggest.rs src/live.rs src/lib.rs src/ui.rs src/ui_tests.rs
```

---

### Task 6: An engine for breaks of any length

**Files:**
- Create: `src/library.rs`, containing only `LoadedBreak` in this task
- Modify: `src/sample.rs` (`at_beat`), `src/lib.rs` (sources, ghost, fallback, bass onsets, audition, marker-position range)

**Interfaces:**
- Consumes: `EditState::slots`, `BreakSlot::beats`, `SlotKind` (Task 5); `Reading` (Task 4)
- Produces:
```rust
// library.rs
pub struct LoadedBreak { pub sample: StereoSample, pub path_hash: u64, pub reading: Reading, pub suggested: [SliceMarker; MAX_STEPS] } // suggested for 8 slices
impl LoadedBreak { pub fn new(sample: StereoSample, path_hash: u64, reading: Reading) -> Self } // computes `suggested`
// sample.rs
pub fn at_beat(&self, beat: f64, beats: f64, channel: usize) -> f32; // 0 outside 0..beats
// lib.rs, on Junglizer
sources: [Option<Arc<LoadedBreak>>; LOOP_COUNT],
fn playable(&self, pattern: &EditState, slot: usize) -> bool;            // audio present and kind != Blank
fn ghost_source(&self, pattern: &EditState, bar_slot: usize) -> Option<usize>; // bar_slot if playable, else lowest playable
fn ghost_onsets(source_beat: f64, beats: f64) -> impl Iterator<Item = f64>; // phrase positions in 0..16
```

Rules:
- **Ghost position:** in phrase beat `b` (0–16), the ghost source plays at source beat `b.rem_euclid(beats)`. A 4-beat break repeats every bar, an 8-beat break plays twice, and 16 or more plays its first 16 beats.
- **Ghost bass onsets:** occur at `source_beat + k × beats` for k ≥ 0 while under 16, within the bar that uses that source.
- **Placements on an unplayable slot** are silent.
- **`load_sources`:** builds the built-in `LoadedBreak`s, with reading 4 beats at 174 BPM and hash 0.

- [ ] **Step 1: Write the failing tests in `lib.rs` `audio_tests`**

```rust
fn ramp(frames: usize, rate: u32) -> Option<Arc<LoadedBreak>> {
    let s = StereoSample { frames: (0..frames).map(|i| [i as f32 / frames as f32; 2]).collect(), sample_rate: rate };
    Some(Arc::new(LoadedBreak::new(s, 1, tempo::reading_for(4, 1.0))))
}

#[test]
fn eight_beat_break_ghost_plays_twice_across_the_phrase() {
    let mut st = Junglizer::default();
    st.sources[0] = ramp(8_000, 44_100);
    st.edit.load_user(0, "/a.wav", 8_000, 8, &[SliceMarker { start: 0, end: 8_000, active: true, bass: false }; 8]).unwrap();
    let bars = [0; 4];
    let at = |b| st.frame_at_beat(&st.edit, b, 174.0, &bars)[0];
    assert!((at(1.0) - at(9.0)).abs() < 1e-6);
    assert!((at(5.0) - 0.625).abs() < 0.01);
}

#[test]
fn thirty_two_beat_break_plays_its_first_sixteen_beats() {
    let mut st = Junglizer::default();
    st.sources[0] = ramp(32_000, 44_100);
    st.edit.load_user(0, "/a.wav", 32_000, 32, &[SliceMarker { start: 0, end: 32_000, active: true, bass: false }; 8]).unwrap();
    assert!((st.frame_at_beat(&st.edit, 15.0, 174.0, &[0; 4])[0] - 15.0 / 32.0).abs() < 0.01);
}

#[test]
fn blank_or_missing_bar_plays_lowest_loaded_slot() {
    let mut st = Junglizer::default();
    st.sources = [None, None, ramp(4_000, 44_100), None]; // 0 missing (no audio), 1 and 3 blank
    st.edit.unload(1);
    st.edit.unload(3);
    assert_eq!(st.ghost_source(&st.edit, 1), Some(2));
    assert_eq!(st.ghost_source(&st.edit, 0), Some(2));
    st.sources[2] = None;
    assert_eq!(st.ghost_source(&st.edit, 0), None);
    assert_eq!(st.frame_at_beat(&st.edit, 3.0, 174.0, &[0; 4]), [0.0; 2]);
}

#[test]
fn ghost_bass_onsets_repeat_with_the_break_length() {
    let onsets: Vec<f64> = Junglizer::ghost_onsets(1.0, 8.0).collect();
    assert_eq!(onsets, vec![1.0, 9.0]);
    assert_eq!(Junglizer::ghost_onsets(17.0, 32.0).count(), 0);
}

#[test]
fn ghost_follows_beats_not_sample_rate() {
    let mut st = Junglizer::default();
    st.sources[0] = ramp(96_000, 96_000);
    st.edit.load_user(0, "/a.wav", 96_000, 4, &[SliceMarker { start: 0, end: 96_000, active: true, bass: false }; 8]).unwrap();
    assert!((st.frame_at_beat(&st.edit, 2.0, 120.0, &[0; 4])[0] - 0.5).abs() < 0.01);
}

#[test]
fn original_speed_slice_uses_file_rate_not_host_rate() {
    // One beat at 120 BPM is 0.5 s: 24,000 frames of a 48 kHz file, whatever the host rate.
    let mut st = Junglizer::default();
    st.sources[0] = ramp(96_000, 48_000);
    st.edit.load_user(0, "/a.wav", 96_000, 4, &[SliceMarker { start: 0, end: 96_000, active: true, bass: false }; 1]).unwrap();
    let p = StepPlacement { active: true, start_slot: 0, span_slots: 32, source_loop: 0, source_slice: 0, mode: 0, bit_depth: 16, repeats: 1, locked: false };
    assert!((st.slice_frame(&st.edit, &p, 1.0, 120.0, 32)[0] - 0.25).abs() < 0.01);
}
```
Update `ghost_player_routes_each_bar_and_stops_with_transport` and `placed_slice_overrides_ghost_then_ghost_resumes` to build `sources` with `Some(Arc::new(LoadedBreak::new(...)))`.

- [ ] **Step 2: Run them to confirm they fail**

Run: `cargo test --locked audio_tests::`
Expected: FAIL (missing `LoadedBreak` and `ghost_source`).

- [ ] **Step 3: Implement it**

- Thread `beats` through `at_beat`, `frame_at_beat`, `midi_frame`, `bass_onset_between`, the audition and the marker-position clamp.
- Replace every hard-coded `4.0` that means “source length in beats” with the slot's beats. Leave the `4.0` meaning “beats per bar” alone.

- [ ] **Step 4: Gate and commit**

Run: `cargo fmt --all -- --check && cargo test --locked`
Expected: PASS.
```bash
git add src/library.rs
git commit -m "Engine: breaks of 4 to 32 beats, ghost alignment, blank-slot fallback" -- src/library.rs src/sample.rs src/lib.rs
```

---

### Task 7: The library: loader thread and audio handoff

**Files:**
- Modify: `src/library.rs`, `src/lib.rs` (`#[skip] pub library: Library` on `JunglizerParams`, `init` starts it, `process` reconciles), `Cargo.toml` (`crossbeam-queue = "0.3"`)

**Interfaces:**
- Consumes: `decode_file` (Task 3); `read_length`, `Reading` (Task 4); `path_hash`, `BreakSlot` (Task 5); `LoadedBreak` (Task 6)
- Produces:
```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum Purpose { Restore, Choose }
#[derive(Clone)] pub enum LoadOutcome { Loaded(Arc<LoadedBreak>), Refused(Refusal), Unreadable, Missing }
#[derive(Clone)] pub struct SlotResult { pub seq: u64, pub path_hash: u64, pub outcome: LoadOutcome }
pub struct Library { /* Arc<Inner> */ }
impl Default for Library   // no thread yet
impl Library {
    pub fn start(&self);                                                    // spawns the loader once; never call from the audio thread
    pub fn request(&self, slot: usize, path: &[u8], purpose: Purpose) -> Option<u64>; // try_lock; None if busy (caller retries)
    pub fn result(&self, slot: usize, purpose: Purpose) -> Option<SlotResult>;        // locks; editor only
    pub fn try_adopt(&self, slot: usize, path_hash: u64) -> Option<LoadOutcome>;      // try_lock on both result slots; audio-safe (clones an Arc)
    pub fn audio_for(&self, slot: usize, path_hash: u64) -> Option<Arc<LoadedBreak>>; // editor: the loaded break matching the state's path
    pub fn retire(&self, old: Arc<LoadedBreak>) -> Result<(), Arc<LoadedBreak>>;      // ArrayQueue push, capacity 16
    pub fn run_pending(&self);                                              // the loader body; tests call it directly
    pub fn collect_garbage(&self);                                          // drains the retire queue (loader and editor)
    pub fn builtin() -> &'static [Arc<LoadedBreak>; LOOP_COUNT];           // OnceLock; first touched in init
}
```

Design decisions:
- **Request slots:** each slot has a `Mutex<PathBuf32>`, a fixed `{ len: u16, bytes: [u8; 1024] }`, plus an `AtomicU64` sequence, per purpose. Requests copy path bytes with `try_lock` and no allocation, then `unpark` the loader thread.
- **Loader:** decodes, runs `read_length` with the file name, builds `LoadedBreak::new`, and publishes a `SlotResult` into that purpose's result slot.
  - A refusal publishes `Refused`, which the DSP never adopts.
  - A non-UTF-8 path, or a decode error other than `Missing`, publishes `Unreadable`.
- **DSP reconcile:** at the top of every `process` block, for each slot:
  - **Builtin:** hold `builtin()[slot]`.
  - **User, with `held.path_hash != slot.path_hash()`:** call `try_adopt`.
    - `Loaded`: swap it in, and `retire` the old one.
    - `Missing` / `Unreadable`: hold `None`.
    - Nothing yet, and `requested[slot] != hash`: post a `Restore` request and set `requested[slot] = hash`.
  - **Blank:** retire and hold `None`.
  - **A failed `retire`:** park the Arc in `Junglizer.overflow: [Option<Arc<LoadedBreak>>; 8]` and retry next block. Overflow is never dropped on the audio thread.
- **Thread lifetime:** the loader holds a `Weak<Inner>`, and `Drop for Library` unparks it so it exits.

- [ ] **Step 1: Write the failing tests in `src/library.rs` and `lib.rs`**

```rust
#[test]
fn choose_publishes_a_loaded_break_with_its_reading() {
    let lib = Library::default();
    let path = fixture("loop_160_4beats_mono16_48k.wav");
    let seq = lib.request(0, path.to_str().unwrap().as_bytes(), Purpose::Choose).unwrap();
    lib.run_pending();
    let r = lib.result(0, Purpose::Choose).unwrap();
    assert_eq!(r.seq, seq);
    assert_eq!(r.path_hash, path_hash(path.to_str().unwrap().as_bytes()));
    let LoadOutcome::Loaded(b) = r.outcome else { panic!("not loaded") };
    assert_eq!((b.reading.beats, (b.reading.bpm * 10.0).round() / 10.0), (4, 160.0));
}

#[test]
fn too_long_and_unreadable_files_are_not_loaded() {
    let lib = Library::default();
    lib.request(1, fixture("not_audio.wav").to_str().unwrap().as_bytes(), Purpose::Choose);
    lib.run_pending();
    assert!(matches!(lib.result(1, Purpose::Choose).unwrap().outcome, LoadOutcome::Unreadable));
}

#[test]
fn missing_file_is_reported_and_dsp_plays_fallback() {
    let params = JunglizerParams::default();
    let mut st = Junglizer::load_sources();
    st.edit.load_user(0, "/nowhere/gone 174.wav", 60_828, 4, &[SliceMarker { start: 0, end: 60_828, active: true, bass: false }; 8]).unwrap();
    st.reconcile_sources(&params.library);   // posts Restore
    params.library.run_pending();
    st.reconcile_sources(&params.library);   // sees Missing
    assert!(st.sources[0].is_none());
    assert_eq!(st.ghost_source(&st.edit, 0), Some(1));
    assert!(st.edit.marker_set(0).unwrap().valid); // markers kept for Relink
}

#[test]
fn adopting_and_retiring_breaks_never_allocates_on_audio_thread() {
    // Under `--features rt-paranoid`: load a user break through the library,
    // then run `process` inside truce's `assert_no_audio_alloc` while the DSP
    // adopts it, then unload the slot and process again (retire path).
}
```
Add `fn reconcile_sources(&mut self, library: &Library)` on `Junglizer`, called first in `process`. Write the rt-paranoid test body with truce's audio-testing helpers; look up the exact names in truce-core 6.3 (`enable_rt_paranoid!` is already in `lib.rs`).

- [ ] **Step 2: Run them to confirm they fail**

Run: `cargo test --locked library:: audio_tests::missing`
Expected: FAIL.

- [ ] **Step 3: Implement the library, the reconcile and `init` → `library.start()`**

- [ ] **Step 4: Run the full gate, including rt-paranoid**

Run: `cargo fmt --all -- --check && cargo test --locked && cargo test --locked --features rt-paranoid`
Expected: PASS. The rt-paranoid run must report no audio-thread allocation.

- [ ] **Step 5: Commit**

```bash
git commit -m "Library: loader thread and lock-free handoff of loaded breaks" -- src/library.rs src/lib.rs Cargo.toml Cargo.lock
```

---

### Task 8: Slot controls in the editor

**Files:**
- Create: `src/ui_slots.rs`, a second `impl JunglizerUi` block holding the slot row, the unload modal and choose/relink handling
- Modify:
  - `src/ui.rs`:
    - `new()` no longer reads `CARGO_MANIFEST_DIR/sample` from disk. That is a beta 1 bug: shipped builds can't find that path. Waveforms come from `Library::builtin()` / `audio_for`.
    - Waveforms are cached per slot and keyed by path hash.
    - The “1 bar · 4 beats · 174 BPM” label becomes the selected slot's reading.
    - The “shared by all 4” label goes.
    - Bar lines are drawn every 4 beats.
  - `src/ui_tests.rs`: fixtures `user-16-beats`, `blank-slot`, `missing`, `unload-modal`
  - `Cargo.toml`: `rfd = "0.17"`. On Linux use rfd's default xdg-portal backend, so no GTK build dependency.

**Interfaces:**
- Consumes: `Library` (Task 7); `EditState::{load_user, unload, relink, set_beats, set_slice_count}` and `SlotError` (Task 5); `Reading::label`, `halved`, `doubled`, `reading_for`, `Refusal::message` (Task 4)
- Produces:
```rust
// ui_slots.rs
pub(crate) enum PendingLoad { None, Choosing { slot: usize, seq: u64, path: String, mode: LoadMode } }
pub(crate) enum LoadMode { Load, Relink, FirstRun } // FirstRun also clears slots 2–4 (Task 9 uses it)
impl JunglizerUi {
    pub(crate) fn choose_file(&mut self, slot: usize, mode: LoadMode);       // rfd dialog → library.request(Choose)
    pub(crate) fn poll_pending_load(&mut self, ctx: &egui::Context);          // commits a Loaded result; shows refusals
    pub(crate) fn slot_row(&mut self, ui: &mut Ui, context: &PluginContext<JunglizerParams>);
}
```

Behavior and copy (exact):
- **The Break 1–4 buttons stay.** A Blank slot's button reads “Empty”, and clicking it opens the file dialog, filtered to “Audio” `wav, aif, aiff, mp3`.
- **The selected User slot** shows its reading, e.g. “16 beats · 174.0 BPM”, and the buttons:
  - **½** and **×2**, each disabled when `halved` or `doubled` returns `None`; they call `set_beats`
  - **Unload**
  - The file name as a tooltip on the reading
- **A User slot with a `Missing` restore result** shows “Missing” in `MUTED` plus **Relink…** (`LoadMode::Relink`).
- **Unload** opens an `egui::Modal`: “Unload this break? Its slices and placements in this project will be removed.” with **Unload** and **Cancel**.
- **Loading, unloading, relinking and half/double** are disabled while frozen or live, with the hover text “Unfreeze or stop live variations to change breaks.”
- **Source slices** edits the selected slot's count: `set_slice_count` plus `context.automate(P::SourceSlices, …)`. Selecting a break automates `SourceSlices` to that slot's count.
- **On a `Loaded` result:**
  - `Load`: `load_user(slot, path, frames, reading.beats, &suggested[..8])`
  - `Relink`: `relink(...)`
  - Message: “Loaded {file name}: {reading label}. Use ½ or ×2 if the tempo is off.”
- **On `Refused(r)`:** show `r.message()`. **On `Unreadable`:** “Junglizer couldn’t read that file. Use a WAV, AIFF or MP3.” **On `SlotError`:** its message. In every failure case the slot is unchanged.
- **Polling:** while a load is pending, `ctx.request_repaint_after(50 ms)`, and call `library.collect_garbage()` once per frame.

- [ ] **Step 1: Write the failing logic test**

The commit path is pure, so test it without the GPU. Put it in `ui_slots.rs`:

```rust
#[test]
fn a_refused_choice_leaves_the_slot_unchanged() {
    let mut s = EditState::default();
    let before = s.clone();
    apply_result(&mut s, 0, "/x/long.wav", LoadMode::Load, &LoadOutcome::Refused(Refusal::TooLong)).unwrap_err();
    assert_eq!(s, before);
}

#[test]
fn first_run_choice_loads_slot_one_and_clears_the_others() {
    let mut s = EditState::default();
    let b = Arc::new(LoadedBreak::new(StereoSample { frames: vec![[0.1; 2]; 60_828], sample_rate: 44_100 }, 7, tempo::reading_for(4, 60_828.0 / 44_100.0)));
    apply_result(&mut s, 0, "/x/my 174.wav", LoadMode::FirstRun, &LoadOutcome::Loaded(b)).unwrap();
    assert_eq!(s.slots.map(|x| x.kind), [SlotKind::User, SlotKind::Blank, SlotKind::Blank, SlotKind::Blank]);
}
```
`pub(crate) fn apply_result(state: &mut EditState, slot: usize, path: &str, mode: LoadMode, outcome: &LoadOutcome) -> Result<Reading, &'static str>` returns the message to show on failure.

- [ ] **Step 2: Run it to confirm it fails**

Run: `cargo test --locked ui_slots::`
Expected: FAIL.

- [ ] **Step 3: Implement `apply_result`, then the UI around it**

- [ ] **Step 4: Run the gate and render the fixtures**

Run: `cargo fmt --all -- --check && cargo test --locked && cargo test render_editor_states -- --ignored --nocapture`
Expected: PASS, and the new PNGs are written. Look at each one: `user-16-beats`, `blank-slot`, `missing`, `unload-modal`, plus every existing state. Check that nothing overlaps and the copy is curly.

- [ ] **Step 5: Manual check in a host**

```bash
cargo truce build --au2 --vst3 --clap && cargo truce install --au2 --user --no-build
```
Then in Logic or Reaper:
1. Load a 16-beat WAV, an AIFF and an MP3.
2. Halve and double.
3. Unload.
4. Save the project, rename the file, reopen and Relink.
5. Confirm playback doesn't glitch when loading during transport.

Report what you did and what you saw.

- [ ] **Step 6: Commit**

```bash
git add src/ui_slots.rs
git commit -m "Editor: load, unload, relink and half/double on each break slot" -- src/ui_slots.rs src/ui.rs src/ui_tests.rs Cargo.toml Cargo.lock
```

---

### Task 9: First launch, the empty state and the credit line

**Files:**
- Create: `src/platform.rs`, with only `pub fn first_run_flag() -> Option<PathBuf>` (`dirs::config_dir()/Junglizer/first-run-0.9.1`)
- Modify: `src/ui.rs`, `src/ui_slots.rs`, `src/ui_tests.rs` (fixtures `first-run`, `empty`), `Cargo.toml` (`dirs = "7"`)

**Interfaces:**
- Consumes: `LoadMode::FirstRun`, `choose_file` (Task 8); `EditState::is_pristine`, `restore_builtin` (Task 5)
- Produces: `pub(crate) fn show_first_run(flag_exists: bool, state: &EditState) -> bool` (true only when there is no flag and the state is pristine), and `pub(crate) fn mark_first_run_done()`

Behavior:
- **When it's decided:** in `opened()`, compute `self.first_run = show_first_run(flag_path.exists(), state)`. If `config_dir()` is `None`, treat the flag as present, so the prompt never nags.
- **First run:**
  - The source waveform area becomes a dashed `ACCENT`-outlined target, and the player below draws at reduced opacity and ignores clicks.
  - Text: “Choose your own break (4 to 32 beats, 80–195 BPM)”. See the drag-and-drop note under Task 13.
  - Buttons: **Choose…** (`choose_file(0, FirstRun)`) and **Keep the amen**.
  - Either button writes the flag (create the folder, then an empty file) and clears `first_run`. Choose writes it only once the load succeeds, so a refused file keeps the prompt up.
- **Empty state:** when no slot is playable (all Blank, or User with a Missing or Unreadable result), the same target appears with a **Use the Junglizer amen** link that calls `restore_builtin()`, then `ensure_markers` for each slot.
- **Credit:** in the bottom status row, right-aligned, `MUTED`, small: “Built-in breaks played on the Salamander Drumkit by Alexander Holm (CC BY-SA 3.0)”.

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn first_run_shows_only_on_a_fresh_instance_without_the_flag() {
    let mut s = EditState::default();
    assert!(show_first_run(false, &s));
    assert!(!show_first_run(true, &s));
    s.set_slice_count(0, 16);
    assert!(!show_first_run(false, &s));
}
```

- [ ] **Step 2: Run it to confirm it fails**

Run: `cargo test --locked first_run`
Expected: FAIL.

- [ ] **Step 3: Implement it**

- [ ] **Step 4: Gate, render and look**

Run: `cargo fmt --all -- --check && cargo test --locked && cargo test render_editor_states -- --ignored --nocapture`
Expected: PASS. Then view `first-run.png` and `empty.png`: the target is visible, the player is dimmed and the copy is curly.

- [ ] **Step 5: Commit**

```bash
git add src/platform.rs
git commit -m "First-launch break prompt, empty-state target, Salamander credit" -- src/platform.rs src/ui.rs src/ui_slots.rs src/ui_tests.rs Cargo.toml Cargo.lock
```

---

### Task 10: Platform paths, reveal and neutral copy

**Files:**
- Modify: `src/platform.rs`, `src/midi.rs` (`export_to_music` uses `platform::export_dir()`), `src/ui.rs` (export button, messages, reveal)

**Interfaces:**
- Produces:
```rust
pub fn export_dir() -> Option<PathBuf>;                      // dirs::audio_dir() or home/Music, joined with "Junglizer/Exports"
pub fn reveal_command(path: &Path) -> (&'static str, Vec<OsString>); // macOS: open -R <path>; Windows: explorer /select,<path>; Linux: xdg-open <parent>
pub const REVEAL_LABEL: &str;                               // "Show in Finder" | "Show in Explorer" | "Show in Files", by cfg(target_os)
```

Copy (exact):
- The button “Export MIDI to Finder” becomes “Export MIDI”.
- After an export, a small button labelled `REVEAL_LABEL` appears next to the message. Keep the automatic reveal only on macOS, matching today's behavior.
- Success messages are now `String`s:
  - “MIDI saved to {dir}. Drag it onto this track, then choose MIDI region.”
  - When frozen, add “; phrase frozen” after `{dir}`.
- Failure message: “Couldn’t save MIDI. Check write access to {dir} and try again.”
- The export tooltip becomes “Save four bars to {dir}. Live variations are frozen before export.”
- Anywhere else the UI says “Logic”, use the host-neutral wording “your DAW”.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn export_dir_ends_in_junglizer_exports() {
    assert!(export_dir().unwrap().ends_with(Path::new("Junglizer").join("Exports")));
}

#[test]
fn reveal_command_matches_this_os() {
    let (program, args) = reveal_command(Path::new("/tmp/x.mid"));
    #[cfg(target_os = "macos")] { assert_eq!(program, "open"); assert_eq!(args[0], "-R"); }
    #[cfg(target_os = "windows")] { assert_eq!(program, "explorer"); assert!(args[0].to_string_lossy().starts_with("/select,")); }
    #[cfg(target_os = "linux")] { assert_eq!(program, "xdg-open"); assert_eq!(args[0], "/tmp"); }
}
```

- [ ] **Step 2: Run them to confirm they fail.** Then implement, so they pass. Make sure `std::process::Command::new("open")` no longer appears outside `platform.rs`.

- [ ] **Step 3: Check that Windows and Linux at least type-check**

Run: `rustup target add x86_64-pc-windows-msvc x86_64-unknown-linux-gnu`, then `cargo check --target x86_64-pc-windows-msvc --no-default-features --features clap,vst3`, and the same for Linux.
Expected: no errors in our crate. Failures that come only from C build scripts in dependencies (x11 or pkg-config) are acceptable here; Task 11's CI covers them. Report which.

- [ ] **Step 4: Gate and commit**

```bash
cargo fmt --all -- --check && cargo test --locked
git commit -m "Platform export folder, reveal and host-neutral copy" -- src/platform.rs src/midi.rs src/ui.rs
```

---

### Task 11: Windows and Linux release builds in CI

**Files:**
- Modify: `.github/workflows/ci-release.yml`, and `docs/RELEASE-SIGNING.md` (note that Windows and Linux zips are unsigned)

Decisions:
- **New jobs:** `bundles-windows` (`windows-latest`) and `bundles-linux` (`ubuntu-24.04`).
- **When they run:** `if: startsWith(github.ref, 'refs/tags/v') || github.event_name == 'workflow_dispatch'`. Dispatch lets them be tested without tagging, and spends no macOS minutes. Both have `needs: test`. The `test` job and pull-request triggers are unchanged.
- **Build:** each runs `cargo install cargo-truce --version 6.3.0 --locked` and `cargo truce build --vst3 --clap`. Confirm the flags; if the `au` default feature breaks off macOS, pass `--no-default-features --features clap,vst3`.
- **Zips:** each job writes `Junglizer-${tag}-{windows|linux}-x86_64-{vst3|clap}.zip` plus `SHA256-{windows|linux}.txt`, uploaded as `bundles-windows` / `bundles-linux`.
- **Linux packages:** apt-install `pkg-config libx11-dev libx11-xcb-dev libxcb1-dev libxcb-icccm4-dev libxcb-dri2-0-dev libxcursor-dev libxrandr-dev libgl1-mesa-dev libdbus-1-dev`. Adjust this to what the first run actually needs.
- **Release:** `release` gets `needs: [bundles, bundles-windows, bundles-linux]`. Its draft notes say the macOS archives must be signed and notarized before publishing, and that the Windows and Linux zips are unsigned.
- **Action versions:** per Todd's standing rule, audit every `uses:` in the file and bump each to its current major after reading the intervening release notes:
  ```bash
  grep -rn "uses:" .github/workflows/ | sed 's/^ *//'
  for r in actions/checkout actions/upload-artifact actions/download-artifact dtolnay/rust-toolchain; do printf '%-32s %s\n' "$r" "$(gh api repos/$r/releases/latest --jq .tag_name 2>/dev/null)"; done
  ```

- [ ] **Step 1: Edit the workflow, then validate it**

Run: `actionlint .github/workflows/ci-release.yml`
Expected: no output.

- [ ] **Step 2: Commit, push the branch and dispatch once**

The active gh account is the Slalom one, so never run `gh auth switch`. Push with:
`git -c credential.helper= -c "credential.helper=!f() { echo username=mrballistic; echo password=\$(gh auth token -u mrballistic); }; f" push -u origin <branch>`. Then:

```bash
GH_TOKEN=$(gh auth token -u mrballistic) gh workflow run ci-release.yml --ref <branch>
GH_TOKEN=$(gh auth token -u mrballistic) gh run watch
```
Expected: `bundles-windows` and `bundles-linux` succeed and upload four zips. Download them and check that each zip holds a `Junglizer.vst3` bundle or a `Junglizer.clap`. Ask Todd before pushing if he hasn't already OK'd pushing this branch.

- [ ] **Step 3: Commit any fixes from the dispatch run**

```bash
git commit -m "CI: Windows and Linux VST3/CLAP bundles on release tags" -- .github/workflows/ci-release.yml docs/RELEASE-SIGNING.md
```

---

### Task 12: Docs, version and site copy

**Files:**
- Modify:
  - `Cargo.toml` (`version = "0.9.1-beta.2"`) and `Cargo.lock`
  - `README.md`
  - `site/index.html`, `site/llms.txt`, and `site/deploy.sh` if it names release assets
- Create: `docs/RELEASE-NOTES-0.9.1-beta.2.md`

**README:**
- The intro becomes macOS, Windows and Linux. It describes the built-in amen and bringing your own WAV, AIFF or MP3 (4–32 beats, 80–195 BPM, tempo read from the filename when present, ½/×2).
- Linked files and Relink, and per-break slice counts.
- An install table for the Windows and Linux VST3 and CLAP paths: `C:\Program Files\Common Files\VST3` and `…\CLAP`, `~/.vst3` and `~/.clap`.
- Export goes to the OS music folder.
- Build notes for Windows and Linux.
- Rewrite the step-1 text (“shared source slice count”) for per-break counts.

**Release notes:**
- What changed and why. Mention the KAN removal neutrally: “Junglizer now ships its own amen.”
- Beta 1 projects open as fresh instances.
- Known limits:
  - Tempo is read from the name, or the fastest fit.
  - Loops over 32 beats without a BPM in the name read at half speed.
  - No time-stretch.
  - Windows and Linux zips are unsigned, and Linux is beta.

**Site:**
- Add the Salamander credit next to the Nick Collins line.
- Platform text becomes macOS, Windows and Linux, including the `<title>` and og/description.
- JSON-LD `operatingSystem` becomes `"macOS, Windows, Linux"`. Update `llms.txt` to match.
- Add Windows and Linux download cards pointing at the new zip names. Update `deploy.sh`'s asset list if it enumerates names.
- Don't deploy.

- [ ] **Step 1: Make the edits**

- [ ] **Step 2: Check the punctuation**

Run: `grep -nP "(?<![=(\[{,:])\s'[^']*'|\"[A-Z][^\"]*\" " README.md docs/RELEASE-NOTES-0.9.1-beta.2.md site/index.html`, then review each hit. Rendered copy must use curly quotes and apostrophes.

- [ ] **Step 3: Gate, then commit**

Run: `cargo test --locked`, so the version bump keeps everything building.
```bash
git add docs/RELEASE-NOTES-0.9.1-beta.2.md
git commit -m "Docs, version 0.9.1-beta.2 and site copy for beta 2" -- Cargo.toml Cargo.lock README.md docs/RELEASE-NOTES-0.9.1-beta.2.md site/index.html site/llms.txt site/deploy.sh
```

---

### Task 13 (only if Todd approves): Drag and drop

truce-egui 6.3 drops baseview's `DragEntered`/`DragDropped` events in its `_ => Ignored` arm (`truce-egui-6.3.0/src/editor.rs`, `on_event`). baseview implements them on macOS and Windows only, not Linux. Shipping drag and drop therefore means:
1. Vendor a patched truce-egui with `[patch.crates-io]`. It returns `EventStatus::AcceptDrop(DropEffect::Copy)` for `DragEntered`/`DragMoved` and feeds `DropData::Files` into `egui::RawInput::dropped_files`.
2. Send the patch upstream.
3. Read `ctx.input(|i| i.raw.dropped_files.clone())` in the waveform and slot rects.
4. Change the first-run copy to “Drop your own break here (4 to 32 beats, 80–195 BPM)” on macOS and Windows only.

Write this task in full only after Todd decides.

---

## Release day (Todd, not part of execution)

1. Tag `v0.9.1-beta.2`, then sign, notarize and staple the macOS archives (`docs/RELEASE-SIGNING.md`), and publish.
2. Retire beta 1:
   - Delete or replace the assets of the `v0.9.0-beta.1` release.
   - Remove the beta 1 zips and links from `site/downloads/`.
   - Run `site/deploy.sh`.
3. Separately, before the repo goes public: rewrite history to remove the KAN WAVs. That is its own task.
