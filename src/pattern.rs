use std::collections::BTreeMap;

pub const BAR_COUNT: usize = 4;
pub const BEATS_PER_BAR: f64 = 4.0;
pub const PATTERN_BEATS: f64 = BAR_COUNT as f64 * BEATS_PER_BAR;
pub const MAX_STEPS: u8 = 32;

/// A curated region of a source loop. `None` in a marker set is an intentionally
/// empty source slot; it can be left alone or used as a pause in the pattern.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SliceRange {
    pub start_frame: u64,
    pub end_frame: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceLoop {
    pub name: String,
    pub frame_count: u64,
    /// Each loop has its own hand-placed boundaries for the shared slice count.
    /// Retaining other counts lets the user switch counts without losing edits.
    pub marker_sets: BTreeMap<u8, Vec<Option<SliceRange>>>,
}

impl SourceLoop {
    pub fn set_marker_set(
        &mut self,
        count: u8,
        slices: Vec<Option<SliceRange>>,
    ) -> Result<(), PatternError> {
        if !(1..=MAX_STEPS).contains(&count) || slices.len() != count as usize {
            return Err(PatternError::InvalidCount);
        }
        if slices
            .iter()
            .flatten()
            .any(|slice| slice.start_frame >= slice.end_frame || slice.end_frame > self.frame_count)
        {
            return Err(PatternError::InvalidSlice);
        }
        self.marker_sets.insert(count, slices);
        Ok(())
    }

    pub fn slices(&self, count: u8) -> Option<&[Option<SliceRange>]> {
        self.marker_sets.get(&count).map(Vec::as_slice)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaybackMode {
    /// Play the source at its original rate; the rest of the placement is quiet
    /// if the slice ends before the placement does.
    OriginalSpeed,
    /// Change playback rate to fill the placement, changing pitch with speed.
    Varispeed,
    /// Fill the placement while preserving pitch. DSP comes after the first
    /// playable prototype; the pattern model already records the choice.
    PreservePitch,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Placement {
    pub start_slot: u8,
    pub span_slots: u8,
    pub source_loop: usize,
    pub source_slice: u8,
    pub mode: PlaybackMode,
    pub bit_depth: u8,
}

impl Placement {
    pub fn end_slot(&self) -> u8 {
        self.start_slot + self.span_slots
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pattern {
    pub source_slices: u8,
    pub player_slots: u8,
    /// A four-beat loop is chosen independently for each of the four bars.
    /// Repeating an index repeats the same loop without duplicating audio.
    pub bar_loops: [usize; BAR_COUNT],
    pub placements: Vec<Placement>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PatternError {
    InvalidCount,
    InvalidSlice,
    InvalidPlacement,
}

impl Pattern {
    pub fn new(source_slices: u8, player_slots: u8) -> Result<Self, PatternError> {
        if !(1..=MAX_STEPS).contains(&source_slices) || !(1..=MAX_STEPS).contains(&player_slots) {
            return Err(PatternError::InvalidCount);
        }
        Ok(Self {
            source_slices,
            player_slots,
            bar_loops: [0; BAR_COUNT],
            placements: Vec::new(),
        })
    }

    pub fn slot_start_beats(&self, slot: u8) -> Option<f64> {
        (slot < self.player_slots)
            .then(|| PATTERN_BEATS * f64::from(slot) / f64::from(self.player_slots))
    }

    pub fn slot_end_beats(&self, slot: u8) -> Option<f64> {
        (slot < self.player_slots)
            .then(|| PATTERN_BEATS * f64::from(slot + 1) / f64::from(self.player_slots))
    }

    pub fn set_bar_loop(&mut self, bar: usize, source_loop: usize) -> Result<(), PatternError> {
        let Some(slot) = self.bar_loops.get_mut(bar) else {
            return Err(PatternError::InvalidPlacement);
        };
        *slot = source_loop;
        Ok(())
    }

    pub fn place(&mut self, placement: Placement) -> Result<(), PatternError> {
        if placement.span_slots == 0
            || placement.start_slot >= self.player_slots
            || placement.end_slot() > self.player_slots
            || placement.source_slice >= self.source_slices
            || !(1..=16).contains(&placement.bit_depth)
        {
            return Err(PatternError::InvalidPlacement);
        }
        // A new drag replaces placements it overlaps. The ghost beneath them
        // remains intact and resumes after the new placement ends.
        self.placements.retain(|old| {
            old.end_slot() <= placement.start_slot || old.start_slot >= placement.end_slot()
        });
        self.placements.push(placement);
        self.placements.sort_by_key(|item| item.start_slot);
        Ok(())
    }

    pub fn layer_at_beat(&self, beat: f64) -> Option<Layer<'_>> {
        if !beat.is_finite() || !(0.0..PATTERN_BEATS).contains(&beat) {
            return None;
        }
        let bar = (beat / BEATS_PER_BAR) as usize;
        let placement = self.placements.iter().find(|item| {
            let start = PATTERN_BEATS * f64::from(item.start_slot) / f64::from(self.player_slots);
            let end = PATTERN_BEATS * f64::from(item.end_slot()) / f64::from(self.player_slots);
            (start..end).contains(&beat)
        });
        match placement {
            Some(item) => Some(Layer::Placement(item)),
            None => Some(Layer::Ghost {
                bar,
                source_loop: self.bar_loops[bar],
                beat_in_loop: beat - bar as f64 * BEATS_PER_BAR,
            }),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Layer<'a> {
    Ghost {
        bar: usize,
        source_loop: usize,
        beat_in_loop: f64,
    },
    Placement(&'a Placement),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn independent_counts_and_four_bar_timing() {
        let pattern = Pattern::new(7, 32).unwrap();
        assert_eq!(pattern.slot_start_beats(0), Some(0.0));
        assert_eq!(pattern.slot_start_beats(31), Some(15.5));
        assert_eq!(pattern.slot_end_beats(31), Some(16.0));
        assert_eq!(pattern.slot_start_beats(32), None);
    }

    #[test]
    fn ghost_uses_each_bar_selection_and_resumes_after_overlay() {
        let mut pattern = Pattern::new(8, 16).unwrap();
        pattern.bar_loops = [0, 1, 0, 2];
        pattern
            .place(Placement {
                start_slot: 3,
                span_slots: 4,
                source_loop: 2,
                source_slice: 5,
                mode: PlaybackMode::Varispeed,
                bit_depth: 8,
            })
            .unwrap();

        assert!(matches!(
            pattern.layer_at_beat(3.5),
            Some(Layer::Placement(_))
        ));
        assert!(matches!(
            pattern.layer_at_beat(5.0),
            Some(Layer::Placement(_))
        ));
        assert_eq!(
            pattern.layer_at_beat(7.0),
            Some(Layer::Ghost {
                bar: 1,
                source_loop: 1,
                beat_in_loop: 3.0,
            })
        );
        assert_eq!(
            pattern.layer_at_beat(12.0),
            Some(Layer::Ghost {
                bar: 3,
                source_loop: 2,
                beat_in_loop: 0.0,
            })
        );
    }

    #[test]
    fn marker_sets_keep_quiet_slices_and_other_counts() {
        let mut source = SourceLoop {
            name: "Break A".into(),
            frame_count: 48_000,
            marker_sets: BTreeMap::new(),
        };
        source
            .set_marker_set(
                2,
                vec![
                    Some(SliceRange {
                        start_frame: 0,
                        end_frame: 8000,
                    }),
                    None,
                ],
            )
            .unwrap();
        source
            .set_marker_set(
                1,
                vec![Some(SliceRange {
                    start_frame: 0,
                    end_frame: 48_000,
                })],
            )
            .unwrap();
        assert_eq!(source.slices(2).unwrap().len(), 2);
        assert_eq!(source.slices(2).unwrap()[1], None);
        assert_eq!(source.slices(1).unwrap().len(), 1);
    }
}
