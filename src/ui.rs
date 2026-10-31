use crate::edit_state::{EditState, SlotKind, StepPlacement};
use crate::library::Library;
use crate::live::CapturedPhrase;
use crate::midi::export_to_music;
use crate::suggest::suggest_bar;
use crate::{JunglizerParams, JunglizerParamsParamId as P};
use egui::{Color32, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, vec2};
use std::borrow::Cow;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use truce::params::FloatParamReadF32;
use truce::prelude::{PluginContext, StateBinding};
use truce_egui::EditorUi;

const CANVAS: Color32 = Color32::from_rgb(23, 25, 20);
const SURFACE: Color32 = Color32::from_rgb(34, 37, 29);
const WELL: Color32 = Color32::from_rgb(16, 18, 15);
const EDGE: Color32 = Color32::from_rgb(76, 81, 61);
const INK: Color32 = Color32::from_rgb(244, 241, 222);
const MUTED: Color32 = Color32::from_rgb(171, 178, 154);
const ACCENT: Color32 = Color32::from_rgb(211, 224, 128);
/// The wordmark's hover text, which carries the drum kit credit.
const WORDMARK_HOVER: &str = "Junglizer • Four-bar break instrument\nBuilt-in breaks played on the Salamander Drumkit by Alexander Holm (CC BY-SA 3.0)";
const INITIAL_MESSAGE: &str =
    "Choose a source slice, then click a player slot. You can drag it there too.";
const UNDO_HINT: &str = if cfg!(target_os = "macos") {
    "Cmd+Z"
} else {
    "Ctrl+Z"
};
/// The key that removes the selected placed slice, as macOS names it.
const DELETE_KEY: &str = if cfg!(target_os = "macos") {
    "Delete"
} else {
    "Delete or Backspace"
};
const SOURCE_COLORS: [Color32; 4] = [
    Color32::from_rgb(238, 151, 87),
    Color32::from_rgb(105, 205, 193),
    Color32::from_rgb(151, 173, 241),
    Color32::from_rgb(222, 139, 190),
];

pub struct JunglizerUi {
    state: StateBinding<EditState>,
    /// Each slot's audio and peaks, from `Library`.
    slot_audio: [SlotAudio; 4],
    /// Reaches the shared `Library`; set when the editor opens or draws.
    params: Option<Arc<JunglizerParams>>,
    pending: PendingLoad,
    /// The open file dialog and the slot and mode it loads into.
    picking: Option<(usize, ui_slots::LoadMode, crate::file_picker::Pick)>,
    /// Selects this slot (and syncs Source slices) on the next frame.
    select_after_load: Option<usize>,
    confirm_unload: Option<usize>,
    unload_modal_fresh: bool,
    /// The frame the confirm dialog opened on; Return waits a frame.
    modal_opened_frame: Option<u64>,
    /// A dropped file waiting on the unload dialog to replace a user break.
    replace_with: Option<PathBuf>,
    /// The empty state's amen link is waiting on the confirm dialog.
    confirm_amen: bool,
    /// This frame's drop targets, registered during layout.
    drop_targets: Vec<(Rect, ui_slots::DropTarget)>,
    /// The first-launch prompt is showing.
    first_run: bool,
    selected_loop: usize,
    selected_slice: usize,
    selected_slot: Option<u8>,
    selected_bar: usize,
    dragging_slice: Option<(usize, usize)>,
    dragging_marker: Option<usize>,
    /// The placed slice a source-slice click swaps into: set by clicking a
    /// placed slice in the player, cleared by placing, removing or Esc.
    swap_slot: Option<u8>,
    message: Cow<'static, str>,
    export_path: Option<PathBuf>,
    wordmark: Option<egui::TextureHandle>,
    /// The single undo for the last Fill or Remove.
    undo: Option<Undo>,
}

/// What a Fill or Remove changes, copied before it runs. `after` is the
/// state version the action wrote; any other change moves it, and the
/// undo no longer applies.
struct Undo {
    label: String,
    placements: [StepPlacement; crate::edit_state::MAX_STEPS],
    seed: u64,
    /// The version and placements the action left behind.
    after: u64,
    after_placements: [StepPlacement; crate::edit_state::MAX_STEPS],
}

impl Undo {
    fn of(state: &EditState, label: String) -> Self {
        Self {
            label,
            placements: state.placements,
            seed: state.seed,
            after: state.version,
            after_placements: state.placements,
        }
    }
}

impl JunglizerUi {
    pub fn new() -> Self {
        Self {
            state: StateBinding::default(),
            slot_audio: std::array::from_fn(SlotAudio::builtin),
            params: None,
            pending: PendingLoad::None,
            picking: None,
            select_after_load: None,
            confirm_unload: None,
            unload_modal_fresh: false,
            modal_opened_frame: None,
            replace_with: None,
            confirm_amen: false,
            drop_targets: Vec::new(),
            first_run: false,
            selected_loop: 0,
            selected_slice: 0,
            selected_slot: None,
            selected_bar: 3,
            dragging_slice: None,
            dragging_marker: None,
            swap_slot: None,
            message: INITIAL_MESSAGE.into(),
            export_path: None,
            wordmark: None,
            undo: None,
        }
    }

    fn ensure_markers(&mut self, source_loop: usize, count: u8) {
        let entry = self.state.get().slots[source_loop];
        // Suggested cuts where there's audio to analyze; even ones for a
        // user break that hasn't loaded yet.
        let suggested = match entry.kind {
            SlotKind::Builtin => Some(
                Library::builtin()[source_loop]
                    .sample
                    .suggested_slices(count),
            ),
            SlotKind::User => self.slot_audio[source_loop]
                .sample()
                .filter(|sample| sample.frames.len() == entry.frames as usize)
                .map(|sample| sample.suggested_slices(count)),
            SlotKind::Blank => None,
        };
        self.state.update(|state| {
            state.set_slice_count(source_loop, count);
            if entry.kind == SlotKind::Blank
                || !state.marker_set(source_loop).is_some_and(|set| !set.valid)
            {
                return;
            }
            match suggested {
                Some(suggested) => {
                    state.set_markers(source_loop, &suggested[..count as usize]);
                }
                None if entry.frames > 0 => {
                    state.fill_even_markers(source_loop, entry.frames);
                }
                None => {}
            }
        });
        if source_loop == self.selected_loop {
            self.selected_slice = self.selected_slice.min(count as usize - 1);
        }
    }

    fn place(&mut self, slot: u8, context: &PluginContext<JunglizerParams>) {
        self.swap_slot = None;
        if self.state.get().frozen || self.state.get().live_enabled {
            self.message = "Choose Edit under Phrase before editing the player.".into();
            return;
        }
        let (source_loop, source_slice) = self
            .dragging_slice
            .unwrap_or((self.selected_loop, self.selected_slice));
        let count = self.state.get().player_slots;
        if slot >= count || self.state.get().slice(source_loop, source_slice).is_none() {
            self.message =
                "This source slice is quiet or unavailable. Choose another slice.".into();
            return;
        }
        let placement = StepPlacement {
            active: true,
            start_slot: slot,
            span_slots: 1,
            source_loop: source_loop as u8,
            source_slice: source_slice as u8,
            mode: 0,
            bit_depth: 16,
            repeats: 1,
            locked: false,
        };
        let mut placed = false;
        self.state.update(|state| placed = state.place(placement));
        if placed {
            self.selected_slot = Some(slot);
            context.automate(P::EditSlot, f64::from(slot) / 31.0);
            self.message = "Slice placed. Change its length and sound below.".into();
        } else {
            self.message = "That space contains a locked slice. Unlock it to replace it.".into();
        }
    }

    fn source_waveform(&mut self, ui: &mut Ui, context: &PluginContext<JunglizerParams>) {
        let width = ui.available_width();
        let (rect, response) = ui.allocate_exact_size(vec2(width, 112.0), Sense::click_and_drag());
        ui.painter().rect_filled(rect, 5.0, WELL);
        if self.shows_break_target() {
            self.break_target(ui, rect);
            return;
        }
        self.drop_targets
            .push((rect, ui_slots::DropTarget::Waveform));
        let middle = rect.center().y;
        for (bin, amplitude) in self.slot_audio[self.selected_loop].peaks.iter().enumerate() {
            if *amplitude <= 0.0 {
                continue;
            }
            let x = rect.left() + rect.width() * bin as f32 / 512.0;
            let height = amplitude.sqrt().min(1.0) * 45.0;
            ui.painter().line_segment(
                [Pos2::new(x, middle - height), Pos2::new(x, middle + height)],
                Stroke::new(1.0_f32, SOURCE_COLORS[self.selected_loop]),
            );
        }
        self.beat_grid(ui.painter(), rect, self.selected_loop);
        let set = self.state.get().marker_set(self.selected_loop).copied();
        let entry = self.state.get().slots[self.selected_loop];
        // Markers count frames of the linked file, whether or not it loaded.
        let frame_count = match entry.kind {
            SlotKind::Builtin => self.slot_audio[self.selected_loop]
                .sample()
                .map_or(0, |sample| sample.frames.len()),
            SlotKind::User => entry.frames as usize,
            SlotKind::Blank => 0,
        };
        if let Some(set) = set.filter(|set| set.valid && frame_count > 0) {
            for i in 0..self.state.get().slice_count(self.selected_loop) as usize {
                let marker = set.slices[i];
                let x = rect.left() + rect.width() * marker.start as f32 / frame_count as f32;
                let end_x = rect.left() + rect.width() * marker.end as f32 / frame_count as f32;
                let region =
                    Rect::from_min_max(Pos2::new(x, rect.top()), Pos2::new(end_x, rect.bottom()));
                if !marker.active {
                    ui.painter()
                        .rect_filled(region, 0.0, Color32::from_black_alpha(170));
                }
                if i == self.selected_slice {
                    ui.painter().rect_filled(
                        region,
                        0.0,
                        SOURCE_COLORS[self.selected_loop].gamma_multiply(0.14),
                    );
                    ui.painter().rect_stroke(
                        region,
                        0.0,
                        Stroke::new(2.0_f32, INK),
                        StrokeKind::Inside,
                    );
                }
                let color = if marker.active { ACCENT } else { MUTED };
                ui.painter().rect_filled(
                    Rect::from_min_size(Pos2::new(x, rect.top()), vec2(8.0, 10.0)),
                    1.0,
                    color,
                );
                if marker.bass {
                    ui.painter()
                        .circle_filled(Pos2::new(x + 5.0, rect.bottom() - 6.0), 2.5, color);
                }
                ui.painter().line_segment(
                    [Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())],
                    Stroke::new(2.0_f32, color),
                );
                ui.painter().text(
                    Pos2::new(x + 3.0, rect.top() + 14.0),
                    egui::Align2::LEFT_TOP,
                    (i + 1).to_string(),
                    egui::FontId::proportional(11.0),
                    color,
                );
            }
            if !self.state.get().frozen
                && !self.state.get().live_enabled
                && response.drag_started()
                && let Some(pointer) = response.interact_pointer_pos()
            {
                self.dragging_marker = (1..self.state.get().slice_count(self.selected_loop)
                    as usize)
                    .filter(|&i| {
                        let x = rect.left()
                            + rect.width() * set.slices[i].start as f32 / frame_count as f32;
                        (pointer.x - x).abs() <= 9.0
                    })
                    .min_by(|&a, &b| {
                        let xa = set.slices[a].start as f32;
                        let xb = set.slices[b].start as f32;
                        (pointer.x - rect.left() - rect.width() * xa / frame_count as f32)
                            .abs()
                            .total_cmp(
                                &(pointer.x - rect.left() - rect.width() * xb / frame_count as f32)
                                    .abs(),
                            )
                    });
                if let Some(index) = self.dragging_marker {
                    self.selected_slice = index;
                    context.automate(P::EditSlice, index as f64 / 31.0);
                    context.begin_edit(P::MarkerPosition);
                }
            }
            if !self.state.get().frozen
                && !self.state.get().live_enabled
                && response.dragged()
                && let (Some(i), Some(pointer)) =
                    (self.dragging_marker, response.interact_pointer_pos())
            {
                let min = set.slices[i - 1].start.saturating_add(1);
                let max = if i + 1 < self.state.get().slice_count(self.selected_loop) as usize {
                    set.slices[i + 1].start.saturating_sub(1)
                } else {
                    frame_count as u32 - 1
                };
                if min <= max {
                    let fraction = ((pointer.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
                    let frame = (fraction * frame_count as f32) as u32;
                    let frame = frame.clamp(min, max);
                    if frame != set.slices[i].start {
                        let loop_index = self.selected_loop;
                        self.state.update(|state| {
                            if let Some(markers) = state.marker_set_mut(loop_index) {
                                markers.slices[i].start = frame;
                                markers.slices[i - 1].end = frame;
                            }
                            state.touch();
                        });
                        context.set_param(P::MarkerPosition, f64::from(frame) / frame_count as f64);
                    }
                }
            }
            if response.drag_stopped() {
                if self.dragging_marker.is_some() {
                    context.end_edit(P::MarkerPosition);
                }
                self.dragging_marker = None;
            }
        }
        // Drawn over the markers, on a plate so their lines never cross it.
        if let Some(notice) = self.waveform_notice(self.selected_loop) {
            let galley =
                ui.painter()
                    .layout_no_wrap(notice, egui::FontId::proportional(13.0), MUTED);
            let plate = Rect::from_center_size(rect.center(), galley.size() + vec2(16.0, 8.0));
            ui.painter().rect_filled(plate, 3.0, WELL);
            ui.painter()
                .galley(plate.min + vec2(8.0, 4.0), galley, MUTED);
        } else if frame_count == 0 {
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                format!(
                    "Break {} is unavailable. Reinstall Junglizer to restore it.",
                    self.selected_loop + 1
                ),
                egui::FontId::proportional(13.0),
                INK,
            );
        }
    }

    fn player(
        &mut self,
        ui: &mut Ui,
        context: &PluginContext<JunglizerParams>,
        live: Option<&CapturedPhrase>,
    ) {
        let width = ui.available_width();
        let (rect, response) = ui.allocate_exact_size(vec2(width, 210.0), Sense::click());
        let body = Rect::from_min_max(
            rect.left_top() + vec2(0.0, 33.0),
            rect.right_bottom() - vec2(0.0, 23.0),
        );
        ui.painter().rect_filled(rect, 4.0, WELL);
        let selected_bars = [
            context.params().bar_1.value(),
            context.params().bar_2.value(),
            context.params().bar_3.value(),
            context.params().bar_4.value(),
        ];
        let bar_sources = live
            .map(|p| p.bars.map(|b| i64::from(b) + 1))
            .unwrap_or_else(|| {
                if self.state.get().frozen {
                    self.state.get().frozen_bars.map(|b| i64::from(b) + 1)
                } else {
                    selected_bars
                }
            });
        let slots = live.map_or(self.state.get().player_slots, |p| p.player_slots) as usize;
        let placements = live.map_or(self.state.get().placements, |p| p.placements);
        let transport = context.transport();
        let tempo = transport.as_ref().map_or(174.0, |t| t.tempo).max(1.0);
        let play_beat = transport
            .filter(|t| t.playing)
            .map(|t| t.position_beats.rem_euclid(16.0));
        for bar in 0..4 {
            let bar_rect = Rect::from_min_max(
                Pos2::new(rect.left() + width * bar as f32 / 4.0, rect.top()),
                Pos2::new(rect.left() + width * (bar + 1) as f32 / 4.0, body.bottom()),
            );
            let active = play_beat.is_some_and(|beat| (beat / 4.0).floor() as usize == bar);
            let heading =
                Rect::from_min_max(bar_rect.left_top(), Pos2::new(bar_rect.right(), body.top()));
            ui.painter().rect_filled(
                heading,
                0.0,
                if active {
                    Color32::from_rgb(61, 69, 32)
                } else {
                    SURFACE
                },
            );
            if active {
                ui.painter().rect_filled(
                    Rect::from_min_max(
                        Pos2::new(bar_rect.left(), body.top()),
                        bar_rect.right_bottom(),
                    ),
                    0.0,
                    Color32::from_rgba_unmultiplied(211, 224, 128, 14),
                );
            }
            ui.painter().rect_stroke(
                bar_rect,
                0.0,
                Stroke::new(1.0_f32, EDGE),
                StrokeKind::Inside,
            );
            let mut child = ui.new_child(
                egui::UiBuilder::new()
                    .id_salt(("bar-header", bar))
                    .max_rect(heading.shrink2(vec2(7.0, 3.0)))
                    .layout(egui::Layout::left_to_right(egui::Align::Center)),
            );
            child.colored_label(
                if active { ACCENT } else { INK },
                format!("Bar {}", bar + 1),
            );
            let locked = self.state.get().frozen || self.state.get().live_enabled;
            let combo = child
                .add_enabled_ui(!locked, |ui| {
                    egui::ComboBox::from_id_salt(("bar-source", bar))
                        .width(90.0)
                        .selected_text(
                            egui::RichText::new(format!("Break {}", bar_sources[bar]))
                                .color(SOURCE_COLORS[(bar_sources[bar] - 1) as usize]),
                        )
                        .show_ui(ui, |ui| {
                            for i in 1..=4 {
                                if ui
                                    .selectable_label(bar_sources[bar] == i, format!("Break {i}"))
                                    .clicked()
                                {
                                    context.automate(
                                        [P::Bar1, P::Bar2, P::Bar3, P::Bar4][bar],
                                        (i - 1) as f64 / 3.0,
                                    );
                                }
                            }
                        });
                })
                .response;
            // The dimmed player behind the break target has its own reason.
            if locked {
                combo.on_disabled_hover_text("Choose Edit under Phrase to change ghost breaks.");
            }
            if active {
                child.colored_label(ACCENT, "Now");
            }
            child.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                let lock = ui
                    .add_enabled(!locked, egui::Button::new("").min_size(vec2(26.0, 22.0)))
                    .on_hover_text(format!("Lock bar {}’s slices", bar + 1))
                    .on_disabled_hover_text("Choose Edit under Phrase to lock a bar.");
                padlock(
                    ui.painter(),
                    lock.rect.center() - vec2(4.0, 1.0),
                    if locked { MUTED } else { INK },
                );
                if lock.clicked() {
                    self.lock_bar(bar);
                }
                if ui
                    .add_enabled(!locked, egui::Button::new("Fill"))
                    .on_hover_text(format!("Suggest a fill for bar {}", bar + 1))
                    .on_disabled_hover_text("Choose Edit under Phrase to suggest a fill.")
                    .clicked()
                {
                    self.fill_bar(bar, context);
                }
            });
            // The part of the break this bar plays: breaks repeat from the
            // top of the phrase, and a blank or silent slot falls back.
            let Some(source) = self.ghost_slot(((bar_sources[bar] - 1) as usize).min(3)) else {
                continue;
            };
            let beats = usize::from(self.slot_beats(source));
            let first = (bar * 4) % beats;
            let bins = (first * 512 / beats)..((first + 4).min(beats) * 512 / beats);
            let span = bins.len().max(1) as f32;
            for (column, amplitude) in self.slot_audio[source].peaks[bins].iter().enumerate() {
                if *amplitude <= 0.0 {
                    continue;
                }
                let x = bar_rect.left() + bar_rect.width() * column as f32 / span;
                let height = amplitude.sqrt().min(1.0) * body.height() * 0.34;
                ui.painter().line_segment(
                    [
                        Pos2::new(x, body.center().y - height),
                        Pos2::new(x, body.center().y + height),
                    ],
                    Stroke::new(1.0_f32, Color32::from_rgb(82, 91, 77)),
                );
            }
        }
        for slot in 0..slots {
            let x = rect.left() + width * slot as f32 / slots as f32;
            ui.painter().line_segment(
                [Pos2::new(x, body.top()), Pos2::new(x, body.bottom())],
                Stroke::new(1.0_f32, SURFACE),
            );
        }
        for item in placements.iter().filter(|p| p.active) {
            let left = rect.left() + width * item.start_slot as f32 / slots as f32;
            let right =
                rect.left() + width * (item.start_slot + item.span_slots) as f32 / slots as f32;
            let block = Rect::from_min_max(
                Pos2::new(left + 2.0, body.top() + 3.0),
                Pos2::new(right - 2.0, body.bottom() - 3.0),
            );
            let color = SOURCE_COLORS[item.source_loop as usize];
            let selected = self.selected_slot.is_some_and(|slot| {
                item.start_slot <= slot && slot < item.start_slot + item.span_slots
            });
            ui.painter().rect_filled(block, 3.0, color);
            source_pattern(
                ui.painter(),
                block.shrink(2.0),
                item.source_loop as usize,
                Color32::from_black_alpha(22),
            );
            self.placement_waveform(ui.painter(), block, *item, slots as u8, tempo);
            let label = if block.width() < 48.0 {
                format!("{}\n{:02}", item.source_loop + 1, item.source_slice + 1)
            } else {
                format!("{}:{:02}", item.source_loop + 1, item.source_slice + 1)
            };
            let paint = ui.painter().with_clip_rect(block.shrink(2.0));
            paint.text(
                block.left_top() + vec2(4.0, 5.0),
                egui::Align2::LEFT_TOP,
                label,
                egui::FontId::proportional(12.0),
                WELL,
            );
            if block.width() > 50.0 {
                let detail = if item.mode == 1 {
                    "Fit".to_owned()
                } else if item.bit_depth < 16 {
                    format!("{} bit", item.bit_depth)
                } else if item.repeats > 1 {
                    format!("{}×", item.repeats)
                } else {
                    String::new()
                };
                paint.text(
                    block.left_bottom() + vec2(5.0, -7.0),
                    egui::Align2::LEFT_BOTTOM,
                    detail,
                    egui::FontId::proportional(11.0),
                    WELL,
                );
            }
            if item.locked {
                paint.rect_stroke(
                    block.shrink(3.0),
                    1.0,
                    Stroke::new(1.0_f32, INK),
                    StrokeKind::Inside,
                );
                padlock(&paint, block.right_bottom() - vec2(14.0, 15.0), WELL);
            }
            ui.painter().rect_stroke(
                block,
                3.0,
                Stroke::new(
                    if selected { 3.0_f32 } else { 1.0_f32 },
                    if selected {
                        WELL
                    } else {
                        color.gamma_multiply(0.6)
                    },
                ),
                StrokeKind::Inside,
            );
            if selected {
                ui.painter().rect_stroke(
                    block.expand(1.0),
                    4.0,
                    Stroke::new(2.0_f32, ACCENT),
                    StrokeKind::Outside,
                );
            }
        }
        if let Some(slot) = self.selected_slot.filter(|s| (*s as usize) < slots)
            && !placements
                .iter()
                .any(|p| p.active && p.start_slot <= slot && slot < p.start_slot + p.span_slots)
        {
            let r = Rect::from_min_max(
                Pos2::new(
                    rect.left() + width * slot as f32 / slots as f32 + 1.0,
                    body.top() + 3.0,
                ),
                Pos2::new(
                    rect.left() + width * (slot + 1) as f32 / slots as f32 - 1.0,
                    body.bottom() - 3.0,
                ),
            );
            ui.painter()
                .rect_stroke(r, 2.0, Stroke::new(2.0_f32, ACCENT), StrokeKind::Inside);
        }
        for beat in 0..16 {
            let x = rect.left() + width * beat as f32 / 16.0;
            ui.painter().text(
                Pos2::new(x + 3.0, body.bottom() + 5.0),
                egui::Align2::LEFT_TOP,
                (beat + 1).to_string(),
                egui::FontId::proportional(12.0),
                if play_beat.is_some_and(|b| b.floor() as usize == beat) {
                    ACCENT
                } else {
                    MUTED
                },
            );
        }
        if let Some(beat) = play_beat {
            let x = rect.left() + width * (beat / 16.0) as f32;
            ui.painter().line_segment(
                [Pos2::new(x, body.top()), Pos2::new(x, rect.bottom())],
                Stroke::new(5.0_f32, WELL),
            );
            ui.painter().line_segment(
                [Pos2::new(x, body.top()), Pos2::new(x, rect.bottom())],
                Stroke::new(2.0_f32, Color32::WHITE),
            );
            ui.painter()
                .circle_filled(Pos2::new(x, body.top()), 4.0, Color32::WHITE);
            ui.ctx().request_repaint();
        }
        if response.clicked()
            && response
                .interact_pointer_pos()
                .is_some_and(|p| body.contains(p))
        {
            response.request_focus();
        }
        if response.has_focus() {
            ui.painter()
                .rect_stroke(rect, 0.0, Stroke::new(2.0_f32, ACCENT), StrokeKind::Inside);
        }
        // An open dialog owns the keyboard: its Return mustn't place a slice.
        if response.has_focus() && !self.confirm_open() {
            let direction = ui.input(|input| {
                i16::from(input.key_pressed(egui::Key::ArrowRight))
                    - i16::from(input.key_pressed(egui::Key::ArrowLeft))
            });
            if direction != 0 {
                let slot = (i16::from(self.selected_slot.unwrap_or(0)) + direction)
                    .clamp(0, slots as i16 - 1) as u8;
                self.selected_slot = Some(slot);
                context.automate(P::EditSlot, f64::from(slot) / 31.0);
            }
            if ui.input(|input| input.key_pressed(egui::Key::Enter)) {
                self.place(self.selected_slot.unwrap_or(0), context);
            }
        }
        if let Some(pointer) = ui.ctx().pointer_latest_pos()
            && body.contains(pointer)
        {
            let slot = (((pointer.x - rect.left()) / rect.width()) * slots as f32)
                .floor()
                .clamp(0.0, slots as f32 - 1.0) as u8;
            let target = Rect::from_min_max(
                Pos2::new(
                    rect.left() + rect.width() * slot as f32 / slots as f32,
                    body.top(),
                ),
                Pos2::new(
                    rect.left() + rect.width() * (slot + 1) as f32 / slots as f32,
                    body.bottom(),
                ),
            );
            ui.painter()
                .rect_stroke(target, 0.0, Stroke::new(1.0_f32, INK), StrokeKind::Inside);
            if self.dragging_slice.is_some() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Copy);
            }
            if let Some(item) = placements.iter().find(|item| {
                item.active && item.start_slot <= slot && slot < item.start_slot + item.span_slots
            }) {
                response.clone().on_hover_text(if self.state.get().frozen {
                    "Phrase frozen. Choose Edit under Phrase to change it."
                } else if item.locked {
                    "This slice is locked against replacement. Select it to change Lock, or use Unlock all slices."
                } else {
                    "Click to edit this placed slice."
                });
            }
            if response.clicked() {
                if placements.iter().any(|item| {
                    item.active
                        && item.start_slot <= slot
                        && slot < item.start_slot + item.span_slots
                }) {
                    self.selected_slot = Some(slot);
                    self.swap_slot = Some(slot);
                    context.automate(P::EditSlot, f64::from(slot) / 31.0);
                    if !self.state.get().frozen && !self.state.get().live_enabled {
                        self.message = format!(
                            "Click a source slice to swap it in, or press {DELETE_KEY} to remove it."
                        )
                        .into();
                    }
                } else {
                    self.place(slot, context);
                }
            }
            if ui.is_enabled()
                && self.dragging_slice.is_some()
                && ui.input(|input| input.pointer.any_released())
            {
                self.place(slot, context);
            }
        }
        if ui.input(|input| input.pointer.any_released()) {
            self.dragging_slice = None;
        }
    }

    fn inspector(&mut self, ui: &mut Ui, context: &PluginContext<JunglizerParams>) {
        let Some(slot) = self.selected_slot else {
            return;
        };
        let Some(current) = self
            .state
            .get()
            .placements
            .iter()
            .find(|item| {
                item.active && item.start_slot <= slot && slot < item.start_slot + item.span_slots
            })
            .copied()
        else {
            return;
        };
        ui.horizontal(|ui| {
            ui.strong("Placed slice");
            ui.colored_label(
                SOURCE_COLORS[current.source_loop as usize],
                format!(
                    "{}:{:02}",
                    current.source_loop + 1,
                    current.source_slice + 1
                ),
            );
            ui.colored_label(
                MUTED,
                if current.span_slots == 1 {
                    format!("· slot {}", current.start_slot + 1)
                } else {
                    format!(
                        "· slots {}–{}",
                        current.start_slot + 1,
                        current.start_slot + current.span_slots
                    )
                },
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add_enabled(!current.locked, egui::Button::new("Remove"))
                    .on_hover_text(format!("Remove this slice. {UNDO_HINT} undoes it."))
                    .on_disabled_hover_text("Uncheck Lock before removing.")
                    .clicked()
                {
                    self.remove_placement(slot);
                }
                let mut locked = current.locked;
                if ui
                    .checkbox(&mut locked, "Lock")
                    .on_hover_text(
                        "Protect this placement from fills and replacement; separate from Freeze.",
                    )
                    .changed()
                    && self.edit_placement(current, |p| p.locked = locked)
                {
                    context.automate(P::EditLock, f64::from(locked as u8));
                }
            });
        });
        ui.horizontal(|ui| {
            field_label(ui, "Length", STEPPER_LABEL);
            let mut span = current.span_slots;
            let max = self.state.get().player_slots - current.start_slot;
            let suffix = if span == 1 { " slot" } else { " slots" };
            if stepper(
                ui,
                &mut span,
                1..=max,
                suffix,
                "Slice length in player slots",
            ) && self.edit_placement(current, |p| p.span_slots = span)
            {
                context.automate(P::EditLength, f64::from(span - 1) / 31.0);
            }
            ui.add_space(14.0);
            field_label(ui, "Speed", CHOICE_LABEL);
            if let Some(mode) = segmented(
                ui,
                CHOICE_WIDTH,
                current.mode,
                &[(0, "Original", true, ""), (1, "Fit (pitch)", true, "")],
                None,
            ) && mode != current.mode
                && self.edit_placement(current, |p| p.mode = mode)
            {
                context.automate(P::EditSpeed, f64::from(mode));
            }
        });
        ui.horizontal(|ui| {
            field_label(ui, "Bits", STEPPER_LABEL);
            let mut bits = current.bit_depth;
            if stepper(
                ui,
                &mut bits,
                1..=16,
                " bit",
                "16 bit is clean; lower values crush the sound.",
            ) && self.edit_placement(current, |p| p.bit_depth = bits)
            {
                context.automate(P::EditBits, f64::from(bits - 1) / 15.0);
            }
            ui.add_space(14.0);
            field_label(ui, "Retrigger", CHOICE_LABEL);
            if let Some(n) = segmented(
                ui,
                CHOICE_WIDTH,
                current.repeats,
                &[
                    (1_u8, "1×", true, ""),
                    (2, "2×", true, ""),
                    (4, "4×", true, ""),
                    (8, "8×", true, ""),
                ],
                None,
            ) && n != current.repeats
                && self.edit_placement(current, |p| p.repeats = n)
            {
                context.automate(P::EditRetriggers, f64::from(n.trailing_zeros()) / 3.0);
            }
        });
        ui.colored_label(
            MUTED,
            if current.mode == 1 {
                "Fit fills the span by changing speed and pitch."
            } else {
                "Original speed plays the slice once per retrigger."
            },
        );
    }

    fn placed_slice(&mut self, ui: &mut Ui, context: &PluginContext<JunglizerParams>) {
        let state = self.state.get();
        let editable = !state.frozen && !state.live_enabled;
        let has_selection = self.selected_slot.is_some_and(|slot| {
            state
                .placements
                .iter()
                .any(|p| p.active && p.start_slot <= slot && slot < p.start_slot + p.span_slots)
        });
        if editable && has_selection {
            self.inspector(ui, context);
        } else {
            self.placed_slice_empty(ui);
        }
    }

    /// The Placed slice panel with nothing editable selected: what a click
    /// would place, or why the player is closed to edits.
    fn placed_slice_empty(&self, ui: &mut Ui) {
        ui.strong("Placed slice");
        ui.add_space(8.0);
        let state = self.state.get();
        let name = format!("{}:{:02}", self.selected_loop + 1, self.selected_slice + 1);
        if state.frozen {
            ui.label("Phrase frozen. Choose Edit under Phrase to change it.");
            ui.colored_label(MUTED, "Individual slice locks are preserved.");
        } else if state.live_enabled {
            ui.label("Live variations are running.");
            ui.colored_label(MUTED, "Choose Frozen to keep the phrase you hear.");
        } else if self.shows_break_target() {
            ui.label("Load a break above to start placing slices.");
        } else if state
            .slice(self.selected_loop, self.selected_slice)
            .is_none()
        {
            ui.label(format!(
                "{name} is muted or quiet. Choose another source slice."
            ));
        } else {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                ui.label("Next: ");
                ui.colored_label(SOURCE_COLORS[self.selected_loop], name);
                ui.label(". Click a slot or press Enter.");
            });
            ui.colored_label(MUTED, "Or drag a source pad onto the player.");
        }
    }

    /// Locks every placement that touches `bar`, so fills keep them.
    fn lock_bar(&mut self, bar: usize) {
        if self.state.get().frozen || self.state.get().live_enabled {
            return;
        }
        self.state.update(|state| {
            let start = (bar * state.player_slots as usize).div_ceil(4);
            let end = ((bar + 1) * state.player_slots as usize).div_ceil(4);
            for item in &mut state.placements {
                if item.active
                    && (item.start_slot as usize) < end
                    && start < item.start_slot as usize + item.span_slots as usize
                {
                    item.locked = true;
                }
            }
            state.touch();
        });
        self.message = format!("Bar {} locked. Fills will preserve these slices.", bar + 1).into();
    }

    /// Suggests a fill for `bar` from the selected source break, as the
    /// footer's Suggest a fill did, keeping a copy for Undo.
    fn fill_bar(&mut self, bar: usize, context: &PluginContext<JunglizerParams>) {
        if self.state.get().frozen || self.state.get().live_enabled || bar >= 4 {
            return;
        }
        self.selected_bar = bar;
        context.automate(P::FillBar, bar as f64 / 3.0);
        let snapshot = Undo::of(self.state.get(), format!("Fill bar {}", bar + 1));
        let source = self.selected_loop;
        let mut count = 0;
        self.state
            .update(|state| count = suggest_bar(state, bar, source));
        self.keep_undo(snapshot);
        self.message = if count > 0 {
            "New fill ready. Lock slices you want to keep, then try another."
        } else {
            "No active source slices, or this bar is fully locked."
        }
        .into();
    }

    /// Removes the unlocked placement covering `slot`, keeping a copy for
    /// Undo. False if there was nothing to remove.
    fn remove_placement(&mut self, slot: u8) -> bool {
        let state = self.state.get();
        if state.frozen || state.live_enabled {
            return false;
        }
        let Some(item) = state
            .placements
            .iter()
            .find(|p| p.active && p.start_slot <= slot && slot < p.start_slot + p.span_slots)
            .copied()
            .filter(|p| !p.locked)
        else {
            return false;
        };
        let snapshot = Undo::of(
            state,
            format!(
                "Remove {}:{:02}",
                item.source_loop + 1,
                item.source_slice + 1
            ),
        );
        let mut removed = false;
        self.state.update(|s| removed = s.remove_at_slot(slot));
        if removed {
            self.keep_undo(snapshot);
            self.selected_slot = None;
            self.swap_slot = None;
        }
        removed
    }

    /// Delete or Backspace: removes the placed slice the inspector shows.
    fn delete_selected(&mut self) {
        let Some(slot) = self.selected_slot else {
            return;
        };
        let state = self.state.get();
        let Some(item) = state
            .placements
            .iter()
            .find(|p| p.active && p.start_slot <= slot && slot < p.start_slot + p.span_slots)
            .copied()
        else {
            return;
        };
        if state.frozen || state.live_enabled {
            self.message = "Choose Edit under Phrase before editing the player.".into();
        } else if item.locked {
            self.message = "Uncheck Lock before removing.".into();
        } else if self.remove_placement(slot) {
            self.message = format!(
                "Removed {}:{:02}. {UNDO_HINT} undoes it.",
                item.source_loop + 1,
                item.source_slice + 1
            )
            .into();
        }
    }

    /// Puts source slice `source_slice` of `source_loop` into the unlocked
    /// placement covering `slot`, keeping its length and sound. Undoable.
    fn swap_into(&mut self, slot: u8, source_loop: usize, source_slice: usize) -> bool {
        let state = self.state.get();
        if state.frozen || state.live_enabled {
            return false;
        }
        let Some(old) = state
            .placements
            .iter()
            .find(|p| p.active && p.start_slot <= slot && slot < p.start_slot + p.span_slots)
            .copied()
        else {
            self.swap_slot = None;
            return false;
        };
        if old.locked {
            self.message = "Uncheck Lock before swapping this slice.".into();
            return false;
        }
        if state.slice(source_loop, source_slice).is_none() {
            self.message =
                "This source slice is quiet or unavailable. Choose another slice.".into();
            return false;
        }
        if (usize::from(old.source_loop), usize::from(old.source_slice))
            == (source_loop, source_slice)
        {
            return false;
        }
        let snapshot = Undo::of(
            state,
            format!("Swap {}:{:02}", old.source_loop + 1, old.source_slice + 1),
        );
        let swapped = self.edit_placement(old, |p| {
            p.source_loop = source_loop as u8;
            p.source_slice = source_slice as u8;
        });
        if swapped {
            self.keep_undo(snapshot);
            self.message = format!(
                "Swapped in {}:{:02}. {UNDO_HINT} undoes it.",
                source_loop + 1,
                source_slice + 1
            )
            .into();
        }
        swapped
    }

    /// Holds `snapshot` as the one undo, tied to the version just written.
    fn keep_undo(&mut self, mut snapshot: Undo) {
        snapshot.after = self.state.get().version;
        snapshot.after_placements = self.state.get().placements;
        self.undo = Some(snapshot);
    }

    /// The pending undo's label, while it can still restore: not frozen or
    /// live, and the state is exactly what the action left.
    fn undo_label(&self) -> Option<&str> {
        let state = self.state.get();
        self.undo
            .as_ref()
            .filter(|u| {
                u.after == state.version
                    && u.after_placements == state.placements
                    && !state.frozen
                    && !state.live_enabled
            })
            .map(|u| u.label.as_str())
    }

    /// Drops the undo once anything else has changed the state: a newer
    /// version, or different placements at the same one. An older version
    /// is the host's snapshot from before the action, which the next block
    /// replaces, so the undo waits rather than vanishing.
    fn prune_undo(&mut self) {
        let state = self.state.get();
        if self.undo.as_ref().is_some_and(|u| {
            state.version > u.after
                || (state.version == u.after && state.placements != u.after_placements)
        }) {
            self.undo = None;
        }
    }

    /// Restores the copy taken before the last Fill or Remove. Single use.
    fn undo(&mut self) -> bool {
        self.prune_undo();
        if self.undo_label().is_none() {
            return false;
        }
        let Some(snapshot) = self.undo.take() else {
            return false;
        };
        self.state.update(|s| {
            s.placements = snapshot.placements;
            s.seed = snapshot.seed;
            s.touch();
        });
        self.message = format!("Undid {}.", snapshot.label).into();
        true
    }

    fn edit_placement(
        &mut self,
        old: StepPlacement,
        change: impl FnOnce(&mut StepPlacement),
    ) -> bool {
        let mut updated = old;
        change(&mut updated);
        let mut replaced = false;
        self.state
            .update(|state| replaced = state.replace(old, updated));
        if !replaced {
            self.message =
                "That length reaches another slice. Move or remove the neighboring slice first."
                    .into();
        }
        replaced
    }

    fn freeze_live(&mut self, context: &PluginContext<JunglizerParams>) -> bool {
        let Some(captured) = context.params().live_snapshot.capture() else {
            self.message =
                "Start playback in your DAW to hear a live variation before freezing it.".into();
            return false;
        };
        self.state.update(|state| {
            state.freeze_phrase(
                captured.placements,
                captured.slice_counts,
                captured.player_slots,
                captured.bars,
            );
        });
        context.automate(P::LiveVariations, 0.0);
        self.message =
            "Frozen: this exact four-bar phrase will repeat. Export it as MIDI when ready.".into();
        true
    }

    fn export_midi(&mut self, context: &PluginContext<JunglizerParams>) {
        if self.state.get().live_enabled && !self.freeze_live(context) {
            return;
        }
        let bars = if self.state.get().frozen {
            self.state.get().frozen_bars
        } else {
            [
                context.params().bar_1.value() as u8 - 1,
                context.params().bar_2.value() as u8 - 1,
                context.params().bar_3.value() as u8 - 1,
                context.params().bar_4.value() as u8 - 1,
            ]
        };
        match export_to_music(
            self.state.get(),
            bars,
            context.params().thin_thicc.value(),
            context.params().root.value() as u8,
        ) {
            Ok(path) => {
                if cfg!(target_os = "macos") {
                    crate::platform::reveal(&path);
                }
                self.export_path = Some(path);
                let frozen = if self.state.get().frozen {
                    "; phrase frozen"
                } else {
                    ""
                };
                self.message = format!(
                    "MIDI saved to {}{frozen}. Drag it onto this track, then choose MIDI region.",
                    export_dir_label()
                )
                .into();
            }
            Err(_) => {
                self.export_path = None;
                self.message = format!(
                    "Couldn’t save MIDI. Check write access to {} and try again.",
                    export_dir_label()
                )
                .into();
            }
        }
    }
}

/// The export folder as shown in messages and tooltips.
fn export_dir_label() -> String {
    crate::platform::export_dir().map_or_else(
        || "your Music folder".to_owned(),
        |dir| {
            short_dir(
                &dir,
                dirs::home_dir().as_deref(),
                cfg!(target_os = "windows"),
            )
        },
    )
}

/// A short form of `dir` for the status row: the home folder collapses to
/// `~`, or on Windows only the last three components remain, as `…\a\b\c`.
/// The full path stays in hover text.
pub(crate) fn short_dir(
    dir: &std::path::Path,
    home: Option<&std::path::Path>,
    windows: bool,
) -> String {
    if windows {
        let parts: Vec<_> = dir
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect();
        let tail = &parts[parts.len().saturating_sub(3)..];
        return format!("…\\{}", tail.join("\\"));
    }
    match home.and_then(|h| dir.strip_prefix(h).ok()) {
        Some(rest) => std::path::Path::new("~").join(rest).display().to_string(),
        None => dir.display().to_string(),
    }
}

impl EditorUi<JunglizerParams> for JunglizerUi {
    fn opened(&mut self, context: &PluginContext<JunglizerParams>) {
        self.state = StateBinding::new(context);
        self.params = Some(context.params().clone());
        // Without a config folder, act as if the prompt was answered.
        let flag_exists = crate::platform::first_run_flag().is_none_or(|flag| flag.exists());
        self.first_run = ui_slots::show_first_run(flag_exists, self.state.get());
        self.refresh_slot_audio();
        for index in 0..4 {
            if self
                .state
                .get()
                .marker_set(index)
                .is_some_and(|set| !set.valid)
            {
                let count = self.state.get().slice_count(index);
                self.ensure_markers(index, count);
            }
        }
    }

    fn state_changed(&mut self, _context: &PluginContext<JunglizerParams>) {
        self.state.sync();
    }

    fn ui(&mut self, ui: &mut Ui, context: &PluginContext<JunglizerParams>) {
        // Host automation changes the DSP state outside the editor. Keep the
        // controls and waveform selection in step with that saved state.
        self.state.sync();
        if self.params.is_none() {
            self.params = Some(context.params().clone());
        }
        self.poll_pick();
        self.poll_pending_load(ui.ctx());
        self.refresh_slot_audio();
        self.prune_undo();
        // Cmd+Z on macOS, Ctrl+Z elsewhere; a field being typed in keeps
        // it, and an open dialog blocks it.
        if !self.confirm_open()
            && !text_field_focused(ui.ctx())
            && ui
                .input(|i| i.modifiers.command && !i.modifiers.shift && i.key_pressed(egui::Key::Z))
        {
            self.undo();
        }
        if !self.confirm_open() && !text_field_focused(ui.ctx()) {
            if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                self.swap_slot = None;
            }
            if ui.input(|i| {
                (i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace))
                    && !i.modifiers.any()
            }) {
                self.delete_selected();
            }
        }
        self.drop_targets.clear();
        let (hovered_files, dropped_files) = ui
            .ctx()
            .input(|i| (i.raw.hovered_files.clone(), i.raw.dropped_files.clone()));
        self.selected_loop = (context.params().edit_break.value() as usize - 1).min(3);
        self.selected_slice = (context.params().edit_slice.value() as usize - 1)
            .min(self.state.get().slice_count(self.selected_loop) as usize - 1);
        self.selected_slot = Some(
            (context.params().edit_slot.value() as u8 - 1).min(self.state.get().player_slots - 1),
        );
        self.selected_bar = (context.params().fill_bar.value() as usize - 1).min(3);
        // State restored or edited after opening: the prompt waits for the
        // next fresh instance, so Choose… never clears someone's work.
        if self.first_run && !self.state.get().is_pristine() {
            self.first_run = false;
        }
        let live = if self.state.get().live_enabled {
            context.params().live_snapshot.capture()
        } else {
            None
        };
        apply_style(ui);
        ui.painter().rect_filled(ui.max_rect(), 0.0, CANVAS);
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                egui::Frame::new().inner_margin(14.0).show(ui, |ui| {
                    self.header(ui, context);
                    ui.add_space(4.0);
                    self.sources_ui(ui, context);
                    ui.add_space(3.0);
                    ui.separator();
                    self.player_header(ui, context, live.as_ref());
                    let dimmed = self.shows_break_target();
                    ui.scope(|ui| {
                        if dimmed {
                            // Drawn faded and deaf to clicks until there's a break.
                            ui.disable();
                            ui.multiply_opacity(0.7);
                        }
                        self.player(ui, context, live.as_ref());
                    });
                    ui.add_space(3.0);
                    ui.horizontal_top(|ui| {
                        // Placed slice takes about 62% and Phrase the rest.
                        let gap = 8.0;
                        ui.spacing_mut().item_spacing.x = gap;
                        let total = ui.available_width() - gap;
                        let left = (total * 0.62).floor();
                        for (width, placed) in [(left, true), (total - left, false)] {
                            ui.allocate_ui_with_layout(
                                vec2(width, 0.0),
                                egui::Layout::top_down(egui::Align::Min),
                                |ui| {
                                    egui::Frame::new()
                                        .fill(SURFACE)
                                        .corner_radius(4.0)
                                        .inner_margin(10.0)
                                        .show(ui, |ui| {
                                            ui.spacing_mut().item_spacing.x = 8.0;
                                            ui.set_width(width - 20.0);
                                            ui.set_min_height(118.0);
                                            if placed {
                                                self.placed_slice(ui, context);
                                            } else {
                                                self.actions(ui, context, live.as_ref());
                                            }
                                        });
                                },
                            );
                        }
                    });
                    ui.add_space(2.0);
                    let mut undo_now = false;
                    ui.horizontal(|ui| {
                        let initial = self.message == INITIAL_MESSAGE;
                        let message = if initial && self.state.get().frozen {
                            "Frozen phrase repeats exactly. Choose Edit to change it."
                        } else if initial && self.state.get().live_enabled {
                            "A new phrase every four bars. Choose Frozen to keep the one you hear."
                        } else {
                            &self.message
                        };
                        // The reveal and undo buttons at the right; the message
                        // takes what's left and truncates, so nothing overlaps.
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let path = self.export_path.clone();
                            if let Some(path) = &path {
                                if ui.small_button(crate::platform::REVEAL_LABEL).clicked() {
                                    crate::platform::reveal(path);
                                }
                            }
                            let undo = self.undo_label().map(|label| format!("Undo {label}"));
                            if let Some(undo) = undo
                                && ui.small_button(undo).on_hover_text(UNDO_HINT).clicked()
                            {
                                undo_now = true;
                            }
                            let response = ui.add(
                                egui::Label::new(egui::RichText::new(message).color(ACCENT))
                                    .truncate(),
                            );
                            // The whole message, then the full export path under it.
                            match path {
                                Some(path) => {
                                    response
                                        .on_hover_text(format!("{message}\n{}", path.display()));
                                }
                                None => {
                                    response.on_hover_text(message);
                                }
                            }
                        });
                    });
                    if undo_now {
                        self.undo();
                    }
                });
            });
        self.handle_file_drop(ui.ctx(), &hovered_files, &dropped_files);
        self.unload_modal(ui.ctx());
    }
}

pub fn visuals() -> egui::Visuals {
    let mut v = egui::Visuals::dark();
    v.panel_fill = CANVAS;
    v.window_fill = SURFACE;
    v.extreme_bg_color = WELL;
    v.faint_bg_color = SURFACE;
    v.override_text_color = Some(INK);
    v.selection.bg_fill = Color32::from_rgb(75, 85, 39);
    v.selection.stroke = Stroke::new(1.5_f32, ACCENT);
    for widget in [
        &mut v.widgets.noninteractive,
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
        &mut v.widgets.open,
    ] {
        widget.corner_radius = egui::CornerRadius::same(3);
        widget.bg_fill = SURFACE;
        widget.weak_bg_fill = SURFACE;
        widget.bg_stroke = Stroke::new(1.0_f32, EDGE);
        widget.fg_stroke = Stroke::new(1.0_f32, INK);
        widget.expansion = 0.0;
    }
    v.widgets.hovered.bg_fill = Color32::from_rgb(57, 64, 39);
    v.widgets.hovered.weak_bg_fill = v.widgets.hovered.bg_fill;
    v.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, ACCENT);
    v.widgets.active.bg_fill = Color32::from_rgb(75, 85, 39);
    v.widgets.active.weak_bg_fill = v.widgets.active.bg_fill;
    v.widgets.active.bg_stroke = Stroke::new(2.0_f32, ACCENT);
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, EDGE);
    v
}

fn apply_style(ui: &mut Ui) {
    let style = ui.style_mut();
    style.visuals = visuals();
    style.spacing.item_spacing = vec2(8.0, 5.0);
    style.spacing.button_padding = vec2(8.0, 4.0);
    style.spacing.interact_size = vec2(28.0, 26.0);
    style.spacing.slider_width = 108.0;
    style
        .text_styles
        .insert(egui::TextStyle::Body, egui::FontId::proportional(13.0));
    style
        .text_styles
        .insert(egui::TextStyle::Button, egui::FontId::proportional(13.0));
    style
        .text_styles
        .insert(egui::TextStyle::Small, egui::FontId::proportional(12.0));
}

// Native slider interaction retains keyboard and host automation behavior.
fn tone_crossfader(ui: &mut Ui, context: &PluginContext<JunglizerParams>) {
    ui.spacing_mut().item_spacing.y = 2.0;
    let mut amount = context.params().thin_thicc.value();
    let response = ui
        .scope(|ui| {
            ui.spacing_mut().slider_width = 280.0;
            ui.spacing_mut().interact_size.y = 32.0;
            ui.spacing_mut().slider_rail_height = 12.0;
            let visuals = ui.visuals_mut();
            for widget in [
                &mut visuals.widgets.inactive,
                &mut visuals.widgets.hovered,
                &mut visuals.widgets.active,
            ] {
                // The rail: EDGE reads clearly against the canvas.
                widget.bg_fill = EDGE;
                widget.fg_stroke = Stroke::new(1.0_f32, INK);
            }
            ui.add(
                egui::Slider::new(&mut amount, -1.0..=1.0)
                    .show_value(false)
                    .handle_shape(egui::style::HandleShape::Rect { aspect_ratio: 0.85 }),
            )
        })
        .inner;
    if response.drag_started() {
        context.begin_edit(P::ThinThicc);
    }
    if response.changed() {
        let value = f64::from((amount + 1.0) * 0.5);
        if response.dragged() {
            context.set_param(P::ThinThicc, value);
        } else {
            context.automate(P::ThinThicc, value);
        }
    }
    if response.drag_stopped() {
        context.end_edit(P::ThinThicc);
    }
    if response.double_clicked() {
        context.automate(P::ThinThicc, 0.5);
    }
    let rect = response.rect;
    let painter = ui.painter();
    let radius = rect.height() / 2.5;
    let half = radius * 0.85;
    let x = egui::lerp(
        (rect.left() + half)..=(rect.right() - half),
        (amount + 1.0) * 0.5,
    );
    let center = rect.center().x;
    // Lime from the Dry detent to the cap, at the rail's own height.
    if amount != 0.0 {
        let rail = 12.0 / 2.0;
        painter.rect_filled(
            Rect::from_min_max(
                Pos2::new(center.min(x), rect.center().y - rail),
                Pos2::new(center.max(x), rect.center().y + rail),
            ),
            0.0,
            ACCENT,
        );
    }
    // Crossing Dry, Thin to Thicc or back, kicks the marks out to 6 px;
    // they ease back to 3 px over 150 ms. Leaving or landing on exact Dry
    // isn't a crossing. Nothing animates at rest.
    let sign = if amount > 0.0 {
        1_i8
    } else if amount < 0.0 {
        -1
    } else {
        0
    };
    let now = ui.input(|i| i.time);
    let id = egui::Id::new("thin-thicc-detent");
    let (side, crossed) = ui
        .memory(|m| m.data.get_temp::<(i8, f64)>(id))
        .unwrap_or((sign, f64::NEG_INFINITY));
    let (did_cross, side) = detent_crossing(side, sign);
    let crossed = if did_cross { now } else { crossed };
    ui.memory_mut(|m| m.data.insert_temp(id, (side, crossed)));
    let t = ((now - crossed) / 0.15) as f32;
    let tick = if (0.0..1.0).contains(&t) {
        ui.ctx().request_repaint();
        3.0 + 3.0 * (1.0 - t) * (1.0 - t)
    } else {
        3.0
    };
    // Center detent marks remain visible above and below the cap.
    for (from, to) in [
        (rect.top(), rect.top() + tick),
        (rect.bottom() - tick, rect.bottom()),
    ] {
        painter.line_segment(
            [Pos2::new(center, from), Pos2::new(center, to)],
            Stroke::new(2.0_f32, ACCENT),
        );
    }
    let cap = Rect::from_center_size(
        Pos2::new(x, rect.center().y),
        vec2(half * 2.0, radius * 2.0),
    );
    painter.rect_filled(cap, 2.0, INK);
    for dx in [-4.0, 0.0, 4.0] {
        painter.line_segment(
            [
                Pos2::new(x + dx, cap.top() + 5.0),
                Pos2::new(x + dx, cap.bottom() - 5.0),
            ],
            Stroke::new(1.0_f32, EDGE),
        );
    }
    response.on_hover_text(
        "Thin to Thicc · center is Dry · double-click for Dry · arrow keys when focused",
    );
    let (labels, _) = ui.allocate_exact_size(vec2(280.0, 16.0), Sense::hover());
    let font = egui::FontId::proportional(11.0);
    for (x, align, label) in [
        (labels.left(), egui::Align2::LEFT_TOP, "Thin"),
        (labels.center().x, egui::Align2::CENTER_TOP, "Dry"),
    ] {
        ui.painter().text(
            Pos2::new(x, labels.top() + 1.0),
            align,
            label,
            font.clone(),
            MUTED,
        );
    }
    dymo_label(ui.painter(), labels.right(), labels.top(), "Thicc", font);
}

/// Whether the fader crossed Dry: `side` is the last side it was on (−1
/// Thin, +1 Thicc, 0 not yet off Dry) and `sign` is its side now (0 at
/// exact Dry). Returns the crossing and the side to remember.
fn detent_crossing(side: i8, sign: i8) -> (bool, i8) {
    let crossed = sign != 0 && side != 0 && sign != side;
    (crossed, if sign == 0 { side } else { sign })
}

/// Embossed label tape, Dymo style: a dark plastic strip with a little
/// margin around the word, a sheen along its top edge and a shadow along
/// its bottom, both ends cut at an angle as scissors leave it, and ivory
/// letters over a darker copy one pixel down. The tape's right end sits at
/// `right`.
fn dymo_label(painter: &egui::Painter, right: f32, top: f32, text: &str, font: egui::FontId) {
    let tape = darken(EDGE, 0.8);
    let galley = painter.layout_no_wrap(text.to_owned(), font, INK);
    let size = galley.size();
    let height = size.y + 3.0;
    let cut = 3.0;
    let pad = 4.0;
    let left = right - size.x - 2.0 * pad - 2.0 * cut;
    let bottom = top + height;
    painter.add(egui::Shape::convex_polygon(
        vec![
            Pos2::new(left + cut, top),
            Pos2::new(right, top),
            Pos2::new(right - cut, bottom),
            Pos2::new(left, bottom),
        ],
        tape,
        Stroke::NONE,
    ));
    // Sheen and shadow, inset so they stop at the angled cuts.
    painter.line_segment(
        [
            Pos2::new(left + cut + 1.0, top + 0.5),
            Pos2::new(right - 1.0, top + 0.5),
        ],
        Stroke::new(1.0_f32, EDGE),
    );
    painter.line_segment(
        [
            Pos2::new(left + 1.0, bottom - 0.5),
            Pos2::new(right - cut - 1.0, bottom - 0.5),
        ],
        Stroke::new(1.0_f32, WELL),
    );
    let at = Pos2::new(left + cut + pad, top + 1.5);
    painter.galley_with_override_text_color(at + vec2(0.0, 1.0), galley.clone(), WELL);
    painter.galley(at, galley, INK);
}

/// `color` scaled toward black, keeping it opaque.
fn darken(color: Color32, factor: f32) -> Color32 {
    let scale = |c: u8| (f32::from(c) * factor).round() as u8;
    Color32::from_rgb(scale(color.r()), scale(color.g()), scale(color.b()))
}

fn output_knob(ui: &mut Ui, context: &PluginContext<JunglizerParams>) {
    let (rect, response) = ui.allocate_exact_size(vec2(66.0, 66.0), Sense::click_and_drag());
    let mut gain = context.params().gain.value();
    if response.clicked() {
        response.request_focus();
    }
    if response.drag_started() {
        context.begin_edit(P::Gain);
    }
    if response.dragged() {
        let delta = ui.input(|i| -i.pointer.delta().y);
        let fine = ui.input(|i| i.modifiers.shift);
        gain = (gain + delta * if fine { 0.025 } else { 0.25 }).clamp(-60.0, 6.0);
        context.set_param(P::Gain, f64::from((gain + 60.0) / 66.0));
    }
    if response.drag_stopped() {
        context.end_edit(P::Gain);
    }
    if response.double_clicked() {
        gain = 0.0;
        context.automate(P::Gain, 60.0 / 66.0);
    }
    if response.has_focus() {
        let step = ui.input(|i| {
            let up = i.key_pressed(egui::Key::ArrowUp) || i.key_pressed(egui::Key::ArrowRight);
            let down = i.key_pressed(egui::Key::ArrowDown) || i.key_pressed(egui::Key::ArrowLeft);
            (i32::from(up) - i32::from(down)) as f32 * if i.modifiers.shift { 0.1 } else { 1.0 }
        });
        if step != 0.0 {
            gain = (gain + step).clamp(-60.0, 6.0);
            context.automate(P::Gain, f64::from((gain + 60.0) / 66.0));
        }
    }
    response.widget_info(|| egui::WidgetInfo::slider(true, f64::from(gain), "Output dB"));
    let center = rect.center();
    let start = std::f32::consts::PI * 0.75;
    let sweep = std::f32::consts::PI * 1.5;
    let fraction = (gain + 60.0) / 66.0;
    let arc = |amount: f32| -> Vec<Pos2> {
        (0..=64)
            .map(|i| {
                let angle = start + sweep * amount * i as f32 / 64.0;
                center + vec2(angle.cos(), angle.sin()) * 30.0
            })
            .collect()
    };
    let painter = ui.painter();
    painter.add(egui::Shape::line(arc(1.0), Stroke::new(3.0_f32, EDGE)));
    painter.add(egui::Shape::line(
        arc(fraction),
        Stroke::new(3.0_f32, ACCENT),
    ));
    painter.circle_filled(center, 23.0, SURFACE);
    painter.circle_stroke(
        center,
        23.0,
        Stroke::new(
            1.5_f32,
            if response.hovered() || response.has_focus() {
                ACCENT
            } else {
                MUTED
            },
        ),
    );
    let angle = start + sweep * fraction;
    let direction = vec2(angle.cos(), angle.sin());
    painter.line_segment(
        [center + direction * 12.0, center + direction * 20.0],
        Stroke::new(3.0_f32, INK),
    );
    response.on_hover_text("Output volume · drag up/down · Shift for fine adjustment · double-click for 0 dB · arrow keys when focused");
    ui.vertical(|ui| {
        ui.label("Output");
        ui.strong(format!("{gain:.1} dB"));
    });
}

impl JunglizerUi {
    fn header(&mut self, ui: &mut Ui, context: &PluginContext<JunglizerParams>) {
        if self.wordmark.is_none() {
            let decoder = png::Decoder::new(std::io::Cursor::new(include_bytes!(
                "../assets/junglizer-wordmark.png"
            )));
            if let Ok(mut reader) = decoder.read_info() {
                let mut bytes = vec![0; reader.output_buffer_size().unwrap_or(0)];
                if let Ok(info) = reader.next_frame(&mut bytes) {
                    let image = egui::ColorImage::from_rgba_unmultiplied(
                        [info.width as usize, info.height as usize],
                        &bytes[..info.buffer_size()],
                    );
                    self.wordmark = Some(ui.ctx().load_texture(
                        "junglizer-wordmark",
                        image,
                        egui::TextureOptions::LINEAR,
                    ));
                }
            }
        }
        ui.horizontal(|ui| {
            if let Some(texture) = &self.wordmark {
                ui.add(
                    egui::Image::new(texture)
                        .uv(Rect::from_min_max(
                            Pos2::new(0.0, 0.15),
                            Pos2::new(1.0, 0.80),
                        ))
                        .maintain_aspect_ratio(false)
                        .fit_to_exact_size(vec2(250.0, 65.0)),
                )
                .on_hover_text(WORDMARK_HOVER);
            } else {
                ui.heading("JUNGLIZER").on_hover_text(WORDMARK_HOVER);
            }
            ui.add_space(12.0);
            ui.spacing_mut().item_spacing.x = 0.0;
            let group_gap = ((ui.available_width() - 616.0) / 3.0).max(8.0);
            ui.allocate_ui_with_layout(
                vec2(108.0, 66.0),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    let transport = context.transport();
                    let playing = transport.as_ref().is_some_and(|t| t.playing);
                    ui.colored_label(
                        if playing { ACCENT } else { MUTED },
                        if playing { "Playing" } else { "Stopped" },
                    );
                    if let Some(t) = transport.as_ref().filter(|t| t.playing) {
                        let beat = t.position_beats.rem_euclid(16.0);
                        ui.label(format!(
                            "Bar {} · beat {}",
                            (beat / 4.0).floor() as u8 + 1,
                            beat.floor() as u8 + 1
                        ));
                    }
                    ui.label(transport.map_or("Host sync".into(), |t| {
                        format!("{:.0} BPM · 4 bars", t.tempo)
                    }));
                },
            );
            ui.add_space(group_gap);
            ui.allocate_ui_with_layout(
                vec2(280.0, 66.0),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    tone_crossfader(ui, context);
                },
            );
            ui.add_space(group_gap);
            ui.allocate_ui_with_layout(
                vec2(88.0, 66.0),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.label("808 root");
                    const KEYS: [&str; 12] = [
                        "C", "C♯", "D", "D♯", "E", "F", "F♯", "G", "G♯", "A", "A♯", "B",
                    ];
                    let root = context.params().root.value() as usize;
                    egui::ComboBox::from_id_salt("root-key")
                        .width(88.0)
                        .selected_text(KEYS[root.min(11)])
                        .show_ui(ui, |ui| {
                            for (i, name) in KEYS.iter().enumerate() {
                                if ui.selectable_label(root == i, *name).clicked() {
                                    context.automate(P::Root, i as f64 / 11.0);
                                }
                            }
                        });
                },
            );
            ui.add_space(group_gap);
            ui.allocate_ui_with_layout(
                vec2(140.0, 66.0),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    output_knob(ui, context);
                },
            );
        });
    }

    fn sources_ui(&mut self, ui: &mut Ui, context: &PluginContext<JunglizerParams>) {
        let editable = !self.state.get().frozen && !self.state.get().live_enabled;
        ui.horizontal(|ui| {
            ui.strong("Source");
            self.slot_row(ui, context);
            ui.separator();
            let mut count = self.state.get().slice_count(self.selected_loop);
            ui.label("Source slices");
            if ui
                .add_enabled(editable, egui::DragValue::new(&mut count).range(1..=32))
                .on_disabled_hover_text("Choose Edit under Phrase to edit source slices.")
                .changed()
            {
                self.ensure_markers(self.selected_loop, count);
                context.automate(P::SourceSlices, f64::from(count - 1) / 31.0);
            }
            ui.separator();
            if ui
                .button(format!(
                    "Audition {}:{:02}",
                    self.selected_loop + 1,
                    self.selected_slice + 1
                ))
                .clicked()
            {
                context.params().audition.store(
                    (self.selected_loop * 32 + self.selected_slice + 1) as u32,
                    Ordering::Relaxed,
                );
            }
            let mut muted = self
                .state
                .get()
                .marker_set(self.selected_loop)
                .is_none_or(|set| !set.valid || !set.slices[self.selected_slice].active);
            if ui
                .add_enabled(editable, egui::Checkbox::new(&mut muted, "Mute"))
                .changed()
            {
                self.set_source_muted(context, muted);
            }
            let mut bass = self
                .state
                .get()
                .marker_set(self.selected_loop)
                .is_some_and(|set| set.valid && set.slices[self.selected_slice].bass);
            if ui
                .add_enabled(editable, egui::Checkbox::new(&mut bass, "Bass hit"))
                .changed()
            {
                let source = self.selected_loop;
                let slice = self.selected_slice;
                self.state.update(|state| {
                    if let Some(set) = state.marker_set_mut(source) {
                        set.slices[slice].bass = bass;
                        state.touch();
                    }
                });
                context.automate(P::BassHit, f64::from(bass as u8));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                self.slot_controls(ui);
            });
        });
        self.source_waveform(ui, context);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 3.0;
            ui.spacing_mut().button_padding.x = 4.0;
            let count = self.state.get().slice_count(self.selected_loop) as usize;
            let width =
                ((ui.available_width() - (count - 1) as f32 * 3.0) / count as f32).max(26.0);
            for i in 0..count {
                let active = self.state.get().slice(self.selected_loop, i).is_some();
                let label = egui::RichText::new(format!("{:02}", i + 1)).color(if active {
                    WELL
                } else {
                    MUTED
                });
                let label = if active { label } else { label.strikethrough() };
                let response = ui.add_sized(
                    [width, 30.0],
                    egui::Button::new(label.strong())
                        .fill(if active {
                            SOURCE_COLORS[self.selected_loop]
                        } else {
                            SURFACE
                        })
                        .stroke(Stroke::new(
                            if self.selected_slice == i {
                                2.0_f32
                            } else {
                                1.0_f32
                            },
                            if self.selected_slice == i {
                                ACCENT
                            } else {
                                EDGE
                            },
                        ))
                        .sense(Sense::click_and_drag()),
                );
                if active {
                    source_pattern(
                        ui.painter(),
                        response.rect.shrink(2.0),
                        self.selected_loop,
                        Color32::from_black_alpha(24),
                    );
                    // Redraw the label above the pattern for crisp figures.
                    ui.painter().text(
                        response.rect.center(),
                        egui::Align2::CENTER_CENTER,
                        format!("{:02}", i + 1),
                        egui::FontId::proportional(13.0),
                        WELL,
                    );
                    if self
                        .state
                        .get()
                        .marker_set(self.selected_loop)
                        .is_some_and(|set| set.slices[i].bass)
                    {
                        ui.painter().circle_filled(
                            response.rect.center_bottom() - vec2(0.0, 4.0),
                            2.0,
                            WELL,
                        );
                    }
                }
                if self.selected_slice == i {
                    ui.painter().rect_stroke(
                        response.rect.shrink(1.0),
                        3.0,
                        Stroke::new(2.0_f32, ACCENT),
                        StrokeKind::Inside,
                    );
                }
                if response.clicked() || response.drag_started() {
                    self.selected_slice = i;
                    context.automate(P::EditSlice, i as f64 / 31.0);
                }
                if response.clicked()
                    && let Some(slot) = self.swap_slot
                {
                    self.swap_into(slot, self.selected_loop, i);
                }
                if response.clicked() {
                    context
                        .params()
                        .audition
                        .store((self.selected_loop * 32 + i + 1) as u32, Ordering::Relaxed);
                }
                if response.drag_started() && editable && active {
                    self.dragging_slice = Some((self.selected_loop, i));
                }
                if response.secondary_clicked() && editable {
                    self.selected_slice = i;
                    context.automate(P::EditSlice, i as f64 / 31.0);
                    self.set_source_muted(context, active);
                }
                response.on_hover_text(if active && self.swap_slot.is_some() {
                    "Click to swap into the selected slice · Drag into player · Right-click to mute"
                } else if active {
                    "Click to audition · Drag into player · Right-click to mute"
                } else {
                    "Muted or quiet slice · Select and uncheck Mute to activate"
                });
            }
        });
    }

    fn placement_waveform(
        &self,
        painter: &egui::Painter,
        block: Rect,
        item: StepPlacement,
        slots: u8,
        tempo: f64,
    ) {
        let source = item.source_loop as usize;
        let Some(slice) = self.state.get().slice(source, item.source_slice as usize) else {
            return;
        };
        let Some(sample) = self.slot_audio[source].sample() else {
            return;
        };
        let frames = sample.frames.len();
        let paint = painter.with_clip_rect(block.shrink(2.0));
        let columns = block.width().max(1.0) as usize;
        for column in 0..columns {
            let fraction = (column as f64 + 0.5) / columns as f64;
            let Some(frame) =
                preview_frame(item, slice, fraction, slots, tempo, sample.sample_rate)
            else {
                continue;
            };
            let bin = ((frame / frames as f64) * 512.0).floor().clamp(0.0, 511.0) as usize;
            let amplitude = self.slot_audio[source].peaks[bin].sqrt().min(1.0);
            let half = amplitude * block.height() * 0.27;
            let x = block.left() + column as f32;
            paint.line_segment(
                [
                    Pos2::new(x, block.center().y - half),
                    Pos2::new(x, block.center().y + half),
                ],
                Stroke::new(1.0_f32, Color32::from_black_alpha(170)),
            );
        }
    }

    fn set_source_muted(&mut self, context: &PluginContext<JunglizerParams>, muted: bool) {
        let source = self.selected_loop;
        let slice = self.selected_slice;
        self.state.update(|state| {
            if let Some(set) = state.marker_set_mut(source) {
                set.slices[slice].active = !muted;
                state.touch();
            }
        });
        context.automate(P::SliceMuted, f64::from(muted as u8));
    }

    fn player_header(
        &mut self,
        ui: &mut Ui,
        context: &PluginContext<JunglizerParams>,
        live: Option<&CapturedPhrase>,
    ) {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Player").size(19.0).strong());
            ui.label("Player slots");
            let mut slots = self.state.get().player_slots;
            if ui.add_enabled(!self.state.get().frozen && !self.state.get().live_enabled, egui::DragValue::new(&mut slots).range(1..=32)).on_disabled_hover_text("Choose Edit under Phrase to change the player grid.").changed() {
                self.state.update(|state| { state.set_player_slots(slots); }); context.automate(P::PlayerSlots, f64::from(slots - 1) / 31.0);
            }
            ui.colored_label(MUTED, "16 beats");
            ui.separator();
            if self.state.get().frozen {
                ui.colored_label(ACCENT, "Frozen");
            } else if let Some(phrase) = live {
                ui.colored_label(ACCENT, format!("Live · variation {}", phrase.cycle + 1));
            } else { ui.colored_label(MUTED, if self.state.get().live_enabled { "Live · waiting for playback" } else { "Editing" }); }
            let locked = self.state.get().placements.iter().filter(|p| p.active && p.locked).count();
            if locked > 0 {
                ui.label(format!("{locked} locked"));
                if ui.add_enabled(!self.state.get().frozen && !self.state.get().live_enabled, egui::Button::new("Unlock all slices")).on_disabled_hover_text("Choose Edit under Phrase first; individual locks stay separate from Freeze.").clicked() {
                    self.state.update(|s| s.unlock_all()); context.automate(P::EditLock, 0.0);
                    self.message = "All placed slices are unlocked.".into();
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.colored_label(SOURCE_COLORS[0], "Placed");
                ui.colored_label(MUTED, "Ghost in empty slots  / ");
            });
        });
    }

    /// Acts on a Mode choice: 0 Edit (stops Live or unfreezes, keeping
    /// slice locks), 1 Live, 2 Frozen (holds what's playing, freezing a
    /// live variation). Choosing the current mode does nothing.
    fn choose_mode(&mut self, choice: u8, context: &PluginContext<JunglizerParams>) {
        let frozen = self.state.get().frozen;
        let live_on = self.state.get().live_enabled;
        match choice {
            0 if frozen => {
                self.state.update(|s| {
                    s.frozen = false;
                    s.touch();
                });
                context.automate(P::LiveVariations, 0.0);
                self.message = "Phrase unfrozen. Individual slice locks are preserved.".into();
            }
            0 if live_on => {
                self.state.update(|s| {
                    s.live_enabled = false;
                    s.touch();
                });
                context.automate(P::LiveVariations, 0.0);
                self.message = "Live stopped. Your hand-edited pattern is back.".into();
            }
            1 if !live_on && !frozen => {
                self.state.update(|s| {
                    s.live_enabled = true;
                    s.touch();
                });
                context.automate(P::LiveVariations, 1.0);
                self.message =
                    "Live variations on. Choose Frozen to keep the phrase you hear.".into();
            }
            2 if !frozen => {
                if live_on {
                    self.freeze_live(context);
                } else {
                    let bars = [
                        context.params().bar_1.value(),
                        context.params().bar_2.value(),
                        context.params().bar_3.value(),
                        context.params().bar_4.value(),
                    ]
                    .map(|v| (v - 1) as u8);
                    self.state.update(|s| {
                        s.freeze_phrase(s.placements, s.slice_counts(), s.player_slots, bars)
                    });
                    context.automate(P::LiveVariations, 0.0);
                    self.message =
                        "Phrase frozen. Choose Edit to change it; slice locks are unchanged."
                            .into();
                }
            }
            _ => {}
        }
    }

    fn actions(
        &mut self,
        ui: &mut Ui,
        context: &PluginContext<JunglizerParams>,
        live: Option<&CapturedPhrase>,
    ) {
        let frozen = self.state.get().frozen;
        let live_on = self.state.get().live_enabled;
        ui.horizontal(|ui| {
            ui.strong("Phrase");
            ui.colored_label(MUTED, "Fills keep locked slices");
        });
        ui.horizontal(|ui| {
            field_label(ui, "Mode", PHRASE_LABEL);
            let mode = if frozen {
                2
            } else if live_on {
                1
            } else {
                0
            };
            let can_freeze = !live_on || live.is_some();
            let choice = segmented(
                ui,
                ui.available_width(),
                mode,
                &[
                    (0, "Edit", true, ""),
                    (1, "Live", !frozen, "Choose Edit first to leave the frozen phrase."),
                    (
                        2,
                        "Frozen",
                        frozen || can_freeze,
                        "Waiting for the first live phrase. Start host playback to capture a variation.",
                    ),
                ],
                Some(&[
                    "Edit by hand. Leaving Frozen keeps individual slice locks.",
                    "A new phrase every four bars.",
                    "Hold what’s playing, live variation included, so it repeats exactly.",
                ]),
            );
            if let Some(choice) = choice {
                self.choose_mode(choice, context);
            }
        });
        ui.horizontal(|ui| {
            field_label(ui, "Playback", PHRASE_LABEL);
            let midi = context.params().playback.value() != 0;
            if let Some(choice) = segmented(
                ui,
                ui.available_width(),
                u8::from(midi),
                &[(0, "Internal", true, ""), (1, "MIDI region", true, "")],
                Some(&[
                    "Junglizer plays the phrase itself.",
                    "Play an exported MIDI region on this track instead.",
                ]),
            ) && choice != u8::from(midi)
            {
                context.automate(P::Playback, f64::from(choice));
            }
        });
        let text = egui::RichText::new("Export MIDI")
            .strong()
            .color(if frozen { WELL } else { ACCENT });
        let button = egui::Button::new(text)
            .fill(if frozen { ACCENT } else { SURFACE })
            .stroke(Stroke::new(1.5_f32, ACCENT));
        if ui
            .add_sized([ui.available_width(), 28.0], button)
            .on_hover_text(format!(
                "Save four bars to {}. Live variations are frozen before export.",
                export_dir_label()
            ))
            .clicked()
        {
            self.export_midi(context);
        }
    }
}

// The footer grid: shared label columns, and one width for the Speed and
// Retrigger choices so their groups end together.
const STEPPER_LABEL: f32 = 52.0;
const CHOICE_LABEL: f32 = 66.0;
const PHRASE_LABEL: f32 = 62.0;
const CHOICE_WIDTH: f32 = 188.0;
/// The widest stepper value, which sets every stepper's value field.
const STEPPER_WIDEST: &str = "32 slots";

/// A text field (including a DragValue being typed into) has keyboard
/// focus, so Cmd/Ctrl+Z belongs to it. Focus on any other widget, such as
/// the player, leaves Undo to the editor.
fn text_field_focused(ctx: &egui::Context) -> bool {
    ctx.memory(|m| m.focused())
        .is_some_and(|id| egui::TextEdit::load_state(ctx, id).is_some())
}

/// A fixed-width field label, so rows of controls line up.
fn field_label(ui: &mut Ui, text: &str, width: f32) {
    ui.allocate_ui_with_layout(
        vec2(width, ui.spacing().interact_size.y),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.set_min_width(width);
            ui.label(text);
        },
    );
}

/// A draggable value with − and + buttons beside it. True if it changed.
fn stepper(
    ui: &mut Ui,
    value: &mut u8,
    range: std::ops::RangeInclusive<u8>,
    suffix: &str,
    hover: &str,
) -> bool {
    let (low, high) = (*range.start(), *range.end());
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.x = 3.0;
        let mut changed = false;
        if ui
            .add_enabled(
                *value > low,
                egui::Button::new("−").min_size(vec2(24.0, 0.0)),
            )
            .on_hover_text(hover)
            .clicked()
        {
            *value -= 1;
            changed = true;
        }
        // One width for every stepper, so the − and + buttons line up.
        let widest = ui
            .painter()
            .layout_no_wrap(
                STEPPER_WIDEST.to_owned(),
                egui::TextStyle::Body.resolve(ui.style()),
                INK,
            )
            .size()
            .x;
        let field = vec2(
            (widest + 2.0 * ui.spacing().button_padding.x).ceil(),
            ui.spacing().interact_size.y,
        );
        changed |= ui
            .add_sized(
                field,
                egui::DragValue::new(value).range(low..=high).suffix(suffix),
            )
            .on_hover_text(format!("{hover} · drag or type a value"))
            .changed();
        if ui
            .add_enabled(
                *value < high,
                egui::Button::new("+").min_size(vec2(24.0, 0.0)),
            )
            .on_hover_text(hover)
            .clicked()
        {
            *value += 1;
            changed = true;
        }
        changed
    })
    .inner
}

/// Framed choices joined into one control `width` wide, split evenly so
/// stacked controls line up: the selected one on SELECTION, the rest on
/// SURFACE. Each option is (value, label, enabled, why it's
/// disabled). With `hovers`, each option explains itself. Returns the
/// clicked value, if any.
fn segmented<T: Copy + PartialEq>(
    ui: &mut Ui,
    width: f32,
    current: T,
    options: &[(T, &str, bool, &str)],
    hovers: Option<&[&str]>,
) -> Option<T> {
    let mut clicked = None;
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        let last = options.len() - 1;
        // Even shares of `width`; the last segment takes the rounding.
        let share = (width / options.len() as f32).floor();
        let height = ui.spacing().interact_size.y;
        for (i, &(value, label, enabled, disabled_hover)) in options.iter().enumerate() {
            let selected = value == current;
            let radius = egui::CornerRadius {
                nw: if i == 0 { 3 } else { 0 },
                sw: if i == 0 { 3 } else { 0 },
                ne: if i == last { 3 } else { 0 },
                se: if i == last { 3 } else { 0 },
            };
            let mut button = egui::Button::new(label)
                .selected(selected)
                .corner_radius(radius);
            if selected {
                button = button.stroke(Stroke::new(1.0_f32, ACCENT));
            }
            let segment = if i == last {
                width - share * last as f32
            } else {
                share
            };
            let mut response = ui
                .add_enabled_ui(enabled, |ui| ui.add_sized(vec2(segment, height), button))
                .inner;
            if let Some(text) = hovers.and_then(|h| h.get(i)) {
                response = response.on_hover_text(*text);
            }
            if !disabled_hover.is_empty() {
                response = response.on_disabled_hover_text(disabled_hover);
            }
            if response.clicked() {
                clicked = Some(value);
            }
        }
    });
    clicked
}

/// The player's padlock: an 8 × 7 body at `at` with its shackle above.
fn padlock(painter: &egui::Painter, at: Pos2, color: Color32) {
    painter.rect_filled(Rect::from_min_size(at, vec2(8.0, 7.0)), 1.0, color);
    painter.rect_stroke(
        Rect::from_min_size(at + vec2(1.0, -5.0), vec2(6.0, 7.0)),
        2.0,
        Stroke::new(1.5_f32, color),
        StrokeKind::Inside,
    );
}

// Patterns reinforce break identity when colors are hard to distinguish.
fn source_pattern(painter: &egui::Painter, rect: Rect, source: usize, color: Color32) {
    let p = painter.with_clip_rect(rect);
    match source {
        1 => {
            let mut x = rect.left() - rect.height();
            while x < rect.right() {
                p.line_segment(
                    [
                        Pos2::new(x, rect.bottom()),
                        Pos2::new(x + rect.height(), rect.top()),
                    ],
                    Stroke::new(2.0_f32, color),
                );
                x += 10.0;
            }
        }
        2 => {
            let mut y = rect.top() + 5.0;
            while y < rect.bottom() {
                let mut x = rect.left() + 5.0;
                while x < rect.right() {
                    p.circle_filled(Pos2::new(x, y), 1.0, color);
                    x += 9.0;
                }
                y += 9.0;
            }
        }
        3 => {
            let mut y = rect.top() + 5.0;
            while y < rect.bottom() {
                p.line_segment(
                    [Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)],
                    Stroke::new(1.0_f32, color),
                );
                y += 7.0;
            }
        }
        _ => {}
    }
}

// Preview follows the engine's time mapping; peaks are an envelope, not a DSP render.
fn preview_frame(
    item: StepPlacement,
    slice: crate::edit_state::SliceMarker,
    fraction: f64,
    slots: u8,
    tempo: f64,
    rate: u32,
) -> Option<f64> {
    if slice.end <= slice.start || slots == 0 || item.repeats == 0 || tempo <= 0.0 {
        return None;
    }
    let phase = (fraction * item.repeats as f64).fract();
    let offset = if item.mode == 1 {
        phase * f64::from(slice.end - slice.start)
    } else {
        phase * 16.0 * f64::from(item.span_slots) / f64::from(slots) / f64::from(item.repeats)
            * f64::from(rate)
            * 60.0
            / tempo
    };
    let frame = f64::from(slice.start) + offset;
    (frame < f64::from(slice.end)).then_some(frame)
}

#[cfg(test)]
#[path = "ui_tests.rs"]
mod tests;

#[path = "ui_slots.rs"]
mod ui_slots;
use ui_slots::{PendingLoad, SlotAudio};
