//! Decoded breaks, ready for the audio thread. Built off the audio thread and
//! handed over behind an `Arc`.

use crate::edit_state::{MAX_STEPS, SliceMarker};
use crate::sample::StereoSample;
use crate::tempo::Reading;

/// The slice count `suggested` markers are computed for.
const SUGGESTED_SLICES: u8 = 8;

pub struct LoadedBreak {
    pub sample: StereoSample,
    #[allow(dead_code)] // matched against the slot's path by the loader (Task 7)
    pub path_hash: u64,
    #[allow(dead_code)] // shown and used by the loader (Task 7)
    pub reading: Reading,
    #[allow(dead_code)] // starting markers for a new user break (Task 7)
    pub suggested: [SliceMarker; MAX_STEPS],
}

impl LoadedBreak {
    /// Wraps decoded audio and computes its suggested 8-slice markers. This
    /// allocates; never call it from the audio thread.
    pub fn new(sample: StereoSample, path_hash: u64, reading: Reading) -> Self {
        let suggested = sample.suggested_slices(SUGGESTED_SLICES);
        Self {
            sample,
            path_hash,
            reading,
            suggested,
        }
    }
}
