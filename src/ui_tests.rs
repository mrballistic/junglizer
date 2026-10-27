//! Reproducible renders of the real editor, without a DAW or audio device.
//! Run: cargo test render_editor_states -- --ignored --nocapture
//! The GPU renderer requires normal desktop GPU access on macOS.
use super::*;
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
    let count = if name == "density-32" { 32 } else { 8 };
    for source in 0..4 {
        editor.ensure_markers(source, count);
    }
    if name != "ghost-stopped" && name != "missing-samples" {
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
        editor.sources = std::array::from_fn(|_| StereoSample::default());
        editor.waveform = [[0.0; 512]; 4];
        editor
            .state
            .update(|s| s.markers = [[Default::default(); 32]; 4]);
    }
    if name == "export-success" {
        editor.export_path = Some(PathBuf::from(
            "~/Music/Junglizer/Exports/junglizer-example.mid",
        ));
        editor.message = "MIDI saved to ~/Music/Junglizer/Exports. Drag to this Logic track, then choose MIDI region.";
    }
    if name == "export-failure" {
        editor.message =
            "Could not save MIDI. Check write access to ~/Music/Junglizer/Exports and try again.";
    }
    (editor, params, ctx)
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
