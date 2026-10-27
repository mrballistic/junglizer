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
        "ghost-stopped" | "missing-samples" | "first-run" | "empty"
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
    if name == "blank-slot" || name == "unload-modal" {
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
    editor.pending = PendingLoad::Choosing {
        slot,
        seq: 0,
        path: path.to_owned(),
        mode,
    };
    editor.poll_pending_load(&egui::Context::default());
    params.library.run_pending();
    editor.poll_pending_load(&egui::Context::default());
    assert!(matches!(editor.pending, PendingLoad::None));
}

#[test]
#[ignore = "writes review PNGs using the desktop GPU"]
fn render_editor_states() {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join(".impeccable/review");
    std::fs::create_dir_all(&directory).unwrap();
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
        "user-16-beats",
        "blank-slot",
        "missing",
        "unload-modal",
        "first-run",
        "empty",
    ] {
        let (mut ui, params, ctx) = fixture(name);
        let mut editor =
            truce_egui::EguiEditor::new(params.clone(), (1100, 720), move |root, _| {
                ui.ui(root, &ctx)
            })
            .with_visuals(visuals());
        editor.set_scale_factor(1.0);
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
        assert!(ui.message.contains("Unfreeze"));
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
