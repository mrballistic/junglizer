use std::path::Path;

/// Decoded stereo frames. Files are opened during plugin initialization,
/// never from the audio callback.
#[derive(Default)]
pub struct StereoSample {
    pub frames: Vec<[f32; 2]>,
}

impl StereoSample {
    pub fn load(path: &Path) -> Result<Self, hound::Error> {
        let mut reader = hound::WavReader::open(path)?;
        let spec = reader.spec();
        if spec.channels != 2 || spec.sample_format != hound::SampleFormat::Int {
            return Err(hound::Error::Unsupported);
        }
        let scale = (1_u64 << (spec.bits_per_sample - 1)) as f32;
        let values: Vec<i32> = reader.samples::<i32>().collect::<Result<_, _>>()?;
        let frames = values
            .chunks_exact(2)
            .map(|pair| [pair[0] as f32 / scale, pair[1] as f32 / scale])
            .collect();
        Ok(Self { frames })
    }

    /// Linear interpolation at a position in the source bar.
    pub fn at_beat(&self, beat: f64, channel: usize) -> f32 {
        if self.frames.is_empty() || !(0.0..4.0).contains(&beat) {
            return 0.0;
        }
        let position = beat * self.frames.len() as f64 / 4.0;
        let first = position.floor() as usize;
        let second = (first + 1).min(self.frames.len() - 1);
        let fraction = (position - first as f64) as f32;
        self.frames[first][channel] * (1.0 - fraction) + self.frames[second][channel] * fraction
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supplied_breaks_decode_as_nonempty_one_bar_stereo_audio() {
        for file in super::super::SOURCE_FILES {
            let path = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("sample")
                .join(file);
            let sample = StereoSample::load(&path).unwrap();
            // One bar at 174 BPM and 44.1 kHz is about 60,828 frames.
            assert!((60_820..=60_840).contains(&sample.frames.len()));
            assert!(sample.frames.iter().any(|frame| frame[0].abs() > 0.01));
            assert!(sample.frames.iter().any(|frame| frame[1].abs() > 0.01));
            assert_eq!(sample.at_beat(4.0, 0), 0.0);
        }
    }
}
