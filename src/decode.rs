//! Decode WAV, AIFF and MP3 into `StereoSample` with Symphonia. Runs on the
//! UI/init thread only, never from the audio callback.

use crate::sample::StereoSample;
use crate::tempo::MAX_BREAK_SECS;
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
    /// The file declares, or decodes to, more audio than the cap allows.
    TooLong,
}

/// The sample rates we accept, declared or decoded. Anything outside is a
/// broken or crafted file: a huge rate would stretch the `max_secs` cap
/// (which is in seconds, so scales with the rate) past any sane memory use.
const MIN_RATE: u32 = 8_000;
const MAX_RATE: u32 = 384_000;

/// An absolute frame cap for capped decodes, independent of the file's
/// rate: the longest break at the highest accepted rate (about 9.3 M
/// frames, 74 MB as stereo f32).
const MAX_CAPPED_FRAMES: u64 = (MAX_BREAK_SECS * MAX_RATE as f64) as u64;

/// Capped loads refuse a file bigger than this before opening it. A
/// legitimate break is at most about 74 MB even as 384 kHz stereo f32 WAV.
const MAX_CAPPED_FILE_BYTES: u64 = 512 * 1024 * 1024;

fn rate_ok(rate: u32) -> bool {
    (MIN_RATE..=MAX_RATE).contains(&rate)
}

pub fn decode_file(path: &Path) -> Result<StereoSample, DecodeError> {
    decode_file_capped(path, None)
}

/// Like `decode_file`, but gives up with `TooLong` as soon as the file
/// declares, or decodes to, more than `max_secs` of audio, so a huge file
/// is never held in memory whole.
pub fn decode_file_capped(path: &Path, max_secs: Option<f64>) -> Result<StereoSample, DecodeError> {
    let io_error = |error: std::io::Error| match error.kind() {
        std::io::ErrorKind::NotFound => DecodeError::Missing,
        _ => DecodeError::Unreadable,
    };
    if max_secs.is_some()
        && std::fs::metadata(path).map_err(io_error)?.len() > MAX_CAPPED_FILE_BYTES
    {
        return Err(DecodeError::TooLong);
    }
    let file = std::fs::File::open(path).map_err(io_error)?;
    let extension = path.extension().and_then(|extension| extension.to_str());
    decode_source(Box::new(file), extension, max_secs)
}

pub fn decode_bytes(bytes: &[u8], extension: Option<&str>) -> Result<StereoSample, DecodeError> {
    decode_source(Box::new(Cursor::new(bytes.to_vec())), extension, None)
}

/// Whether `frames` at `rate` run longer than `max_secs`, or past
/// `MAX_CAPPED_FRAMES` whatever the rate. Callers check `rate_ok` first.
fn over_cap(frames: u64, rate: u32, max_secs: Option<f64>) -> bool {
    max_secs.is_some_and(|max| frames > MAX_CAPPED_FRAMES || frames as f64 > max * f64::from(rate))
}

fn decode_source(
    source: Box<dyn MediaSource>,
    extension: Option<&str>,
    max_secs: Option<f64>,
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
    if params.sample_rate.is_some_and(|rate| !rate_ok(rate)) {
        return Err(DecodeError::Unreadable);
    }
    if let Some(declared) = declared_frames
        && over_cap(declared, params.sample_rate.unwrap_or(MAX_RATE), max_secs)
    {
        return Err(DecodeError::TooLong);
    }
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
                if !rate_ok(sample_rate) {
                    return Err(DecodeError::Unreadable);
                }
                // Covers files that declare no length, or lie about it. Checked
                // before copying, so an oversized buffer is never copied.
                if over_cap(
                    (frames.len() + buffer.frames()) as u64,
                    sample_rate,
                    max_secs,
                ) {
                    return Err(DecodeError::TooLong);
                }
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
    fn a_declared_length_over_the_cap_is_refused_before_decoding() {
        let path = fixture("loop_160_4beats_mono16_48k.wav"); // 1.5 s
        assert_eq!(
            decode_file_capped(&path, Some(1.0)).err(),
            Some(DecodeError::TooLong)
        );
        assert!(decode_file_capped(&path, Some(2.0)).is_ok());
    }

    #[test]
    fn an_undeclared_length_is_cut_off_at_the_cap_while_decoding() {
        // A streaming WAV: RIFF and data sizes of u32::MAX mean "unknown".
        let mut bytes = std::fs::read(fixture("loop_160_4beats_mono16_48k.wav")).unwrap();
        bytes[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
        let data = bytes.windows(4).position(|w| w == b"data").unwrap();
        bytes[data + 4..data + 8].copy_from_slice(&u32::MAX.to_le_bytes());
        let decode = |cap| decode_source(Box::new(Cursor::new(bytes.clone())), Some("wav"), cap);
        assert_eq!(decode(Some(1.0)).err(), Some(DecodeError::TooLong));
        let whole = decode(None).unwrap();
        assert!((whole.duration_secs() - 1.5).abs() <= 0.015);
    }

    /// The mono 48 kHz fixture with its `fmt ` sample rate (and byte rate,
    /// to stay consistent) replaced by `rate`.
    fn wav_with_rate(rate: u32) -> Vec<u8> {
        let mut bytes = std::fs::read(fixture("loop_160_4beats_mono16_48k.wav")).unwrap();
        let fmt = bytes.windows(4).position(|w| w == b"fmt ").unwrap();
        let block_align = u32::from(u16::from_le_bytes([bytes[fmt + 20], bytes[fmt + 21]]));
        bytes[fmt + 12..fmt + 16].copy_from_slice(&rate.to_le_bytes());
        bytes[fmt + 16..fmt + 20].copy_from_slice(&rate.wrapping_mul(block_align).to_le_bytes());
        bytes
    }

    #[test]
    fn sample_rates_outside_the_accepted_range_are_unreadable() {
        for rate in [4_000_000_000, 0, 7_999, 384_001] {
            let bytes = wav_with_rate(rate);
            for cap in [None, Some(MAX_BREAK_SECS)] {
                assert_eq!(
                    decode_source(Box::new(Cursor::new(bytes.clone())), Some("wav"), cap).err(),
                    Some(DecodeError::Unreadable),
                    "{rate} Hz, cap {cap:?}"
                );
            }
        }
        // The edges of the range are fine.
        for rate in [8_000, 384_000] {
            let s = decode_source(
                Box::new(Cursor::new(wav_with_rate(rate))),
                Some("wav"),
                None,
            )
            .unwrap();
            assert_eq!(s.sample_rate, rate);
        }
    }

    #[test]
    fn the_absolute_frame_cap_holds_whatever_the_rate() {
        assert!(over_cap(MAX_CAPPED_FRAMES + 1, MAX_RATE, Some(f64::MAX)));
        assert!(!over_cap(
            MAX_CAPPED_FRAMES,
            MAX_RATE,
            Some(MAX_BREAK_SECS + 1.0)
        ));
        assert!(!over_cap(u64::MAX, MAX_RATE, None));
    }

    #[test]
    fn a_capped_load_refuses_an_oversized_file_before_opening_it() {
        let dir = std::env::temp_dir().join(format!("junglizer-decode-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("huge.wav");
        // Sparse: sets the length without writing 512 MB.
        let file = std::fs::File::create(&path).unwrap();
        file.set_len(MAX_CAPPED_FILE_BYTES + 1).unwrap();
        drop(file);
        let capped = decode_file_capped(&path, Some(MAX_BREAK_SECS)).err();
        let uncapped = decode_file(&path).err();
        std::fs::remove_file(&path).unwrap();
        let _ = std::fs::remove_dir(&dir);
        assert_eq!(capped, Some(DecodeError::TooLong));
        // Uncapped decodes keep their behavior: zeros aren't audio.
        assert_eq!(uncapped, Some(DecodeError::Unreadable));
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
