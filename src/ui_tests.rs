//! Reproducible renders of the real editor, without a DAW or audio device.
//! Run: cargo test render_editor_states -- --ignored --nocapture
//! The GPU renderer requires normal desktop GPU access on macOS.
use super::*;
use crate::library::{Library, Purpose};
use crate::sample::StereoSample;
use std::path::Path;
use std::sync::Arc;
use truce::core::editor::{ClosureBridge, Editor};
use truce::params::Params;
use truce::prelude::*;

fn context(params: Arc<JunglizerParams>, playing: bool) -> PluginContext<JunglizerParams> {
    let get = params.clone();
    let plain = params.clone();
    let set = params.clone();
    let bridge = ClosureBridge {
        begin_edit: Box::new(|_| {}),
        set_param: Box::new(move |id, value| set.set_normalized(id, value)),
        end_edit: Box::new(|_| {}),
        request_resize: Box::new(|_, _| false),
        get_param: Box::new(move |id| get.get_normalized(id).unwrap_or(0.0)),
        get_param_plain: Box::new(move |id| plain.get_plain(id).unwrap_or(0.0)),
        format_param: Box::new(|_| String::new()),
        get_meter: Box::new(|_| 0.0),
        get_state: Box::new(Vec::new),
        set_state: Box::new(|_| {}),
        transport: Box::new(move || {
            Some(TransportInfo {
                playing,
                tempo: 174.0,
                position_beats: 6.5,
                ..Default::default()
            })
        }),
    };
    PluginContext::from_closures(bridge, params.clone()).with_params(params)
}

fn fixture(
    name: &str,
) -> (
    JunglizerUi,
    Arc<JunglizerParams>,
    PluginContext<JunglizerParams>,
) {
    let params = Arc::new(JunglizerParams::default());
    let ctx = context(
        params.clone(),
        matches!(name, "playing" | "live-1" | "live-2" | "frozen-midi"),
    );
    let mut editor = JunglizerUi::new();
    editor.params = Some(params.clone());
    let count = if name == "density-32" { 32 } else { 8 };
    for source in 0..4 {
        editor.ensure_markers(source, count);
    }
    if !matches!(
        name,
        "ghost-stopped" | "missing-samples" | "first-run" | "empty" | "amen-modal"
    ) {
        editor.state.update(|s| {
            for (slot, source, slice, length) in [
                (0, 0, 0, 3),
                (5, 1, 2, 1),
                (10, 2, 3, 2),
                (24, 3, 5, 2),
                (28, 0, 6, 1),
                (30, 1, 1, 2),
            ] {
                assert!(s.place(StepPlacement {
                    active: true,
                    start_slot: slot,
                    span_slots: length,
                    source_loop: source,
                    source_slice: slice,
                    mode: 0,
                    bit_depth: 16,
                    repeats: 1,
                    locked: slot == 10
                }));
            }
        });
        params.set_normalized(P::EditSlot.into(), 10.0 / 31.0);
        params.set_normalized(P::EditBreak.into(), 2.0 / 3.0);
        params.set_normalized(P::EditSlice.into(), 3.0 / 31.0);
    }
    if name == "density-32" {
        editor.state.update(|s| {
            s.placements = Default::default();
            for slot in 0..32 {
                assert!(s.place(StepPlacement {
                    active: true,
                    start_slot: slot,
                    span_slots: 1,
                    source_loop: slot % 4,
                    source_slice: slot,
                    mode: 0,
                    bit_depth: 16,
                    repeats: 1,
                    locked: slot % 5 == 0
                }));
            }
        });
    }
    if name == "muted" {
        editor.state.update(|s| {
            s.marker_set_mut(2).unwrap().slices[3].active = false;
        });
    }
    if name == "collision" {
        let old = editor
            .state
            .get()
            .placements
            .iter()
            .find(|p| p.active && p.start_slot == 0)
            .copied()
            .unwrap();
        assert!(!editor.edit_placement(old, |p| p.span_slots = 7));
    }
    if name.starts_with("live-") {
        editor.state.update(|s| {
            s.live_enabled = true;
        });
        let cycle = if name == "live-1" { 1 } else { 2 };
        let mut live = editor.state.get().clone();
        suggest_bar(&mut live, 3, cycle as usize);
        params
            .live_snapshot
            .publish(&live, cycle, [0, 1, 2, cycle as usize]);
    }
    if name == "frozen-midi" || name == "legacy-locks" {
        editor.state.update(|s| {
            if name == "legacy-locks" {
                for p in &mut s.placements {
                    if p.active {
                        p.locked = true;
                    }
                }
            }
            s.freeze_phrase(s.placements, s.slice_counts(), s.player_slots, [0, 1, 2, 3]);
        });
        params.set_normalized(P::Playback.into(), 1.0);
    }
    if name == "missing-samples" {
        editor.slot_audio = std::array::from_fn(|_| {
            ui_slots::SlotAudio::new((crate::edit_state::SlotKind::Builtin, 0), None)
        });
        editor
            .state
            .update(|s| s.markers = [[Default::default(); 32]; 4]);
    }
    if name == "export-success" {
        editor.export_path = Some(PathBuf::from(
            "~/Music/Junglizer/Exports/junglizer-example.mid",
        ));
        editor.message = "MIDI saved to ~/Music/Junglizer/Exports. Drag it onto this track, then choose MIDI region.".into();
    }
    if name == "export-success-long" {
        let full = r"C:\Users\averylongusername\Music\Junglizer\Exports";
        editor.export_path = Some(PathBuf::from(format!(r"{full}\Junglizer-1.mid")));
        // What a Windows build shows (this host splits paths on '/' only).
        let dir = r"…\Music\Junglizer\Exports";
        editor.message = format!(
            "MIDI saved to {dir}; phrase frozen. Drag it onto this track, then choose MIDI region."
        )
        .into();
    }
    if name == "loaded-long" {
        editor.message = "Loaded a-very-long-sample-pack-folder_amen-break-remix-final-FINAL-v2-174bpm.wav: 16 beats · 174.0 BPM. Use ½ or ×2 if the tempo is off.".into();
    }
    if name == "export-failure" {
        editor.message =
            "Couldn’t save MIDI. Check write access to ~/Music/Junglizer/Exports and try again."
                .into();
    }
    if name == "user-16-beats" {
        // A 16-beat break at 174 BPM: the four built-in bars back to back,
        // loaded through the library and committed as a dialog choice is.
        let path = std::env::temp_dir().join("junglizer-fixture-break-174.wav");
        write_wav(&path, &four_bars());
        let path = path.to_str().unwrap().to_owned();
        choose(&mut editor, &params, 1, &path, ui_slots::LoadMode::Load);
        params.set_normalized(P::EditBreak.into(), 1.0 / 3.0);
        params.set_normalized(P::EditSlice.into(), 0.0);
    }
    if name == "blank-slot" || name == "unload-modal" || name == "unload-modal-replace" {
        params.set_normalized(P::EditBreak.into(), 1.0 / 3.0);
        params.set_normalized(P::EditSlice.into(), 0.0);
    }
    if name == "blank-slot" {
        editor.state.update(|s| {
            s.unload(1);
        });
    }
    if name == "unload-modal" {
        editor.confirm_unload = Some(1);
        editor.unload_modal_fresh = true;
    }
    if name == "unload-modal-replace" {
        // A WAV dropped on built-in Break 2.
        editor.confirm_unload = Some(1);
        editor.unload_modal_fresh = true;
        editor.replace_with = Some(PathBuf::from("/Users/me/Breaks/think break 174.wav"));
    }
    if name == "amen-modal" {
        // The empty state with Break 2 missing: the amen link asks first.
        let path = "/Volumes/Old Drive/Breaks/think break 174.wav";
        let suggested = Library::builtin()[1].suggested;
        editor.state.update(|s| {
            for slot in [0, 2, 3] {
                s.unload(slot);
            }
            s.load_user(1, path, 60_828, 4, &suggested[..8]).unwrap();
        });
        params
            .library
            .request(1, path.as_bytes(), Purpose::Restore)
            .unwrap();
        params.library.run_pending();
        editor.refresh_slot_audio();
        editor.use_builtin_amen();
        assert!(editor.confirm_amen);
    }
    if name == "missing" {
        // A user break whose file is gone: the restore reports Missing.
        let path = "/Volumes/Old Drive/Breaks/think break 174.wav";
        let suggested = Library::builtin()[1].suggested;
        editor.state.update(|s| {
            s.load_user(1, path, 60_828, 4, &suggested[..8]).unwrap();
        });
        params
            .library
            .request(1, path.as_bytes(), Purpose::Restore)
            .unwrap();
        params.library.run_pending();
        params.set_normalized(P::EditBreak.into(), 1.0 / 3.0);
        params.set_normalized(P::EditSlice.into(), 0.0);
    }
    if name == "first-run" {
        editor.first_run = true;
    }
    if name == "fader-thicc" {
        params.set_normalized(P::ThinThicc.into(), 0.85);
    }
    if name == "fader-thin" {
        params.set_normalized(P::ThinThicc.into(), 0.2);
    }
    if name == "undo-available" {
        editor.fill_bar(1, &ctx);
    }
    if name == "mode-live" {
        // Live, stopped: waiting for the first phrase, so Frozen waits too.
        editor.state.update(|s| s.live_enabled = true);
    }
    if name == "empty" {
        // Everything unloaded: no slot can sound.
        editor.state.update(|s| {
            for slot in 0..4 {
                s.unload(slot);
            }
        });
        editor.refresh_slot_audio();
    }
    (editor, params, ctx)
}

fn four_bars() -> StereoSample {
    let builtin = Library::builtin();
    StereoSample {
        frames: builtin
            .iter()
            .flat_map(|b| b.sample.frames.iter().copied())
            .collect(),
        sample_rate: builtin[0].sample.sample_rate,
    }
}

fn write_wav(path: &Path, sample: &StereoSample) {
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: sample.sample_rate,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut writer = hound::WavWriter::create(path, spec).unwrap();
    for frame in &sample.frames {
        writer.write_sample(frame[0]).unwrap();
        writer.write_sample(frame[1]).unwrap();
    }
    writer.finalize().unwrap();
}

/// What `choose_file` and `poll_pending_load` do, without the dialog: the
/// loader runs inline instead of on its thread.
fn choose(
    editor: &mut JunglizerUi,
    params: &Arc<JunglizerParams>,
    slot: usize,
    path: &str,
    mode: ui_slots::LoadMode,
) {
    editor.params = Some(params.clone());
    editor.load_path(slot, PathBuf::from(path), mode);
    editor.poll_pending_load(&egui::Context::default());
    params.library.run_pending();
    editor.poll_pending_load(&egui::Context::default());
    assert!(matches!(editor.pending, PendingLoad::None));
}

#[test]
fn reloading_a_rebounced_file_at_the_same_path_plays_and_draws_the_new_audio() {
    // Re-bounce `same 174.wav` over itself, then Replace it at the same
    // path: the DSP and the editor both move to the new decode.
    let params = Arc::new(JunglizerParams::default());
    let mut editor = JunglizerUi::new();
    editor.params = Some(params.clone());
    let path = std::env::temp_dir().join(format!("junglizer-{}-same 174.wav", std::process::id()));
    let path_str = path.to_str().unwrap().to_owned();
    let builtin = &Library::builtin()[0].sample;
    let one_bar = StereoSample {
        frames: builtin.frames.clone(),
        sample_rate: builtin.sample_rate,
    };
    write_wav(&path, &one_bar);
    choose(&mut editor, &params, 1, &path_str, ui_slots::LoadMode::Load);
    let mut dsp = crate::Junglizer::load_sources();
    dsp.edit = editor.state.get().clone();
    dsp.reconcile_sources(&params.library);
    params.library.run_pending();
    dsp.reconcile_sources(&params.library);
    let old_len = one_bar.frames.len();
    assert_eq!(
        dsp.sources[1].as_ref().unwrap().sample.frames.len(),
        old_len
    );
    assert_eq!(editor.slot_audio[1].sample().unwrap().frames.len(), old_len);

    let four = four_bars();
    write_wav(&path, &four);
    choose(&mut editor, &params, 1, &path_str, ui_slots::LoadMode::Load);
    std::fs::remove_file(&path).unwrap();
    assert_eq!(
        editor.slot_audio[1].sample().unwrap().frames.len(),
        four.frames.len(),
        "editor"
    );
    dsp.edit = editor.state.get().clone();
    dsp.reconcile_sources(&params.library);
    assert_eq!(
        dsp.sources[1].as_ref().unwrap().sample.frames.len(),
        four.frames.len(),
        "DSP"
    );
    // A later refresh keeps the new decode rather than an older one.
    editor.refresh_slot_audio();
    assert_eq!(
        editor.slot_audio[1].sample().unwrap().frames.len(),
        four.frames.len()
    );
}

#[test]
#[ignore = "writes review PNGs using the desktop GPU"]
fn render_editor_states() {
    // JUNGLIZER_RENDER_DIR (default .impeccable/review) is where the PNGs
    // go, JUNGLIZER_RENDER_SCALE (default 1) sets pixels-per-point and
    // JUNGLIZER_RENDER_ONLY renders one named state. They exist so
    // tools/screenshots/render_site_screenshot.sh can make a 2x image
    // without overwriting the tracked review fixtures.
    let directory = std::env::var_os("JUNGLIZER_RENDER_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join(".impeccable/review"));
    std::fs::create_dir_all(&directory).unwrap();
    let scale: f64 = std::env::var("JUNGLIZER_RENDER_SCALE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1.0);
    let only = std::env::var("JUNGLIZER_RENDER_ONLY").ok();
    for name in [
        "ghost-stopped",
        "selected",
        "muted",
        "density-32",
        "collision",
        "playing",
        "live-1",
        "live-2",
        "frozen-midi",
        "legacy-locks",
        "missing-samples",
        "export-success",
        "export-failure",
        "export-success-long",
        "loaded-long",
        "user-16-beats",
        "blank-slot",
        "missing",
        "unload-modal",
        "unload-modal-replace",
        "first-run",
        "empty",
        "amen-modal",
        "fader-thicc",
        "fader-thin",
        "undo-available",
        "mode-live",
    ] {
        if only.as_deref().is_some_and(|o| o != name) {
            continue;
        }
        let (mut ui, params, ctx) = fixture(name);
        let mut editor =
            truce_egui::EguiEditor::new(params.clone(), (1100, 720), move |root, _| {
                ui.ui(root, &ctx)
            })
            .with_visuals(visuals());
        editor.set_scale_factor(scale);
        let (rgba, width, height) = editor
            .screenshot(params)
            .expect("GPU screenshot unavailable");
        let file = std::fs::File::create(directory.join(format!("{name}.png"))).unwrap();
        let mut encoder = png::Encoder::new(file, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&rgba)
            .unwrap();
        println!("Rendered {name}: {width} × {height}");
    }
}

#[test]
fn rejected_length_keeps_placement_and_explains_recovery() {
    let (mut ui, _, _) = fixture("selected");
    let before = ui.state.get().placements;
    let first = before
        .iter()
        .find(|p| p.active && p.start_slot == 0)
        .copied()
        .unwrap();
    assert!(!ui.edit_placement(first, |p| p.span_slots = 7));
    assert_eq!(before, ui.state.get().placements);
    assert!(ui.message.contains("neighboring slice"));
}

#[test]
fn frozen_and_live_players_reject_new_placements() {
    for name in ["frozen-midi", "live-1"] {
        let (mut ui, _, ctx) = fixture(name);
        let before = ui.state.get().placements;
        ui.place(20, &ctx);
        assert_eq!(before, ui.state.get().placements);
        assert!(ui.message.contains("Choose Edit"));
    }
}

#[test]
fn placement_preview_tracks_original_fit_and_retrigger_time() {
    let slice = crate::edit_state::SliceMarker {
        start: 100,
        end: 1100,
        active: true,
        bass: false,
    };
    let mut item = StepPlacement {
        active: true,
        start_slot: 0,
        span_slots: 2,
        source_loop: 0,
        source_slice: 0,
        mode: 0,
        bit_depth: 16,
        repeats: 1,
        locked: false,
    };
    // Two of 32 slots = one beat = half a second at 120 BPM.
    assert_eq!(
        preview_frame(item, slice, 0.5, 32, 120.0, 1000),
        Some(350.0)
    );
    item.mode = 1;
    assert_eq!(
        preview_frame(item, slice, 0.5, 32, 120.0, 1000),
        Some(600.0)
    );
    item.repeats = 2;
    assert_eq!(
        preview_frame(item, slice, 0.75, 32, 120.0, 1000),
        Some(600.0)
    );
    item.mode = 0;
    item.repeats = 1;
    item.span_slots = 16;
    assert_eq!(
        preview_frame(item, slice, 0.75, 32, 120.0, 1000),
        None,
        "original-speed preview must leave silence after the source slice ends"
    );
}

#[test]
fn a_chosen_break_is_committed_with_its_reading_and_message() {
    let (editor, _, _) = fixture("user-16-beats");
    let slot = editor.state.get().slots[1];
    assert_eq!(
        (slot.kind, slot.beats, slot.slice_count),
        (crate::edit_state::SlotKind::User, 16, 8)
    );
    assert_eq!(
        editor.message,
        "Loaded junglizer-fixture-break-174.wav: 16 beats · 174.0 BPM. Use ½ or ×2 if the tempo is off."
    );
    assert_eq!(editor.select_after_load, Some(1));
}

#[test]
fn a_missing_restore_shows_as_missing() {
    let (mut editor, _, _) = fixture("missing");
    editor.refresh_slot_audio();
    assert_eq!(editor.slot_status(1), ui_slots::SlotStatus::Missing);
    assert_eq!(editor.slot_status(0), ui_slots::SlotStatus::Builtin);
}

#[test]
fn waveforms_come_from_the_embedded_breaks() {
    let editor = JunglizerUi::new();
    for slot in 0..4 {
        assert!(editor.slot_audio[slot].sample().is_some());
        assert!(editor.slot_audio[slot].peaks.iter().any(|&p| p > 0.0));
    }
}

#[test]
fn the_empty_state_shows_when_no_slot_can_sound_and_the_amen_restores() {
    let (mut editor, _, _) = fixture("empty");
    assert!(editor.no_playable_break());
    assert!(editor.shows_break_target());
    editor.use_builtin_amen();
    assert!(!editor.no_playable_break());
    for slot in 0..4 {
        assert_eq!(
            editor.state.get().slots[slot].kind,
            crate::edit_state::SlotKind::Builtin
        );
        assert!(editor.state.get().marker_set(slot).is_some_and(|m| m.valid));
    }
}

#[test]
fn a_missing_break_alone_counts_as_empty() {
    let (mut editor, _, _) = fixture("missing");
    editor.state.update(|s| {
        for slot in [0, 2, 3] {
            s.unload(slot);
        }
    });
    editor.refresh_slot_audio();
    assert!(editor.no_playable_break());
    // A built-in or loaded break is enough to hide the target.
    let (editor, _, _) = fixture("blank-slot");
    assert!(!editor.shows_break_target());
}

#[test]
fn a_refused_first_run_choice_keeps_the_prompt_up() {
    let (mut editor, _, _) = fixture("first-run");
    let before = editor.state.get().clone();
    let path = std::env::temp_dir().join("junglizer-fixture-not-audio.wav");
    std::fs::write(&path, b"not audio").unwrap();
    let path = path.to_str().unwrap().to_owned();
    let params = editor.params.clone().unwrap();
    choose(&mut editor, &params, 0, &path, ui_slots::LoadMode::FirstRun);
    assert!(editor.first_run);
    assert_eq!(*editor.state.get(), before);
    assert!(editor.message.contains("couldn’t read"));
}

#[test]
fn short_dir_collapses_home_or_keeps_the_last_three_parts() {
    use std::path::Path;
    let dir = Path::new("/Users/dj/Music/Junglizer/Exports");
    assert_eq!(
        crate::ui::short_dir(dir, Some(Path::new("/Users/dj")), false),
        "~/Music/Junglizer/Exports"
    );
    assert_eq!(
        crate::ui::short_dir(dir, None, false),
        "/Users/dj/Music/Junglizer/Exports"
    );
    assert_eq!(
        crate::ui::short_dir(Path::new("a/b/c/d/Music/Junglizer/Exports"), None, true),
        "…\\Music\\Junglizer\\Exports"
    );
}

#[test]
fn fill_then_undo_restores_the_exact_placements() {
    let (mut ui, params, ctx) = fixture("selected");
    let before = ui.state.get().clone();
    ui.fill_bar(1, &ctx);
    assert_eq!(params.fill_bar.value(), 2, "Fill sets the Fill bar param");
    assert_ne!(before.placements, ui.state.get().placements);
    assert_eq!(ui.undo_label(), Some("Fill bar 2"));
    assert!(ui.undo());
    assert_eq!(before.placements, ui.state.get().placements);
    assert_eq!(before.seed, ui.state.get().seed);
    assert_eq!(ui.undo_label(), None, "undo is single-use");
    assert!(!ui.undo());
}

#[test]
fn remove_then_undo_restores_the_placement() {
    let (mut ui, _, _) = fixture("selected");
    let before = ui.state.get().placements;
    assert!(ui.remove_placement(5));
    assert!(
        !ui.state
            .get()
            .placements
            .iter()
            .any(|p| p.active && p.start_slot == 5)
    );
    assert_eq!(ui.undo_label(), Some("Remove 2:03"));
    assert!(ui.undo());
    assert_eq!(before, ui.state.get().placements);
    // A locked placement isn't removed, so there's nothing to undo.
    assert!(!ui.remove_placement(10));
    assert_eq!(ui.undo_label(), None);
}

#[test]
fn an_intervening_edit_clears_the_undo() {
    let (mut ui, _, ctx) = fixture("selected");
    ui.fill_bar(3, &ctx);
    let first = ui
        .state
        .get()
        .placements
        .iter()
        .find(|p| p.active && p.start_slot == 0)
        .copied()
        .unwrap();
    assert!(ui.edit_placement(first, |p| p.bit_depth = 8));
    let after_edit = ui.state.get().placements;
    assert_eq!(ui.undo_label(), None);
    assert!(!ui.undo());
    assert_eq!(after_edit, ui.state.get().placements, "newer work is kept");
}

#[test]
fn undo_is_unavailable_while_frozen_or_live() {
    for freeze in [true, false] {
        let (mut ui, _, ctx) = fixture("selected");
        ui.fill_bar(1, &ctx);
        // Without a version bump, so only the mode guard can refuse it.
        ui.state.update(|s| {
            if freeze {
                s.frozen = true;
            } else {
                s.live_enabled = true;
            }
        });
        let held = ui.state.get().placements;
        assert_eq!(ui.undo_label(), None);
        assert!(!ui.undo());
        assert_eq!(held, ui.state.get().placements);
    }
    for name in ["frozen-midi", "live-1"] {
        let (mut ui, _, ctx) = fixture(name);
        let before = ui.state.get().placements;
        ui.fill_bar(1, &ctx);
        assert!(!ui.remove_placement(5));
        assert_eq!(before, ui.state.get().placements);
        assert_eq!(ui.undo_label(), None);
    }
}

#[test]
fn a_stale_state_snapshot_hides_the_undo_without_dropping_it() {
    // The host publishes editor state once per audio block, so the frame
    // after a Fill can still sync the state from before it.
    let (mut ui, _, ctx) = fixture("selected");
    let before = ui.state.get().clone();
    ui.fill_bar(1, &ctx);
    let filled = ui.state.get().clone();
    ui.state.update(|s| *s = before.clone());
    ui.prune_undo();
    assert_eq!(ui.undo_label(), None);
    ui.state.update(|s| *s = filled.clone());
    ui.prune_undo();
    assert_eq!(ui.undo_label(), Some("Fill bar 2"));
    // A newer version, from any other edit, drops it for good.
    ui.state.update(|s| s.touch());
    ui.prune_undo();
    ui.state.update(|s| *s = filled.clone());
    assert_eq!(ui.undo_label(), None);
}

/// Drives the whole editor through egui frames at the default size.
struct Driver {
    egui: egui::Context,
    editor: JunglizerUi,
    ctx: PluginContext<JunglizerParams>,
    time: f64,
}

impl Driver {
    fn new(name: &str) -> Self {
        let (editor, _, ctx) = fixture(name);
        let mut driver = Self {
            egui: egui::Context::default(),
            editor,
            ctx,
            time: 0.0,
        };
        driver.frame(Vec::new(), egui::Modifiers::NONE);
        driver
    }

    fn frame(&mut self, events: Vec<egui::Event>, modifiers: egui::Modifiers) {
        self.time += 0.1;
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1100.0, 720.0))),
            time: Some(self.time),
            modifiers,
            events,
            ..Default::default()
        };
        let (editor, ctx) = (&mut self.editor, &self.ctx);
        let _ = self.egui.run_ui(input, |ui| editor.ui(ui, ctx));
    }

    /// A click at `at`, over two frames.
    fn click(&mut self, at: Pos2) {
        let button = |pressed| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        self.frame(
            vec![egui::Event::PointerMoved(at), button(true)],
            egui::Modifiers::NONE,
        );
        self.frame(vec![button(false)], egui::Modifiers::NONE);
    }

    fn key(&mut self, key: egui::Key, modifiers: egui::Modifiers) {
        self.frame(
            vec![egui::Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers,
            }],
            modifiers,
        );
    }
}

/// The 2:03 placement in slot 6 of the fixture's player.
const SLOT_SIX: Pos2 = Pos2::new(198.0, 430.0);

#[test]
fn return_in_an_open_dialog_never_places_a_slice_in_the_player() {
    let mut d = Driver::new("selected");
    d.click(SLOT_SIX);
    assert_eq!(d.editor.selected_slot, Some(5), "the click selects slot 6");
    let before = d.editor.state.get().clone();
    // A dialog opens on a frame where the player still has focus and
    // Enter arrives. The player runs before the dialog draws, so it must
    // check for the dialog itself; neither may act on the Enter.
    d.editor.confirm_unload = Some(1);
    d.editor.unload_modal_fresh = true;
    d.key(egui::Key::Enter, egui::Modifiers::NONE);
    assert!(d.editor.confirm_open());
    assert!(*d.editor.state.get() == before, "Enter changed the pattern");
    // With the dialog gone (one frame for its backdrop to go), the same
    // Enter places 3:04 in slot 6.
    d.editor.close_unload_modal(false);
    d.frame(Vec::new(), egui::Modifiers::NONE);
    d.click(SLOT_SIX);
    d.key(egui::Key::Enter, egui::Modifiers::NONE);
    let placed = d.editor.state.get().placements;
    assert!(
        placed
            .iter()
            .any(|p| p.active && p.start_slot == 5 && p.source_loop == 2 && p.source_slice == 3),
        "the control Enter didn't place"
    );
}

#[test]
fn cmd_z_undoes_after_a_click_has_focused_the_player() {
    let mut d = Driver::new("selected");
    let before = d.editor.state.get().placements;
    d.editor.fill_bar(1, &d.ctx);
    assert_ne!(before, d.editor.state.get().placements);
    d.click(SLOT_SIX);
    assert!(
        d.egui.memory(|m| m.focused()).is_some(),
        "the player has focus"
    );
    d.key(egui::Key::Z, egui::Modifiers::COMMAND);
    assert_eq!(
        before,
        d.editor.state.get().placements,
        "Cmd/Ctrl+Z didn't undo"
    );
}

#[test]
fn the_detent_ticks_only_when_the_fader_crosses_dry() {
    // (crossed, side to remember)
    assert_eq!(detent_crossing(-1, 1), (true, 1));
    assert_eq!(detent_crossing(1, -1), (true, -1));
    assert_eq!(detent_crossing(1, 1), (false, 1));
    // Leaving or landing on exact Dry isn't a crossing.
    assert_eq!(detent_crossing(0, 1), (false, 1));
    assert_eq!(detent_crossing(1, 0), (false, 1));
    // Thin, to Dry, to Thicc: the crossing lands on reaching Thicc.
    assert_eq!(detent_crossing(-1, 0), (false, -1));
    assert_eq!(detent_crossing(-1, 1), (true, 1));
}

#[test]
fn edit_from_frozen_unfreezes_and_keeps_slice_locks() {
    let (mut ui, params, ctx) = fixture("frozen-midi");
    let placements = ui.state.get().placements;
    ui.choose_mode(0, &ctx);
    assert!(!ui.state.get().frozen);
    assert_eq!(
        placements,
        ui.state.get().placements,
        "locks and slices kept"
    );
    assert!(!params.live_variations.value());
    assert_eq!(
        ui.message,
        "Phrase unfrozen. Individual slice locks are preserved."
    );
}

#[test]
fn edit_from_live_stops_live_and_restores_the_hand_edited_pattern() {
    let (mut ui, params, ctx) = fixture("live-1");
    params.set_normalized(P::LiveVariations.into(), 1.0);
    let placements = ui.state.get().placements;
    ui.choose_mode(0, &ctx);
    assert!(!ui.state.get().live_enabled && !ui.state.get().frozen);
    assert_eq!(placements, ui.state.get().placements);
    assert!(!params.live_variations.value());
    assert_eq!(
        ui.message,
        "Live stopped. Your hand-edited pattern is back."
    );
}

#[test]
fn frozen_from_live_freezes_the_live_variation_and_keeps_locks() {
    let (mut ui, params, ctx) = fixture("live-1");
    params.set_normalized(P::LiveVariations.into(), 1.0);
    let captured = params.live_snapshot.capture().unwrap();
    let hand_edited = ui.state.get().placements;
    assert_ne!(captured.placements, hand_edited, "the variation differs");
    ui.choose_mode(2, &ctx);
    let state = ui.state.get();
    assert!(state.frozen && !state.live_enabled);
    assert_eq!(state.placements, captured.placements);
    assert_eq!(state.frozen_bars, captured.bars);
    assert!(
        state
            .placements
            .iter()
            .any(|p| p.active && p.locked && p.start_slot == 10),
        "the locked 3:04 stays locked"
    );
    assert!(!params.live_variations.value());
}
