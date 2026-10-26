use crate::edit_state::{EditState, StepPlacement};
use crate::sample::StereoSample;
use crate::suggest::suggest_bar;
use crate::{JunglizerParams, JunglizerParamsParamId as P, SOURCE_FILES};
use egui::{Color32, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, vec2};
use std::path::Path;
use std::sync::atomic::Ordering;
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

    fn place(&mut self, slot: u8) {
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
            self.message = "Slice placed. Change its length and sound below.";
        } else {
            self.message = "That space contains a locked slice. Unlock it to replace it.";
        }
    }

    fn source_waveform(&mut self, ui: &mut Ui) {
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
            if response.drag_started()
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
            }
            if response.dragged()
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
                    }
                }
            }
            if response.drag_stopped() {
                self.dragging_marker = None;
            }
        }
    }

    fn player(&mut self, ui: &mut Ui, context: &PluginContext<JunglizerParams>) {
        let width = ui.available_width();
        let (rect, response) = ui.allocate_exact_size(vec2(width, 150.0), Sense::click());
        ui.painter()
            .rect_filled(rect, 5.0, Color32::from_rgb(25, 35, 43));
        let bar_sources = [
            context.params().bar_1.value(),
            context.params().bar_2.value(),
            context.params().bar_3.value(),
            context.params().bar_4.value(),
        ];
        let slots = self.state.get().player_slots as usize;
        for bar in 0..4 {
            let bar_rect = Rect::from_min_max(
                Pos2::new(rect.left() + rect.width() * bar as f32 / 4.0, rect.top()),
                Pos2::new(
                    rect.left() + rect.width() * (bar + 1) as f32 / 4.0,
                    rect.bottom(),
                ),
            );
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
                MUTED,
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
        for placement in self
            .state
            .get()
            .placements
            .iter()
            .filter(|item| item.active)
        {
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
        if let Some(pointer) = ui.ctx().pointer_latest_pos()
            && rect.contains(pointer)
        {
            let slot = (((pointer.x - rect.left()) / rect.width()) * slots as f32)
                .floor()
                .clamp(0.0, slots as f32 - 1.0) as u8;
            if response.clicked() {
                if self
                    .state
                    .get()
                    .placement_at(self.state.get().slot_start(slot) + 0.001)
                    .is_some()
                {
                    self.selected_slot = Some(slot);
                } else {
                    self.place(slot);
                }
            }
            if self.dragging_slice.is_some() && ui.input(|input| input.pointer.any_released()) {
                self.place(slot);
            }
        }
        if ui.input(|input| input.pointer.any_released()) {
            self.dragging_slice = None;
        }
    }

    fn inspector(&mut self, ui: &mut Ui) {
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
            }
        });
        ui.horizontal(|ui| {
            let mut bit_depth = current.bit_depth;
            if ui
                .add(egui::Slider::new(&mut bit_depth, 1..=16).text("Bit depth (16 = clean)"))
                .changed()
            {
                self.edit_placement(current, |item| item.bit_depth = bit_depth);
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
            }
            let mut locked = current.locked;
            if ui.checkbox(&mut locked, "Lock").changed() {
                self.edit_placement(current, |item| item.locked = locked);
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
        ui.visuals_mut().dark_mode = true;
        ui.spacing_mut().item_spacing = vec2(9.0, 8.0);
        ui.heading(egui::RichText::new("JUNGLIZER").color(INK).size(24.0));
        ui.label(
            egui::RichText::new("Four bars of break. Choose the source, then remake the rhythm.")
                .color(MUTED),
        );
        ui.separator();
        ui.horizontal(|ui| {
            for (index, id) in [P::Bar1, P::Bar2, P::Bar3, P::Bar4].iter().enumerate() {
                param_dropdown(ui, context, *id, &format!("Bar {} source", index + 1), 2);
            }
            param_knob(ui, context, P::Gain, "Output");
        });
        ui.separator();
        ui.horizontal(|ui| {
            ui.strong("SOURCE SLICES");
            let mut count = self.state.get().source_count;
            if ui
                .add(egui::Slider::new(&mut count, 1..=32).text("Slices per loop"))
                .changed()
            {
                self.ensure_markers(count);
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
                }
            }
        });
        self.source_waveform(ui);
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
                    context
                        .params()
                        .audition
                        .store((self.selected_loop * 32 + i + 1) as u32, Ordering::Relaxed);
                }
                if response.drag_started() {
                    self.selected_slice = i;
                    self.dragging_slice = Some((self.selected_loop, i));
                }
                if response.secondary_clicked() {
                    let loop_index = self.selected_loop;
                    self.state.update(|state| {
                        if let Some(set) = state.marker_set_mut(loop_index) {
                            set.slices[i].active = !set.slices[i].active;
                            state.touch();
                        }
                    });
                }
                response.on_hover_text(if active {
                    "Click to select, drag into player, or right-click to mute"
                } else {
                    "Quiet slice. Right-click to activate"
                });
            }
        });
        ui.separator();
        ui.horizontal(|ui| {
            ui.strong("PLAYER");
            let mut slots = self.state.get().player_slots;
            if ui
                .add(egui::Slider::new(&mut slots, 1..=32).text("Even slots across four bars"))
                .changed()
            {
                self.state.update(|state| {
                    state.set_player_slots(slots);
                });
            }
            ui.label(
                egui::RichText::new("The original break plays in every empty space.").color(MUTED),
            );
        });
        self.player(ui, context);
        self.inspector(ui);
        ui.horizontal(|ui| {
            ui.strong("SUGGEST A FILL");
            for bar in 0..4 {
                if ui
                    .selectable_label(self.selected_bar == bar, format!("Bar {}", bar + 1))
                    .clicked()
                {
                    self.selected_bar = bar;
                }
            }
            if ui.button("Suggest / try another").clicked() {
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
            if ui.button("Lock this bar").clicked() {
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
        ui.separator();
        ui.label(egui::RichText::new(self.message).color(ACCENT));
    }
}
