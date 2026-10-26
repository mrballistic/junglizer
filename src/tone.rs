//! Global Thin ↔ Thicc processing. No allocations or file access in audio.

use std::f64::consts::TAU;

#[derive(Clone, Copy, Default)]
struct BassVoice {
    active: bool,
    phase: f64,
    age: f64,
    hz: f64,
    sub: bool,
}

#[derive(Default)]
pub struct Tone {
    hp_x: [f32; 2],
    hp_y: [f32; 2],
    voices: [BassVoice; 12],
    next_voice: usize,
}

impl Tone {
    pub fn trigger(&mut self, root: u8) {
        let semitone = root.min(11) as f64;
        // MIDI C1 = 24, C0 = 12 under the common scientific convention.
        let hz = 440.0 * 2.0_f64.powf((24.0 + semitone - 69.0) / 12.0);
        for (frequency, sub) in [(hz, false), (hz / 2.0, true)] {
            self.voices[self.next_voice] = BassVoice {
                active: true,
                phase: 0.0,
                age: 0.0,
                hz: frequency,
                sub,
            };
            self.next_voice = (self.next_voice + 1) % self.voices.len();
        }
    }

    pub fn process(&mut self, frame: [f32; 2], amount: f32, sample_rate: f64) -> [f32; 2] {
        let thin = (-amount).clamp(0.0, 1.0);
        let cutoff = 28.0_f64 * (1200.0_f64 / 28.0).powf(thin as f64);
        let rc = 1.0 / (TAU * cutoff);
        let alpha = (rc / (rc + 1.0 / sample_rate)) as f32;
        let mut output = frame;
        for ch in 0..2 {
            let hp = alpha * (self.hp_y[ch] + frame[ch] - self.hp_x[ch]);
            self.hp_x[ch] = frame[ch];
            self.hp_y[ch] = hp;
            if thin > 0.0 {
                output[ch] = frame[ch] * (1.0 - thin) + hp * thin;
            }
        }
        let thicc = amount.clamp(0.0, 1.0);
        let extra = ((thicc - 0.72) / 0.28).clamp(0.0, 1.0);
        let mut bass = 0.0_f32;
        for voice in &mut self.voices {
            if !voice.active {
                continue;
            }
            let envelope = (-voice.age * if voice.sub { 4.5 } else { 7.0 }).exp();
            if envelope < 0.001 {
                voice.active = false;
                continue;
            }
            let pitch_sweep = if voice.sub {
                0.0
            } else {
                1.4 * (-voice.age * 55.0).exp()
            };
            let hz = voice.hz * (1.0 + pitch_sweep);
            voice.phase = (voice.phase + TAU * hz / sample_rate).rem_euclid(TAU);
            let attack = (voice.age * 800.0).min(1.0);
            let level = if voice.sub {
                0.27 * extra
            } else {
                0.42 * thicc
            };
            bass += (voice.phase.sin() * envelope * attack) as f32 * level;
            voice.age += 1.0 / sample_rate;
        }
        output[0] += bass;
        output[1] += bass;
        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn center_is_dry_and_thin_removes_dc() {
        let mut tone = Tone::default();
        assert_eq!(tone.process([0.25, -0.25], 0.0, 44_100.0), [0.25, -0.25]);
        let mut last = [0.0; 2];
        for _ in 0..44_100 {
            last = tone.process([0.5, 0.5], -1.0, 44_100.0);
        }
        assert!(last[0].abs() < 0.005);
    }

    #[test]
    fn thicc_generates_a_tuned_decay() {
        let mut tone = Tone::default();
        tone.trigger(0);
        let peak = (0..4_410)
            .map(|_| tone.process([0.0, 0.0], 1.0, 44_100.0)[0].abs())
            .fold(0.0_f32, f32::max);
        assert!(peak > 0.1);
    }
}
