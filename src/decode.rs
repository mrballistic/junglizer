//! Decode WAV, AIFF and MP3 into `StereoSample` with Symphonia. Runs on the
//! UI/init thread only, never from the audio callback.

use crate::sample::StereoSample;
use std::io::Cursor;
use std::path::Path;
use symphonia::core::codecs::audio::AudioDecoderOptions;
use symphonia::core::errors::Error;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, TrackType};
use symphonia::core::io::{MediaSource, MediaSourceStream};
use symphonia::core::meta::MetadataOptions;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodeError {
    /// The file does not exist.
    Missing,
    /// The file could not be read or is not audio we can decode.
    Unreadable,
    /// The file decoded, but held no audio frames.
    Empty,
}

pub fn decode_file(path: &Path) -> Result<StereoSample, DecodeError> {
    let file = std::fs::File::open(path).map_err(|error| match error.kind() {
        std::io::ErrorKind::NotFound => DecodeError::Missing,
        _ => DecodeError::Unreadable,
    })?;
    let extension = path.extension().and_then(|extension| extension.to_str());
    decode_source(Box::new(file), extension)
}

pub fn decode_bytes(bytes: &[u8], extension: Option<&str>) -> Result<StereoSample, DecodeError> {
    decode_source(Box::new(Cursor::new(bytes.to_vec())), extension)
}

fn decode_source(
    source: Box<dyn MediaSource>,
    extension: Option<&str>,
) -> Result<StereoSample, DecodeError> {
    let stream = MediaSourceStream::new(source, Default::default());
    let mut hint = Hint::new();
    if let Some(extension) = extension {
        hint.with_extension(extension);
    }
    let mut format = symphonia::default::get_probe()
        .probe(
            &hint,
            stream,
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .map_err(|_| DecodeError::Unreadable)?;
    let track = format
        .default_track(TrackType::Audio)
        .ok_or(DecodeError::Unreadable)?;
    let track_id = track.id;
    let declared_frames = track.num_frames;
    let params = track
        .codec_params
        .as_ref()
        .and_then(|params| params.audio())
        .ok_or(DecodeError::Unreadable)?;
    // Gapless is on by default; spelled out so encoder padding never skews length.
    let mut decoder = symphonia::default::get_codecs()
        .make_audio_decoder(params, &AudioDecoderOptions::default().gapless(true))
        .map_err(|_| DecodeError::Unreadable)?;

    let mut frames: Vec<[f32; 2]> = Vec::new();
    let mut sample_rate = params.sample_rate.unwrap_or(0);
    let mut interleaved: Vec<f32> = Vec::new();
    loop {
        let packet = match format.next_packet() {
            Ok(Some(packet)) => packet,
            Ok(None) => break,
            Err(_) if !frames.is_empty() => break,
            Err(_) => return Err(DecodeError::Unreadable),
        };
        if packet.track_id != track_id {
            continue;
        }
        match decoder.decode(&packet) {
            Ok(buffer) => {
                let channels = buffer.spec().channels().count();
                if channels == 0 {
                    return Err(DecodeError::Unreadable);
                }
                sample_rate = buffer.spec().rate();
                buffer.copy_to_vec_interleaved::<f32>(&mut interleaved);
                frames.extend(interleaved.chunks_exact(channels).map(|frame| {
                    let left = frame[0];
                    [left, if channels == 1 { left } else { frame[1] }]
                }));
            }
            Err(Error::DecodeError(_)) => {}
            Err(_) => break,
        }
    }
    if frames.is_empty() {
        return Err(DecodeError::Empty);
    }
    // A container that promises more audio than it holds was cut off. Allow 1%
    // slack for codecs whose declared length includes padding.
    if let Some(declared) = declared_frames
        && (frames.len() as u64) < declared - declared / 100
    {
        return Err(DecodeError::Unreadable);
    }
    if sample_rate == 0 {
        return Err(DecodeError::Unreadable);
    }
    Ok(StereoSample {
        frames,
        sample_rate,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    fn fixture(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name)
    }

    #[test]
    fn every_supported_format_decodes_to_stereo_at_its_own_rate() {
        for (name, rate) in [
            ("loop_160_4beats_mono16_48k.wav", 48_000),
            ("loop_160_4beats_stereo24_44k.aif", 44_100),
            ("loop_160_4beats_float32_48k.wav", 48_000),
            ("loop_160_4beats.mp3", 44_100),
        ] {
            let s = decode_file(&fixture(name)).unwrap();
            assert_eq!(s.sample_rate, rate, "{name}");
            assert!(
                (s.duration_secs() - 1.5).abs() <= 0.015,
                "{name}: {}",
                s.duration_secs()
            );
            assert!(
                s.frames
                    .iter()
                    .any(|f| f[0].abs() > 0.1 && f[1].abs() > 0.1),
                "{name}"
            );
        }
    }

    #[test]
    fn mono_is_copied_to_both_channels() {
        let s = decode_file(&fixture("loop_160_4beats_mono16_48k.wav")).unwrap();
        assert!(s.frames.iter().all(|f| f[0] == f[1]));
    }

    #[test]
    fn bad_files_return_errors_not_panics() {
        assert_eq!(
            decode_file(&fixture("no_such_file.wav")).err(),
            Some(DecodeError::Missing)
        );
        assert_eq!(
            decode_file(&fixture("not_audio.wav")).err(),
            Some(DecodeError::Unreadable)
        );
        assert!(matches!(
            decode_file(&fixture("empty.wav")),
            Err(DecodeError::Unreadable | DecodeError::Empty)
        ));
        assert!(decode_file(&fixture("truncated.wav")).is_err());
    }
}
