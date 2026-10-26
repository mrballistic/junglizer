use truce::prelude::*;

pub mod edit_state;
pub mod pattern;
mod sample;
mod suggest;
mod ui;

use edit_state::{EditState, StepPlacement};
use sample::StereoSample;
use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};

#[derive(Params)]
pub struct JunglizerParams {
    #[skip]
    pub audition: AtomicU32,
    #[param(
        name = "Gain",
        range = "linear(-60, 6)",
        unit = "dB",
        smooth = "exp(5)"
    )]
    pub gain: FloatParam,
    #[param(name = "Bar 1", range = "discrete(1, 4)", default = 1)]
    pub bar_1: IntParam,
    #[param(name = "Bar 2", range = "discrete(1, 4)", default = 2)]
    pub bar_2: IntParam,
    #[param(name = "Bar 3", range = "discrete(1, 4)", default = 3)]
    pub bar_3: IntParam,
    #[param(name = "Bar 4", range = "discrete(1, 4)", default = 4)]
    pub bar_4: IntParam,
}

#[derive(Default)]
pub struct Junglizer {
    sources: [StereoSample; 4],
    edit: EditState,
    audition: Option<Audition>,
}

#[derive(Clone, Copy, Default)]
struct Audition {
    source_loop: usize,
    position: f64,
    end: u32,
}

const SOURCE_FILES: [&str; 4] = [
    "174_AmenBreak_Cut_01_NoStab.wav",
    "174_AmenBreak_Cut_02.wav",
    "174_AmenBreak_Cut_03.wav",
    "174_AmenBreak_Cut_04.wav",
];

impl Junglizer {
    fn load_sources() -> Self {
        let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("sample");
        let sources: [StereoSample; 4] = std::array::from_fn(|index| {
            StereoSample::load(&directory.join(SOURCE_FILES[index])).unwrap_or_default()
        });
        let mut edit = EditState::default();
        for (index, source) in sources.iter().enumerate() {
            let suggested = source.suggested_slices(edit.source_count);
            edit.set_markers(index, &suggested[..edit.source_count as usize]);
        }
        Self {
            sources,
            edit,
            audition: None,
        }
    }

    fn placement_frame(&self, placement: &StepPlacement, beat: f64, tempo: f64) -> [f32; 2] {
        let Some(source) = self.sources.get(placement.source_loop as usize) else {
            return [0.0; 2];
        };
        let Some(slice) = self.edit.slice(
            placement.source_loop as usize,
            placement.source_slice as usize,
        ) else {
            return [0.0; 2];
        };
        if slice.end <= slice.start || slice.end as usize > source.frames.len() {
            return [0.0; 2];
        }
        let start = self.edit.slot_start(placement.start_slot);
        let duration = 16.0 * f64::from(placement.span_slots) / f64::from(self.edit.player_slots);
        let repeat_duration = duration / f64::from(placement.repeats);
        let beat_in_repeat = (beat - start).rem_euclid(repeat_duration);
        let position = if placement.mode == 1 {
            f64::from(slice.start)
                + beat_in_repeat / repeat_duration * f64::from(slice.end - slice.start)
        } else {
            f64::from(slice.start) + beat_in_repeat * f64::from(source.sample_rate) * 60.0 / tempo
        };
        if position >= f64::from(slice.end) {
            return [0.0; 2];
        }
        let levels = (1_u32 << (placement.bit_depth - 1)) as f32;
        [0, 1].map(|ch| {
            let value = source.at_frame(position, ch);
            if placement.bit_depth == 16 {
                value
            } else {
                (value * levels).round() / levels
            }
        })
    }

    fn frame_at_beat(&self, beat: f64, tempo: f64, bar_loops: &[usize; 4]) -> [f32; 2] {
        let bar = (beat / 4.0) as usize;
        let source = bar_loops[bar].min(3);
        let beat_in_bar = beat - bar as f64 * 4.0;
        let ghost = [
            self.sources[source].at_beat(beat_in_bar, 0),
            self.sources[source].at_beat(beat_in_bar, 1),
        ];
        let Some(placement) = self.edit.placement_at(beat) else {
            return ghost;
        };
        let placed = self.placement_frame(placement, beat, tempo);
        let start = self.edit.slot_start(placement.start_slot);
        let end = 16.0 * f64::from(placement.start_slot + placement.span_slots)
            / f64::from(self.edit.player_slots);
        let fade_beats = (tempo / 60.0 * 0.0005).min((end - start) / 8.0);
        let mix = ((beat - start) / fade_beats)
            .min((end - beat) / fade_beats)
            .clamp(0.0, 1.0) as f32;
        [
            ghost[0] * (1.0 - mix) + placed[0] * mix,
            ghost[1] * (1.0 - mix) + placed[1] * mix,
        ]
    }
}

impl PluginLogic for Junglizer {
    type Params = JunglizerParams;
    type DspState = Self;

    fn bus_layouts() -> Vec<BusLayout> {
        vec![BusLayout::new().with_output("Main", ChannelConfig::Stereo)]
    }

    fn init(_params: &Self::Params, _cx: &InitContext) -> Self::DspState {
        Self::load_sources()
    }

    fn process(
        state: &mut Self::DspState,
        params: &Self::Params,
        buffer: &mut AudioBuffer,
        _events: &EventList,
        context: &mut ProcessContext,
    ) -> ProcessStatus {
        let bar_loops = [
            params.bar_1.value() as usize - 1,
            params.bar_2.value() as usize - 1,
            params.bar_3.value() as usize - 1,
            params.bar_4.value() as usize - 1,
        ];
        let transport = context.transport;
        let beat_step = transport.tempo / (60.0 * context.sample_rate);
        let request = params.audition.swap(0, Ordering::Relaxed);
        if request > 0 {
            let index = request as usize - 1;
            let source_loop = index / 32;
            let slice_index = index % 32;
            state.audition = state
                .edit
                .slice(source_loop, slice_index)
                .map(|slice| Audition {
                    source_loop,
                    position: slice.start as f64,
                    end: slice.end,
                });
        }
        for i in 0..buffer.num_samples() {
            let mut frame = [0.0_f32; 2];
            if transport.playing && beat_step.is_finite() && beat_step > 0.0 {
                let beat = (transport.position_beats + i as f64 * beat_step).rem_euclid(16.0);
                frame = state.frame_at_beat(beat, transport.tempo, &bar_loops);
            }
            if let Some(mut preview) = state.audition {
                if let Some(source) = state.sources.get(preview.source_loop) {
                    if preview.position < preview.end as f64 {
                        frame[0] += source.at_frame(preview.position, 0);
                        frame[1] += source.at_frame(preview.position, 1);
                        preview.position += f64::from(source.sample_rate) / context.sample_rate;
                        state.audition = Some(preview);
                    } else {
                        state.audition = None;
                    }
                } else {
                    state.audition = None;
                }
            }
            let gain = 10.0_f32.powf(params.gain.read() / 20.0);
            for (ch, sample) in frame
                .iter()
                .enumerate()
                .take(buffer.num_output_channels().min(2))
            {
                buffer.output(ch)[i] = sample * gain;
            }
        }
        ProcessStatus::Normal
    }

    fn snapshot_into(state: &Self::DspState, buf: &mut Vec<u8>) -> bool {
        state.edit.serialize_into(buf);
        true
    }

    fn snapshot_version(state: &Self::DspState) -> Option<u64> {
        Some(state.edit.version)
    }

    fn snapshot_prealloc_hint() -> usize {
        edit_state::SERIALIZED_LEN
    }

    fn load_state(state: &mut Self::DspState, data: &[u8]) -> Result<(), StateLoadError> {
        let Some(edit) = EditState::deserialize(data) else {
            return Err(StateLoadError::Malformed("Junglizer pattern"));
        };
        state.edit = edit;
        Ok(())
    }

    fn editor(params: Arc<JunglizerParams>) -> Box<dyn Editor> {
        truce_egui::EguiEditor::with_ui(params, (1100, 720), ui::JunglizerUi::new()).into_editor()
    }
}

truce::plugin! {
    logic: Junglizer,
    params: JunglizerParams,
}

// Installs the real-time allocation checker under `--features rt-paranoid`
// (a no-op otherwise). Wrap a driver run in `assert_no_audio_alloc` to
// fail a test if `process` ever allocates. See the audio-testing guide.
truce::enable_rt_paranoid!();

#[cfg(test)]
mod audio_tests {
    use super::*;
    use crate::edit_state::SliceMarker;

    #[test]
    fn ghost_player_routes_each_bar_and_stops_with_transport() {
        let mut state = Junglizer {
            sources: std::array::from_fn(|index| StereoSample {
                frames: vec![[(index + 1) as f32 * 0.1; 2]],
                sample_rate: 44_100,
            }),
            edit: EditState::default(),
            audition: None,
        };
        let params = JunglizerParams::default();
        let events = EventList::with_capacity(0);
        let mut outputs = [0.0_f32; 4];

        for bar in 0..4 {
            let transport = TransportInfo {
                playing: true,
                tempo: 174.0,
                position_beats: bar as f64 * 4.0 + 0.5,
                ..TransportInfo::default()
            };
            let mut channel_slices: [&mut [f32]; 1] = [&mut outputs[bar..bar + 1]];
            let mut buffer = AudioBuffer::from_slices_checked(&[], &mut channel_slices, 1);
            let mut output_events = EventList::with_capacity(0);
            let mut context = ProcessContext::new(&transport, 44_100.0, 1, &mut output_events);
            Junglizer::process(&mut state, &params, &mut buffer, &events, &mut context);
        }
        assert!(outputs[0] > 0.0);
        for bar in 1..4 {
            assert!((outputs[bar] / outputs[0] - (bar + 1) as f32).abs() < 0.01);
        }

        let stopped = TransportInfo::default();
        let mut quiet = [1.0_f32];
        let mut channel_slices: [&mut [f32]; 1] = [&mut quiet];
        let mut buffer = AudioBuffer::from_slices_checked(&[], &mut channel_slices, 1);
        let mut output_events = EventList::with_capacity(0);
        let mut context = ProcessContext::new(&stopped, 44_100.0, 1, &mut output_events);
        Junglizer::process(&mut state, &params, &mut buffer, &events, &mut context);
        assert_eq!(quiet[0], 0.0);
    }

    #[test]
    fn placed_slice_overrides_ghost_then_ghost_resumes() {
        let mut state = Junglizer {
            sources: std::array::from_fn(|index| StereoSample {
                frames: vec![[if index == 0 { 0.2 } else { 0.8 }; 2]; 100],
                sample_rate: 100,
            }),
            edit: EditState::default(),
            audition: None,
        };
        state.edit.set_markers(
            1,
            &[SliceMarker {
                start: 0,
                end: 100,
                active: true,
            }; 8],
        );
        assert!(state.edit.place(StepPlacement {
            active: true,
            start_slot: 0,
            span_slots: 1,
            source_loop: 1,
            source_slice: 0,
            mode: 1,
            bit_depth: 16,
            repeats: 1,
            locked: false,
        }));
        let bar_loops = [0; 4];
        assert!((state.frame_at_beat(0.25, 120.0, &bar_loops)[0] - 0.8).abs() < 0.01);
        assert!((state.frame_at_beat(0.75, 120.0, &bar_loops)[0] - 0.2).abs() < 0.01);
    }
}
