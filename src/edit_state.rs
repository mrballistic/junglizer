//! The editable pattern shared by the editor, project state, and audio engine.
//! Fixed-size storage keeps state loads and audio reads allocation-free.

use truce::prelude::StateTrait;

pub const LOOP_COUNT: usize = 4;
pub const MAX_STEPS: usize = 32;
const MAGIC: &[u8; 4] = b"JZP1";
pub const SERIALIZED_LEN: usize =
    4 + 2 + 8 + 8 + LOOP_COUNT * MAX_STEPS * (1 + MAX_STEPS * 9) + MAX_STEPS * 9;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SliceMarker {
    pub start: u32,
    pub end: u32,
    pub active: bool,
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditState {
    pub source_count: u8,
    pub player_slots: u8,
    pub markers: [[MarkerSet; MAX_STEPS]; LOOP_COUNT],
    pub placements: [StepPlacement; MAX_STEPS],
    pub seed: u64,
    pub version: u64,
}

impl Default for EditState {
    fn default() -> Self {
        Self {
            source_count: 8,
            player_slots: 32,
            markers: [[MarkerSet::default(); MAX_STEPS]; LOOP_COUNT],
            placements: [StepPlacement::default(); MAX_STEPS],
            seed: 1,
            version: 1,
        }
    }
}

impl EditState {
    pub fn marker_set(&self, source_loop: usize) -> Option<&MarkerSet> {
        self.markers
            .get(source_loop)?
            .get(self.source_count as usize - 1)
    }

    pub fn marker_set_mut(&mut self, source_loop: usize) -> Option<&mut MarkerSet> {
        self.markers
            .get_mut(source_loop)?
            .get_mut(self.source_count as usize - 1)
    }

    pub fn slice(&self, source_loop: usize, source_slice: usize) -> Option<SliceMarker> {
        let set = self.marker_set(source_loop)?;
        if !set.valid || source_slice >= self.source_count as usize {
            return None;
        }
        let slice = set.slices[source_slice];
        slice.active.then_some(slice)
    }

    pub fn set_markers(&mut self, source_loop: usize, slices: &[SliceMarker]) -> bool {
        if slices.len() != self.source_count as usize {
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
            || placement.source_slice >= self.source_count
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
}

impl StateTrait for EditState {
    fn serialize_into(&self, buf: &mut Vec<u8>) {
        buf.clear();
        buf.extend_from_slice(MAGIC);
        buf.push(self.source_count);
        buf.push(self.player_slots);
        buf.extend_from_slice(&self.seed.to_le_bytes());
        buf.extend_from_slice(&self.version.to_le_bytes());
        for loop_sets in &self.markers {
            for set in loop_sets {
                buf.push(u8::from(set.valid));
                for slice in &set.slices {
                    buf.extend_from_slice(&slice.start.to_le_bytes());
                    buf.extend_from_slice(&slice.end.to_le_bytes());
                    buf.push(u8::from(slice.active));
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
            source_count: next(1)[0],
            player_slots: next(1)[0],
            seed: u64::from_le_bytes(next(8).try_into().ok()?),
            version: u64::from_le_bytes(next(8).try_into().ok()?),
            ..Self::default()
        };
        if !(1..=MAX_STEPS as u8).contains(&state.source_count)
            || !(1..=MAX_STEPS as u8).contains(&state.player_slots)
        {
            return None;
        }
        for loop_sets in &mut state.markers {
            for set in loop_sets {
                set.valid = next(1)[0] != 0;
                for slice in &mut set.slices {
                    slice.start = u32::from_le_bytes(next(4).try_into().ok()?);
                    slice.end = u32::from_le_bytes(next(4).try_into().ok()?);
                    slice.active = next(1)[0] != 0;
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
