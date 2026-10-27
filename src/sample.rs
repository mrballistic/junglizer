use crate::decode::{DecodeError, decode_bytes, decode_file};
use crate::edit_state::{MAX_STEPS, SliceMarker};
use std::path::Path;

/// Decoded stereo frames. Files are opened during plugin initialization,
/// never from the audio callback.
#[derive(Default)]
pub struct StereoSample {
    pub frames: Vec<[f32; 2]>,
    pub sample_rate: u32,
}

impl StereoSample {
    pub fn load(path: &Path) -> Result<Self, DecodeError> {
        decode_file(path)
    }

    pub fn from_bytes(bytes: &'static [u8]) -> Result<Self, DecodeError> {
        decode_bytes(bytes, Some("wav"))
    }

    #[allow(dead_code)] // used by the break importer in later tasks
    pub fn duration_secs(&self) -> f64 {
        if self.sample_rate == 0 {
            return 0.0;
        }
        self.frames.len() as f64 / f64::from(self.sample_rate)
    }

    pub fn at_frame(&self, position: f64, channel: usize) -> f32 {
        if !position.is_finite() || position < 0.0 || position >= self.frames.len() as f64 {
            return 0.0;
        }
        let first = position.floor() as usize;
        let second = (first + 1).min(self.frames.len() - 1);
        let fraction = (position - first as f64) as f32;
        self.frames[first][channel] * (1.0 - fraction) + self.frames[second][channel] * fraction
    }

    /// Linear interpolation at a position in a break `beats` long. The file
    /// spans the beats exactly, whatever its sample rate.
    pub fn at_beat(&self, beat: f64, beats: f64, channel: usize) -> f32 {
        if self.frames.is_empty() || !(beats > 0.0) || !(0.0..beats).contains(&beat) {
            return 0.0;
        }
        self.at_frame(beat * self.frames.len() as f64 / beats, channel)
    }

    /// Offer sound-aligned starting markers for a new slice count. The user
    /// can move any marker afterward; a quiet bin becomes an inactive slice.
    pub fn suggested_slices(&self, count: u8) -> [SliceMarker; MAX_STEPS] {
        let mut slices = [SliceMarker::default(); MAX_STEPS];
        if self.frames.is_empty() || !(1..=MAX_STEPS as u8).contains(&count) {
            return slices;
        }
        if count == 1 {
            slices[0] = SliceMarker {
                start: 0,
                end: self.frames.len() as u32,
                active: true,
                bass: true,
            };
            return slices;
        }
        const HOP: usize = 256;
        let energies: Vec<f32> = self
            .frames
            .chunks(HOP)
            .map(|window| {
                window.iter().map(|f| f[0].abs() + f[1].abs()).sum::<f32>()
                    / (2 * window.len()) as f32
            })
            .collect();
        let novelty: Vec<f32> = (0..energies.len())
            .map(|i| (energies[i] - energies[i.saturating_sub(2)]).max(0.0))
            .collect();
        let max_novelty = novelty.iter().copied().fold(0.0_f32, f32::max);
        let threshold = (max_novelty * 0.04).max(0.008);
        let frame_count = self.frames.len() as u32;
        for slot in 0..count as usize {
            let bin_start = frame_count as usize * slot / count as usize;
            let bin_end = frame_count as usize * (slot + 1) / count as usize;
            let early_end = bin_start + (bin_end - bin_start) * 45 / 100;
            let mut early = None;
            let mut strongest = None;
            for i in (bin_start / HOP).max(2)..(bin_end / HOP).min(novelty.len().saturating_sub(2))
            {
                if novelty[i] < threshold
                    || novelty[i] < novelty[i - 1]
                    || novelty[i] < novelty[i + 1]
                {
                    continue;
                }
                if i * HOP < early_end && early.is_none() {
                    early = Some(i);
                }
                if strongest.is_none_or(|prev: usize| novelty[i] > novelty[prev]) {
                    strongest = Some(i);
                }
            }
            let chosen = early.or(strongest);
            let start = chosen.map_or(bin_start as u32, |i| (i.saturating_sub(1) * HOP) as u32);
            slices[slot] = SliceMarker {
                start,
                end: frame_count,
                active: chosen.is_some(),
                bass: false,
            };
        }
        for slot in 0..count as usize - 1 {
            slices[slot].end = slices[slot + 1].start.max(slices[slot].start + 1);
        }
        // Rank the sound-aligned onsets by low-frequency energy. The user can
        // correct any guessed kick markers in the source editor.
        let mut lows = [0.0_f32; MAX_STEPS];
        let alpha = (2.0 * std::f32::consts::PI * 130.0 / self.sample_rate as f32).min(1.0);
        for (index, slice) in slices[..count as usize].iter().enumerate() {
            if !slice.active {
                continue;
            }
            let start = slice.start as usize;
            let end = (start + (self.sample_rate as usize / 9))
                .min(slice.end as usize)
                .min(self.frames.len());
            let mut low = 0.0_f32;
            let mut sum = 0.0_f32;
            for frame in &self.frames[start..end] {
                low += alpha * ((frame[0] + frame[1]) * 0.5 - low);
                sum += low * low;
            }
            lows[index] = (sum / (end - start).max(1) as f32).sqrt();
        }
        let peak = lows[..count as usize]
            .iter()
            .copied()
            .fold(0.0_f32, f32::max);
        for index in 0..count as usize {
            slices[index].bass = slices[index].active && lows[index] >= (peak * 0.45).max(0.008);
        }
        slices
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supplied_breaks_decode_as_nonempty_one_bar_stereo_audio() {
        for (index, file) in super::super::SOURCE_FILES.iter().enumerate() {
            let path = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("sample")
                .join(file);
            let sample = StereoSample::load(&path).unwrap();
            let embedded = StereoSample::from_bytes(super::super::SOURCE_AUDIO[index]).unwrap();
            assert_eq!(embedded.frames, sample.frames);
            // One bar at 174 BPM and 44.1 kHz is about 60,828 frames.
            assert!((60_820..=60_840).contains(&sample.frames.len()));
            assert!(sample.frames.iter().any(|frame| frame[0].abs() > 0.01));
            assert!(sample.frames.iter().any(|frame| frame[1].abs() > 0.01));
            assert_eq!(sample.at_beat(4.0, 4.0, 0), 0.0);
        }
    }

    #[test]
    fn suggested_markers_stay_ordered_and_include_playable_slices() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("sample")
            .join(super::super::SOURCE_FILES[0]);
        let sample = StereoSample::load(&path).unwrap();
        for count in [1, 8, 16, 32] {
            let markers = sample.suggested_slices(count);
            assert!(markers[..count as usize].iter().any(|marker| marker.active));
            for (index, marker) in markers[..count as usize].iter().enumerate() {
                assert!(marker.start < marker.end);
                assert!(marker.end <= sample.frames.len() as u32);
                if index > 0 {
                    assert_eq!(markers[index - 1].end, marker.start);
                }
            }
        }
    }

    #[test]
    fn every_supplied_break_has_a_bass_hit_at_the_default_slice_count() {
        for file in super::super::SOURCE_FILES {
            let path = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("sample")
                .join(file);
            let markers = StereoSample::load(&path).unwrap().suggested_slices(8);
            assert!(
                markers[..8]
                    .iter()
                    .any(|marker| marker.active && marker.bass)
            );
        }
    }
}
