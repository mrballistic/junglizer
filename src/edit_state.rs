//! The editable pattern shared by the editor, project state, and audio engine.
//! Fixed-size storage keeps state loads and audio reads allocation-free.

use truce::prelude::StateTrait;

pub const LOOP_COUNT: usize = 4;
pub const MAX_STEPS: usize = 32;
/// Longest file path a slot can link to, in UTF-8 bytes.
pub const MAX_PATH_BYTES: usize = 1024;
/// Beta 1 blobs (`JZP1`, `JZP2`) are not migrated; they fail to load.
const MAGIC: &[u8; 4] = b"JZP3";
const SLOT_LEN: usize = 1 + 1 + 1 + 4 + 2 + MAX_PATH_BYTES;
pub const SERIALIZED_LEN: usize = 4
    + 1
    + 8
    + 8
    + 6
    + LOOP_COUNT * SLOT_LEN
    + LOOP_COUNT * MAX_STEPS * (1 + MAX_STEPS * 10)
    + MAX_STEPS * 9;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum SlotKind {
    #[default]
    Builtin = 0,
    User = 1,
    Blank = 2,
}

impl SlotKind {
    fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Builtin),
            1 => Some(Self::User),
            2 => Some(Self::Blank),
            _ => None,
        }
    }
}

/// One of the four source breaks. A user break is linked by path; its audio
/// is never stored in the project.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BreakSlot {
    pub kind: SlotKind,
    pub beats: u8,
    pub slice_count: u8,
    /// Frame count of the linked file. Built-in slots keep 0; the DSP knows
    /// their length.
    pub frames: u32,
    pub path_len: u16,
    pub path: [u8; MAX_PATH_BYTES],
}

impl BreakSlot {
    pub fn builtin() -> Self {
        Self {
            kind: SlotKind::Builtin,
            beats: 4,
            slice_count: 8,
            frames: 0,
            path_len: 0,
            path: [0; MAX_PATH_BYTES],
        }
    }

    pub fn path(&self) -> Option<&str> {
        if self.kind != SlotKind::User || self.path_len == 0 {
            return None;
        }
        let bytes = self.path.get(..self.path_len as usize)?;
        std::str::from_utf8(bytes).ok()
    }

    pub fn path_hash(&self) -> u64 {
        self.path().map_or(0, |path| path_hash(path.as_bytes()))
    }

    fn set_path(&mut self, path: &str) {
        self.path = [0; MAX_PATH_BYTES];
        self.path[..path.len()].copy_from_slice(path.as_bytes());
        self.path_len = path.len() as u16;
    }
}

/// FNV-1a 64-bit hash of a path's bytes.
pub fn path_hash(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, &byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlotError {
    PathTooLong,
    BadSlot,
    BadBeats,
}

impl SlotError {
    pub fn message(&self) -> &'static str {
        match self {
            Self::PathTooLong => "That file’s path is too long. Move it to a shorter folder.",
            Self::BadSlot => "That break can’t go in this slot.",
            Self::BadBeats => "A break must be 4 to 32 beats long, in whole bars.",
        }
    }
}

fn valid_beats(beats: u8) -> bool {
    (4..=32).contains(&beats) && beats % 4 == 0
}

fn valid_count(count: u8) -> bool {
    (1..=MAX_STEPS as u8).contains(&count)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SliceMarker {
    pub start: u32,
    pub end: u32,
    pub active: bool,
    /// Triggers the tuned bass reinforcement at this slice's onset.
    pub bass: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MarkerSet {
    pub valid: bool,
    pub slices: [SliceMarker; MAX_STEPS],
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StepPlacement {
    pub active: bool,
    pub start_slot: u8,
    pub span_slots: u8,
    pub source_loop: u8,
    pub source_slice: u8,
    /// 0: original speed, 1: fit the selected span by varispeed.
    pub mode: u8,
    /// 16 means effectively uncrushed.
    pub bit_depth: u8,
    /// Number of retriggers within the placement: 1, 2, 4, or 8.
    pub repeats: u8,
    pub locked: bool,
}

impl StepPlacement {
    /// Compact representation for the editor's lock-free live preview.
    pub fn pack(self) -> u32 {
        let repeat_code = match self.repeats {
            2 => 1,
            4 => 2,
            8 => 3,
            _ => 0,
        };
        u32::from(self.active)
            | (u32::from(self.start_slot) << 1)
            | (u32::from(self.span_slots) << 6)
            | (u32::from(self.source_loop) << 12)
            | (u32::from(self.source_slice) << 14)
            | (u32::from(self.mode) << 19)
            | (u32::from(self.bit_depth) << 20)
            | (repeat_code << 25)
            | (u32::from(self.locked) << 27)
    }

    pub fn unpack(bits: u32) -> Self {
        Self {
            active: bits & 1 != 0,
            start_slot: ((bits >> 1) & 31) as u8,
            span_slots: ((bits >> 6) & 63) as u8,
            source_loop: ((bits >> 12) & 3) as u8,
            source_slice: ((bits >> 14) & 31) as u8,
            mode: ((bits >> 19) & 1) as u8,
            bit_depth: ((bits >> 20) & 31) as u8,
            repeats: 1 << ((bits >> 25) & 3),
            locked: bits & (1 << 27) != 0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditState {
    pub slots: [BreakSlot; LOOP_COUNT],
    pub player_slots: u8,
    pub markers: [[MarkerSet; MAX_STEPS]; LOOP_COUNT],
    pub placements: [StepPlacement; MAX_STEPS],
    pub live_enabled: bool,
    pub frozen: bool,
    pub frozen_bars: [u8; 4],
    pub seed: u64,
    pub version: u64,
}

impl Default for EditState {
    fn default() -> Self {
        Self {
            slots: [BreakSlot::builtin(); LOOP_COUNT],
            player_slots: 32,
            markers: [[MarkerSet::default(); MAX_STEPS]; LOOP_COUNT],
            placements: [StepPlacement::default(); MAX_STEPS],
            live_enabled: false,
            frozen: false,
            frozen_bars: [0, 1, 2, 3],
            seed: 1,
            version: 1,
        }
    }
}

impl EditState {
    /// The slice count of one source loop; 0 for a loop that doesn't exist.
    pub fn slice_count(&self, source_loop: usize) -> u8 {
        self.slots
            .get(source_loop)
            .map_or(0, |slot| slot.slice_count)
    }

    pub fn slice_counts(&self) -> [u8; LOOP_COUNT] {
        [0, 1, 2, 3].map(|index| self.slots[index].slice_count)
    }

    /// Sets one loop's slice count and deactivates that loop's placements
    /// that point past the new count.
    pub fn set_slice_count(&mut self, source_loop: usize, count: u8) -> bool {
        if source_loop >= LOOP_COUNT
            || !valid_count(count)
            || self.slots[source_loop].slice_count == count
        {
            return false;
        }
        self.slots[source_loop].slice_count = count;
        for item in &mut self.placements {
            if item.active && item.source_loop as usize == source_loop && item.source_slice >= count
            {
                item.active = false;
            }
        }
        self.touch();
        true
    }

    /// Fills the loop's current marker set with even, active slices over
    /// `frames` if it is invalid. Allocation-free, for the audio thread.
    pub fn fill_even_markers(&mut self, source_loop: usize, frames: u32) -> bool {
        if self
            .slots
            .get(source_loop)
            .is_none_or(|slot| slot.kind == SlotKind::Blank)
        {
            return false;
        }
        let count = u32::from(self.slice_count(source_loop));
        let Some(set) = self.marker_set_mut(source_loop) else {
            return false;
        };
        if set.valid {
            return false;
        }
        for (index, slice) in set.slices.iter_mut().enumerate() {
            let index = index as u32;
            *slice = if index < count {
                SliceMarker {
                    start: (u64::from(frames) * u64::from(index) / u64::from(count)) as u32,
                    end: (u64::from(frames) * u64::from(index + 1) / u64::from(count)) as u32,
                    active: true,
                    bass: false,
                }
            } else {
                SliceMarker::default()
            };
        }
        set.valid = true;
        self.touch();
        true
    }

    pub fn marker_set(&self, source_loop: usize) -> Option<&MarkerSet> {
        let count = self.slice_count(source_loop) as usize;
        self.markers.get(source_loop)?.get(count.checked_sub(1)?)
    }

    pub fn marker_set_mut(&mut self, source_loop: usize) -> Option<&mut MarkerSet> {
        let count = self.slice_count(source_loop) as usize;
        self.markers
            .get_mut(source_loop)?
            .get_mut(count.checked_sub(1)?)
    }

    pub fn slice(&self, source_loop: usize, source_slice: usize) -> Option<SliceMarker> {
        let set = self.marker_set(source_loop)?;
        if !set.valid || source_slice >= self.slice_count(source_loop) as usize {
            return None;
        }
        let slice = set.slices[source_slice];
        slice.active.then_some(slice)
    }

    fn check_slot(slot: usize, path: &str, beats: u8) -> Result<(), SlotError> {
        if slot >= LOOP_COUNT || path.is_empty() {
            return Err(SlotError::BadSlot);
        }
        if path.len() > MAX_PATH_BYTES {
            return Err(SlotError::PathTooLong);
        }
        if !valid_beats(beats) {
            return Err(SlotError::BadBeats);
        }
        Ok(())
    }

    fn clear_slot_edits(&mut self, slot: usize) {
        for set in &mut self.markers[slot] {
            set.valid = false;
        }
        for item in &mut self.placements {
            if item.active && item.source_loop as usize == slot {
                item.active = false;
            }
        }
    }

    /// Links a user break into `slot`, replacing that slot's slices and
    /// removing every placement on it, locked ones included.
    pub fn load_user(
        &mut self,
        slot: usize,
        path: &str,
        frames: u32,
        beats: u8,
        markers: &[SliceMarker],
    ) -> Result<(), SlotError> {
        Self::check_slot(slot, path, beats)?;
        if markers.is_empty() || markers.len() > MAX_STEPS {
            return Err(SlotError::BadSlot);
        }
        self.clear_slot_edits(slot);
        let count = markers.len();
        let entry = &mut self.slots[slot];
        entry.kind = SlotKind::User;
        entry.set_path(path);
        entry.frames = frames;
        entry.beats = beats;
        entry.slice_count = count as u8;
        let set = &mut self.markers[slot][count - 1];
        set.slices = [SliceMarker::default(); MAX_STEPS];
        set.slices[..count].copy_from_slice(markers);
        set.valid = true;
        self.touch();
        Ok(())
    }

    /// Leaves `slot` blank and removes its slices and placements.
    pub fn unload(&mut self, slot: usize) -> bool {
        if slot >= LOOP_COUNT || self.slots[slot].kind == SlotKind::Blank {
            return false;
        }
        self.clear_slot_edits(slot);
        let entry = &mut self.slots[slot];
        entry.kind = SlotKind::Blank;
        entry.path = [0; MAX_PATH_BYTES];
        entry.path_len = 0;
        entry.beats = 0;
        entry.frames = 0;
        self.touch();
        true
    }

    /// Points a user slot at a moved or changed file. Placements are kept;
    /// markers are scaled to the new length and clamped inside it.
    pub fn relink(
        &mut self,
        slot: usize,
        path: &str,
        frames: u32,
        beats: u8,
    ) -> Result<(), SlotError> {
        Self::check_slot(slot, path, beats)?;
        if self.slots[slot].kind != SlotKind::User {
            return Err(SlotError::BadSlot);
        }
        let old_frames = self.slots[slot].frames;
        for (index, set) in self.markers[slot].iter_mut().enumerate() {
            if set.valid {
                rescale(&mut set.slices[..=index], old_frames, frames);
            }
        }
        let entry = &mut self.slots[slot];
        entry.set_path(path);
        entry.frames = frames;
        entry.beats = beats;
        self.touch();
        Ok(())
    }

    /// Halves or doubles a user break's beat count.
    pub fn set_beats(&mut self, slot: usize, beats: u8) -> bool {
        let Some(entry) = self.slots.get_mut(slot) else {
            return false;
        };
        if entry.kind != SlotKind::User || !valid_beats(beats) || entry.beats == beats {
            return false;
        }
        entry.beats = beats;
        self.touch();
        true
    }

    /// Puts the built-in amen back in all four slots.
    pub fn restore_builtin(&mut self) {
        self.slots = [BreakSlot::builtin(); LOOP_COUNT];
        for sets in &mut self.markers {
            for set in sets {
                set.valid = false;
            }
        }
        self.placements = [StepPlacement::default(); MAX_STEPS];
        self.touch();
    }

    /// An instance nobody has edited yet.
    pub fn is_pristine(&self) -> bool {
        self.slots
            .iter()
            .all(|slot| slot.kind == SlotKind::Builtin && slot.slice_count == 8)
            && self.placements.iter().all(|item| !item.active)
            && !self.frozen
            && !self.live_enabled
    }

    pub fn set_markers(&mut self, source_loop: usize, slices: &[SliceMarker]) -> bool {
        if slices.len() != self.slice_count(source_loop) as usize {
            return false;
        }
        let Some(set) = self.marker_set_mut(source_loop) else {
            return false;
        };
        set.slices[..slices.len()].copy_from_slice(slices);
        set.valid = true;
        self.touch();
        true
    }

    pub fn slot_start(&self, slot: u8) -> f64 {
        16.0 * f64::from(slot) / f64::from(self.player_slots)
    }

    pub fn slot_end(&self, slot: u8) -> f64 {
        16.0 * f64::from(slot + 1) / f64::from(self.player_slots)
    }

    pub fn placement_at(&self, beat: f64) -> Option<&StepPlacement> {
        self.placements.iter().find(|placement| {
            placement.active
                && beat >= self.slot_start(placement.start_slot)
                && beat
                    < 16.0 * f64::from(placement.start_slot + placement.span_slots)
                        / f64::from(self.player_slots)
        })
    }

    pub fn place(&mut self, placement: StepPlacement) -> bool {
        if !placement.active
            || placement.span_slots == 0
            || placement.start_slot >= self.player_slots
            || placement.start_slot as u16 + placement.span_slots as u16 > self.player_slots as u16
            || placement.source_loop as usize >= LOOP_COUNT
            || self.slots[placement.source_loop as usize].kind == SlotKind::Blank
            || placement.source_slice >= self.slots[placement.source_loop as usize].slice_count
            || placement.mode > 1
            || !(1..=16).contains(&placement.bit_depth)
            || ![1, 2, 4, 8].contains(&placement.repeats)
        {
            return false;
        }
        let end = placement.start_slot + placement.span_slots;
        if self.placements.iter().any(|old| {
            old.active
                && old.locked
                && old.start_slot < end
                && old.start_slot + old.span_slots > placement.start_slot
        }) {
            return false;
        }
        for old in &mut self.placements {
            if old.active
                && !old.locked
                && old.start_slot < end
                && old.start_slot + old.span_slots > placement.start_slot
            {
                old.active = false;
            }
        }
        let Some(slot) = self.placements.iter_mut().find(|item| !item.active) else {
            return false;
        };
        *slot = placement;
        self.touch();
        true
    }

    pub fn remove_at_slot(&mut self, slot: u8) -> bool {
        if let Some(item) = self.placements.iter_mut().find(|item| {
            item.active && item.start_slot <= slot && slot < item.start_slot + item.span_slots
        }) {
            item.active = false;
            self.touch();
            true
        } else {
            false
        }
    }

    pub fn replace(&mut self, old: StepPlacement, updated: StepPlacement) -> bool {
        let Some(index) = self
            .placements
            .iter()
            .position(|item| *item == old && item.active)
        else {
            return false;
        };
        // Keep the previous edit intact if the requested length reaches a
        // neighboring edit. Replacing a slice should never silently erase it.
        let end = updated.start_slot as u16 + updated.span_slots as u16;
        if updated.span_slots == 0
            || end > self.player_slots as u16
            || self.placements.iter().enumerate().any(|(i, item)| {
                i != index
                    && item.active
                    && (item.start_slot as u16) < end
                    && (item.start_slot as u16 + item.span_slots as u16) > updated.start_slot as u16
            })
        {
            return false;
        }
        self.placements[index] = updated;
        self.touch();
        true
    }

    pub fn set_player_slots(&mut self, count: u8) -> bool {
        if !(1..=MAX_STEPS as u8).contains(&count) || count == self.player_slots {
            return false;
        }
        let old_count = self.player_slots;
        for item in self.placements.iter_mut().filter(|item| item.active) {
            let start = (f32::from(item.start_slot) * f32::from(count) / f32::from(old_count))
                .round() as u8;
            let end = (f32::from(item.start_slot + item.span_slots) * f32::from(count)
                / f32::from(old_count))
            .round() as u8;
            item.start_slot = start.min(count - 1);
            item.span_slots = end.max(item.start_slot + 1).min(count) - item.start_slot;
        }
        self.player_slots = count;
        // Coarser grids can cause two edits to land on the same slot. Keep
        // the earlier one and leave the rest underneath as ghost playback.
        for index in 0..MAX_STEPS {
            if !self.placements[index].active {
                continue;
            }
            for later in index + 1..MAX_STEPS {
                let a = self.placements[index];
                let b = self.placements[later];
                if b.active
                    && a.start_slot < b.start_slot + b.span_slots
                    && b.start_slot < a.start_slot + a.span_slots
                {
                    self.placements[later].active = false;
                }
            }
        }
        self.touch();
        true
    }

    pub fn touch(&mut self) {
        self.version = self.version.wrapping_add(1);
    }

    /// Hold an exact phrase for repeat playback without changing the user's
    /// per-slice locks. Freezing and locking serve different purposes.
    pub fn freeze_phrase(
        &mut self,
        placements: [StepPlacement; MAX_STEPS],
        slice_counts: [u8; LOOP_COUNT],
        player_slots: u8,
        bars: [u8; 4],
    ) {
        self.placements = placements;
        for (slot, count) in self.slots.iter_mut().zip(slice_counts) {
            if valid_count(count) {
                slot.slice_count = count;
            }
        }
        self.player_slots = player_slots;
        self.frozen_bars = bars;
        self.live_enabled = false;
        self.frozen = true;
        self.touch();
    }

    /// Recovery action for older phrases whose slices were locked by export.
    pub fn unlock_all(&mut self) {
        if self
            .placements
            .iter()
            .any(|item| item.active && item.locked)
        {
            for item in &mut self.placements {
                if item.active {
                    item.locked = false;
                }
            }
            self.touch();
        }
    }
}

/// Scales markers from `old_frames` to `new_frames`, clamps them inside the
/// new length, and restores `start < end` and contiguity where it can.
fn rescale(slices: &mut [SliceMarker], old_frames: u32, new_frames: u32) {
    let scale = |frame: u32| {
        let scaled = if old_frames == 0 {
            u64::from(frame)
        } else {
            u64::from(frame) * u64::from(new_frames) / u64::from(old_frames)
        };
        scaled.min(u64::from(new_frames)) as u32
    };
    let count = slices.len();
    let mut last_end = scale(slices[count - 1].end);
    for slice in slices.iter_mut() {
        slice.start = scale(slice.start);
    }
    for index in 1..count {
        slices[index].start = slices[index]
            .start
            .max(slices[index - 1].start.saturating_add(1));
    }
    last_end = last_end
        .max(slices[count - 1].start.saturating_add(1))
        .min(new_frames);
    for index in (0..count).rev() {
        let limit = if index + 1 < count {
            slices[index + 1].start
        } else {
            last_end
        };
        slices[index].start = slices[index].start.min(limit.saturating_sub(1));
    }
    for index in 0..count {
        slices[index].end = if index + 1 < count {
            slices[index + 1].start
        } else {
            last_end
        };
    }
}

impl StateTrait for EditState {
    fn serialize_into(&self, buf: &mut Vec<u8>) {
        buf.clear();
        buf.extend_from_slice(MAGIC);
        buf.push(self.player_slots);
        buf.extend_from_slice(&self.seed.to_le_bytes());
        buf.extend_from_slice(&self.version.to_le_bytes());
        buf.push(u8::from(self.live_enabled));
        buf.push(u8::from(self.frozen));
        buf.extend_from_slice(&self.frozen_bars);
        for slot in &self.slots {
            buf.push(slot.kind as u8);
            buf.push(slot.beats);
            buf.push(slot.slice_count);
            buf.extend_from_slice(&slot.frames.to_le_bytes());
            buf.extend_from_slice(&slot.path_len.to_le_bytes());
            buf.extend_from_slice(&slot.path);
        }
        for loop_sets in &self.markers {
            for set in loop_sets {
                buf.push(u8::from(set.valid));
                for slice in &set.slices {
                    buf.extend_from_slice(&slice.start.to_le_bytes());
                    buf.extend_from_slice(&slice.end.to_le_bytes());
                    buf.push(u8::from(slice.active));
                    buf.push(u8::from(slice.bass));
                }
            }
        }
        for item in &self.placements {
            buf.extend_from_slice(&[
                u8::from(item.active),
                item.start_slot,
                item.span_slots,
                item.source_loop,
                item.source_slice,
                item.mode,
                item.bit_depth,
                item.repeats,
                u8::from(item.locked),
            ]);
        }
    }

    fn deserialize(data: &[u8]) -> Option<Self> {
        if data.len() != SERIALIZED_LEN || &data[..4] != MAGIC {
            return None;
        }
        let mut cursor = 4;
        let mut next = |count: usize| {
            let bytes = &data[cursor..cursor + count];
            cursor += count;
            bytes
        };
        let mut state = Self {
            player_slots: next(1)[0],
            seed: u64::from_le_bytes(next(8).try_into().ok()?),
            version: u64::from_le_bytes(next(8).try_into().ok()?),
            ..Self::default()
        };
        state.live_enabled = next(1)[0] != 0;
        state.frozen = next(1)[0] != 0;
        state.frozen_bars.copy_from_slice(next(4));
        if state.frozen_bars.iter().any(|&bar| bar > 3) || !valid_count(state.player_slots) {
            return None;
        }
        for slot in &mut state.slots {
            slot.kind = SlotKind::from_u8(next(1)[0])?;
            slot.beats = next(1)[0];
            slot.slice_count = next(1)[0];
            slot.frames = u32::from_le_bytes(next(4).try_into().ok()?);
            slot.path_len = u16::from_le_bytes(next(2).try_into().ok()?);
            slot.path.copy_from_slice(next(MAX_PATH_BYTES));
            if slot.path_len as usize > MAX_PATH_BYTES || !valid_count(slot.slice_count) {
                return None;
            }
            if slot.kind == SlotKind::User
                && (slot.path_len == 0 || slot.path().is_none() || !valid_beats(slot.beats))
            {
                return None;
            }
        }
        for loop_sets in &mut state.markers {
            for set in loop_sets {
                set.valid = next(1)[0] != 0;
                for slice in &mut set.slices {
                    slice.start = u32::from_le_bytes(next(4).try_into().ok()?);
                    slice.end = u32::from_le_bytes(next(4).try_into().ok()?);
                    slice.active = next(1)[0] != 0;
                    slice.bass = next(1)[0] != 0;
                }
            }
        }
        for item in &mut state.placements {
            let bytes = next(9);
            *item = StepPlacement {
                active: bytes[0] != 0,
                start_slot: bytes[1],
                span_slots: bytes[2],
                source_loop: bytes[3],
                source_slice: bytes[4],
                mode: bytes[5],
                bit_depth: bytes[6],
                repeats: bytes[7],
                locked: bytes[8] != 0,
            };
            if item.active
                && (item.span_slots == 0
                    || item.start_slot as u16 + item.span_slots as u16 > state.player_slots as u16
                    || item.source_loop as usize >= LOOP_COUNT
                    || item.source_slice as usize >= MAX_STEPS
                    || item.mode > 1
                    || !(1..=16).contains(&item.bit_depth)
                    || ![1, 2, 4, 8].contains(&item.repeats))
            {
                return None;
            }
        }
        Some(state)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn marks(n: u8, frames: u32) -> Vec<SliceMarker> {
        (0..n as u32)
            .map(|i| SliceMarker {
                start: frames * i / n as u32,
                end: frames * (i + 1) / n as u32,
                active: true,
                bass: i == 0,
            })
            .collect()
    }

    #[test]
    fn loading_a_user_break_replaces_only_that_slot() {
        let mut s = EditState::default();
        for slot in [0_u8, 1] {
            assert!(s.place(StepPlacement {
                active: true,
                start_slot: slot * 4,
                span_slots: 1,
                source_loop: slot,
                source_slice: 0,
                mode: 0,
                bit_depth: 16,
                repeats: 1,
                locked: slot == 0
            }));
        }
        s.load_user(0, "/breaks/Think 114.wav", 200_000, 8, &marks(16, 200_000))
            .unwrap();
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
        assert!(!s.place(StepPlacement {
            active: true,
            start_slot: 0,
            span_slots: 1,
            source_loop: 2,
            source_slice: 0,
            mode: 0,
            bit_depth: 16,
            repeats: 1,
            locked: false
        }));
    }

    #[test]
    fn relink_scales_markers_and_keeps_placements() {
        let mut s = EditState::default();
        s.load_user(1, "/old/a.wav", 100_000, 8, &marks(4, 100_000))
            .unwrap();
        assert!(s.place(StepPlacement {
            active: true,
            start_slot: 0,
            span_slots: 2,
            source_loop: 1,
            source_slice: 3,
            mode: 0,
            bit_depth: 16,
            repeats: 1,
            locked: true
        }));
        s.relink(1, "/new/a.wav", 50_000, 8).unwrap();
        let set = s.marker_set(1).unwrap();
        assert_eq!(set.slices[1].start, 12_500);
        assert_eq!(set.slices[3].end, 50_000);
        assert!(
            s.placements
                .iter()
                .any(|p| p.active && p.source_loop == 1 && p.locked)
        );
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
        assert_eq!(
            s.load_user(0, &long, 1000, 4, &marks(8, 1000)),
            Err(SlotError::PathTooLong)
        );
        assert_eq!(s.slots[0].kind, SlotKind::Builtin);
    }

    #[test]
    fn v3_roundtrips_slots_and_beta1_blobs_are_rejected() {
        let mut s = EditState::default();
        s.load_user(
            2,
            "C:\\Breaks\\Amen 172.wav",
            99_000,
            16,
            &marks(12, 99_000),
        )
        .unwrap();
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

    #[test]
    fn freezing_preserves_individual_locks_and_unlock_all_recovers_old_phrases() {
        let mut state = EditState::default();
        let unlocked = StepPlacement {
            active: true,
            start_slot: 0,
            span_slots: 1,
            source_loop: 0,
            source_slice: 0,
            mode: 0,
            bit_depth: 16,
            repeats: 1,
            locked: false,
        };
        let locked = StepPlacement {
            start_slot: 1,
            locked: true,
            ..unlocked
        };
        state.placements[0] = unlocked;
        state.placements[1] = locked;
        state.live_enabled = true;
        state.freeze_phrase(state.placements, [8; LOOP_COUNT], 32, [3, 2, 1, 0]);
        assert!(state.frozen);
        assert!(!state.live_enabled);
        assert_eq!(state.frozen_bars, [3, 2, 1, 0]);
        assert!(!state.placements[0].locked);
        assert!(state.placements[1].locked);

        state.frozen = false;
        state.placements[0].locked = true; // Simulate an older exported phrase.
        state.unlock_all();
        assert!(!state.placements[0].locked);
        assert!(!state.placements[1].locked);
    }

    #[test]
    fn placed_slice_overrides_and_returns_to_ghost() {
        let mut state = EditState::default();
        let cut = StepPlacement {
            active: true,
            start_slot: 6,
            span_slots: 3,
            source_loop: 2,
            source_slice: 1,
            mode: 1,
            bit_depth: 8,
            repeats: 1,
            locked: false,
        };
        assert!(state.place(cut));
        assert_eq!(state.placement_at(3.25), Some(&cut));
        assert!(state.placement_at(4.5).is_none());
    }

    #[test]
    fn state_roundtrips_marker_sets_and_placements() {
        let mut state = EditState::default();
        state.set_markers(
            1,
            &[SliceMarker {
                start: 7,
                end: 77,
                active: true,
                bass: false,
            }; 8],
        );
        state.place(StepPlacement {
            active: true,
            start_slot: 3,
            span_slots: 2,
            source_loop: 1,
            source_slice: 4,
            mode: 0,
            bit_depth: 12,
            repeats: 4,
            locked: true,
        });
        let bytes = state.serialize();
        assert_eq!(EditState::deserialize(&bytes), Some(state));
        assert!(EditState::deserialize(&bytes[..bytes.len() - 1]).is_none());
    }

    #[test]
    fn changing_grid_remaps_existing_placements() {
        let mut state = EditState::default();
        assert!(state.place(StepPlacement {
            active: true,
            start_slot: 16,
            span_slots: 4,
            source_loop: 0,
            source_slice: 0,
            mode: 0,
            bit_depth: 16,
            repeats: 1,
            locked: false,
        }));
        assert!(state.set_player_slots(16));
        assert_eq!(state.placements[0].start_slot, 8);
        assert_eq!(state.placements[0].span_slots, 2);
    }
}
