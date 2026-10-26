use crate::edit_state::{EditState, StepPlacement};
use crate::live::CapturedPhrase;
use crate::midi::export_to_music;
use crate::sample::StereoSample;
use crate::suggest::suggest_bar;
use crate::{JunglizerParams, JunglizerParamsParamId as P, SOURCE_FILES};
use egui::{Color32, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, vec2};
use std::path::Path;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use truce::params::FloatParamReadF32;
use truce::prelude::{PluginContext, StateBinding};
use truce_egui::EditorUi;
use truce_egui::widgets::{param_dropdown, param_knob};

const INK: Color32 = Color32::from_rgb(219, 231, 238);
const MUTED: Color32 = Color32::from_rgb(121, 143, 157);
const ACCENT: Color32 = Color32::from_rgb(245, 174, 75);
const SOURCE_COLORS: [Color32; 4] = [
    Color32::from_rgb(238, 151, 87),
    Color32::from_rgb(105, 205, 193),
    Color32::from_rgb(151, 173, 241),
    Color32::from_rgb(222, 139, 190),
];

pub struct JunglizerUi {
    state: StateBinding<EditState>,
    sources: [StereoSample; 4],
    waveform: [[f32; 512]; 4],
    selected_loop: usize,
    selected_slice: usize,
    selected_slot: Option<u8>,
    selected_bar: usize,
    dragging_slice: Option<(usize, usize)>,
    dragging_marker: Option<usize>,
    message: &'static str,
    export_path: Option<PathBuf>,
}

impl JunglizerUi {
    pub fn new() -> Self {
        let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("sample");
        let sources: [StereoSample; 4] = std::array::from_fn(|i| {
            StereoSample::load(&directory.join(SOURCE_FILES[i])).unwrap_or_default()
        });
        let waveform = std::array::from_fn(|i| {
            std::array::from_fn(|bin| {
                let len = sources[i].frames.len();
                let start = bin * len / 512;
                let end = ((bin + 1) * len / 512).max(start + 1).min(len);
                sources[i].frames[start..end]
                    .iter()
                    .map(|frame| frame[0].abs().max(frame[1].abs()))
                    .fold(0.0_f32, f32::max)
            })
        });
        Self {
            state: StateBinding::default(),
            sources,
            waveform,
            selected_loop: 0,
            selected_slice: 0,
            selected_slot: None,
            selected_bar: 3,
            dragging_slice: None,
            dragging_marker: None,
            message: "Choose a source slice, then click a player slot. You can drag it there too.",
            export_path: None,
        }
    }

    fn ensure_markers(&mut self, count: u8) {
        let suggestions: [_; 4] = std::array::from_fn(|i| self.sources[i].suggested_slices(count));
        self.state.update(|state| {
            state.source_count = count;
            state.touch();
            for (i, suggested) in suggestions.iter().enumerate() {
                if !state.markers[i][count as usize - 1].valid {
                    state.set_markers(i, &suggested[..count as usize]);
                }
            }
            for placement in &mut state.placements {
                if placement.active && placement.source_slice >= count {
                    placement.active = false;
                }
            }
        });
        self.selected_slice = self.selected_slice.min(count as usize - 1);
    }

    fn place(&mut self, slot: u8, context: &PluginContext<JunglizerParams>) {
        if self.state.get().frozen || self.state.get().live_enabled {
            self.message = "Unfreeze or turn off live variations before editing the player.";
            return;
        }
        let (source_loop, source_slice) = self
            .dragging_slice
            .unwrap_or((self.selected_loop, self.selected_slice));
        let count = self.state.get().player_slots;
        if slot >= count || self.state.get().slice(source_loop, source_slice).is_none() {
            self.message = "This source slice is quiet or unavailable. Choose another slice.";
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
            self.message = "Slice placed. Change its length and sound below.";
        } else {
            self.message = "That space contains a locked slice. Unlock it to replace it.";
        }
    }

    fn source_waveform(&mut self, ui: &mut Ui, context: &PluginContext<JunglizerParams>) {
        let width = ui.available_width();
        let (rect, response) = ui.allocate_exact_size(vec2(width, 112.0), Sense::click_and_drag());
        ui.painter()
            .rect_filled(rect, 5.0, Color32::from_rgb(25, 35, 43));
        let middle = rect.center().y;
        for (bin, amplitude) in self.waveform[self.selected_loop].iter().enumerate() {
            let x = rect.left() + rect.width() * bin as f32 / 512.0;
            let height = amplitude.sqrt().min(1.0) * 47.0;
            ui.painter().line_segment(
                [Pos2::new(x, middle - height), Pos2::new(x, middle + height)],
                Stroke::new(1.0_f32, SOURCE_COLORS[self.selected_loop]),
            );
        }
        for quarter in 1..4 {
            let x = rect.left() + rect.width() * quarter as f32 / 4.0;
            ui.painter().line_segment(
                [Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())],
                Stroke::new(1.0_f32, Color32::from_gray(64)),
            );
        }
        let set = self.state.get().marker_set(self.selected_loop).copied();
        let frame_count = self.sources[self.selected_loop].frames.len();
        if let Some(set) = set.filter(|set| set.valid && frame_count > 0) {
            for i in 0..self.state.get().source_count as usize {
                let marker = set.slices[i];
                let x = rect.left() + rect.width() * marker.start as f32 / frame_count as f32;
                let color = if marker.active { ACCENT } else { MUTED };
                ui.painter().line_segment(
                    [Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())],
                    Stroke::new(2.0_f32, color),
                );
                ui.painter().text(
                    Pos2::new(x + 3.0, rect.top() + 3.0),
                    egui::Align2::LEFT_TOP,
                    (i + 1).to_string(),
                    egui::FontId::proportional(11.0),
                    color,
                );
            }
            if !self.state.get().frozen
                && response.drag_started()
                && let Some(pointer) = response.interact_pointer_pos()
            {
                self.dragging_marker = (1..self.state.get().source_count as usize)
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
                && response.dragged()
                && let (Some(i), Some(pointer)) =
                    (self.dragging_marker, response.interact_pointer_pos())
            {
                let min = set.slices[i - 1].start.saturating_add(1);
                let max = if i + 1 < self.state.get().source_count as usize {
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
    }

    fn player(
        &mut self,
        ui: &mut Ui,
        context: &PluginContext<JunglizerParams>,
        live: Option<&CapturedPhrase>,
    ) {
        let width = ui.available_width();
        let (rect, response) = ui.allocate_exact_size(vec2(width, 150.0), Sense::click());
        ui.painter()
            .rect_filled(rect, 5.0, Color32::from_rgb(25, 35, 43));
        let selected_bars = [
            context.params().bar_1.value(),
            context.params().bar_2.value(),
            context.params().bar_3.value(),
            context.params().bar_4.value(),
        ];
        let bar_sources = if let Some(phrase) = live {
            phrase.bars.map(|bar| i64::from(bar) + 1)
        } else if self.state.get().frozen {
            self.state.get().frozen_bars.map(|bar| i64::from(bar) + 1)
        } else {
            selected_bars
        };
        let slots =
            live.map_or(self.state.get().player_slots, |phrase| phrase.player_slots) as usize;
        let placements = live.map_or(&self.state.get().placements, |phrase| &phrase.placements);
        let play_beat = context
            .transport()
            .filter(|transport| transport.playing)
            .map(|transport| transport.position_beats.rem_euclid(16.0));
        for bar in 0..4 {
            let bar_rect = Rect::from_min_max(
                Pos2::new(rect.left() + rect.width() * bar as f32 / 4.0, rect.top()),
                Pos2::new(
                    rect.left() + rect.width() * (bar + 1) as f32 / 4.0,
                    rect.bottom(),
                ),
            );
            if play_beat.is_some_and(|beat| (beat / 4.0).floor() as usize == bar) {
                ui.painter().rect_filled(
                    bar_rect,
                    0.0,
                    Color32::from_rgba_unmultiplied(245, 174, 75, 25),
                );
            }
            ui.painter().rect_stroke(
                bar_rect,
                0.0,
                Stroke::new(1.0_f32, Color32::from_gray(94)),
                StrokeKind::Inside,
            );
            ui.painter().text(
                bar_rect.left_top() + vec2(7.0, 5.0),
                egui::Align2::LEFT_TOP,
                format!("BAR {}", bar + 1),
                egui::FontId::proportional(11.0),
                if play_beat.is_some_and(|beat| (beat / 4.0).floor() as usize == bar) {
                    ACCENT
                } else {
                    MUTED
                },
            );
            let source = (bar_sources[bar] - 1).clamp(0, 3) as usize;
            for (bin, amplitude) in self.waveform[source].iter().enumerate() {
                let x = bar_rect.left() + bar_rect.width() * bin as f32 / 512.0;
                let height = amplitude.sqrt().min(1.0) * 50.0;
                ui.painter().line_segment(
                    [
                        Pos2::new(x, bar_rect.center().y - height),
                        Pos2::new(x, bar_rect.center().y + height),
                    ],
                    Stroke::new(1.0_f32, SOURCE_COLORS[source].gamma_multiply(0.35)),
                );
            }
        }
        for slot in 1..slots {
            let x = rect.left() + rect.width() * slot as f32 / slots as f32;
            ui.painter().line_segment(
                [Pos2::new(x, rect.top() + 25.0), Pos2::new(x, rect.bottom())],
                Stroke::new(1.0_f32, Color32::from_gray(48)),
            );
        }
        for placement in placements.iter().filter(|item| item.active) {
            let left = rect.left() + rect.width() * placement.start_slot as f32 / slots as f32;
            let right = rect.left()
                + rect.width() * (placement.start_slot + placement.span_slots) as f32
                    / slots as f32;
            let block = Rect::from_min_max(
                Pos2::new(left + 2.0, rect.top() + 33.0),
                Pos2::new(right - 2.0, rect.bottom() - 8.0),
            );
            let color = SOURCE_COLORS[placement.source_loop as usize];
            ui.painter()
                .rect_filled(block, 3.0, color.gamma_multiply(0.35));
            ui.painter()
                .rect_stroke(block, 3.0, Stroke::new(1.0_f32, color), StrokeKind::Inside);
            ui.painter().text(
                block.left_top() + vec2(5.0, 6.0),
                egui::Align2::LEFT_TOP,
                format!(
                    "{}:{}{}",
                    placement.source_loop + 1,
                    placement.source_slice + 1,
                    if placement.locked { "  ◆" } else { "" }
                ),
                egui::FontId::proportional(12.0),
                INK,
            );
        }
        if let Some(beat) = play_beat {
            let x = rect.left() + rect.width() * (beat / 16.0) as f32;
            ui.painter().line_segment(
                [Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())],
                Stroke::new(2.0_f32, ACCENT),
            );
            ui.painter()
                .circle_filled(Pos2::new(x, rect.top() + 3.0), 4.0, ACCENT);
            ui.ctx().request_repaint();
        }
        if let Some(pointer) = ui.ctx().pointer_latest_pos()
            && rect.contains(pointer)
        {
            let slot = (((pointer.x - rect.left()) / rect.width()) * slots as f32)
                .floor()
                .clamp(0.0, slots as f32 - 1.0) as u8;
            if response.clicked() {
                if placements.iter().any(|item| {
                    item.active
                        && item.start_slot <= slot
                        && slot < item.start_slot + item.span_slots
                }) {
                    self.selected_slot = Some(slot);
                    context.automate(P::EditSlot, f64::from(slot) / 31.0);
                } else {
                    self.place(slot, context);
                }
            }
            if self.dragging_slice.is_some() && ui.input(|input| input.pointer.any_released()) {
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
            ui.label(format!(
                "PLACED SLICE  {}:{}",
                current.source_loop + 1,
                current.source_slice + 1
            ));
            ui.separator();
            let mut span = current.span_slots;
            if ui
                .add(
                    egui::Slider::new(
                        &mut span,
                        1..=self.state.get().player_slots - current.start_slot,
                    )
                    .text("Length in slots"),
                )
                .changed()
            {
                self.edit_placement(current, |item| item.span_slots = span);
                context.automate(P::EditLength, f64::from(span - 1) / 31.0);
            }
            let mut mode = current.mode;
            egui::ComboBox::from_id_salt("placement-mode")
                .selected_text(if mode == 0 {
                    "Original speed"
                } else {
                    "Fit to length / pitch shifts"
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut mode, 0, "Original speed");
                    ui.selectable_value(&mut mode, 1, "Fit to length / pitch shifts");
                });
            if mode != current.mode {
                self.edit_placement(current, |item| item.mode = mode);
                context.automate(P::EditSpeed, f64::from(mode));
            }
        });
        ui.horizontal(|ui| {
            let mut bit_depth = current.bit_depth;
            if ui
                .add(egui::Slider::new(&mut bit_depth, 1..=16).text("Bit depth (16 = clean)"))
                .changed()
            {
                self.edit_placement(current, |item| item.bit_depth = bit_depth);
                context.automate(P::EditBits, f64::from(bit_depth - 1) / 15.0);
            }
            let mut repeats = current.repeats;
            egui::ComboBox::from_id_salt("placement-repeats")
                .selected_text(format!(
                    "{} trigger{}",
                    repeats,
                    if repeats == 1 { "" } else { "s" }
                ))
                .show_ui(ui, |ui| {
                    for value in [1, 2, 4, 8] {
                        ui.selectable_value(&mut repeats, value, value.to_string());
                    }
                });
            if repeats != current.repeats {
                self.edit_placement(current, |item| item.repeats = repeats);
                context.automate(P::EditRetriggers, f64::from(repeats.trailing_zeros()) / 3.0);
            }
            let mut locked = current.locked;
            if ui.checkbox(&mut locked, "Lock").changed() {
                self.edit_placement(current, |item| item.locked = locked);
                context.automate(P::EditLock, f64::from(locked as u8));
            }
            if ui.button("Remove").clicked() {
                self.state.update(|state| {
                    state.remove_at_slot(slot);
                });
                self.selected_slot = None;
            }
        });
    }

    fn edit_placement(&mut self, old: StepPlacement, change: impl FnOnce(&mut StepPlacement)) {
        let mut updated = old;
        change(&mut updated);
        let mut replaced = false;
        self.state
            .update(|state| replaced = state.replace(old, updated));
        if !replaced {
            self.message =
                "That length reaches another slice. Move or remove the neighboring slice first.";
        }
    }

    fn freeze_live(&mut self, context: &PluginContext<JunglizerParams>) -> bool {
        let Some(captured) = context.params().live_snapshot.capture() else {
            self.message = "Start Logic playback to hear a live variation before freezing it.";
            return false;
        };
        self.state.update(|state| {
            state.placements = captured.placements;
            for item in &mut state.placements {
                if item.active {
                    item.locked = true;
                }
            }
            state.source_count = captured.source_count;
            state.player_slots = captured.player_slots;
            state.frozen_bars = captured.bars;
            state.live_enabled = false;
            state.frozen = true;
            state.touch();
        });
        context.automate(P::LiveVariations, 0.0);
        self.message =
            "Frozen: this exact four-bar phrase will repeat. Export it as MIDI when ready.";
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
                let _ = std::process::Command::new("open")
                    .arg("-R")
                    .arg(&path)
                    .spawn();
                self.export_path = Some(path);
                self.message = "MIDI is in Finder. Drag it onto Junglizer's Logic track, then switch Playback to MIDI.";
            }
            Err(_) => {
                self.message = "Could not save the MIDI file in Music/Junglizer/Exports.";
            }
        }
    }
}

impl EditorUi<JunglizerParams> for JunglizerUi {
    fn opened(&mut self, context: &PluginContext<JunglizerParams>) {
        self.state = StateBinding::new(context);
        let count = self.state.get().source_count;
        if (0..4).any(|index| {
            self.state
                .get()
                .marker_set(index)
                .is_some_and(|set| !set.valid)
        }) {
            self.ensure_markers(count);
        }
    }

    fn state_changed(&mut self, _context: &PluginContext<JunglizerParams>) {
        self.state.sync();
    }

    fn ui(&mut self, ui: &mut Ui, context: &PluginContext<JunglizerParams>) {
        // Host automation changes the DSP state outside the editor. Keep the
        // controls and waveform selection in step with that saved state.
        self.state.sync();
        self.selected_loop = (context.params().edit_break.value() as usize - 1).min(3);
        self.selected_slice = (context.params().edit_slice.value() as usize - 1)
            .min(self.state.get().source_count as usize - 1);
        self.selected_slot = Some(
            (context.params().edit_slot.value() as u8 - 1).min(self.state.get().player_slots - 1),
        );
        self.selected_bar = (context.params().fill_bar.value() as usize - 1).min(3);
        let live = if self.state.get().live_enabled {
            context.params().live_snapshot.capture()
        } else {
            None
        };
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.visuals_mut().dark_mode = true;
            ui.spacing_mut().item_spacing = vec2(9.0, 8.0);
            ui.heading(egui::RichText::new("JUNGLIZER").color(INK).size(24.0));
            ui.label(
                egui::RichText::new(
                    "Four bars of break. Choose the source, then remake the rhythm.",
                )
                .color(MUTED),
            );
            ui.separator();
            ui.horizontal(|ui| {
                ui.add_enabled_ui(!self.state.get().frozen, |ui| {
                    for (index, id) in [P::Bar1, P::Bar2, P::Bar3, P::Bar4].iter().enumerate() {
                        param_dropdown(ui, context, *id, &format!("Bar {} source", index + 1), 2);
                    }
                });
                param_knob(ui, context, P::Gain, "Output");
            });
            ui.horizontal(|ui| {
                ui.strong("THIN  ↔  THICC");
                let mut amount = context.params().thin_thicc.value();
                let response = ui.add_sized(
                    [360.0, 22.0],
                    egui::Slider::new(&mut amount, -1.0..=1.0).show_value(false),
                );
                if response.drag_started() {
                    context.begin_edit(P::ThinThicc);
                }
                if response.changed() {
                    context.set_param(P::ThinThicc, f64::from((amount + 1.0) * 0.5));
                }
                if response.drag_stopped() {
                    context.end_edit(P::ThinThicc);
                }
                if ui.button("Center / dry").clicked() {
                    context.automate(P::ThinThicc, 0.5);
                }
                ui.separator();
                ui.label("808 key");
                let root = context.params().root.value() as usize;
                const KEYS: [&str; 12] = [
                    "C", "C♯", "D", "D♯", "E", "F", "F♯", "G", "G♯", "A", "A♯", "B",
                ];
                egui::ComboBox::from_id_salt("root-key")
                    .selected_text(KEYS[root.min(11)])
                    .show_ui(ui, |ui| {
                        for (index, name) in KEYS.iter().enumerate() {
                            if ui.selectable_label(root == index, *name).clicked() {
                                context.automate(P::Root, index as f64 / 11.0);
                            }
                        }
                    });
                ui.label(
                    egui::RichText::new("Thicc adds a tuned 808; far right adds its lower octave.")
                        .color(MUTED),
                );
            });
            ui.separator();
            ui.horizontal(|ui| {
                ui.strong("SOURCE SLICES");
                let mut count = self.state.get().source_count;
                if ui
                    .add_enabled(
                        !self.state.get().frozen,
                        egui::Slider::new(&mut count, 1..=32).text("Slices per loop"),
                    )
                    .changed()
                {
                    self.ensure_markers(count);
                    context.automate(P::SourceSlices, f64::from(count - 1) / 31.0);
                }
                ui.label(
                    egui::RichText::new("Drag the gold lines to start each slice on a sound.")
                        .color(MUTED),
                );
            });
            ui.horizontal(|ui| {
                for i in 0..4 {
                    if ui
                        .selectable_label(self.selected_loop == i, format!("BREAK {}", i + 1))
                        .clicked()
                    {
                        self.selected_loop = i;
                        self.selected_slice = 0;
                        context.automate(P::EditBreak, i as f64 / 3.0);
                        context.automate(P::EditSlice, 0.0);
                    }
                }
            });
            self.source_waveform(ui, context);
            ui.horizontal_wrapped(|ui| {
                let count = self.state.get().source_count as usize;
                for i in 0..count {
                    let active = self.state.get().slice(self.selected_loop, i).is_some();
                    let label = if active {
                        format!("{:02}", i + 1)
                    } else {
                        format!("{:02} ·", i + 1)
                    };
                    let response = ui.add(
                        egui::Button::new(label)
                            .selected(self.selected_slice == i)
                            .sense(Sense::click_and_drag()),
                    );
                    if response.clicked() {
                        self.selected_slice = i;
                        context.automate(P::EditSlice, i as f64 / 31.0);
                        context
                            .params()
                            .audition
                            .store((self.selected_loop * 32 + i + 1) as u32, Ordering::Relaxed);
                    }
                    if response.drag_started() {
                        self.selected_slice = i;
                        context.automate(P::EditSlice, i as f64 / 31.0);
                        self.dragging_slice = Some((self.selected_loop, i));
                    }
                    if response.secondary_clicked() && !self.state.get().frozen {
                        self.selected_slice = i;
                        context.automate(P::EditSlice, i as f64 / 31.0);
                        let loop_index = self.selected_loop;
                        let muted = active;
                        self.state.update(|state| {
                            if let Some(set) = state.marker_set_mut(loop_index) {
                                set.slices[i].active = !set.slices[i].active;
                                state.touch();
                            }
                        });
                        context.automate(P::SliceMuted, f64::from(muted as u8));
                    }
                    response.on_hover_text(if active {
                        "Click to select, drag into player, or right-click to mute"
                    } else {
                        "Quiet slice. Right-click to activate"
                    });
                }
            });
            ui.horizontal(|ui| {
                let selected = self.selected_slice;
                let source = self.selected_loop;
                let mut bass = self
                    .state
                    .get()
                    .marker_set(source)
                    .is_some_and(|set| set.valid && set.slices[selected].bass);
                if ui
                    .add_enabled(
                        !self.state.get().frozen,
                        egui::Checkbox::new(&mut bass, "Bass hit → tuned 808"),
                    )
                    .changed()
                {
                    self.state.update(|state| {
                        if let Some(set) = state.marker_set_mut(source) {
                            set.slices[selected].bass = bass;
                            state.touch();
                        }
                    });
                    context.automate(P::BassHit, f64::from(bass as u8));
                }
                ui.label(
                    egui::RichText::new("Select a source slice to mark its kick.").color(MUTED),
                );
            });
            ui.separator();
            ui.horizontal(|ui| {
                ui.strong("PLAYER");
                let mut slots = self.state.get().player_slots;
                if ui
                    .add_enabled(
                        !self.state.get().frozen && !self.state.get().live_enabled,
                        egui::Slider::new(&mut slots, 1..=32).text("Even slots across four bars"),
                    )
                    .changed()
                {
                    self.state.update(|state| {
                        state.set_player_slots(slots);
                    });
                    context.automate(P::PlayerSlots, f64::from(slots - 1) / 31.0);
                }
                ui.label(
                    egui::RichText::new("The original break plays in every empty space.")
                        .color(MUTED),
                );
            });
            self.player(ui, context, live.as_ref());
            if !self.state.get().frozen && !self.state.get().live_enabled {
                self.inspector(ui, context);
            }
            ui.horizontal(|ui| {
                ui.strong("SUGGEST A FILL");
                for bar in 0..4 {
                    if ui
                        .selectable_label(self.selected_bar == bar, format!("Bar {}", bar + 1))
                        .clicked()
                    {
                        self.selected_bar = bar;
                        context.automate(P::FillBar, bar as f64 / 3.0);
                    }
                }
                if ui
                    .add_enabled(
                        !self.state.get().live_enabled && !self.state.get().frozen,
                        egui::Button::new("Suggest / try another"),
                    )
                    .clicked()
                {
                    let bar = self.selected_bar;
                    let source = self.selected_loop;
                    let mut count = 0;
                    self.state
                        .update(|state| count = suggest_bar(state, bar, source));
                    self.message = if count > 0 {
                        "New fill ready. Lock any slices you want to keep, then try another."
                    } else {
                        "No source slices are active for this break, or the bar is fully locked."
                    };
                }
                if ui
                    .add_enabled(
                        !self.state.get().live_enabled && !self.state.get().frozen,
                        egui::Button::new("Lock this bar"),
                    )
                    .clicked()
                {
                    let bar = self.selected_bar;
                    self.state.update(|state| {
                        let start = (bar * state.player_slots as usize + 3) / 4;
                        let end = ((bar + 1) * state.player_slots as usize + 3) / 4;
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
                    self.message =
                        "The bar's placed slices are locked. Future suggestions will keep them.";
                }
            });
            ui.horizontal(|ui| {
                ui.strong("LIVE VARIATIONS");
                let live_enabled = self.state.get().live_enabled;
                if ui
                    .button(if live_enabled {
                        "Stop evolving"
                    } else {
                        "Evolve every four bars"
                    })
                    .clicked()
                {
                    self.state.update(|state| {
                        state.live_enabled = !state.live_enabled;
                        if state.live_enabled {
                            state.frozen = false;
                        }
                        state.touch();
                    });
                    context.automate(P::LiveVariations, f64::from((!live_enabled) as u8));
                    self.message = if live_enabled {
                        "Live variations stopped; the hand-edited pattern is back."
                    } else {
                        "Live variations are on. Each four-bar pass brings a new fill."
                    };
                }
                if ui
                    .add_enabled(live_enabled, egui::Button::new("Freeze this version"))
                    .clicked()
                {
                    self.freeze_live(context);
                }
                if ui
                    .add_enabled(
                        self.state.get().frozen,
                        egui::Button::new("Unfreeze for editing"),
                    )
                    .clicked()
                {
                    self.state.update(|state| {
                        state.frozen = false;
                        state.touch();
                    });
                    self.message = "Editing is unlocked. Existing slices stay in place.";
                }
                if let Some(phrase) = live {
                    ui.label(
                        egui::RichText::new(format!("Playing variation {}", phrase.cycle + 1))
                            .color(ACCENT),
                    );
                } else if self.state.get().frozen {
                    ui.label(egui::RichText::new("Frozen phrase").color(ACCENT));
                }
            });
            ui.horizontal(|ui| {
                ui.strong("LOGIC MIDI");
                if ui.button("Export four-bar MIDI to Finder").clicked() {
                    self.export_midi(context);
                }
                ui.separator();
                ui.label("Playback:");
                let midi_mode = context.params().playback.value() != 0;
                if ui
                    .selectable_label(!midi_mode, "Internal pattern")
                    .clicked()
                {
                    context.automate(P::Playback, 0.0);
                }
                if ui.selectable_label(midi_mode, "MIDI region").clicked() {
                    context.automate(P::Playback, 1.0);
                }
                ui.label(
                    egui::RichText::new(
                        "Switch to MIDI region after dropping the file on this track.",
                    )
                    .color(MUTED),
                );
            });
            if let Some(path) = &self.export_path {
                ui.label(egui::RichText::new(path.display().to_string()).color(MUTED));
            }
            ui.label(
                egui::RichText::new(
                    "CS12: assign Junglizer's Logic automation parameters to your MIDI controls.",
                )
                .color(MUTED),
            );
            ui.separator();
            ui.label(egui::RichText::new(self.message).color(ACCENT));
        });
    }
}
