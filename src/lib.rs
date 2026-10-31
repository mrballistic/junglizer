use truce::prelude::*;

mod decode;
pub mod edit_state;
mod file_picker;
mod library;
mod live;
mod midi;
pub mod pattern;
mod platform;
mod sample;
mod suggest;
pub mod tempo;
mod tone;
mod ui;

use edit_state::{EditState, LOOP_COUNT, MAX_STEPS, MarkerSet, SlotKind, StepPlacement};
use library::{Library, LoadOutcome, LoadedBreak, Purpose};
use live::LiveSnapshot;
use std::sync::atomic::{AtomicU32, Ordering};
use suggest::variation_for_cycle;
use tone::Tone;
use truce::params::FloatParamReadF32;

#[derive(Params)]
pub struct JunglizerParams {
    #[skip]
    pub audition: AtomicU32,
    #[skip]
    pub library: Library,
    #[skip]
    pub live_snapshot: LiveSnapshot,
    #[param(
        name = "Gain",
        range = "linear(-60, 6)",
        unit = "dB",
        smooth = "exp(5)"
    )]
    pub gain: FloatParam,
    #[param(
        name = "Thin / Thicc",
        range = "linear(-1, 1)",
        default = 0,
        smooth = "exp(10)"
    )]
    pub thin_thicc: FloatParam,
    #[param(name = "808 Root", range = "discrete(0, 11)", default = 0)]
    pub root: IntParam,
    #[param(name = "Playback", range = "discrete(0, 1)", default = 0)]
    pub playback: IntParam,
    #[param(name = "Source Slices", range = "discrete(1, 32)", default = 8)]
    pub source_slices: IntParam,
    #[param(name = "Player Slots", range = "discrete(1, 32)", default = 32)]
    pub player_slots: IntParam,
    #[param(name = "Edit Slot", range = "discrete(1, 32)", default = 1)]
    pub edit_slot: IntParam,
    #[param(name = "Fill Bar", range = "discrete(1, 4)", default = 4)]
    pub fill_bar: IntParam,
    #[param(name = "Edit Break", range = "discrete(1, 4)", default = 1)]
    pub edit_break: IntParam,
    #[param(name = "Edit Slice", range = "discrete(1, 32)", default = 1)]
    pub edit_slice: IntParam,
    #[param(name = "Edit Length", range = "discrete(1, 32)", default = 1)]
    pub edit_length: IntParam,
    #[param(name = "Edit Speed", range = "discrete(0, 1)", default = 0)]
    pub edit_speed: IntParam,
    #[param(name = "Edit Bits", range = "discrete(1, 16)", default = 16)]
    pub edit_bits: IntParam,
    #[param(name = "Edit Retriggers", range = "discrete(0, 3)", default = 0)]
    pub edit_retriggers: IntParam,
    #[param(name = "Edit Lock")]
    pub edit_lock: BoolParam,
    #[param(name = "Bass Hit")]
    pub bass_hit: BoolParam,
    #[param(name = "Slice Muted")]
    pub slice_muted: BoolParam,
    #[param(name = "Marker Position", range = "linear(0, 1)", default = 0)]
    pub marker_position: FloatParam,
    #[param(name = "Live Variations")]
    pub live_variations: BoolParam,
    #[param(name = "Place Slice")]
    pub place_slice: BoolParam,
    #[param(name = "Clear Slice")]
    pub clear_slice: BoolParam,
    #[param(name = "Suggest Fill")]
    pub suggest_fill: BoolParam,
    #[param(name = "Lock Bar")]
    pub lock_bar: BoolParam,
    #[param(name = "Freeze Variation")]
    pub freeze_variation: BoolParam,
    #[param(name = "Unfreeze")]
    pub unfreeze: BoolParam,
    #[param(name = "Unlock All Slices")]
    pub unlock_all_slices: BoolParam,
    #[param(name = "Bar 1", range = "discrete(1, 4)", default = 1)]
    pub bar_1: IntParam,
    #[param(name = "Bar 2", range = "discrete(1, 4)", default = 2)]
    pub bar_2: IntParam,
    #[param(name = "Bar 3", range = "discrete(1, 4)", default = 3)]
    pub bar_3: IntParam,
    #[param(name = "Bar 4", range = "discrete(1, 4)", default = 4)]
    pub bar_4: IntParam,
}

#[derive(Default)]
pub struct Junglizer {
    sources: [Option<Arc<LoadedBreak>>; LOOP_COUNT],
    edit: EditState,
    live_pattern: EditState,
    live_version: u64,
    live_cycle: i64,
    live_bars: [usize; 4],
    loop_wraps: i64,
    last_transport_beat: Option<f64>,
    tone: Tone,
    midi: MidiPlayback,
    controls: Option<ControlValues>,
    audition: Option<Audition>,
    /// Retired audio the library's queue had no room for. Handed over on a
    /// later block; never dropped on the audio thread.
    overflow: [Option<Arc<LoadedBreak>>; 8],
    /// The path hash of the last `Restore` posted per slot, so a slot asks
    /// once per path per project load.
    requested: [u64; LOOP_COUNT],
    /// The built-in breaks' suggested marker sets for every slice count,
    /// computed once in `load_sources`.
    builtin_markers: [[MarkerSet; MAX_STEPS]; LOOP_COUNT],
}

#[derive(Clone, Copy)]
struct ControlValues {
    source_slices: u8,
    player_slots: u8,
    slot: u8,
    fill_bar: u8,
    source_loop: u8,
    source_slice: u8,
    length: u8,
    mode: u8,
    bits: u8,
    repeats: u8,
    locked: bool,
    bass: bool,
    muted: bool,
    marker_position: f32,
    live: bool,
    place: bool,
    clear: bool,
    suggest: bool,
    lock_bar: bool,
    freeze: bool,
    unfreeze: bool,
    unlock_all: bool,
}

impl ControlValues {
    fn read(params: &JunglizerParams) -> Self {
        Self {
            source_slices: params.source_slices.value() as u8,
            player_slots: params.player_slots.value() as u8,
            slot: (params.edit_slot.value() - 1) as u8,
            fill_bar: (params.fill_bar.value() - 1) as u8,
            source_loop: (params.edit_break.value() - 1) as u8,
            source_slice: (params.edit_slice.value() - 1) as u8,
            length: params.edit_length.value() as u8,
            mode: params.edit_speed.value() as u8,
            bits: params.edit_bits.value() as u8,
            repeats: 1 << params.edit_retriggers.value(),
            locked: params.edit_lock.value(),
            bass: params.bass_hit.value(),
            muted: params.slice_muted.value(),
            marker_position: params.marker_position.value(),
            live: params.live_variations.value(),
            place: params.place_slice.value(),
            clear: params.clear_slice.value(),
            suggest: params.suggest_fill.value(),
            lock_bar: params.lock_bar.value(),
            freeze: params.freeze_variation.value(),
            unfreeze: params.unfreeze.value(),
            unlock_all: params.unlock_all_slices.value(),
        }
    }
}

#[derive(Clone, Copy, Default)]
struct MidiVoice {
    note: u8,
    placement: StepPlacement,
    elapsed_beats: f64,
}

struct MidiPlayback {
    bars: [usize; 4],
    player_slots: u8,
    mode: u8,
    bit_depth: u8,
    repeats: u8,
    span_slots: u8,
    voice: Option<MidiVoice>,
    thin_override: Option<f32>,
    root_override: Option<u8>,
}

impl Default for MidiPlayback {
    fn default() -> Self {
        Self {
            bars: [0, 1, 2, 3],
            player_slots: 32,
            mode: 0,
            bit_depth: 16,
            repeats: 1,
            span_slots: 1,
            voice: None,
            thin_override: None,
            root_override: None,
        }
    }
}

impl MidiPlayback {
    fn handle(&mut self, body: &EventBody, beat: f64, pattern: &EditState) -> bool {
        match *body {
            EventBody::ControlChange {
                channel: 0,
                cc,
                value,
                ..
            } => match cc {
                20 => self.mode = value.min(1),
                21 => self.bit_depth = value.clamp(1, 16),
                22 => {
                    self.repeats = if [1, 2, 4, 8].contains(&value) {
                        value
                    } else {
                        1
                    }
                }
                23 => self.span_slots = value.clamp(1, 32),
                24 => self.player_slots = value.clamp(1, 32),
                25 => self.thin_override = Some(f32::from(value) / 63.5 - 1.0),
                26 => self.root_override = Some(value.min(11)),
                30 => {
                    self.bars[((beat.rem_euclid(16.0) / 4.0) as usize).min(3)] =
                        value.min(3) as usize
                }
                _ => {}
            },
            EventBody::NoteOn {
                channel: 0,
                note,
                velocity,
                ..
            } if velocity > 0 => {
                let source_loop = note / 32;
                let source_slice = note % 32;
                let Some(slice) = pattern.slice(source_loop as usize, source_slice as usize) else {
                    return false;
                };
                self.voice = Some(MidiVoice {
                    note,
                    placement: StepPlacement {
                        active: true,
                        start_slot: 0,
                        span_slots: self.span_slots.min(self.player_slots),
                        source_loop,
                        source_slice,
                        mode: self.mode,
                        bit_depth: self.bit_depth,
                        repeats: self.repeats,
                        locked: false,
                    },
                    elapsed_beats: 0.0,
                });
                return slice.bass;
            }
            EventBody::NoteOff {
                channel: 0, note, ..
            } => {
                if self.voice.is_some_and(|voice| voice.note == note) {
                    self.voice = None;
                }
            }
            _ => {}
        }
        false
    }
}

#[derive(Clone, Copy, Default)]
struct Audition {
    source_loop: usize,
    position: f64,
    end: u32,
}

#[cfg(test)]
const SOURCE_FILES: [&str; 4] = [
    "174_JunglizerAmen_Bar_01.wav",
    "174_JunglizerAmen_Bar_02.wav",
    "174_JunglizerAmen_Bar_03.wav",
    "174_JunglizerAmen_Bar_04.wav",
];
pub(crate) const SOURCE_AUDIO: [&[u8]; 4] = [
    include_bytes!("../sample/174_JunglizerAmen_Bar_01.wav"),
    include_bytes!("../sample/174_JunglizerAmen_Bar_02.wav"),
    include_bytes!("../sample/174_JunglizerAmen_Bar_03.wav"),
    include_bytes!("../sample/174_JunglizerAmen_Bar_04.wav"),
];

impl Junglizer {
    fn load_sources() -> Self {
        let builtin = Library::builtin();
        let sources: [Option<Arc<LoadedBreak>>; LOOP_COUNT] =
            std::array::from_fn(|index| Some(builtin[index].clone()));
        let builtin_markers: [[MarkerSet; MAX_STEPS]; LOOP_COUNT] = std::array::from_fn(|index| {
            let source = &builtin[index];
            if source.sample.frames.is_empty() {
                return [MarkerSet::default(); MAX_STEPS];
            }
            std::array::from_fn(|count_index| {
                let count = count_index as u8 + 1;
                MarkerSet {
                    valid: true,
                    slices: if count == 8 {
                        source.suggested
                    } else {
                        source.sample.suggested_slices(count)
                    },
                }
            })
        });
        let mut edit = EditState::default();
        edit.markers = builtin_markers;
        edit.touch();
        let live_pattern = edit.clone();
        Self {
            sources,
            edit,
            live_pattern,
            live_version: 0,
            live_cycle: 0,
            live_bars: [0, 1, 2, 3],
            loop_wraps: 0,
            last_transport_beat: None,
            tone: Tone::default(),
            midi: MidiPlayback::default(),
            controls: None,
            audition: None,
            overflow: Default::default(),
            requested: [0; LOOP_COUNT],
            builtin_markers,
        }
    }

    /// Makes each slot hold exactly its own audio: the shared built-in for a
    /// built-in slot, the newest loaded file whose path matches for a user
    /// slot, and nothing for a blank one. Runs first in every block, so it must
    /// not allocate, free or block: new audio comes from the library with
    /// `try_lock`, and old audio goes back to it to be freed elsewhere.
    fn reconcile_sources(&mut self, library: &Library) {
        self.flush_overflow(library);
        for slot in 0..LOOP_COUNT {
            match self.edit.slots[slot].kind {
                SlotKind::Builtin => {
                    let builtin = &Library::builtin()[slot];
                    if !self.sources[slot]
                        .as_ref()
                        .is_some_and(|held| Arc::ptr_eq(held, builtin))
                    {
                        self.hold(slot, Some(builtin.clone()), library);
                    }
                }
                SlotKind::Blank => self.hold(slot, None, library),
                SlotKind::User => {
                    let hash = self.edit.slots[slot].path_hash();
                    let held = self.sources[slot]
                        .as_ref()
                        .filter(|held| held.path_hash == hash)
                        .map(|held| held.seq);
                    if let Some(held) = held {
                        // A newer decode of the same path (a re-bounced file
                        // loaded again) replaces the one playing.
                        if let Some(LoadOutcome::Loaded(audio)) =
                            library.try_adopt(slot, hash, held)
                        {
                            self.hold(slot, Some(audio), library);
                        }
                        continue;
                    }
                    // Never play another file's audio while this one loads.
                    self.hold(slot, None, library);
                    match library.try_adopt(slot, hash, 0) {
                        Some(LoadOutcome::Loaded(audio)) => self.sources[slot] = Some(audio),
                        // Missing, blocked, unreadable or refused: stay
                        // silent so the bar falls back, and keep the markers
                        // for Relink. Blocked is final, even across reloads:
                        // the same path is refused the same way.
                        // A Missing from before the last project load is
                        // stale, though: the drive may be back, so ask again
                        // (`load_state` clears `requested`).
                        Some(LoadOutcome::Missing) | None if self.requested[slot] != hash => {
                            let Some(path) = self.edit.slots[slot].path() else {
                                continue;
                            };
                            if library
                                .request(slot, path.as_bytes(), Purpose::Restore)
                                .is_some()
                            {
                                self.requested[slot] = hash;
                            }
                        }
                        Some(_) | None => {}
                    }
                }
            }
        }
    }

    /// Replaces what `slot` holds, retiring the old audio.
    fn hold(&mut self, slot: usize, audio: Option<Arc<LoadedBreak>>, library: &Library) {
        if let Some(old) = std::mem::replace(&mut self.sources[slot], audio) {
            self.retire(old, library);
        }
    }

    fn retire(&mut self, old: Arc<LoadedBreak>, library: &Library) {
        let Err(old) = library.retire(old) else {
            return;
        };
        if let Some(free) = self.overflow.iter_mut().find(|entry| entry.is_none()) {
            *free = Some(old);
        } else {
            // Both the queue and the overflow are full, which needs dozens of
            // swaps before the loader runs once. Leaking beats freeing here.
            std::mem::forget(old);
        }
    }

    fn flush_overflow(&mut self, library: &Library) {
        for entry in &mut self.overflow {
            if let Some(old) = entry.take() {
                if let Err(old) = library.retire(old) {
                    *entry = Some(old);
                    return;
                }
            }
        }
    }

    /// The one phrase position in the bar starting at `bar_start` where a
    /// ghost break `beats` long plays `source_beat` (0 ≤ `source_beat` <
    /// `beats`). Breaks are at least 4 beats, so a bar holds at most one.
    fn bar_onset(source_beat: f64, beats: f64, bar_start: f64) -> Option<f64> {
        if beats <= 0.0 {
            return None;
        }
        let onset = bar_start + (source_beat - bar_start).rem_euclid(beats);
        (onset < bar_start + 4.0 && onset < 16.0).then_some(onset)
    }

    fn sync_controls(&mut self, params: &JunglizerParams, bars: [usize; 4]) {
        let current = ControlValues::read(params);
        let previous = self.controls.replace(current);
        if previous.is_some_and(|before| current.source_slices != before.source_slices)
            && !self.edit.frozen
        {
            let loop_index = current.source_loop as usize;
            self.edit.set_slice_count(loop_index, current.source_slices);
            let frames = self.source_frames(loop_index);
            self.edit.fill_even_markers(loop_index, frames);
        }
        if previous.is_some_and(|before| current.player_slots != before.player_slots)
            && !self.edit.frozen
        {
            self.edit.set_player_slots(current.player_slots);
        }
        if previous.is_some_and(|before| current.live != before.live) && !self.edit.frozen {
            self.edit.live_enabled = current.live;
            self.edit.touch();
        }
        let Some(before) = previous else { return };
        if current.bass != before.bass && !self.edit.frozen {
            let source_count = self.edit.slice_count(current.source_loop as usize);
            if let Some(set) = self.edit.marker_set_mut(current.source_loop as usize) {
                if current.source_slice < source_count {
                    set.slices[current.source_slice as usize].bass = current.bass;
                    self.edit.touch();
                }
            }
        }
        if current.muted != before.muted && !self.edit.frozen {
            let source_count = self.edit.slice_count(current.source_loop as usize);
            if let Some(set) = self.edit.marker_set_mut(current.source_loop as usize)
                && current.source_slice < source_count
            {
                set.slices[current.source_slice as usize].active = !current.muted;
                self.edit.touch();
            }
        }
        if current.marker_position != before.marker_position
            && !self.edit.frozen
            && current.source_slice > 0
            && current.source_slice < self.edit.slice_count(current.source_loop as usize)
        {
            let loop_index = current.source_loop as usize;
            let index = current.source_slice as usize;
            let frames = self.source_frames(loop_index);
            let source_count = self.edit.slice_count(loop_index) as usize;
            if let Some(set) = self.edit.marker_set_mut(loop_index) {
                let min = set.slices[index - 1].start.saturating_add(1);
                let max = if index + 1 < source_count {
                    set.slices[index + 1].start.saturating_sub(1)
                } else {
                    frames.saturating_sub(1)
                };
                if min <= max {
                    let frame = (current.marker_position.clamp(0.0, 1.0) * frames as f32) as u32;
                    set.slices[index].start = frame.clamp(min, max);
                    set.slices[index - 1].end = set.slices[index].start;
                    self.edit.touch();
                }
            }
        }
        let edit_changed = current.length != before.length
            || current.mode != before.mode
            || current.bits != before.bits
            || current.repeats != before.repeats
            || current.locked != before.locked;
        if edit_changed && !self.edit.frozen && !self.edit.live_enabled {
            let old = self
                .edit
                .placements
                .iter()
                .find(|item| {
                    item.active
                        && item.start_slot <= current.slot
                        && current.slot < item.start_slot + item.span_slots
                })
                .copied();
            if let Some(old) = old {
                let mut updated = old;
                if current.length != before.length {
                    updated.span_slots = current.length;
                }
                if current.mode != before.mode {
                    updated.mode = current.mode;
                }
                if current.bits != before.bits {
                    updated.bit_depth = current.bits;
                }
                if current.repeats != before.repeats {
                    updated.repeats = current.repeats;
                }
                if current.locked != before.locked {
                    updated.locked = current.locked;
                }
                self.edit.replace(old, updated);
            }
        }
        if current.clear && !before.clear && !self.edit.frozen && !self.edit.live_enabled {
            self.edit.remove_at_slot(current.slot);
        }
        if current.place && !before.place && !self.edit.frozen && !self.edit.live_enabled {
            self.edit.place(StepPlacement {
                active: true,
                start_slot: current.slot,
                span_slots: current
                    .length
                    .min(self.edit.player_slots.saturating_sub(current.slot)),
                source_loop: current.source_loop,
                source_slice: current.source_slice,
                mode: current.mode,
                bit_depth: current.bits,
                repeats: current.repeats,
                locked: current.locked,
            });
        }
        if current.suggest && !before.suggest && !self.edit.frozen && !self.edit.live_enabled {
            suggest::suggest_bar(
                &mut self.edit,
                current.fill_bar as usize,
                current.source_loop as usize,
            );
        }
        if current.lock_bar && !before.lock_bar && !self.edit.frozen {
            let bar = current.fill_bar as usize;
            let start = (bar * self.edit.player_slots as usize + 3) / 4;
            let end = ((bar + 1) * self.edit.player_slots as usize + 3) / 4;
            for item in &mut self.edit.placements {
                if item.active
                    && (item.start_slot as usize) < end
                    && start < item.start_slot as usize + item.span_slots as usize
                {
                    item.locked = true;
                }
            }
            self.edit.touch();
        }
        if current.freeze && !before.freeze {
            let bars = if self.edit.live_enabled {
                self.live_bars.map(|bar| bar as u8)
            } else {
                bars.map(|bar| bar as u8)
            };
            let placements = if self.edit.live_enabled {
                self.live_pattern.placements
            } else {
                self.edit.placements
            };
            self.edit.freeze_phrase(
                placements,
                self.edit.slice_counts(),
                self.edit.player_slots,
                bars,
            );
        }
        if current.unfreeze && !before.unfreeze {
            self.edit.frozen = false;
            self.edit.touch();
        }
        if current.unlock_all && !before.unlock_all && !self.edit.frozen {
            self.edit.unlock_all();
        }
    }

    /// Frame count of a slot's break: the loaded audio's, or the linked
    /// file's while its audio is missing.
    fn source_frames(&self, slot: usize) -> u32 {
        match self.sources.get(slot).and_then(Option::as_deref) {
            Some(source) if !source.sample.frames.is_empty() => source.sample.frames.len() as u32,
            _ => self.edit.slots.get(slot).map_or(0, |entry| entry.frames),
        }
    }

    /// The audio of a slot that can sound: loaded, non-empty and not blank.
    fn playable_source(&self, pattern: &EditState, slot: usize) -> Option<&LoadedBreak> {
        let entry = pattern.slots.get(slot)?;
        if entry.kind == SlotKind::Blank || entry.beats == 0 {
            return None;
        }
        self.sources
            .get(slot)?
            .as_deref()
            .filter(|source| !source.sample.frames.is_empty())
    }

    fn playable(&self, pattern: &EditState, slot: usize) -> bool {
        self.playable_source(pattern, slot).is_some()
    }

    /// The slot a bar's ghost plays: the bar's own slot if it can sound,
    /// otherwise the lowest slot that can.
    fn ghost_source(&self, pattern: &EditState, bar_slot: usize) -> Option<usize> {
        if self.playable(pattern, bar_slot) {
            return Some(bar_slot);
        }
        (0..LOOP_COUNT).find(|&slot| self.playable(pattern, slot))
    }

    /// Phrase positions (0..16) where a ghost break plays `source_beat`: once
    /// per repeat of a break `beats` long. The reference `bar_onset` is
    /// checked against.
    #[cfg(test)]
    fn ghost_onsets(source_beat: f64, beats: f64) -> impl Iterator<Item = f64> {
        // Breaks are at least 4 beats, so 16 repeats always cover the phrase.
        (0..16_u32)
            .map(move |repeat| source_beat + f64::from(repeat) * beats)
            .take_while(move |&onset| beats > 0.0 && (0.0..16.0).contains(&onset))
    }

    /// The ghost loop at phrase beat `beat` (0..16). A break repeats every
    /// `beats` beats from the top of the phrase.
    fn ghost_frame(&self, pattern: &EditState, beat: f64, bar_loops: &[usize; 4]) -> [f32; 2] {
        let bar = ((beat / 4.0) as usize).min(3);
        let Some(slot) = self.ghost_source(pattern, bar_loops[bar].min(3)) else {
            return [0.0; 2];
        };
        let Some(source) = self.playable_source(pattern, slot) else {
            return [0.0; 2];
        };
        let beats = f64::from(pattern.slots[slot].beats);
        let source_beat = beat.rem_euclid(beats);
        [0, 1].map(|ch| source.sample.at_beat(source_beat, beats, ch))
    }

    fn slice_frame(
        &self,
        pattern: &EditState,
        placement: &StepPlacement,
        elapsed_beats: f64,
        tempo: f64,
        player_slots: u8,
    ) -> [f32; 2] {
        let Some(source) = self.playable_source(pattern, placement.source_loop as usize) else {
            return [0.0; 2];
        };
        let source = &source.sample;
        let Some(slice) = pattern.slice(
            placement.source_loop as usize,
            placement.source_slice as usize,
        ) else {
            return [0.0; 2];
        };
        if slice.end <= slice.start || slice.end as usize > source.frames.len() {
            return [0.0; 2];
        }
        let duration = 16.0 * f64::from(placement.span_slots) / f64::from(player_slots);
        let repeat_duration = duration / f64::from(placement.repeats);
        let beat_in_repeat = elapsed_beats.rem_euclid(repeat_duration);
        let position = if placement.mode == 1 {
            f64::from(slice.start)
                + beat_in_repeat / repeat_duration * f64::from(slice.end - slice.start)
        } else {
            f64::from(slice.start) + beat_in_repeat * f64::from(source.sample_rate) * 60.0 / tempo
        };
        if position >= f64::from(slice.end) {
            return [0.0; 2];
        }
        let levels = (1_u32 << (placement.bit_depth - 1)) as f32;
        [0, 1].map(|ch| {
            let value = source.at_frame(position, ch);
            if placement.bit_depth == 16 {
                value
            } else {
                (value * levels).round() / levels
            }
        })
    }

    fn frame_at_beat(
        &self,
        pattern: &EditState,
        beat: f64,
        tempo: f64,
        bar_loops: &[usize; 4],
    ) -> [f32; 2] {
        let ghost = self.ghost_frame(pattern, beat, bar_loops);
        let Some(placement) = pattern.placement_at(beat) else {
            return ghost;
        };
        let start = pattern.slot_start(placement.start_slot);
        let placed = self.slice_frame(
            pattern,
            placement,
            beat - start,
            tempo,
            pattern.player_slots,
        );
        let end = 16.0 * f64::from(placement.start_slot + placement.span_slots)
            / f64::from(pattern.player_slots);
        let fade_beats = (tempo / 60.0 * 0.0005).min((end - start) / 8.0);
        let mix = ((beat - start) / fade_beats)
            .min((end - beat) / fade_beats)
            .clamp(0.0, 1.0) as f32;
        [
            ghost[0] * (1.0 - mix) + placed[0] * mix,
            ghost[1] * (1.0 - mix) + placed[1] * mix,
        ]
    }

    fn midi_frame(&self, beat: f64, tempo: f64) -> [f32; 2] {
        let ghost = self.ghost_frame(&self.edit, beat, &self.midi.bars);
        let Some(voice) = self.midi.voice else {
            return ghost;
        };
        let duration =
            16.0 * f64::from(voice.placement.span_slots) / f64::from(self.midi.player_slots);
        if voice.elapsed_beats >= duration {
            return ghost;
        }
        let placed = self.slice_frame(
            &self.edit,
            &voice.placement,
            voice.elapsed_beats,
            tempo,
            self.midi.player_slots,
        );
        let fade = (tempo / 60.0 * 0.0005).min(duration / 8.0);
        let mix = (voice.elapsed_beats / fade)
            .min((duration - voice.elapsed_beats) / fade)
            .clamp(0.0, 1.0) as f32;
        [
            ghost[0] * (1.0 - mix) + placed[0] * mix,
            ghost[1] * (1.0 - mix) + placed[1] * mix,
        ]
    }

    fn bass_onset_between(
        &self,
        pattern: &EditState,
        bars: &[usize; 4],
        previous: f64,
        current: f64,
        include_placements: bool,
    ) -> bool {
        let crosses = |onset: f64| {
            if previous <= current {
                previous < onset && onset <= current
            } else {
                onset > previous || onset <= current
            }
        };
        if include_placements {
            for item in pattern.placements.iter().filter(|item| item.active) {
                if !self.playable(pattern, item.source_loop as usize)
                    || !pattern
                        .slice(item.source_loop as usize, item.source_slice as usize)
                        .is_some_and(|slice| slice.bass)
                {
                    continue;
                }
                let start = pattern.slot_start(item.start_slot);
                let duration = 16.0 * f64::from(item.span_slots) / f64::from(pattern.player_slots);
                for repeat in 0..item.repeats {
                    if crosses(start + f64::from(repeat) * duration / f64::from(item.repeats)) {
                        return true;
                    }
                }
            }
        }
        for bar in 0..4 {
            let Some(source_index) = self.ghost_source(pattern, bars[bar].min(3)) else {
                continue;
            };
            let frame_count = self
                .playable_source(pattern, source_index)
                .map_or(0, |source| source.sample.frames.len());
            if frame_count == 0 {
                continue;
            }
            let beats = f64::from(pattern.slots[source_index].beats);
            let bar_start = bar as f64 * 4.0;
            if let Some(set) = pattern.marker_set(source_index) {
                for marker in set.slices[..pattern.slice_count(source_index) as usize]
                    .iter()
                    .filter(|marker| marker.active && marker.bass)
                {
                    let source_beat = beats * f64::from(marker.start) / frame_count as f64;
                    if let Some(onset) = Self::bar_onset(source_beat, beats, bar_start)
                        && crosses(onset)
                        && (!include_placements || pattern.placement_at(onset).is_none())
                        && (include_placements || self.midi.voice.is_none())
                    {
                        return true;
                    }
                }
            }
        }
        false
    }
}

impl PluginLogic for Junglizer {
    type Params = JunglizerParams;
    type DspState = Self;

    fn bus_layouts() -> Vec<BusLayout> {
        vec![BusLayout::new().with_output("Main", ChannelConfig::Stereo)]
    }

    fn init(params: &Self::Params, _cx: &InitContext) -> Self::DspState {
        // Decodes the built-ins here, so the audio thread only reads them.
        let state = Self::load_sources();
        params.library.start();
        state
    }

    fn process(
        state: &mut Self::DspState,
        params: &Self::Params,
        buffer: &mut AudioBuffer,
        events: &EventList,
        context: &mut ProcessContext,
    ) -> ProcessStatus {
        let selected_bars = [
            params.bar_1.value() as usize - 1,
            params.bar_2.value() as usize - 1,
            params.bar_3.value() as usize - 1,
            params.bar_4.value() as usize - 1,
        ];
        state.reconcile_sources(&params.library);
        state.sync_controls(params, selected_bars);
        let bar_loops = if state.edit.frozen {
            state.edit.frozen_bars.map(usize::from)
        } else {
            selected_bars
        };
        let midi_mode = params.playback.value() != 0;
        let transport = context.transport;
        let beat_step = transport.tempo / (60.0 * context.sample_rate);
        let mut event_iter = events.iter().peekable();
        let request = params.audition.swap(0, Ordering::Relaxed);
        if request > 0 {
            let index = request as usize - 1;
            let source_loop = index / 32;
            let slice_index = index % 32;
            state.audition = state
                .edit
                .slice(source_loop, slice_index)
                .map(|slice| Audition {
                    source_loop,
                    position: slice.start as f64,
                    end: slice.end,
                });
        }
        for i in 0..buffer.num_samples() {
            let mut frame = [0.0_f32; 2];
            let running = transport.playing && beat_step.is_finite() && beat_step > 0.0;
            let host_beat = transport.position_beats + i as f64 * beat_step;
            let beat = host_beat.rem_euclid(16.0);
            let previous = state
                .last_transport_beat
                .map(|value| value.rem_euclid(16.0))
                .unwrap_or_else(|| (beat - beat_step).rem_euclid(16.0));
            while event_iter
                .peek()
                .is_some_and(|event| event.sample_offset as usize <= i)
            {
                let event = event_iter.next().expect("peeked event");
                if midi_mode && state.midi.handle(&event.body, beat, &state.edit) {
                    state.tone.trigger(
                        state
                            .midi
                            .root_override
                            .unwrap_or(params.root.value() as u8),
                    );
                }
            }
            if running {
                if state
                    .last_transport_beat
                    .is_some_and(|last| host_beat + 0.5 < last)
                {
                    state.loop_wraps = state.loop_wraps.wrapping_add(1);
                }
                let cycle = (host_beat / 16.0).floor() as i64 + state.loop_wraps;
                if !midi_mode
                    && state.edit.live_enabled
                    && (state.live_version != state.edit.version
                        || state.live_cycle != cycle
                        || state.live_bars != bar_loops)
                {
                    state.live_pattern = variation_for_cycle(&state.edit, cycle, bar_loops);
                    state.live_version = state.edit.version;
                    state.live_cycle = cycle;
                    state.live_bars = bar_loops;
                    params
                        .live_snapshot
                        .publish(&state.live_pattern, cycle, bar_loops);
                }
                if midi_mode {
                    frame = state.midi_frame(beat, transport.tempo);
                    if state.bass_onset_between(
                        &state.edit,
                        &state.midi.bars,
                        previous,
                        beat,
                        false,
                    ) {
                        state.tone.trigger(
                            state
                                .midi
                                .root_override
                                .unwrap_or(params.root.value() as u8),
                        );
                    }
                } else {
                    let pattern = if state.edit.live_enabled {
                        &state.live_pattern
                    } else {
                        &state.edit
                    };
                    frame = state.frame_at_beat(pattern, beat, transport.tempo, &bar_loops);
                    if state.bass_onset_between(pattern, &bar_loops, previous, beat, true) {
                        state.tone.trigger(params.root.value() as u8);
                    }
                }
                state.last_transport_beat = Some(host_beat);
            } else {
                state.last_transport_beat = None;
                state.loop_wraps = 0;
            }
            if let Some(voice) = &mut state.midi.voice {
                voice.elapsed_beats += if beat_step.is_finite() && beat_step > 0.0 {
                    beat_step
                } else {
                    174.0 / (60.0 * context.sample_rate)
                };
            }
            if let Some(mut preview) = state.audition {
                if let Some(source) = state.playable_source(&state.edit, preview.source_loop) {
                    let source = &source.sample;
                    if preview.position < preview.end as f64 {
                        frame[0] += source.at_frame(preview.position, 0);
                        frame[1] += source.at_frame(preview.position, 1);
                        preview.position += f64::from(source.sample_rate) / context.sample_rate;
                        state.audition = Some(preview);
                    } else {
                        state.audition = None;
                    }
                } else {
                    state.audition = None;
                }
            }
            let thin_thicc = if midi_mode {
                state.midi.thin_override.unwrap_or(params.thin_thicc.read())
            } else {
                params.thin_thicc.read()
            };
            frame = state.tone.process(frame, thin_thicc, context.sample_rate);
            let gain = 10.0_f32.powf(params.gain.read() / 20.0);
            for (ch, sample) in frame
                .iter()
                .enumerate()
                .take(buffer.num_output_channels().min(2))
            {
                buffer.output(ch)[i] = sample * gain;
            }
        }
        ProcessStatus::Normal
    }

    fn snapshot_into(state: &Self::DspState, buf: &mut Vec<u8>) -> bool {
        state.edit.serialize_into(buf);
        true
    }

    fn snapshot_version(state: &Self::DspState) -> Option<u64> {
        Some(state.edit.version)
    }

    fn snapshot_prealloc_hint() -> usize {
        edit_state::SERIALIZED_LEN
    }

    fn load_state(state: &mut Self::DspState, data: &[u8]) -> Result<(), StateLoadError> {
        let Some(mut edit) = EditState::deserialize(data) else {
            return Err(StateLoadError::Malformed("Junglizer pattern"));
        };
        for source in 0..LOOP_COUNT {
            // User and blank slots keep exactly what the project saved.
            if edit.slots[source].kind != SlotKind::Builtin {
                continue;
            }
            for count in 0..MAX_STEPS {
                if !edit.markers[source][count].valid {
                    edit.markers[source][count] = state.builtin_markers[source][count];
                }
            }
        }
        // A saved placement on a blank slot, or past its slot's slice count,
        // can't sound; switch it off rather than leave it dangling.
        let counts = edit.slice_counts();
        let blank = edit
            .slots
            .each_ref()
            .map(|slot| slot.kind == SlotKind::Blank);
        for item in &mut edit.placements {
            let source = item.source_loop as usize;
            if item.active
                && (blank.get(source).is_none_or(|&blank| blank)
                    || counts
                        .get(source)
                        .is_none_or(|&count| item.source_slice >= count))
            {
                item.active = false;
            }
        }
        state.edit = edit;
        // Ask the library again for every user break, so a file that was
        // missing before this load gets another try.
        state.requested = [0; LOOP_COUNT];
        state.live_version = 0;
        state.midi.player_slots = state.edit.player_slots;
        state.controls = None;
        Ok(())
    }

    fn editor(params: Arc<JunglizerParams>) -> Box<dyn Editor> {
        truce_egui::EguiEditor::with_ui(params, (1100, 720), ui::JunglizerUi::new())
            .with_visuals(ui::visuals())
            .into_editor()
    }
}

truce::plugin! {
    logic: Junglizer,
    params: JunglizerParams,
}

// Installs the real-time allocation checker under `--features rt-paranoid`
// (a no-op otherwise). Wrap a driver run in `assert_no_audio_alloc` to
// fail a test if `process` ever allocates. See the audio-testing guide.
truce::enable_rt_paranoid!();

#[cfg(test)]
mod audio_tests {
    use super::*;
    use crate::edit_state::SliceMarker;
    use crate::sample::StereoSample;

    fn ramp(frames: usize, rate: u32) -> Option<Arc<LoadedBreak>> {
        let s = StereoSample {
            frames: (0..frames).map(|i| [i as f32 / frames as f32; 2]).collect(),
            sample_rate: rate,
        };
        Some(Arc::new(LoadedBreak::new(s, 1, tempo::reading_for(4, 1.0))))
    }

    #[test]
    fn eight_beat_break_ghost_plays_twice_across_the_phrase() {
        let mut st = Junglizer::default();
        st.sources[0] = ramp(8_000, 44_100);
        st.edit
            .load_user(
                0,
                "/a.wav",
                8_000,
                8,
                &[SliceMarker {
                    start: 0,
                    end: 8_000,
                    active: true,
                    bass: false,
                }; 8],
            )
            .unwrap();
        let bars = [0; 4];
        let at = |b| st.frame_at_beat(&st.edit, b, 174.0, &bars)[0];
        assert!((at(1.0) - at(9.0)).abs() < 1e-6);
        assert!((at(5.0) - 0.625).abs() < 0.01);
    }

    #[test]
    fn thirty_two_beat_break_plays_its_first_sixteen_beats() {
        let mut st = Junglizer::default();
        st.sources[0] = ramp(32_000, 44_100);
        st.edit
            .load_user(
                0,
                "/a.wav",
                32_000,
                32,
                &[SliceMarker {
                    start: 0,
                    end: 32_000,
                    active: true,
                    bass: false,
                }; 8],
            )
            .unwrap();
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
        st.edit
            .load_user(
                0,
                "/a.wav",
                96_000,
                4,
                &[SliceMarker {
                    start: 0,
                    end: 96_000,
                    active: true,
                    bass: false,
                }; 8],
            )
            .unwrap();
        assert!((st.frame_at_beat(&st.edit, 2.0, 120.0, &[0; 4])[0] - 0.5).abs() < 0.01);
    }

    #[test]
    fn original_speed_slice_uses_file_rate_not_host_rate() {
        // One beat at 120 BPM is 0.5 s: 24,000 frames of a 48 kHz file, whatever the host rate.
        let mut st = Junglizer::default();
        st.sources[0] = ramp(96_000, 48_000);
        st.edit
            .load_user(
                0,
                "/a.wav",
                96_000,
                4,
                &[SliceMarker {
                    start: 0,
                    end: 96_000,
                    active: true,
                    bass: false,
                }; 1],
            )
            .unwrap();
        let p = StepPlacement {
            active: true,
            start_slot: 0,
            span_slots: 32,
            source_loop: 0,
            source_slice: 0,
            mode: 0,
            bit_depth: 16,
            repeats: 1,
            locked: false,
        };
        assert!((st.slice_frame(&st.edit, &p, 1.0, 120.0, 32)[0] - 0.25).abs() < 0.01);
    }

    #[test]
    fn ghost_player_routes_each_bar_and_stops_with_transport() {
        // User slots whose audio matches their paths, so `process` keeps it.
        let mut edit = EditState::default();
        let mark = [SliceMarker {
            start: 0,
            end: 1,
            active: true,
            bass: false,
        }];
        for slot in 0..LOOP_COUNT {
            edit.load_user(slot, &format!("/{slot}.wav"), 1, 4, &mark)
                .unwrap();
        }
        let mut state = Junglizer {
            sources: std::array::from_fn(|index| {
                let sample = StereoSample {
                    frames: vec![[(index + 1) as f32 * 0.1; 2]],
                    sample_rate: 44_100,
                };
                Some(Arc::new(LoadedBreak::new(
                    sample,
                    edit.slots[index].path_hash(),
                    tempo::reading_for(4, 1.0),
                )))
            }),
            edit,
            ..Junglizer::default()
        };
        let params = JunglizerParams::default();
        let events = EventList::with_capacity(0);
        let mut outputs = [0.0_f32; 4];

        for bar in 0..4 {
            let transport = TransportInfo {
                playing: true,
                tempo: 174.0,
                position_beats: bar as f64 * 4.0 + 0.5,
                ..TransportInfo::default()
            };
            let mut channel_slices: [&mut [f32]; 1] = [&mut outputs[bar..bar + 1]];
            let mut buffer = AudioBuffer::from_slices_checked(&[], &mut channel_slices, 1);
            let mut output_events = EventList::with_capacity(0);
            let mut context = ProcessContext::new(&transport, 44_100.0, 1, &mut output_events);
            Junglizer::process(&mut state, &params, &mut buffer, &events, &mut context);
        }
        assert!(outputs[0] > 0.0);
        for bar in 1..4 {
            assert!((outputs[bar] / outputs[0] - (bar + 1) as f32).abs() < 0.01);
        }

        let stopped = TransportInfo::default();
        let mut quiet = [1.0_f32];
        let mut channel_slices: [&mut [f32]; 1] = [&mut quiet];
        let mut buffer = AudioBuffer::from_slices_checked(&[], &mut channel_slices, 1);
        let mut output_events = EventList::with_capacity(0);
        let mut context = ProcessContext::new(&stopped, 44_100.0, 1, &mut output_events);
        Junglizer::process(&mut state, &params, &mut buffer, &events, &mut context);
        assert_eq!(quiet[0], 0.0);
    }

    #[test]
    fn source_slices_sets_the_selected_slot_and_fills_even_markers() {
        use truce::params::Params;
        let mut state = Junglizer::load_sources();
        let frames = state.sources[1]
            .as_ref()
            .expect("built-in break")
            .sample
            .frames
            .len() as u32;
        let marks = [0_u32, 1, 2, 3].map(|i| SliceMarker {
            start: frames * i / 4,
            end: frames * (i + 1) / 4,
            active: true,
            bass: false,
        });
        state
            .edit
            .load_user(1, "/a.wav", frames, 4, &marks)
            .unwrap();
        let params = JunglizerParams::default();
        params.set_normalized(JunglizerParamsParamId::EditBreak.into(), 1.0 / 3.0);
        state.sync_controls(&params, [0, 1, 2, 3]);
        params.set_normalized(JunglizerParamsParamId::SourceSlices.into(), 11.0 / 31.0);
        state.sync_controls(&params, [0, 1, 2, 3]);
        assert_eq!(state.edit.slice_count(0), 8);
        assert_eq!(state.edit.slice_count(1), 12);
        let set = state.edit.marker_set(1).expect("marker set");
        assert!(set.valid);
        assert_eq!(set.slices[0].start, 0);
        assert_eq!(set.slices[11].end, frames);
        assert!(set.slices[..12].iter().all(|slice| slice.active));
    }

    #[test]
    fn placed_slice_overrides_ghost_then_ghost_resumes() {
        let mut state = Junglizer {
            sources: std::array::from_fn(|index| {
                let sample = StereoSample {
                    frames: vec![[if index == 0 { 0.2 } else { 0.8 }; 2]; 100],
                    sample_rate: 100,
                };
                Some(Arc::new(LoadedBreak::new(
                    sample,
                    0,
                    tempo::reading_for(4, 1.0),
                )))
            }),
            edit: EditState::default(),
            ..Junglizer::default()
        };
        state.edit.set_markers(
            1,
            &[SliceMarker {
                start: 0,
                end: 100,
                active: true,
                bass: true,
            }; 8],
        );
        assert!(state.edit.place(StepPlacement {
            active: true,
            start_slot: 0,
            span_slots: 1,
            source_loop: 1,
            source_slice: 0,
            mode: 1,
            bit_depth: 16,
            repeats: 1,
            locked: false,
        }));
        let bar_loops = [0; 4];
        assert!((state.frame_at_beat(&state.edit, 0.25, 120.0, &bar_loops)[0] - 0.8).abs() < 0.01);
        assert!((state.frame_at_beat(&state.edit, 0.75, 120.0, &bar_loops)[0] - 0.2).abs() < 0.01);
    }

    fn fixture(name: &str) -> String {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name)
            .to_str()
            .unwrap()
            .to_owned()
    }

    const WHOLE: [SliceMarker; 8] = [SliceMarker {
        start: 0,
        end: 60_828,
        active: true,
        bass: false,
    }; 8];

    #[test]
    fn missing_file_is_reported_and_dsp_plays_fallback() {
        let params = JunglizerParams::default();
        let mut st = Junglizer::load_sources();
        st.edit
            .load_user(0, "/nowhere/gone 174.wav", 60_828, 4, &WHOLE)
            .unwrap();
        st.reconcile_sources(&params.library); // posts Restore
        params.library.run_pending();
        st.reconcile_sources(&params.library); // sees Missing
        assert!(st.sources[0].is_none());
        assert_eq!(st.ghost_source(&st.edit, 0), Some(1));
        assert!(st.edit.marker_set(0).unwrap().valid); // markers kept for Relink
    }

    #[test]
    fn a_blocked_restore_holds_nothing_keeps_markers_and_is_not_re_requested() {
        let params = JunglizerParams::default();
        let lib = &params.library;
        let mut st = Junglizer::load_sources();
        st.edit
            .load_user(0, "//srv/share/amen 174.wav", 60_828, 4, &WHOLE)
            .unwrap();
        st.reconcile_sources(lib); // posts Restore
        lib.run_pending();
        st.reconcile_sources(lib); // sees Blocked
        assert!(matches!(
            lib.result(0, Purpose::Restore).unwrap().outcome,
            LoadOutcome::Blocked
        ));
        assert!(st.sources[0].is_none());
        assert_eq!(st.ghost_source(&st.edit, 0), Some(1));
        assert!(st.edit.marker_set(0).unwrap().valid);
        let seq = lib.result(0, Purpose::Restore).unwrap().seq;
        for _ in 0..3 {
            st.reconcile_sources(lib);
            lib.run_pending();
        }
        assert_eq!(
            lib.result(0, Purpose::Restore).unwrap().seq,
            seq,
            "no re-request"
        );
        assert!(st.sources[0].is_none());
    }

    #[test]
    fn a_missing_file_is_retried_after_the_project_reloads() {
        let params = JunglizerParams::default();
        let lib = &params.library;
        let mut st = Junglizer::load_sources();
        let path =
            std::env::temp_dir().join(format!("junglizer-{}-relink.wav", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let path_str = path.to_str().unwrap();
        st.edit.load_user(0, path_str, 72_000, 4, &WHOLE).unwrap();
        st.reconcile_sources(lib);
        lib.run_pending();
        st.reconcile_sources(lib);
        assert!(st.sources[0].is_none(), "missing");
        std::fs::copy(fixture("loop_160_4beats_mono16_48k.wav"), &path).unwrap();
        // Without a reload, Missing stays final: nothing is re-requested.
        st.reconcile_sources(lib);
        lib.run_pending();
        st.reconcile_sources(lib);
        assert!(st.sources[0].is_none(), "no retry without a reload");
        // Reopening the project asks again and finds the file.
        let mut blob = Vec::new();
        st.edit.serialize_into(&mut blob);
        Junglizer::load_state(&mut st, &blob).unwrap();
        st.reconcile_sources(lib);
        lib.run_pending();
        st.reconcile_sources(lib);
        std::fs::remove_file(&path).unwrap();
        let held = st.sources[0].as_ref().expect("adopted after reload");
        assert_eq!(held.path_hash, st.edit.slots[0].path_hash());
    }

    #[test]
    fn sources_only_ever_hold_their_slots_audio() {
        let params = JunglizerParams::default();
        let lib = &params.library;
        let mut st = Junglizer::default();
        // Builtin slots pick up the shared built-in audio.
        st.reconcile_sources(lib);
        for slot in 0..LOOP_COUNT {
            assert!(Arc::ptr_eq(
                st.sources[slot].as_ref().unwrap(),
                &library::Library::builtin()[slot]
            ));
        }
        // A user slot drops the built-in audio at once, before its own loads.
        let path = fixture("loop_160_4beats_mono16_48k.wav");
        st.edit.load_user(1, &path, 48_000, 4, &WHOLE).unwrap();
        st.edit.unload(2);
        st.reconcile_sources(lib);
        assert!(st.sources[1].is_none());
        assert!(st.sources[2].is_none());
        lib.run_pending();
        st.reconcile_sources(lib);
        let held = st.sources[1].as_ref().unwrap();
        assert_eq!(held.path_hash, st.edit.slots[1].path_hash());
        // Audio for another path is never kept, even for one block.
        st.sources[3] = st.sources[1].clone();
        st.reconcile_sources(lib);
        assert!(Arc::ptr_eq(
            st.sources[3].as_ref().unwrap(),
            &library::Library::builtin()[3]
        ));
        st.edit.restore_builtin();
        st.reconcile_sources(lib);
        assert!(Arc::ptr_eq(
            st.sources[1].as_ref().unwrap(),
            &library::Library::builtin()[1]
        ));
    }

    #[test]
    fn full_retire_queue_parks_old_audio_in_overflow_then_flushes() {
        let params = JunglizerParams::default();
        let lib = &params.library;
        let mut st = Junglizer::load_sources();
        let spare = library::Library::builtin()[0].clone();
        while lib.retire(spare.clone()).is_ok() {}
        st.edit.unload(0);
        st.edit.unload(1);
        st.reconcile_sources(lib);
        assert!(st.sources[0].is_none() && st.sources[1].is_none());
        assert_eq!(st.overflow.iter().filter(|x| x.is_some()).count(), 2);
        lib.collect_garbage();
        st.reconcile_sources(lib);
        assert!(st.overflow.iter().all(Option::is_none));
    }

    #[test]
    fn source_slices_fill_uses_the_adopted_audio_length() {
        use truce::params::Params;
        let params = JunglizerParams::default();
        let mut st = Junglizer::load_sources();
        let path = fixture("loop_160_4beats_mono16_48k.wav");
        // The state claims a different length than the file holds.
        st.edit.load_user(0, &path, 12_345, 4, &WHOLE).unwrap();
        st.reconcile_sources(&params.library);
        params.library.run_pending();
        st.reconcile_sources(&params.library);
        let held = st.sources[0].as_ref().unwrap().sample.frames.len() as u32;
        assert_ne!(held, 12_345);
        st.sync_controls(&params, [0, 1, 2, 3]);
        params.set_normalized(JunglizerParamsParamId::SourceSlices.into(), 3.0 / 31.0);
        st.sync_controls(&params, [0, 1, 2, 3]);
        assert_eq!(st.edit.slice_count(0), 4);
        assert_eq!(st.edit.marker_set(0).unwrap().slices[3].end, held);
    }

    #[test]
    fn load_state_merges_builtin_markers_from_the_builtin_cache() {
        let mut st = Junglizer::load_sources();
        let builtin = st.edit.markers[0];
        // The running instance holds a user break in slot 0.
        st.edit
            .load_user(0, "/user.wav", 60_828, 4, &WHOLE[..3])
            .unwrap();
        st.edit.markers[0][7] = edit_state::MarkerSet {
            valid: true,
            slices: [SliceMarker {
                start: 7,
                end: 9,
                active: true,
                bass: true,
            }; edit_state::MAX_STEPS],
        };
        // The project's slot 0 is built-in with no saved marker sets.
        let mut saved = EditState::default();
        for set in &mut saved.markers[0] {
            set.valid = false;
        }
        let mut blob = Vec::new();
        saved.serialize_into(&mut blob);
        Junglizer::load_state(&mut st, &blob).unwrap();
        assert_eq!(st.edit.markers[0], builtin);
    }

    #[test]
    fn load_state_deactivates_placements_it_left_invalid() {
        let mut st = Junglizer::load_sources();
        let mut saved = EditState::default();
        let at = |slot: u8, source_loop: u8, source_slice: u8| StepPlacement {
            active: true,
            start_slot: slot,
            span_slots: 1,
            source_loop,
            source_slice,
            mode: 0,
            bit_depth: 16,
            repeats: 1,
            locked: false,
        };
        saved.placements[0] = at(0, 0, 2);
        saved.placements[1] = at(1, 1, 7);
        saved.placements[2] = at(2, 2, 3);
        saved.slots[1].slice_count = 4; // slice 7 is past the count
        saved.slots[2].kind = SlotKind::Blank;
        let mut blob = Vec::new();
        saved.serialize_into(&mut blob);
        Junglizer::load_state(&mut st, &blob).unwrap();
        let active = st.edit.placements.map(|p| p.active);
        assert_eq!(&active[..3], &[true, false, false]);
    }

    fn eight_beat_bass_on_beat_one(st: &mut Junglizer, slot: usize) {
        let marks = [
            SliceMarker {
                start: 0,
                end: 1_000,
                active: true,
                bass: false,
            },
            SliceMarker {
                start: 1_000,
                end: 8_000,
                active: true,
                bass: true,
            },
        ];
        st.sources[slot] = ramp(8_000, 44_100);
        st.edit.load_user(slot, "/a.wav", 8_000, 8, &marks).unwrap();
    }

    #[test]
    fn bass_onsets_of_an_eight_beat_break_fire_once_per_repeat() {
        let mut st = Junglizer::default();
        eight_beat_bass_on_beat_one(&mut st, 0);
        let fires =
            |st: &Junglizer, b: f64| st.bass_onset_between(&st.edit, &[0; 4], b - 0.1, b, true);
        assert!(fires(&st, 1.0));
        assert!(fires(&st, 9.0));
        assert!(!fires(&st, 5.0));
        assert!(!fires(&st, 13.0));
    }

    #[test]
    fn bass_onsets_follow_the_fallback_slot() {
        let mut st = Junglizer::default();
        st.edit.unload(0);
        eight_beat_bass_on_beat_one(&mut st, 1);
        let fires =
            |st: &Junglizer, b: f64| st.bass_onset_between(&st.edit, &[0; 4], b - 0.1, b, true);
        assert!(fires(&st, 1.0));
        assert!(fires(&st, 9.0));
        assert!(!fires(&st, 5.0));
    }

    #[test]
    fn bar_onset_matches_walking_every_repeat() {
        for beats in [4.0, 8.0, 12.0, 16.0, 20.0, 32.0] {
            for step in 0..(beats as i32 * 8) {
                let source_beat = f64::from(step) / 8.0;
                for bar in 0..4 {
                    let bar_start = bar as f64 * 4.0;
                    let walked: Vec<f64> = Junglizer::ghost_onsets(source_beat, beats)
                        .filter(|&o| (bar_start..bar_start + 4.0).contains(&o))
                        .collect();
                    let direct = Junglizer::bar_onset(source_beat, beats, bar_start);
                    assert_eq!(walked, direct.into_iter().collect::<Vec<_>>());
                }
            }
        }
    }

    fn run_block(st: &mut Junglizer, params: &JunglizerParams, beat: f64) -> u32 {
        let events = EventList::with_capacity(0);
        let transport = TransportInfo {
            playing: true,
            tempo: 174.0,
            position_beats: beat,
            ..TransportInfo::default()
        };
        let mut left = [0.0_f32; 64];
        let mut right = [0.0_f32; 64];
        let mut channel_slices: [&mut [f32]; 2] = [&mut left, &mut right];
        let mut buffer = AudioBuffer::from_slices_checked(&[], &mut channel_slices, 64);
        let mut output_events = EventList::with_capacity(0);
        let mut context = ProcessContext::new(&transport, 44_100.0, 64, &mut output_events);
        let ((), count) = truce::rt::audit(|| {
            let _section = truce::rt::RtSection::enter();
            Junglizer::process(st, params, &mut buffer, &events, &mut context);
        });
        count
    }

    /// Flags frees on the audio thread too, for one test, and puts the
    /// setting back even if the test fails.
    struct CheckFrees(bool);

    impl CheckFrees {
        fn on() -> Self {
            let before = truce::rt::check_dealloc();
            truce::rt::set_check_dealloc(true);
            Self(before)
        }
    }

    impl Drop for CheckFrees {
        fn drop(&mut self) {
            truce::rt::set_check_dealloc(self.0);
        }
    }

    #[test]
    fn adopting_and_retiring_breaks_never_allocates_on_audio_thread() {
        let _frees = CheckFrees::on();
        let params = JunglizerParams::default();
        let lib = &params.library;
        let mut st = Junglizer::load_sources();
        let path = fixture("loop_160_4beats_mono16_48k.wav");
        st.edit.load_user(0, &path, 48_000, 4, &WHOLE).unwrap();
        st.edit.load_user(1, &path, 48_000, 4, &WHOLE).unwrap();
        // Lets go of two built-ins and posts a Restore for each user slot.
        assert_eq!(run_block(&mut st, &params, 0.0), 0, "posting requests");
        lib.run_pending();
        assert_eq!(run_block(&mut st, &params, 0.1), 0, "adopting");
        for slot in 0..2 {
            let held = st.sources[slot].as_ref().expect("adopted");
            assert_eq!(held.path_hash, st.edit.slots[slot].path_hash());
        }
        // A newer decode of the same path (a re-bounced file chosen again)
        // swaps in, retiring the old one through the queue.
        let old_seq = st.sources[0].as_ref().unwrap().seq;
        lib.request(0, path.as_bytes(), Purpose::Choose).unwrap();
        lib.run_pending();
        assert_eq!(run_block(&mut st, &params, 0.15), 0, "same-path swap");
        assert!(st.sources[0].as_ref().unwrap().seq > old_seq);
        lib.collect_garbage();
        // Replace the library's results, so the DSP holds the last reference
        // and freeing it on the audio thread would be a real free.
        for slot in 0..2 {
            lib.request(slot, b"/nowhere/other.wav", Purpose::Restore)
                .unwrap();
        }
        lib.request(0, b"/nowhere/other.wav", Purpose::Choose)
            .unwrap();
        lib.run_pending();
        assert!(
            st.sources[..2]
                .iter()
                .flatten()
                .all(|b| Arc::strong_count(b) == 1)
        );
        st.edit.unload(1);
        assert_eq!(run_block(&mut st, &params, 0.2), 0, "retiring to the queue");
        assert!(st.sources[1].is_none());
        let spare = library::Library::builtin()[3].clone();
        while lib.retire(spare.clone()).is_ok() {}
        st.edit.unload(0);
        assert_eq!(run_block(&mut st, &params, 0.3), 0, "retiring to overflow");
        assert!(st.sources[0].is_none());
        assert!(st.overflow.iter().any(Option::is_some));
        lib.collect_garbage();
        assert_eq!(run_block(&mut st, &params, 0.4), 0, "flushing overflow");
        assert!(st.overflow.iter().all(Option::is_none));
        lib.collect_garbage();
        st.edit.restore_builtin();
        assert_eq!(run_block(&mut st, &params, 0.5), 0, "back to the built-ins");
        lib.collect_garbage();
    }

    #[test]
    fn midi_note_uses_selected_source_and_playback_controls() {
        let mut midi = MidiPlayback::default();
        let mut edit = EditState::default();
        edit.set_markers(
            2,
            &[SliceMarker {
                start: 0,
                end: 100,
                active: true,
                bass: true,
            }; 8],
        );
        for (cc, value) in [(20, 1), (21, 8), (22, 4), (23, 3), (24, 16), (30, 2)] {
            midi.handle(
                &EventBody::ControlChange {
                    group: 0,
                    channel: 0,
                    cc,
                    value,
                },
                4.0,
                &edit,
            );
        }
        assert_eq!(midi.bars[1], 2);
        assert!(midi.handle(
            &EventBody::NoteOn {
                group: 0,
                channel: 0,
                note: 2 * 32 + 3,
                velocity: 100,
            },
            4.0,
            &edit,
        ));
        let voice = midi.voice.expect("MIDI slice voice");
        assert_eq!(voice.placement.source_loop, 2);
        assert_eq!(voice.placement.source_slice, 3);
        assert_eq!(voice.placement.bit_depth, 8);
        assert_eq!(voice.placement.repeats, 4);
        assert_eq!(voice.placement.mode, 1);
        midi.handle(
            &EventBody::NoteOff {
                group: 0,
                channel: 0,
                note: 2 * 32 + 3,
                velocity: 0,
            },
            4.5,
            &edit,
        );
        assert!(midi.voice.is_none());
    }
}
