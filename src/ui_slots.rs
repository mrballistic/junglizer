//! The break slot controls: load, unload, relink and half/double.

use super::{ACCENT, EDGE, INK, JunglizerUi, MUTED, SOURCE_COLORS, WELL, apply_style};
use crate::edit_state::{EditState, MAX_PATH_BYTES, SlotError, SlotKind};
use crate::library::{Library, LoadOutcome, LoadedBreak, Purpose};
use crate::sample::StereoSample;
use crate::tempo::{self, Reading};
use crate::{JunglizerParams, JunglizerParamsParamId as P};
use egui::{Pos2, Rect, Sense, Stroke, Ui, vec2};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use truce::prelude::PluginContext;

/// Shown on every break control that's disabled while frozen or live.
pub(crate) const LOCKED_HINT: &str = "Unfreeze or stop live variations to change breaks.";
const UNREADABLE: &str = "Junglizer couldn’t read that file. Use a WAV, AIFF or MP3.";
const TARGET_PROMPT: &str = if DROP_SUPPORTED {
    "Drop your own break here (4 to 32 beats, 80–195 BPM)"
} else {
    "Choose your own break (4 to 32 beats, 80–195 BPM)"
};
/// Shown when nothing in a drop is a file type Junglizer reads.
const NOT_AUDIO: &str = "Junglizer can load WAV, AIFF or MP3 files.";
const FREE_PACKS: &str = "Need breaks? Free packs from KAN Samples";
const FREE_PACKS_URL: &str = "https://kansamples.com/blogs/free-drum-and-bass-sample-packs";
/// Asked before the empty state's amen link replaces missing breaks.
const AMEN_QUESTION: &str = "Replace all four breaks with the Junglizer amen? Slices and placements on missing breaks will be removed.";
/// The file types Junglizer decodes, for the dialog filter and drops.
const AUDIO_EXTENSIONS: &[&str] = &["wav", "aif", "aiff", "mp3"];
/// How many suggested markers a fresh load starts with.
const LOAD_SLICES: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LoadMode {
    Load,
    Relink,
    /// Loads into the slot and clears every other slot (the first-launch prompt).
    FirstRun,
}

/// A file the user chose that the loader hasn't finished yet. `seq` is 0
/// until the library accepts the request. `expect` is what the target slot
/// held when the load was asked for (for a dropped replace, when the user
/// confirmed it).
pub(crate) enum PendingLoad {
    None,
    Choosing {
        slot: usize,
        seq: u64,
        path: String,
        mode: LoadMode,
        expect: SlotSnapshot,
    },
}

/// What one slot held, to tell whether it changed while a file decoded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SlotSnapshot {
    kind: SlotKind,
    path_hash: u64,
    /// Whether any active placement plays from the slot.
    placed: bool,
}

impl SlotSnapshot {
    pub(crate) fn of(state: &EditState, slot: usize) -> Self {
        let entry = state.slots[slot];
        Self {
            kind: entry.kind,
            path_hash: entry.path_hash(),
            placed: state
                .placements
                .iter()
                .any(|p| p.active && usize::from(p.source_loop) == slot),
        }
    }

    /// Whether a load asked for when the slot held `self` would now replace
    /// something the user never agreed to lose: the slot holds a different
    /// break, or (except for a relink, which keeps them) gained placements.
    /// A slot that is now blank has nothing to lose.
    fn changed_for(self, now: Self, mode: LoadMode) -> bool {
        now.kind != SlotKind::Blank
            && (now.kind != self.kind
                || now.path_hash != self.path_hash
                || (mode != LoadMode::Relink && now.placed && !self.placed))
    }
}

/// Where a file drop can land. Each frame's layout registers the rects.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DropTarget {
    /// The first-launch prompt: a FirstRun load into Break 1.
    FirstRun,
    /// The empty state: Break 1.
    Empty,
    /// The source waveform: the selected slot.
    Waveform,
    /// A Break 1–4 button.
    Slot(usize),
}

/// What the editor knows about one slot's audio.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum SlotStatus {
    Builtin,
    Blank,
    Ready(Reading),
    Loading,
    Missing,
    Unreadable,
}

/// One slot's audio and waveform peaks, keyed by what the slot holds so it
/// is only recomputed when that changes.
pub(crate) struct SlotAudio {
    key: (SlotKind, u64),
    pub(crate) audio: Option<Arc<LoadedBreak>>,
    pub(crate) peaks: [f32; 512],
}

impl SlotAudio {
    pub(crate) fn new(key: (SlotKind, u64), audio: Option<Arc<LoadedBreak>>) -> Self {
        let peaks = audio.as_deref().map_or([0.0; 512], |a| peaks(&a.sample));
        Self { key, audio, peaks }
    }

    pub(crate) fn builtin(slot: usize) -> Self {
        Self::new(
            (SlotKind::Builtin, 0),
            Some(Library::builtin()[slot].clone()),
        )
    }

    /// The audio, if there is any to draw or measure.
    pub(crate) fn sample(&self) -> Option<&StereoSample> {
        self.audio
            .as_deref()
            .map(|a| &a.sample)
            .filter(|s| !s.frames.is_empty() && s.sample_rate > 0)
    }
}

/// The loudest sample in each of 512 equal bins.
fn peaks(sample: &StereoSample) -> [f32; 512] {
    let len = sample.frames.len();
    std::array::from_fn(|bin| {
        if len == 0 {
            return 0.0;
        }
        let start = bin * len / 512;
        let end = ((bin + 1) * len / 512).max(start + 1).min(len);
        sample.frames[start..end]
            .iter()
            .map(|frame| frame[0].abs().max(frame[1].abs()))
            .fold(0.0_f32, f32::max)
    })
}

/// The last component of `path`, for messages and tooltips.
pub(crate) fn file_name(path: &str) -> &str {
    std::path::Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(path)
}

/// Drag and drop needs baseview's drop support, which exists on macOS and
/// Windows only. Linux keeps click to choose.
pub(crate) const DROP_SUPPORTED: bool = cfg!(any(target_os = "macos", target_os = "windows"));

/// The first dropped file with an audio extension Junglizer reads (wav,
/// aif, aiff or mp3, any case).
pub(crate) fn accepted_drop(files: &[egui::DroppedFile]) -> Option<PathBuf> {
    files
        .iter()
        .filter_map(|f| f.path.as_ref())
        .find_map(|path| {
            let extension = path.extension()?.to_str()?.to_ascii_lowercase();
            AUDIO_EXTENSIONS
                .contains(&extension.as_str())
                .then(|| path.clone())
        })
}

/// Whether to show the first-launch prompt: only when this machine hasn't
/// answered it, and only on an instance nobody has edited, so **Choose…**
/// can never clear someone's work.
pub(crate) fn show_first_run(flag_exists: bool, state: &EditState) -> bool {
    !flag_exists && state.is_pristine()
}

/// Remembers, for this machine, that the first-launch prompt was answered.
/// Without a config folder there's nothing to write, and the prompt never
/// shows there anyway.
pub(crate) fn mark_first_run_done() {
    if let Some(flag) = crate::platform::first_run_flag() {
        let _ = write_flag(&flag);
    }
}

/// Creates `flag`'s folder, then `flag` as an empty file.
fn write_flag(flag: &Path) -> std::io::Result<()> {
    if let Some(folder) = flag.parent() {
        std::fs::create_dir_all(folder)?;
    }
    std::fs::File::create(flag).map(drop)
}

/// Commits a finished load into `state`, or explains why it can't. Every
/// failure leaves `state` exactly as it was.
pub(crate) fn apply_result(
    state: &mut EditState,
    slot: usize,
    path: &str,
    mode: LoadMode,
    outcome: &LoadOutcome,
) -> Result<Reading, &'static str> {
    let audio = match outcome {
        LoadOutcome::Loaded(audio) => audio,
        LoadOutcome::Refused(refusal) => return Err(refusal.message()),
        // A chosen file that vanished before it loaded reads the same.
        LoadOutcome::Unreadable | LoadOutcome::Missing => return Err(UNREADABLE),
    };
    let frames = u32::try_from(audio.sample.frames.len())
        .ok()
        .filter(|&frames| frames > 0)
        .ok_or(UNREADABLE)?;
    let markers = &audio.suggested[..LOAD_SLICES];
    if markers.iter().any(|m| m.start >= m.end || m.end > frames) {
        return Err(UNREADABLE);
    }
    let reading = audio.reading;
    let result = match mode {
        LoadMode::Load => state.load_user(slot, path, frames, reading.beats, markers),
        LoadMode::Relink => state.relink(slot, path, frames, reading.beats),
        LoadMode::FirstRun => state
            .load_user(slot, path, frames, reading.beats, markers)
            .map(|()| {
                for other in (0..state.slots.len()).filter(|&other| other != slot) {
                    state.unload(other);
                }
            }),
    };
    result.map_err(|e: SlotError| e.message())?;
    Ok(reading)
}

impl JunglizerUi {
    fn library(&self) -> Option<&Library> {
        self.params.as_deref().map(|p| &p.library)
    }

    pub(crate) fn breaks_editable(&self) -> bool {
        !self.state.get().frozen && !self.state.get().live_enabled
    }

    /// Picks up audio for any slot whose contents changed, or whose user
    /// file has finished loading since the last frame.
    pub(crate) fn refresh_slot_audio(&mut self) {
        for slot in 0..self.slot_audio.len() {
            let entry = self.state.get().slots[slot];
            let key = (entry.kind, entry.path_hash());
            let cached = &self.slot_audio[slot];
            if cached.key == key && (entry.kind != SlotKind::User || cached.audio.is_some()) {
                continue;
            }
            let audio = match entry.kind {
                SlotKind::Builtin => Some(Library::builtin()[slot].clone()),
                SlotKind::User => self.library().and_then(|l| l.audio_for(slot, key.1)),
                SlotKind::Blank => None,
            };
            self.slot_audio[slot] = SlotAudio::new(key, audio);
        }
    }

    pub(crate) fn slot_status(&self, slot: usize) -> SlotStatus {
        let entry = self.state.get().slots[slot];
        match entry.kind {
            SlotKind::Builtin => SlotStatus::Builtin,
            SlotKind::Blank => SlotStatus::Blank,
            SlotKind::User => {
                if let Some(sample) = self.slot_audio[slot].sample() {
                    return SlotStatus::Ready(tempo::reading_for(
                        entry.beats,
                        sample.duration_secs(),
                    ));
                }
                let restored = self
                    .library()
                    .and_then(|l| l.result(slot, Purpose::Restore))
                    .filter(|r| r.path_hash == entry.path_hash());
                match restored.map(|r| r.outcome) {
                    Some(LoadOutcome::Missing) => SlotStatus::Missing,
                    Some(LoadOutcome::Unreadable | LoadOutcome::Refused(_)) => {
                        SlotStatus::Unreadable
                    }
                    _ => SlotStatus::Loading,
                }
            }
        }
    }

    /// Whether a bar can sound from `slot`'s own audio.
    pub(crate) fn playable(&self, slot: usize) -> bool {
        let entry = self.state.get().slots[slot];
        entry.kind != SlotKind::Blank
            && (entry.kind == SlotKind::Builtin || entry.beats > 0)
            && self.slot_audio[slot].sample().is_some()
    }

    /// The slot a bar's ghost plays, as the engine picks it: the bar's own
    /// slot if it can sound, otherwise the lowest one that can.
    pub(crate) fn ghost_slot(&self, slot: usize) -> Option<usize> {
        if self.playable(slot) {
            return Some(slot);
        }
        (0..self.slot_audio.len()).find(|&s| self.playable(s))
    }

    /// Beats in a slot's break; built-in bars are 4.
    pub(crate) fn slot_beats(&self, slot: usize) -> u8 {
        let entry = self.state.get().slots[slot];
        match entry.kind {
            SlotKind::User if entry.beats > 0 => entry.beats,
            _ => 4,
        }
    }

    /// Opens the file dialog and asks the library to load the chosen file.
    pub(crate) fn choose_file(&mut self, slot: usize, mode: LoadMode) {
        if !self.breaks_editable() {
            self.message = LOCKED_HINT.into();
            return;
        }
        let Some(picked) = rfd::FileDialog::new()
            .add_filter("Audio", AUDIO_EXTENSIONS)
            .pick_file()
        else {
            return;
        };
        self.load_path(slot, picked, mode);
    }

    /// Asks the library to load `path` into `slot`. Shared by the file
    /// dialog and drops, so both refuse, report and handle the first-run
    /// prompt the same way.
    pub(crate) fn load_path(&mut self, slot: usize, path: PathBuf, mode: LoadMode) {
        if !self.breaks_editable() {
            self.message = LOCKED_HINT.into();
            return;
        }
        let Ok(path) = path.into_os_string().into_string() else {
            self.message = UNREADABLE.into();
            return;
        };
        if path.len() > MAX_PATH_BYTES {
            self.message = SlotError::PathTooLong.message().into();
            return;
        }
        self.pending = PendingLoad::Choosing {
            slot,
            seq: 0,
            path,
            mode,
            expect: SlotSnapshot::of(self.state.get(), slot),
        };
        self.post_pending();
    }

    /// Loads the first audio file in a drop on `target`.
    pub(crate) fn drop_files(&mut self, target: DropTarget, files: &[egui::DroppedFile]) {
        if !self.breaks_editable() {
            self.message = LOCKED_HINT.into();
            return;
        }
        let Some(path) = accepted_drop(files) else {
            self.message = NOT_AUDIO.into();
            return;
        };
        let slot = match target {
            DropTarget::FirstRun => 0,
            DropTarget::Empty => {
                let (slot, mode) = self.empty_target_slot();
                return self.load_path(slot, path, mode);
            }
            DropTarget::Waveform => self.selected_loop,
            DropTarget::Slot(slot) => slot,
        };
        // On the first-launch prompt, Break 1 takes a drop exactly as the
        // prompt does, but only on an untouched instance. ui() clears
        // first_run on the frame after an edit, so check here too: once the
        // state is edited, commit_load would downgrade FirstRun to a plain
        // Load that replaces Break 1's slices and placements without asking.
        // An edited instance goes through drop_into_slot's confirm dialog.
        if self.first_run && slot == 0 && self.state.get().is_pristine() {
            return self.load_path(0, path, LoadMode::FirstRun);
        }
        self.drop_into_slot(slot, path);
    }

    /// Where the empty state's Choose… and a drop on it load: a blank slot
    /// takes the file; with none, the selected slot is relinked so its
    /// slices and placements survive.
    pub(crate) fn empty_target_slot(&self) -> (usize, LoadMode) {
        let blank = |s: usize| self.state.get().slots[s].kind == SlotKind::Blank;
        if blank(self.selected_loop) {
            return (self.selected_loop, LoadMode::Load);
        }
        match (0..self.slot_audio.len()).find(|&s| blank(s)) {
            Some(slot) => (slot, LoadMode::Load),
            None => (self.selected_loop, LoadMode::Relink),
        }
    }

    /// A blank slot just loads. A missing or unreadable break is relinked,
    /// which keeps its slices and placements. Any other break, built-in or
    /// user, asks first, because the new file replaces its slices and
    /// placements.
    fn drop_into_slot(&mut self, slot: usize, path: PathBuf) {
        match self.slot_status(slot) {
            SlotStatus::Blank => self.load_path(slot, path, LoadMode::Load),
            SlotStatus::Missing | SlotStatus::Unreadable => {
                self.load_path(slot, path, LoadMode::Relink);
            }
            SlotStatus::Builtin | SlotStatus::Ready(_) | SlotStatus::Loading => {
                self.confirm_unload = Some(slot);
                self.unload_modal_fresh = true;
                self.replace_with = Some(path);
            }
        }
    }

    /// Outlines the drop target under the pointer while files hover, and
    /// loads a drop that lands on one. Call once per frame, after layout
    /// has registered this frame's targets.
    pub(crate) fn handle_file_drop(
        &mut self,
        ctx: &egui::Context,
        hovered: &[egui::HoveredFile],
        dropped: &[egui::DroppedFile],
    ) {
        // The confirm dialog is modal: nothing under it takes a drop.
        if (hovered.is_empty() && dropped.is_empty()) || self.confirm_open() {
            return;
        }
        let Some(pos) = ctx.input(|i| i.pointer.latest_pos()) else {
            return;
        };
        let Some(&(rect, target)) = self
            .drop_targets
            .iter()
            .rev()
            .find(|(rect, _)| rect.contains(pos))
        else {
            return;
        };
        if !hovered.is_empty() {
            let layer = egui::LayerId::new(egui::Order::Foreground, egui::Id::new("drop-target"));
            // Muted while frozen or live, so it's clear the drop won't take.
            let color = if self.breaks_editable() {
                ACCENT
            } else {
                MUTED
            };
            dashed_rect(
                &ctx.layer_painter(layer),
                rect.expand(2.0),
                Stroke::new(2.0_f32, color),
            );
        }
        if !dropped.is_empty() {
            self.drop_files(target, dropped);
        }
    }

    /// Sends the pending request if the library hasn't taken it yet.
    fn post_pending(&mut self) {
        let library = self.params.as_deref().map(|p| &p.library);
        if let (
            PendingLoad::Choosing {
                slot,
                seq: seq @ 0,
                path,
                ..
            },
            Some(library),
        ) = (&mut self.pending, library)
            && let Some(posted) = library.request(*slot, path.as_bytes(), Purpose::Choose)
        {
            *seq = posted;
        }
    }

    /// Frees audio the DSP let go of, and commits the pending load once
    /// the loader has finished it.
    pub(crate) fn poll_pending_load(&mut self, ctx: &egui::Context) {
        let Some(library) = self.library() else {
            return;
        };
        library.collect_garbage();
        self.post_pending();
        let (slot, seq, mode, expect) = match &self.pending {
            PendingLoad::None => return,
            PendingLoad::Choosing {
                slot,
                seq,
                mode,
                expect,
                ..
            } => (*slot, *seq, *mode, *expect),
        };
        let result = (seq != 0)
            .then(|| self.library().and_then(|l| l.result(slot, Purpose::Choose)))
            .flatten();
        match result {
            Some(result) if result.seq == seq => {
                let PendingLoad::Choosing { path, .. } =
                    std::mem::replace(&mut self.pending, PendingLoad::None)
                else {
                    return;
                };
                self.commit_load(slot, &path, mode, expect, &result.outcome);
            }
            // A newer request replaced this one.
            Some(result) if result.seq > seq => self.pending = PendingLoad::None,
            _ => ctx.request_repaint_after(Duration::from_millis(50)),
        }
    }

    fn commit_load(
        &mut self,
        slot: usize,
        path: &str,
        mode: LoadMode,
        expect: SlotSnapshot,
        outcome: &LoadOutcome,
    ) {
        if !self.breaks_editable() {
            self.message = LOCKED_HINT.into();
            return;
        }
        // The slot changed while the file decoded (a host project load, or
        // slices placed from it), so the user never agreed to replace what
        // it holds now.
        if expect.changed_for(SlotSnapshot::of(self.state.get(), slot), mode) {
            self.message = format!(
                "Break {} changed while loading. Drop or choose the file again to replace it.",
                slot + 1
            )
            .into();
            return;
        }
        // A first-run load clears the other slots only if the prompt is
        // still up on an untouched instance. The file decodes after the
        // dialog closes; if the state was edited or restored meanwhile, or
        // Keep the amen was clicked, it loads like any other choice.
        let mode =
            if mode == LoadMode::FirstRun && !(self.first_run && self.state.get().is_pristine()) {
                LoadMode::Load
            } else {
                mode
            };
        // Work on a copy so a failure never reaches the DSP.
        let mut next = self.state.get().clone();
        match apply_result(&mut next, slot, path, mode, outcome) {
            Ok(reading) => {
                self.state.update(|state| *state = next);
                self.refresh_slot_audio();
                // Draw the audio just committed, even when the path (and so
                // the cache key) is unchanged: a re-bounced file reloaded.
                if let LoadOutcome::Loaded(audio) = outcome {
                    let entry = self.state.get().slots[slot];
                    self.slot_audio[slot] =
                        SlotAudio::new((entry.kind, entry.path_hash()), Some(audio.clone()));
                }
                self.select_after_load = Some(slot);
                // Only a load that took dismisses the prompt for good.
                if mode == LoadMode::FirstRun {
                    mark_first_run_done();
                    self.first_run = false;
                }
                self.message = format!(
                    "Loaded {}: {}. Use ½ or ×2 if the tempo is off.",
                    file_name(path),
                    reading.label()
                )
                .into();
            }
            Err(message) => self.message = message.into(),
        }
    }

    /// Selects `slot` for editing and keeps Source slices in step with it.
    /// Automates the break before the count, so the DSP never applies one
    /// slot's count to the previously selected slot.
    pub(crate) fn select_break(&mut self, slot: usize, context: &PluginContext<JunglizerParams>) {
        self.selected_loop = slot;
        self.selected_slice = 0;
        context.automate(P::EditBreak, slot as f64 / 3.0);
        context.automate(P::EditSlice, 0.0);
        let count = self.state.get().slice_count(slot).max(1);
        context.automate(P::SourceSlices, f64::from(count - 1) / 31.0);
    }

    /// The Break 1–4 buttons. A blank slot reads “Empty” and opens the file
    /// dialog.
    pub(crate) fn slot_row(&mut self, ui: &mut Ui, context: &PluginContext<JunglizerParams>) {
        if let Some(slot) = self.select_after_load.take() {
            self.select_break(slot, context);
        }
        let editable = self.breaks_editable();
        for (i, color) in SOURCE_COLORS.iter().enumerate() {
            let blank = self.state.get().slots[i].kind == SlotKind::Blank;
            let text = if blank {
                egui::RichText::new("Empty").color(MUTED)
            } else {
                egui::RichText::new(format!("Break {}", i + 1)).color(*color)
            };
            let button = egui::Button::new(text).selected(self.selected_loop == i);
            let response = if blank {
                let hint = if DROP_SUPPORTED {
                    "Click to choose a break, or drop a WAV, AIFF or MP3 here.".to_owned()
                } else {
                    format!(
                        "Break {} is empty. Click to choose a WAV, AIFF or MP3.",
                        i + 1
                    )
                };
                ui.add_enabled(editable, button)
                    .on_hover_text(hint)
                    .on_disabled_hover_text(LOCKED_HINT)
            } else {
                ui.add(button)
            };
            self.drop_targets.push((response.rect, DropTarget::Slot(i)));
            if response.clicked() {
                self.select_break(i, context);
                if blank {
                    self.choose_file(i, LoadMode::Load);
                }
            }
        }
    }

    /// The selected slot's reading and its controls. Call inside a
    /// right-to-left layout: items are added from the right edge.
    pub(crate) fn slot_controls(&mut self, ui: &mut Ui) {
        let slot = self.selected_loop;
        let editable = self.breaks_editable();
        let status = self.slot_status(slot);
        if status == SlotStatus::Blank {
            return;
        }
        if ui
            .add_enabled(editable, egui::Button::new("Unload"))
            .on_disabled_hover_text(LOCKED_HINT)
            .clicked()
        {
            self.confirm_unload = Some(slot);
            self.unload_modal_fresh = true;
            self.replace_with = None;
        }
        let path = self.state.get().slots[slot]
            .path()
            .unwrap_or_default()
            .to_owned();
        match status {
            SlotStatus::Builtin => {
                ui.colored_label(MUTED, Library::builtin()[slot].reading.label());
            }
            SlotStatus::Ready(reading) => {
                for (label, next) in [
                    ("×2", tempo::doubled(reading)),
                    ("½", tempo::halved(reading)),
                ] {
                    if ui
                        .add_enabled(editable && next.is_some(), egui::Button::new(label))
                        .on_disabled_hover_text(if editable {
                            "That would leave 4 to 32 beats or 80–195 BPM."
                        } else {
                            LOCKED_HINT
                        })
                        .clicked()
                        && let Some(next) = next
                    {
                        self.state.update(|s| {
                            s.set_beats(slot, next.beats);
                        });
                    }
                }
                ui.colored_label(MUTED, reading.label())
                    .on_hover_text(file_name(&path));
            }
            SlotStatus::Loading => {
                ui.colored_label(MUTED, "Loading…")
                    .on_hover_text(file_name(&path));
            }
            SlotStatus::Missing | SlotStatus::Unreadable => {
                if ui
                    .add_enabled(editable, egui::Button::new("Relink…"))
                    .on_disabled_hover_text(LOCKED_HINT)
                    .clicked()
                {
                    self.choose_file(slot, LoadMode::Relink);
                }
                let label = if status == SlotStatus::Missing {
                    "Missing"
                } else {
                    "Unreadable"
                };
                ui.colored_label(MUTED, label).on_hover_text(path);
            }
            SlotStatus::Blank => {}
        }
    }

    /// The message drawn in the source waveform when there's no audio to
    /// show, or `None` when there is.
    pub(crate) fn waveform_notice(&self, slot: usize) -> Option<String> {
        let path = self.state.get().slots[slot].path().unwrap_or_default();
        match self.slot_status(slot) {
            SlotStatus::Blank => {
                Some("This slot is empty. Click Empty to choose a WAV, AIFF or MP3.".into())
            }
            SlotStatus::Loading => Some(format!("Loading {}…", file_name(path))),
            SlotStatus::Missing => Some(format!(
                "Can’t find {}. Use Relink… to find it.",
                file_name(path)
            )),
            SlotStatus::Unreadable => Some(format!(
                "Junglizer couldn’t read {}. Use Relink… to choose it again.",
                file_name(path)
            )),
            SlotStatus::Builtin | SlotStatus::Ready(_) => None,
        }
    }

    /// Faint beat lines and stronger bar lines every 4 beats.
    /// Nothing is drawn in a blank well or one with no audio.
    pub(crate) fn beat_grid(&self, painter: &egui::Painter, rect: egui::Rect, slot: usize) {
        if self.state.get().slots[slot].kind == SlotKind::Blank
            || self.slot_audio[slot].sample().is_none()
        {
            return;
        }
        let beats = self.slot_beats(slot);
        for beat in 1..beats {
            let x = rect.left() + rect.width() * f32::from(beat) / f32::from(beats);
            let color = if beat % 4 == 0 {
                EDGE
            } else {
                EDGE.gamma_multiply(0.5)
            };
            painter.line_segment(
                [
                    egui::Pos2::new(x, rect.top()),
                    egui::Pos2::new(x, rect.bottom()),
                ],
                egui::Stroke::new(1.0_f32, color),
            );
        }
    }

    /// True when no slot has a break that can sound: every slot is blank,
    /// or holds a user break that's missing or unreadable.
    pub(crate) fn no_playable_break(&self) -> bool {
        (0..self.slot_audio.len()).all(|slot| {
            matches!(
                self.slot_status(slot),
                SlotStatus::Blank | SlotStatus::Missing | SlotStatus::Unreadable
            )
        })
    }

    /// Whether the source waveform is replaced by the break target.
    pub(crate) fn shows_break_target(&self) -> bool {
        self.first_run || self.no_playable_break()
    }

    /// Whether the unload, replace or amen confirm dialog is up.
    pub(crate) fn confirm_open(&self) -> bool {
        self.confirm_unload.is_some() || self.confirm_amen
    }

    /// The empty state's Use the Junglizer amen. A missing or unreadable
    /// break's slices and placements go with it, so ask first when there
    /// is one.
    pub(crate) fn use_builtin_amen(&mut self) {
        if !self.breaks_editable() {
            self.message = LOCKED_HINT.into();
            return;
        }
        if self
            .state
            .get()
            .slots
            .iter()
            .any(|slot| slot.kind == SlotKind::User)
        {
            self.confirm_amen = true;
            self.unload_modal_fresh = true;
            return;
        }
        self.restore_amen();
    }

    /// Puts the four built-in breaks back, with fresh markers.
    fn restore_amen(&mut self) {
        if !self.breaks_editable() {
            self.message = LOCKED_HINT.into();
            return;
        }
        self.state.update(EditState::restore_builtin);
        self.refresh_slot_audio();
        for slot in 0..self.slot_audio.len() {
            let count = self.state.get().slice_count(slot);
            self.ensure_markers(slot, count);
        }
        self.select_after_load = Some(self.selected_loop);
        self.message = "The Junglizer amen is back in all four slots.".into();
    }

    /// The dashed target drawn in place of the source waveform: the
    /// first-launch prompt, or the empty state when no slot can sound.
    pub(crate) fn break_target(&mut self, ui: &mut Ui, rect: Rect) {
        let target = if self.first_run {
            DropTarget::FirstRun
        } else {
            DropTarget::Empty
        };
        self.drop_targets.push((rect, target));
        dashed_rect(ui.painter(), rect.shrink(1.5), Stroke::new(1.5_f32, ACCENT));
        let center = rect.center().x;
        ui.painter().text(
            Pos2::new(center, rect.top() + 24.0),
            egui::Align2::CENTER_CENTER,
            TARGET_PROMPT,
            egui::FontId::proportional(15.0),
            INK,
        );
        let editable = self.breaks_editable();
        let row = rect.top() + 56.0;
        let choose =
            egui::Button::new(egui::RichText::new("Choose…").color(WELL).strong()).fill(ACCENT);
        let choose_width = text_width(ui, "Choose…", 13.0) + 20.0;
        if self.first_run {
            let keep_width = text_width(ui, "Keep the amen", 13.0) + 16.0;
            let left = center - (choose_width + 8.0 + keep_width) / 2.0;
            let choose_rect =
                Rect::from_min_size(Pos2::new(left, row - 13.0), vec2(choose_width, 26.0));
            let keep_rect = Rect::from_min_size(
                Pos2::new(choose_rect.right() + 8.0, row - 13.0),
                vec2(keep_width, 26.0),
            );
            if ui
                .place(choose_rect, choose)
                .on_hover_text("Loads into Break 1 and clears Breaks 2 to 4.")
                .clicked()
            {
                self.choose_file(0, LoadMode::FirstRun);
            }
            if ui
                .place(keep_rect, egui::Button::new("Keep the amen"))
                .clicked()
            {
                mark_first_run_done();
                self.first_run = false;
            }
        } else {
            let restore = "Use the Junglizer amen";
            let restore_width = text_width(ui, restore, 13.0);
            let left = center - (choose_width + 16.0 + restore_width) / 2.0;
            let choose_rect =
                Rect::from_min_size(Pos2::new(left, row - 13.0), vec2(choose_width, 26.0));
            if ui
                .add_enabled_ui(editable, |ui| ui.place(choose_rect, choose))
                .inner
                .on_disabled_hover_text(LOCKED_HINT)
                .clicked()
            {
                let (slot, mode) = self.empty_target_slot();
                self.choose_file(slot, mode);
            }
            let at = Pos2::new(choose_rect.right() + 16.0 + restore_width / 2.0, row);
            let response = link(ui, at, restore, editable);
            let response = if editable {
                response.on_hover_text("Puts the four built-in breaks back.")
            } else {
                response.on_hover_text(LOCKED_HINT)
            };
            if response.clicked() {
                self.use_builtin_amen();
            }
        }
        if link(
            ui,
            Pos2::new(center, rect.bottom() - 18.0),
            FREE_PACKS,
            true,
        )
        .on_hover_text(FREE_PACKS_URL)
        .clicked()
        {
            crate::platform::open_url(FREE_PACKS_URL);
        }
    }

    /// Asks before unloading a break, before a dropped file replaces one,
    /// or before the amen replaces missing breaks.
    pub(crate) fn unload_modal(&mut self, ctx: &egui::Context) {
        if !self.confirm_open() {
            return;
        }
        let slot = self.confirm_unload.unwrap_or(0);
        if std::mem::take(&mut self.unload_modal_fresh) {
            // A new area is invisible while it measures itself; redo the
            // pass so the dialog shows on the first frame.
            ctx.request_discard("unload dialog first frame");
        }
        let id = egui::Id::new("unload-break");
        // No fade: the dialog is there on the frame it opens.
        let modal = egui::Modal::new(id).area(egui::Modal::default_area(id).fade_in(false));
        let (question, confirm) = match &self.replace_with {
            _ if self.confirm_amen => (AMEN_QUESTION.to_owned(), "Replace"),
            Some(path) => (
                format!(
                    "Replace Break {} with “{}”? Its slices and placements in this project will be removed.",
                    slot + 1,
                    path.file_name()
                        .unwrap_or(path.as_os_str())
                        .to_string_lossy()
                ),
                "Replace",
            ),
            None => (
                "Unload this break? Its slices and placements in this project will be removed."
                    .to_owned(),
                "Unload",
            ),
        };
        let response = modal.show(ctx, |ui| {
            apply_style(ui);
            ui.set_max_width(330.0);
            ui.label(question);
            ui.add_space(10.0);
            modal_buttons(ui, confirm)
        });
        match response.inner {
            ModalChoice::Action => self.close_unload_modal(true),
            ModalChoice::Cancel => self.close_unload_modal(false),
            // Esc or a click outside the dialog.
            ModalChoice::Open if response.should_close() => self.close_unload_modal(false),
            ModalChoice::Open => {}
        }
    }

    /// Acts on the dialog's answer: unload the break, load the dropped file
    /// over it, or put the amen back. Cancel leaves everything as it was.
    pub(crate) fn close_unload_modal(&mut self, confirmed: bool) {
        if std::mem::take(&mut self.confirm_amen) {
            if confirmed {
                self.restore_amen();
            }
            return;
        }
        let Some(slot) = self.confirm_unload.take() else {
            return;
        };
        let replacement = self.replace_with.take();
        if !confirmed {
            return;
        }
        // A dropped file replaces the break. The old break goes only if the
        // new one loads.
        if let Some(path) = replacement {
            self.load_path(slot, path, LoadMode::Load);
            return;
        }
        if !self.breaks_editable() {
            self.message = LOCKED_HINT.into();
            return;
        }
        let mut unloaded = false;
        self.state.update(|s| unloaded = s.unload(slot));
        if unloaded {
            self.refresh_slot_audio();
            self.message = format!("Break {} unloaded.", slot + 1).into();
        }
    }
}

/// A confirm dialog's answer this frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ModalChoice {
    Open,
    Action,
    Cancel,
}

/// The confirm buttons left to right, in each platform's own order, all
/// right-aligned: Cancel then the action on macOS and Linux (GNOME), the
/// action then Cancel on Windows.
pub(crate) fn modal_button_order(windows: bool) -> [ModalChoice; 2] {
    if windows {
        [ModalChoice::Action, ModalChoice::Cancel]
    } else {
        [ModalChoice::Cancel, ModalChoice::Action]
    }
}

/// The shared confirm row for the Unload, Replace and amen dialogs: a
/// filled primary action and an outlined Cancel, right-aligned in the
/// platform's order. Return takes the action; Esc closes the modal, which
/// the caller reads as Cancel.
pub(crate) fn modal_buttons(ui: &mut Ui, action: &str) -> ModalChoice {
    let mut choice = ModalChoice::Open;
    let height = ui.spacing().interact_size.y;
    ui.allocate_ui_with_layout(
        egui::vec2(ui.available_width(), height),
        egui::Layout::right_to_left(egui::Align::Center),
        |ui| {
            // Right to left, so the order is reversed.
            for button in modal_button_order(cfg!(target_os = "windows"))
                .into_iter()
                .rev()
            {
                let clicked = match button {
                    ModalChoice::Action => ui
                        .add(
                            egui::Button::new(egui::RichText::new(action).color(WELL).strong())
                                .fill(ACCENT)
                                .stroke(Stroke::new(1.0_f32, ACCENT))
                                .min_size(egui::vec2(76.0, 0.0)),
                        )
                        .clicked(),
                    _ => ui
                        .add(egui::Button::new("Cancel").min_size(egui::vec2(76.0, 0.0)))
                        .clicked(),
                };
                if clicked {
                    choice = button;
                }
            }
        },
    );
    if choice == ModalChoice::Open && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
        choice = ModalChoice::Action;
    }
    choice
}

/// A dashed rectangle outline, as the break target draws it.
fn dashed_rect(painter: &egui::Painter, rect: Rect, stroke: Stroke) {
    let corners = [
        rect.left_top(),
        rect.right_top(),
        rect.right_bottom(),
        rect.left_bottom(),
        rect.left_top(),
    ];
    painter.extend(egui::Shape::dashed_line(&corners, stroke, 7.0, 5.0));
}

fn text_width(ui: &Ui, text: &str, size: f32) -> f32 {
    ui.painter()
        .layout_no_wrap(text.to_owned(), egui::FontId::proportional(size), INK)
        .size()
        .x
}

/// Link-styled text centered on `at`: ACCENT, underlined on hover. egui's
/// own hyperlink can't be used, because the host ignores its `open_url`.
fn link(ui: &mut Ui, at: Pos2, text: &str, enabled: bool) -> egui::Response {
    let color = if enabled { ACCENT } else { MUTED };
    let galley =
        ui.painter()
            .layout_no_wrap(text.to_owned(), egui::FontId::proportional(13.0), color);
    let rect = Rect::from_center_size(at, galley.size());
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let response = ui.interact(rect, ui.id().with(("link", text)), sense);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Link, enabled, text));
    if enabled && response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        ui.painter().line_segment(
            [rect.left_bottom(), rect.right_bottom()],
            Stroke::new(1.0_f32, color),
        );
    }
    ui.painter().galley(rect.min, galley, color);
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit_state::SlotKind;
    use crate::library::LoadedBreak;
    use crate::sample::StereoSample;
    use crate::tempo::{self, Refusal};
    use std::sync::Arc;

    #[test]
    fn a_refused_choice_leaves_the_slot_unchanged() {
        let mut s = EditState::default();
        let before = s.clone();
        apply_result(
            &mut s,
            0,
            "/x/long.wav",
            LoadMode::Load,
            &LoadOutcome::Refused(Refusal::TooLong),
        )
        .unwrap_err();
        assert_eq!(s, before);
    }

    #[test]
    fn first_run_choice_loads_slot_one_and_clears_the_others() {
        let mut s = EditState::default();
        let b = Arc::new(LoadedBreak::new(
            StereoSample {
                frames: vec![[0.1; 2]; 60_828],
                sample_rate: 44_100,
            },
            7,
            tempo::reading_for(4, 60_828.0 / 44_100.0),
        ));
        apply_result(
            &mut s,
            0,
            "/x/my 174.wav",
            LoadMode::FirstRun,
            &LoadOutcome::Loaded(b),
        )
        .unwrap();
        assert_eq!(
            s.slots.map(|x| x.kind),
            [
                SlotKind::User,
                SlotKind::Blank,
                SlotKind::Blank,
                SlotKind::Blank
            ]
        );
    }

    fn loaded(frames: usize, beats: u8) -> LoadOutcome {
        // Real drums, so the suggested slices are active.
        let amen = &Library::builtin()[0].sample.frames;
        let sample = StereoSample {
            frames: amen.iter().copied().cycle().take(frames).collect(),
            sample_rate: 44_100,
        };
        let secs = frames as f64 / 44_100.0;
        LoadOutcome::Loaded(Arc::new(LoadedBreak::new(
            sample,
            7,
            tempo::reading_for(beats, secs),
        )))
    }

    #[test]
    fn a_loaded_choice_links_the_file_with_eight_slices() {
        let mut s = EditState::default();
        let reading =
            apply_result(&mut s, 2, "/x/b.wav", LoadMode::Load, &loaded(243_312, 16)).unwrap();
        assert_eq!(reading.beats, 16);
        let slot = s.slots[2];
        assert_eq!(
            (slot.kind, slot.beats, slot.frames, slot.slice_count),
            (SlotKind::User, 16, 243_312, 8)
        );
        assert_eq!(slot.path(), Some("/x/b.wav"));
        assert!(s.marker_set(2).is_some_and(|set| set.valid));
        assert_eq!(s.slots[0].kind, SlotKind::Builtin);
    }

    #[test]
    fn unreadable_missing_and_empty_audio_leave_the_state_unchanged() {
        let mut s = EditState::default();
        let before = s.clone();
        for outcome in [LoadOutcome::Unreadable, LoadOutcome::Missing, loaded(0, 4)] {
            let message =
                apply_result(&mut s, 0, "/x/a.wav", LoadMode::Load, &outcome).unwrap_err();
            assert_eq!(message, UNREADABLE);
            assert_eq!(s, before);
        }
    }

    #[test]
    fn a_slot_error_is_reported_and_changes_nothing() {
        let mut s = EditState::default();
        let before = s.clone();
        // Relink only applies to a user slot.
        let message =
            apply_result(&mut s, 1, "/x/a.wav", LoadMode::Relink, &loaded(60_828, 4)).unwrap_err();
        assert_eq!(message, SlotError::BadSlot.message());
        assert_eq!(s, before);
    }

    #[test]
    fn relink_keeps_placements_and_takes_the_new_reading() {
        let mut s = EditState::default();
        apply_result(&mut s, 0, "/old/a.wav", LoadMode::Load, &loaded(60_828, 4)).unwrap();
        let slice = s.slice(0, 1).unwrap();
        assert!(s.place(crate::edit_state::StepPlacement {
            active: true,
            start_slot: 0,
            span_slots: 1,
            source_loop: 0,
            source_slice: 1,
            mode: 0,
            bit_depth: 16,
            repeats: 1,
            locked: false,
        }));
        apply_result(
            &mut s,
            0,
            "/new/a.wav",
            LoadMode::Relink,
            &loaded(121_656, 8),
        )
        .unwrap();
        assert_eq!(s.slots[0].path(), Some("/new/a.wav"));
        assert_eq!((s.slots[0].beats, s.slots[0].frames), (8, 121_656));
        assert!(s.placements.iter().any(|p| p.active && p.source_slice == 1));
        assert!(s.slice(0, 1).unwrap().start >= slice.start);
    }

    #[test]
    fn first_run_shows_only_on_a_fresh_instance_without_the_flag() {
        let mut s = EditState::default();
        assert!(show_first_run(false, &s));
        assert!(!show_first_run(true, &s));
        s.set_slice_count(0, 16);
        assert!(!show_first_run(false, &s));
    }

    #[test]
    fn the_first_run_flag_is_an_empty_file_in_a_new_folder() {
        let dir = std::env::temp_dir().join(format!("junglizer-flag-{}", std::process::id()));
        let flag = dir.join("Junglizer").join("first-run-0.9.1");
        write_flag(&flag).unwrap();
        assert_eq!(std::fs::metadata(&flag).unwrap().len(), 0);
        // Writing it again is harmless.
        write_flag(&flag).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// An editor with its first-run prompt up and a FirstRun load in
    /// flight for slot 0.
    fn editor_with_pending_first_run() -> JunglizerUi {
        let mut editor = JunglizerUi::new();
        editor.params = Some(Arc::new(crate::JunglizerParams::default()));
        editor.first_run = true;
        editor.pending = PendingLoad::Choosing {
            slot: 0,
            seq: 1,
            path: "/x/mine 174.wav".to_owned(),
            mode: LoadMode::FirstRun,
            expect: SlotSnapshot::of(editor.state.get(), 0),
        };
        editor
    }

    /// What `poll_pending_load` does once the loader reports back.
    fn finish_pending(editor: &mut JunglizerUi, outcome: &LoadOutcome) {
        let PendingLoad::Choosing {
            slot,
            path,
            mode,
            expect,
            ..
        } = std::mem::replace(&mut editor.pending, PendingLoad::None)
        else {
            panic!("no pending load");
        };
        editor.commit_load(slot, &path, mode, expect, outcome);
    }

    #[test]
    fn a_first_run_load_never_clears_slots_edited_while_it_decoded() {
        let mut editor = editor_with_pending_first_run();
        editor.state.update(|s| {
            s.set_slice_count(2, 16);
        });
        assert!(!editor.state.get().is_pristine());
        finish_pending(&mut editor, &loaded(60_828, 4));
        let state = editor.state.get();
        assert_eq!(state.slots[0].kind, SlotKind::User);
        assert_eq!(state.slots[0].path(), Some("/x/mine 174.wav"));
        assert_eq!(
            state.slots[1..].iter().map(|s| s.kind).collect::<Vec<_>>(),
            [SlotKind::Builtin; 3]
        );
        assert_eq!(state.slice_count(2), 16);
    }

    #[test]
    fn keep_the_amen_during_a_first_run_load_keeps_the_other_slots() {
        let mut editor = editor_with_pending_first_run();
        // Keep the amen dismisses the prompt while the file decodes.
        editor.first_run = false;
        finish_pending(&mut editor, &loaded(60_828, 4));
        let state = editor.state.get();
        assert_eq!(state.slots[0].kind, SlotKind::User);
        assert_eq!(
            state.slots[1..].iter().map(|s| s.kind).collect::<Vec<_>>(),
            [SlotKind::Builtin; 3]
        );
    }

    #[test]
    fn only_supported_audio_files_are_accepted_from_a_drop() {
        let f = |p: &str| egui::DroppedFile {
            path: Some(PathBuf::from(p)),
            ..Default::default()
        };
        assert_eq!(
            accepted_drop(&[f("/a/notes.txt"), f("/a/Think 114.AIF")]),
            Some(PathBuf::from("/a/Think 114.AIF"))
        );
        assert_eq!(
            accepted_drop(&[f("C:\\Breaks\\amen.MP3")]),
            Some(PathBuf::from("C:\\Breaks\\amen.MP3"))
        );
        assert_eq!(
            accepted_drop(&[f("/a/clip.flac"), egui::DroppedFile::default()]),
            None
        );
    }

    fn dropped(path: &str) -> [egui::DroppedFile; 1] {
        [egui::DroppedFile {
            path: Some(PathBuf::from(path)),
            ..Default::default()
        }]
    }

    fn pending_load(editor: &JunglizerUi) -> Option<(usize, &str, LoadMode)> {
        match &editor.pending {
            PendingLoad::None => None,
            PendingLoad::Choosing {
                slot, path, mode, ..
            } => Some((*slot, path.as_str(), *mode)),
        }
    }

    #[test]
    fn a_drop_on_the_first_run_target_is_a_first_run_load_into_break_one() {
        let mut editor = JunglizerUi::new();
        editor.first_run = true;
        editor.drop_files(DropTarget::FirstRun, &dropped("/x/mine 174.wav"));
        assert_eq!(
            pending_load(&editor),
            Some((0, "/x/mine 174.wav", LoadMode::FirstRun))
        );
    }

    #[test]
    fn a_drop_on_a_blank_slot_loads_without_asking() {
        let mut editor = JunglizerUi::new();
        editor.state.update(|s| {
            s.unload(2);
            s.unload(3);
        });
        editor.refresh_slot_audio();
        editor.drop_files(DropTarget::Slot(2), &dropped("/x/b.aiff"));
        assert_eq!(
            pending_load(&editor),
            Some((2, "/x/b.aiff", LoadMode::Load))
        );
        // The waveform targets the selected slot.
        editor.selected_loop = 3;
        editor.drop_files(DropTarget::Waveform, &dropped("/x/c.mp3"));
        assert_eq!(pending_load(&editor), Some((3, "/x/c.mp3", LoadMode::Load)));
        assert_eq!(editor.confirm_unload, None);
    }

    /// An editor whose built-in Break 3 has a slice placed from it.
    fn editor_with_builtin_placement() -> JunglizerUi {
        let mut editor = JunglizerUi::new();
        editor.ensure_markers(2, 8);
        editor.state.update(|s| {
            assert!(s.place(crate::edit_state::StepPlacement {
                active: true,
                start_slot: 4,
                span_slots: 1,
                source_loop: 2,
                source_slice: 3,
                mode: 0,
                bit_depth: 16,
                repeats: 1,
                locked: false,
            }));
        });
        editor
    }

    #[test]
    fn a_drop_on_a_builtin_break_asks_first_and_cancel_keeps_it() {
        for target in [DropTarget::Slot(2), DropTarget::Waveform] {
            let mut editor = editor_with_builtin_placement();
            editor.selected_loop = 2;
            let before = editor.state.get().clone();
            editor.drop_files(target, &dropped("/x/new.wav"));
            assert_eq!(editor.confirm_unload, Some(2));
            assert_eq!(editor.replace_with, Some(PathBuf::from("/x/new.wav")));
            assert!(pending_load(&editor).is_none());
            assert!(*editor.state.get() == before, "the state changed");
            editor.close_unload_modal(false);
            assert_eq!(editor.confirm_unload, None);
            assert_eq!(editor.replace_with, None);
            assert!(pending_load(&editor).is_none());
            assert!(*editor.state.get() == before, "the state changed");
            assert_eq!(editor.state.get().slots[2].kind, SlotKind::Builtin);
        }
    }

    #[test]
    fn replacing_a_builtin_break_loads_the_drop_once_confirmed() {
        let mut editor = editor_with_builtin_placement();
        editor.drop_files(DropTarget::Slot(2), &dropped("/x/new.wav"));
        editor.close_unload_modal(true);
        assert_eq!(editor.confirm_unload, None);
        assert_eq!(
            pending_load(&editor),
            Some((2, "/x/new.wav", LoadMode::Load))
        );
    }

    #[test]
    fn during_first_run_break_one_takes_a_drop_as_the_prompt_does() {
        let mut editor = JunglizerUi::new();
        editor.first_run = true;
        editor.drop_files(DropTarget::Slot(0), &dropped("/x/a.wav"));
        assert_eq!(
            pending_load(&editor),
            Some((0, "/x/a.wav", LoadMode::FirstRun))
        );
        editor.selected_loop = 0;
        editor.drop_files(DropTarget::Waveform, &dropped("/x/b.wav"));
        assert_eq!(
            pending_load(&editor),
            Some((0, "/x/b.wav", LoadMode::FirstRun))
        );
        // Breaks 2 to 4 still ask before replacing a built-in break.
        editor.pending = PendingLoad::None;
        editor.drop_files(DropTarget::Slot(2), &dropped("/x/c.wav"));
        assert_eq!(editor.confirm_unload, Some(2));
        assert!(pending_load(&editor).is_none());
    }

    #[test]
    fn during_first_run_an_edited_break_one_asks_before_a_drop_replaces_it() {
        for target in [
            DropTarget::Slot(0),
            DropTarget::Waveform,
            DropTarget::FirstRun,
        ] {
            let mut editor = JunglizerUi::new();
            editor.first_run = true;
            editor.selected_loop = 0;
            editor.ensure_markers(0, 8);
            editor.state.update(|s| {
                assert!(s.place(crate::edit_state::StepPlacement {
                    active: true,
                    start_slot: 0,
                    span_slots: 1,
                    source_loop: 0,
                    source_slice: 2,
                    mode: 0,
                    bit_depth: 16,
                    repeats: 1,
                    locked: false,
                }));
            });
            assert!(!editor.state.get().is_pristine());
            let before = editor.state.get().clone();
            editor.drop_files(target, &dropped("/x/a.wav"));
            assert_eq!(editor.confirm_unload, Some(0), "{target:?}");
            assert_eq!(editor.replace_with, Some(PathBuf::from("/x/a.wav")));
            assert!(pending_load(&editor).is_none(), "{target:?}");
            assert!(*editor.state.get() == before, "the state changed");
        }
    }

    #[test]
    fn the_empty_target_drop_lands_where_choose_would() {
        let mut editor = JunglizerUi::new();
        editor.params = Some(Arc::new(crate::JunglizerParams::default()));
        editor.state.update(|s| {
            s.unload(2);
        });
        editor.refresh_slot_audio();
        // The selected slot isn't blank, so the first blank one takes it.
        assert_eq!(editor.empty_target_slot(), (2, LoadMode::Load));
        editor.drop_files(DropTarget::Empty, &dropped("/x/a.wav"));
        assert_eq!(pending_load(&editor), Some((2, "/x/a.wav", LoadMode::Load)));
        // With no blank slot, the selected slot is relinked.
        editor.state.update(|s| {
            let suggested = Library::builtin()[2].suggested;
            s.load_user(2, "/gone/b.wav", 60_828, 4, &suggested[..8])
                .unwrap();
        });
        editor.selected_loop = 2;
        assert_eq!(editor.empty_target_slot(), (2, LoadMode::Relink));
        editor.drop_files(DropTarget::Empty, &dropped("/x/c.wav"));
        assert_eq!(
            pending_load(&editor),
            Some((2, "/x/c.wav", LoadMode::Relink))
        );
    }

    #[test]
    fn a_drop_on_a_user_break_asks_first_and_replaces_it_only_on_unload() {
        let mut editor = JunglizerUi::new();
        let suggested = Library::builtin()[1].suggested;
        editor.state.update(|s| {
            s.load_user(1, "/x/old.wav", 60_828, 4, &suggested[..8])
                .unwrap();
        });
        editor.slot_audio[1] = SlotAudio::new(
            (SlotKind::User, editor.state.get().slots[1].path_hash()),
            Some(Library::builtin()[1].clone()),
        );
        editor.drop_files(DropTarget::Slot(1), &dropped("/x/new.wav"));
        assert_eq!(editor.confirm_unload, Some(1));
        assert_eq!(editor.replace_with, Some(PathBuf::from("/x/new.wav")));
        assert!(pending_load(&editor).is_none());
        // A second drop while the dialog is up is ignored.
        editor.handle_file_drop(&egui::Context::default(), &[], &dropped("/x/other.wav"));
        assert_eq!(editor.replace_with, Some(PathBuf::from("/x/new.wav")));
        editor.close_unload_modal(true);
        assert_eq!(
            pending_load(&editor),
            Some((1, "/x/new.wav", LoadMode::Load))
        );
    }

    #[test]
    fn a_drop_on_a_missing_break_relinks_it() {
        let mut editor = JunglizerUi::new();
        editor.params = Some(Arc::new(crate::JunglizerParams::default()));
        let suggested = Library::builtin()[1].suggested;
        let path = "/Volumes/Gone/think.wav";
        editor.state.update(|s| {
            s.load_user(1, path, 60_828, 4, &suggested[..8]).unwrap();
        });
        let library = &editor.params.as_ref().unwrap().library;
        library
            .request(1, path.as_bytes(), Purpose::Restore)
            .unwrap();
        library.run_pending();
        editor.refresh_slot_audio();
        assert_eq!(editor.slot_status(1), SlotStatus::Missing);
        editor.drop_files(DropTarget::Slot(1), &dropped("/x/found.wav"));
        assert_eq!(
            pending_load(&editor),
            Some((1, "/x/found.wav", LoadMode::Relink))
        );
        assert_eq!(editor.confirm_unload, None);
    }

    #[test]
    fn a_drop_with_no_audio_file_or_while_locked_only_explains() {
        let mut editor = JunglizerUi::new();
        editor.drop_files(DropTarget::Slot(0), &dropped("/x/notes.txt"));
        assert_eq!(editor.message, NOT_AUDIO);
        assert!(pending_load(&editor).is_none());
        editor.state.update(|s| s.frozen = true);
        editor.drop_files(DropTarget::Slot(0), &dropped("/x/a.wav"));
        assert_eq!(editor.message, LOCKED_HINT);
        assert!(pending_load(&editor).is_none());
    }

    /// An editor whose Break 2 is a user break whose file is missing, with
    /// a slice placed from it, and the other slots blank: the empty state.
    fn editor_with_a_missing_break() -> JunglizerUi {
        let mut editor = JunglizerUi::new();
        editor.params = Some(Arc::new(crate::JunglizerParams::default()));
        let suggested = Library::builtin()[1].suggested;
        let path = "/Volumes/Gone/think.wav";
        editor.state.update(|s| {
            for slot in [0, 2, 3] {
                s.unload(slot);
            }
            s.load_user(1, path, 60_828, 4, &suggested[..8]).unwrap();
            assert!(s.place(crate::edit_state::StepPlacement {
                active: true,
                start_slot: 0,
                span_slots: 1,
                source_loop: 1,
                source_slice: 2,
                mode: 0,
                bit_depth: 16,
                repeats: 1,
                locked: false,
            }));
        });
        let library = &editor.params.as_ref().unwrap().library;
        library
            .request(1, path.as_bytes(), Purpose::Restore)
            .unwrap();
        library.run_pending();
        editor.refresh_slot_audio();
        assert!(editor.no_playable_break());
        editor
    }

    #[test]
    fn the_amen_link_asks_before_it_removes_a_missing_breaks_work() {
        let mut editor = editor_with_a_missing_break();
        let before = editor.state.get().clone();
        editor.use_builtin_amen();
        assert!(*editor.state.get() == before, "the state changed");
        assert!(editor.confirm_amen && editor.confirm_open());
        // The dialog is modal: a drop under it is ignored, though the
        // pointer is over a drop target.
        let ctx = pointer_over_target(&mut editor);
        editor.handle_file_drop(&ctx, &[], &dropped("/x/a.wav"));
        assert!(pending_load(&editor).is_none());
        assert_eq!(editor.confirm_unload, None);
        editor.close_unload_modal(false);
        assert!(!editor.confirm_open());
        assert!(*editor.state.get() == before, "Cancel changed the state");
        editor.use_builtin_amen();
        editor.close_unload_modal(true);
        assert!(!editor.confirm_open());
        assert_eq!(
            editor.state.get().slots.map(|s| s.kind),
            [SlotKind::Builtin; 4]
        );
        assert!(editor.state.get().placements.iter().all(|p| !p.active));
        assert_eq!(
            editor.message,
            "The Junglizer amen is back in all four slots."
        );
        // With the dialog closed, the same drop reaches the target: a
        // built-in break asks before the file replaces it.
        editor.handle_file_drop(&ctx, &[], &dropped("/x/a.wav"));
        assert_eq!(editor.confirm_unload, Some(0));
    }

    /// A context whose pointer rests on a drop target for Break 1.
    fn pointer_over_target(editor: &mut JunglizerUi) -> egui::Context {
        let ctx = egui::Context::default();
        let _ = ctx.run_ui(
            egui::RawInput {
                events: vec![egui::Event::PointerMoved(Pos2::new(20.0, 20.0))],
                ..Default::default()
            },
            |_| {},
        );
        assert!(ctx.input(|i| i.pointer.latest_pos()).is_some());
        editor.drop_targets.push((
            Rect::from_min_size(Pos2::ZERO, vec2(100.0, 100.0)),
            DropTarget::Slot(0),
        ));
        ctx
    }

    #[test]
    fn confirm_buttons_follow_each_platforms_order() {
        use ModalChoice::{Action, Cancel};
        // macOS and Linux: Cancel, then the action on the far right.
        assert_eq!(modal_button_order(false), [Cancel, Action]);
        // Windows: the action, then Cancel.
        assert_eq!(modal_button_order(true), [Action, Cancel]);
    }

    #[test]
    fn with_no_user_break_the_amen_link_restores_without_asking() {
        let mut editor = JunglizerUi::new();
        editor.state.update(|s| {
            for slot in 0..4 {
                s.unload(slot);
            }
        });
        editor.use_builtin_amen();
        assert!(!editor.confirm_open());
        assert_eq!(
            editor.state.get().slots.map(|s| s.kind),
            [SlotKind::Builtin; 4]
        );
    }

    const CHANGED: &str = "changed while loading. Drop or choose the file again to replace it.";

    #[test]
    fn a_load_into_a_blank_slot_the_host_filled_meanwhile_is_dropped() {
        let mut editor = JunglizerUi::new();
        editor.state.update(|s| {
            s.unload(2);
        });
        editor.refresh_slot_audio();
        editor.drop_files(DropTarget::Slot(2), &dropped("/x/b.wav"));
        assert_eq!(pending_load(&editor), Some((2, "/x/b.wav", LoadMode::Load)));
        // A host project load fills Break 3 while the file decodes.
        let suggested = Library::builtin()[2].suggested;
        editor.state.update(|s| {
            s.load_user(2, "/host/kept.wav", 60_828, 4, &suggested[..8])
                .unwrap();
        });
        let before = editor.state.get().clone();
        finish_pending(&mut editor, &loaded(60_828, 4));
        assert!(*editor.state.get() == before, "the state changed");
        assert_eq!(editor.message, format!("Break 3 {CHANGED}"));
    }

    #[test]
    fn a_first_run_load_never_replaces_break_one_once_it_has_placements() {
        let mut editor = JunglizerUi::new();
        editor.first_run = true;
        editor.drop_files(DropTarget::FirstRun, &dropped("/x/mine 174.wav"));
        assert_eq!(
            pending_load(&editor),
            Some((0, "/x/mine 174.wav", LoadMode::FirstRun))
        );
        // A slice from Break 1 is placed while the file decodes.
        editor.ensure_markers(0, 8);
        editor.state.update(|s| {
            assert!(s.place(crate::edit_state::StepPlacement {
                active: true,
                start_slot: 0,
                span_slots: 1,
                source_loop: 0,
                source_slice: 2,
                mode: 0,
                bit_depth: 16,
                repeats: 1,
                locked: false,
            }));
        });
        let before = editor.state.get().clone();
        finish_pending(&mut editor, &loaded(60_828, 4));
        assert!(*editor.state.get() == before, "the state changed");
        assert_eq!(editor.message, format!("Break 1 {CHANGED}"));
    }

    #[test]
    fn a_confirmed_replace_still_loads_and_a_slot_emptied_meanwhile_takes_it() {
        let mut editor = editor_with_builtin_placement();
        editor.drop_files(DropTarget::Slot(2), &dropped("/x/new.wav"));
        editor.close_unload_modal(true);
        finish_pending(&mut editor, &loaded(60_828, 4));
        assert_eq!(editor.state.get().slots[2].path(), Some("/x/new.wav"));
        // Unloading the target while its file decodes loses nothing.
        editor.drop_files(DropTarget::Slot(1), &dropped("/x/other.wav"));
        editor.close_unload_modal(true);
        editor.state.update(|s| {
            s.unload(1);
        });
        finish_pending(&mut editor, &loaded(60_828, 4));
        assert_eq!(editor.state.get().slots[1].path(), Some("/x/other.wav"));
    }

    #[test]
    fn file_name_is_the_last_component() {
        assert_eq!(file_name("/Users/me/Breaks/think 174.wav"), "think 174.wav");
        assert_eq!(file_name("loop.aif"), "loop.aif");
    }
}
