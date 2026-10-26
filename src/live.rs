//! Lock-free copy of the exact live phrase currently rendered by the DSP.
//! The UI uses this copy when the user freezes a variation.

use crate::edit_state::{EditState, MAX_STEPS, StepPlacement};
use std::sync::atomic::{AtomicI64, AtomicU32, AtomicU64, Ordering};

pub struct LiveSnapshot {
    sequence: AtomicU64,
    cycle: AtomicI64,
    bars: AtomicU32,
    counts: AtomicU32,
    placements: [AtomicU32; MAX_STEPS],
}

#[derive(Clone, Copy)]
pub struct CapturedPhrase {
    pub cycle: i64,
    pub bars: [u8; 4],
    pub source_count: u8,
    pub player_slots: u8,
    pub placements: [StepPlacement; MAX_STEPS],
}

impl Default for LiveSnapshot {
    fn default() -> Self {
        Self {
            sequence: AtomicU64::new(0),
            cycle: AtomicI64::new(0),
            bars: AtomicU32::new(0),
            counts: AtomicU32::new(0),
            placements: std::array::from_fn(|_| AtomicU32::new(0)),
        }
    }
}

impl LiveSnapshot {
    pub fn publish(&self, pattern: &EditState, cycle: i64, bars: [usize; 4]) {
        self.sequence.fetch_add(1, Ordering::AcqRel);
        self.cycle.store(cycle, Ordering::Relaxed);
        self.bars.store(
            bars.into_iter()
                .enumerate()
                .fold(0_u32, |bits, (i, bar)| bits | ((bar as u32 & 3) << (i * 2))),
            Ordering::Relaxed,
        );
        self.counts.store(
            u32::from(pattern.source_count) | (u32::from(pattern.player_slots) << 8),
            Ordering::Relaxed,
        );
        for (atom, item) in self.placements.iter().zip(pattern.placements) {
            atom.store(item.pack(), Ordering::Relaxed);
        }
        self.sequence.fetch_add(1, Ordering::Release);
    }

    pub fn capture(&self) -> Option<CapturedPhrase> {
        for _ in 0..3 {
            let before = self.sequence.load(Ordering::Acquire);
            if before == 0 || before & 1 != 0 {
                continue;
            }
            let cycle = self.cycle.load(Ordering::Relaxed);
            let bars_bits = self.bars.load(Ordering::Relaxed);
            let counts = self.counts.load(Ordering::Relaxed);
            let placements = std::array::from_fn(|i| {
                StepPlacement::unpack(self.placements[i].load(Ordering::Relaxed))
            });
            if self.sequence.load(Ordering::Acquire) == before {
                return Some(CapturedPhrase {
                    cycle,
                    bars: std::array::from_fn(|i| ((bars_bits >> (i * 2)) & 3) as u8),
                    source_count: (counts & 255) as u8,
                    player_slots: ((counts >> 8) & 255) as u8,
                    placements,
                });
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_roundtrips_phrase() {
        let mut pattern = EditState::default();
        pattern.placements[0] = StepPlacement {
            active: true,
            start_slot: 17,
            span_slots: 4,
            source_loop: 3,
            source_slice: 23,
            mode: 1,
            bit_depth: 7,
            repeats: 8,
            locked: true,
        };
        let snapshot = LiveSnapshot::default();
        snapshot.publish(&pattern, 4, [3, 1, 2, 0]);
        let captured = snapshot.capture().unwrap();
        assert_eq!(captured.cycle, 4);
        assert_eq!(captured.bars, [3, 1, 2, 0]);
        assert_eq!(captured.placements[0], pattern.placements[0]);
    }
}
