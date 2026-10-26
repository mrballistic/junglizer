//! Small, repeatable fill generator. It produces editable placements; it never
//! changes the rendering path or alters locked cuts.

use crate::edit_state::{EditState, StepPlacement};

fn next(seed: &mut u64) -> u64 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 7;
    *seed ^= *seed << 17;
    *seed
}

pub fn suggest_bar(state: &mut EditState, bar: usize, source_loop: usize) -> usize {
    if bar >= 4 || source_loop >= 4 {
        return 0;
    }
    let count = state.player_slots as usize;
    let start = (bar * count + 3) / 4;
    let end = ((bar + 1) * count + 3) / 4;
    if start >= end {
        return 0;
    }
    for old in &mut state.placements {
        if old.active && !old.locked {
            let a = old.start_slot as usize;
            let b = a + old.span_slots as usize;
            if a >= start && b <= end {
                old.active = false;
            }
        }
    }
    let choices: Vec<u8> = (0..state.source_count)
        .filter(|&slice| state.slice(source_loop, slice as usize).is_some())
        .collect();
    if choices.is_empty() {
        state.touch();
        return 0;
    }
    let mut seed = state.seed.wrapping_add(0x9e37_79b9_7f4a_7c15);
    let mut slot = start;
    let mut added = 0;
    while slot < end {
        let occupied = state.placements.iter().any(|item| {
            item.active
                && (item.start_slot as usize) <= slot
                && slot < item.start_slot as usize + item.span_slots as usize
        });
        if occupied {
            slot += 1;
            continue;
        }
        // Favor short cuts, with occasional odd three-slot phrasing.
        let roll = next(&mut seed) % 8;
        let preferred = if roll == 0 {
            3
        } else if roll <= 2 {
            2
        } else {
            1
        };
        let next_occupied = state
            .placements
            .iter()
            .filter(|item| item.active && item.start_slot as usize > slot)
            .map(|item| item.start_slot as usize)
            .min()
            .unwrap_or(end);
        let span = preferred.min(end - slot).min(next_occupied - slot).max(1);
        let slice = choices[(next(&mut seed) as usize) % choices.len()];
        let repeats = if slot + span >= end && next(&mut seed) % 3 == 0 {
            4
        } else {
            1
        };
        let edit = StepPlacement {
            active: true,
            start_slot: slot as u8,
            span_slots: span as u8,
            source_loop: source_loop as u8,
            source_slice: slice,
            mode: if next(&mut seed) % 4 == 0 { 1 } else { 0 },
            bit_depth: 16,
            repeats,
            locked: false,
        };
        if state.place(edit) {
            added += 1;
        }
        slot += span;
    }
    state.seed = seed;
    state.touch();
    added
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit_state::SliceMarker;

    #[test]
    fn suggestion_is_repeatable_and_preserves_locked_cuts() {
        let mut initial = EditState::default();
        initial.set_markers(
            0,
            &[SliceMarker {
                start: 0,
                end: 100,
                active: true,
            }; 8],
        );
        let locked = StepPlacement {
            active: true,
            start_slot: 2,
            span_slots: 2,
            source_loop: 0,
            source_slice: 0,
            mode: 0,
            bit_depth: 16,
            repeats: 1,
            locked: true,
        };
        assert!(initial.place(locked));
        let mut a = initial.clone();
        let mut b = initial;
        assert_eq!(suggest_bar(&mut a, 0, 0), suggest_bar(&mut b, 0, 0));
        assert_eq!(a, b);
        assert!(a.placements.contains(&locked));
        assert!(a.placements.iter().filter(|item| item.active).all(|item| {
            *item == locked || item.start_slot + item.span_slots <= 2 || item.start_slot >= 4
        }));
    }
}
