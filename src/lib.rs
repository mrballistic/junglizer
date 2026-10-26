use truce::prelude::*;
use truce_gui_types::layout::{GridLayout, knob, widgets};

pub mod pattern;
mod sample;

use sample::StereoSample;
use std::path::Path;

#[derive(Params)]
pub struct JunglizerParams {
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

use JunglizerParamsParamId as P;

#[derive(Default)]
pub struct Junglizer {
    sources: [StereoSample; 4],
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
        Self {
            sources: std::array::from_fn(|index| {
                StereoSample::load(&directory.join(SOURCE_FILES[index])).unwrap_or_default()
            }),
        }
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
        for i in 0..buffer.num_samples() {
            let mut frame = [0.0_f32; 2];
            if transport.playing && beat_step.is_finite() && beat_step > 0.0 {
                let beat = (transport.position_beats + i as f64 * beat_step).rem_euclid(16.0);
                let bar = (beat / 4.0) as usize;
                let source = bar_loops[bar].min(3);
                let beat_in_bar = beat - bar as f64 * 4.0;
                frame[0] = state.sources[source].at_beat(beat_in_bar, 0);
                frame[1] = state.sources[source].at_beat(beat_in_bar, 1);
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

    fn editor(params: Arc<JunglizerParams>) -> Box<dyn Editor> {
        truce_gui::default_editor(
            params,
            GridLayout::build(vec![widgets(vec![
                knob(P::Bar1, "Bar 1"),
                knob(P::Bar2, "Bar 2"),
                knob(P::Bar3, "Bar 3"),
                knob(P::Bar4, "Bar 4"),
                knob(P::Gain, "Gain"),
            ])]),
        )
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

    #[test]
    fn ghost_player_routes_each_bar_and_stops_with_transport() {
        let mut state = Junglizer {
            sources: std::array::from_fn(|index| StereoSample {
                frames: vec![[(index + 1) as f32 * 0.1; 2]],
            }),
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
}
