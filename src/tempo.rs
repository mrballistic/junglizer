//! The tempo rule for user-loaded breaks: how many beats a file holds, and its BPM.
//! Mirrors `tools/tempo/check_tempo_rule.py`; keep them in step.

pub const MIN_BEATS: u8 = 4;
pub const MAX_BEATS: u8 = 32;
pub const MIN_BPM: f64 = 80.0;
pub const MAX_BPM: f64 = 195.0;
pub const TOLERANCE: f64 = 0.01;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Reading {
    pub beats: u8,
    pub bpm: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    TooShort,
    TooLong,
}

impl Reading {
    pub fn label(&self) -> String {
        format!("{} beats · {:.1} BPM", self.beats, self.bpm)
    }
}

impl Refusal {
    pub fn message(&self) -> &'static str {
        match self {
            Refusal::TooShort => {
                "That file is too short. Breaks need at least 4 beats at 195 BPM or slower."
            }
            Refusal::TooLong => "That file is too long. Breaks can be up to 32 beats.",
        }
    }
}

fn in_bpm_range(bpm: f64) -> bool {
    (MIN_BPM..=MAX_BPM).contains(&bpm)
}

/// `^\d{2,3}(\.\d)?$`
fn parse_tempo_token(t: &str) -> Option<f64> {
    let (int, frac) = match t.split_once('.') {
        Some((i, f)) => (i, Some(f)),
        None => (t, None),
    };
    let digits = |s: &str| s.bytes().all(|b| b.is_ascii_digit());
    if !(2..=3).contains(&int.len()) || !digits(int) {
        return None;
    }
    if let Some(f) = frac
        && (f.len() != 1 || !digits(f))
    {
        return None;
    }
    t.parse().ok()
}

/// Split on `_`, `-`, space and `.`, except a `.` between two digits.
fn tokens(stem: &str) -> Vec<&str> {
    let b = stem.as_bytes();
    let mut out = Vec::new();
    let mut start = 0;
    for (i, &c) in b.iter().enumerate() {
        let sep = match c {
            b'_' | b'-' | b' ' => true,
            b'.' => {
                !(i > 0
                    && b[i - 1].is_ascii_digit()
                    && b.get(i + 1).is_some_and(u8::is_ascii_digit))
            }
            _ => false,
        };
        if sep {
            if i > start {
                out.push(&stem[start..i]);
            }
            start = i + 1;
        }
    }
    if start < stem.len() {
        out.push(&stem[start..]);
    }
    out
}

/// A BPM carried in the file name, if one in 80–195 is there. Accepts a bare number token
/// (`174`, `119.9`), a number with `bpm` attached (`174bpm`) or a number then a `bpm` token.
pub fn bpm_from_name(file_name: &str) -> Option<f64> {
    let stem = match file_name.rsplit_once('.') {
        Some((s, ext)) if !ext.is_empty() && !ext.bytes().all(|b| b.is_ascii_digit()) => s,
        _ => file_name,
    };
    tokens(stem)
        .into_iter()
        .filter_map(|tok| {
            let lower = tok.to_ascii_lowercase();
            match lower.strip_suffix("bpm") {
                Some(num) => parse_tempo_token(num),
                None => parse_tempo_token(tok),
            }
        })
        .find(|&v| in_bpm_range(v))
}

pub fn reading_for(beats: u8, duration_secs: f64) -> Reading {
    Reading {
        beats,
        bpm: f64::from(beats) * 60.0 / duration_secs,
    }
}

pub fn read_length(duration_secs: f64, file_name: &str) -> Result<Reading, Refusal> {
    if let Some(h) = bpm_from_name(file_name) {
        let exact = h * duration_secs / 60.0;
        let beats = (exact / 4.0).round() * 4.0;
        if beats > 0.0 && (exact - beats).abs() <= beats * TOLERANCE {
            return if beats > f64::from(MAX_BEATS) {
                Err(Refusal::TooLong)
            } else {
                Ok(reading_for(beats as u8, duration_secs))
            };
        }
    }
    let mut best = None;
    for beats in (MIN_BEATS..=MAX_BEATS).step_by(4) {
        let r = reading_for(beats, duration_secs);
        if bpm_fits(r.bpm) {
            best = Some(r);
        }
    }
    best.ok_or(if duration_secs < f64::from(MIN_BEATS) * 60.0 / MAX_BPM {
        Refusal::TooShort
    } else {
        Refusal::TooLong
    })
}

fn bpm_fits(bpm: f64) -> bool {
    bpm >= MIN_BPM * (1.0 - TOLERANCE) && bpm <= MAX_BPM * (1.0 + TOLERANCE)
}

fn within_rule(r: Reading) -> bool {
    (MIN_BEATS..=MAX_BEATS).contains(&r.beats) && r.beats % 4 == 0 && bpm_fits(r.bpm)
}

/// Read the same audio as half the beats at half the tempo.
pub fn halved(r: Reading) -> Option<Reading> {
    let h = Reading {
        beats: r.beats / 2,
        bpm: r.bpm / 2.0,
    };
    (r.beats % 2 == 0 && within_rule(h)).then_some(h)
}

/// Read the same audio as twice the beats at twice the tempo.
pub fn doubled(r: Reading) -> Option<Reading> {
    let d = Reading {
        beats: r.beats.checked_mul(2)?,
        bpm: r.bpm * 2.0,
    };
    within_rule(d).then_some(d)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(name: &str, secs: f64) -> (u8, f64) {
        let r = read_length(secs, name).unwrap();
        (r.beats, (r.bpm * 10.0).round() / 10.0)
    }

    #[test]
    fn filename_tempo_wins_over_fastest_fit() {
        assert_eq!(
            ok("KTB1_114_ThinkBreak_Original_NoVocal.wav", 4.211),
            (8, 114.0)
        );
        assert_eq!(
            ok("KSB1_119.9_SportBreak_Original_Vinyl_HD.wav", 8.007),
            (16, 119.9)
        );
        assert_eq!(ok("loop 174 BPM.aif", 60_828.0 / 44_100.0), (4, 174.0));
        assert_eq!(ok("loop_174bpm.mp3", 60_828.0 / 44_100.0), (4, 174.0));
    }

    #[test]
    fn longer_than_32_beats_at_the_named_tempo_is_refused() {
        assert_eq!(
            read_length(22.588, "AJ_170_Drum_Loop_Continuum.wav"),
            Err(Refusal::TooLong)
        );
        assert_eq!(
            read_length(14.458, "AJ_166_Drum_Loop_Boxing.wav"),
            Err(Refusal::TooLong)
        );
    }

    #[test]
    fn without_a_usable_hint_the_fastest_fit_decides() {
        assert_eq!(
            ok("174_JunglizerAmen_Bar_01.wav", 60_828.0 / 44_100.0),
            (4, 174.0)
        );
        assert_eq!(ok("Break 2.wav", 2.759), (8, 174.0));
        assert_eq!(ok("loop.wav", 22.588), (32, 85.0)); // known limit, documented in the design
        // A named tempo that doesn't make whole bars is ignored.
        assert_eq!(
            ok("KSCB1_114_ScorpioBreak_00_Original_Vinyl.wav", 6.665),
            (20, 180.0)
        );
    }

    #[test]
    fn outside_the_range_is_refused() {
        assert_eq!(read_length(1.0, "x.wav"), Err(Refusal::TooShort));
        assert_eq!(read_length(30.0, "x.wav"), Err(Refusal::TooLong));
    }

    #[test]
    fn name_numbers_that_are_not_tempos_are_ignored() {
        for name in [
            "Bar_01.wav",
            "Break 2.wav",
            "808 Kick.wav",
            "Session 2024.wav",
            "AmenBreak_Cut_02.wav",
            "KAPB1.wav",
        ] {
            assert_eq!(bpm_from_name(name), None, "{name}");
        }
        assert_eq!(bpm_from_name("AJ_166_Drum_Loop_Baracks.wav"), Some(166.0));
        assert_eq!(bpm_from_name("KSB1_119.9_SportBreak.wav"), Some(119.9));
        assert_eq!(bpm_from_name("70_slow_loop.wav"), None); // outside 80–195
    }

    #[test]
    fn half_and_double_stay_inside_the_rule() {
        let r = reading_for(16, 16.0 * 60.0 / 174.0);
        assert_eq!(halved(r).map(|h| h.beats), Some(8));
        assert_eq!(doubled(r), None); // 348 BPM
        assert_eq!(halved(reading_for(4, 1.38)), None); // 2 beats isn't whole bars
        assert_eq!(halved(reading_for(12, 4.14)), None); // 6 beats isn't whole bars
        assert_eq!(
            reading_for(16, 16.0 * 60.0 / 174.0).label(),
            "16 beats · 174.0 BPM"
        );
    }
}
