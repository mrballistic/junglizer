use truce::prelude::*;

mod decode;
pub mod edit_state;
mod live;
mod midi;
pub mod pattern;
mod sample;
mod suggest;
pub mod tempo;
mod tone;
mod ui;

use edit_state::{EditState, SlotKind, StepPlacement};
use live::LiveSnapshot;
use sample::StereoSample;
use std::sync::atomic::{AtomicU32, Ordering};
use suggest::variation_for_cycle;
use tone::Tone;
use truce::params::FloatParamReadF32;

#[derive(Params)]
pub struct JunglizerParams {
    #[skip]
    pub audition: AtomicU32,
    #[skip]
    pub live_snapshot: LiveSnapshot,
    #[param(
        name = "Gain",
        range = "linear(-60, 6)",
        unit = "dB",
        smooth = "exp(5)"
    )]
    pub gain: FloatParam,
    #[param(
        name = "Thin / Thicc",
        range = "linear(-1, 1)",
        default = 0,
        smooth = "exp(10)"
    )]
    pub thin_thicc: FloatParam,
    #[param(name = "808 Root", range = "discrete(0, 11)", default = 0)]
    pub root: IntParam,
    #[param(name = "Playback", range = "discrete(0, 1)", default = 0)]
    pub playback: IntParam,
    #[param(name = "Source Slices", range = "discrete(1, 32)", default = 8)]
    pub source_slices: IntParam,
    #[param(name = "Player Slots", range = "discrete(1, 32)", default = 32)]
    pub player_slots: IntParam,
    #[param(name = "Edit Slot", range = "discrete(1, 32)", default = 1)]
    pub edit_slot: IntParam,
    #[param(name = "Fill Bar", range = "discrete(1, 4)", default = 4)]
    pub fill_bar: IntParam,
    #[param(name = "Edit Break", range = "discrete(1, 4)", default = 1)]
    pub edit_break: IntParam,
    #[param(name = "Edit Slice", range = "discrete(1, 32)", default = 1)]
    pub edit_slice: IntParam,
    #[param(name = "Edit Length", range = "discrete(1, 32)", default = 1)]
    pub edit_length: IntParam,
    #[param(name = "Edit Speed", range = "discrete(0, 1)", default = 0)]
    pub edit_speed: IntParam,
    #[param(name = "Edit Bits", range = "discrete(1, 16)", default = 16)]
    pub edit_bits: IntParam,
    #[param(name = "Edit Retriggers", range = "discrete(0, 3)", default = 0)]
    pub edit_retriggers: IntParam,
    #[param(name = "Edit Lock")]
    pub edit_lock: BoolParam,
    #[param(name = "Bass Hit")]
    pub bass_hit: BoolParam,
    #[param(name = "Slice Muted")]
    pub slice_muted: BoolParam,
    #[param(name = "Marker Position", range = "linear(0, 1)", default = 0)]
    pub marker_position: FloatParam,
    #[param(name = "Live Variations")]
    pub live_variations: BoolParam,
    #[param(name = "Place Slice")]
    pub place_slice: BoolParam,
    #[param(name = "Clear Slice")]
    pub clear_slice: BoolParam,
    #[param(name = "Suggest Fill")]
    pub suggest_fill: BoolParam,
    #[param(name = "Lock Bar")]
    pub lock_bar: BoolParam,
    #[param(name = "Freeze Variation")]
    pub freeze_variation: BoolParam,
    #[param(name = "Unfreeze")]
    pub unfreeze: BoolParam,
    #[param(name = "Unlock All Slices")]
    pub unlock_all_slices: BoolParam,
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
    live_pattern: EditState,
    live_version: u64,
    live_cycle: i64,
    live_bars: [usize; 4],
    loop_wraps: i64,
    last_transport_beat: Option<f64>,
    tone: Tone,
    midi: MidiPlayback,
    controls: Option<ControlValues>,
    audition: Option<Audition>,
}

#[derive(Clone, Copy)]
struct ControlValues {
    source_slices: u8,
    player_slots: u8,
    slot: u8,
    fill_bar: u8,
    source_loop: u8,
    source_slice: u8,
    length: u8,
    mode: u8,
    bits: u8,
    repeats: u8,
    locked: bool,
    bass: bool,
    muted: bool,
    marker_position: f32,
    live: bool,
    place: bool,
    clear: bool,
    suggest: bool,
    lock_bar: bool,
    freeze: bool,
    unfreeze: bool,
    unlock_all: bool,
}

impl ControlValues {
    fn read(params: &JunglizerParams) -> Self {
        Self {
            source_slices: params.source_slices.value() as u8,
            player_slots: params.player_slots.value() as u8,
            slot: (params.edit_slot.value() - 1) as u8,
            fill_bar: (params.fill_bar.value() - 1) as u8,
            source_loop: (params.edit_break.value() - 1) as u8,
            source_slice: (params.edit_slice.value() - 1) as u8,
            length: params.edit_length.value() as u8,
            mode: params.edit_speed.value() as u8,
            bits: params.edit_bits.value() as u8,
            repeats: 1 << params.edit_retriggers.value(),
            locked: params.edit_lock.value(),
            bass: params.bass_hit.value(),
            muted: params.slice_muted.value(),
            marker_position: params.marker_position.value(),
            live: params.live_variations.value(),
            place: params.place_slice.value(),
            clear: params.clear_slice.value(),
            suggest: params.suggest_fill.value(),
            lock_bar: params.lock_bar.value(),
            freeze: params.freeze_variation.value(),
            unfreeze: params.unfreeze.value(),
            unlock_all: params.unlock_all_slices.value(),
        }
    }
}

#[derive(Clone, Copy, Default)]
struct MidiVoice {
    note: u8,
    placement: StepPlacement,
    elapsed_beats: f64,
}

struct MidiPlayback {
    bars: [usize; 4],
    player_slots: u8,
    mode: u8,
    bit_depth: u8,
    repeats: u8,
    span_slots: u8,
    voice: Option<MidiVoice>,
    thin_override: Option<f32>,
    root_override: Option<u8>,
}

impl Default for MidiPlayback {
    fn default() -> Self {
        Self {
            bars: [0, 1, 2, 3],
            player_slots: 32,
            mode: 0,
            bit_depth: 16,
            repeats: 1,
            span_slots: 1,
            voice: None,
            thin_override: None,
            root_override: None,
        }
    }
}

impl MidiPlayback {
    fn handle(&mut self, body: &EventBody, beat: f64, pattern: &EditState) -> bool {
        match *body {
            EventBody::ControlChange {
                channel: 0,
                cc,
                value,
                ..
            } => match cc {
                20 => self.mode = value.min(1),
                21 => self.bit_depth = value.clamp(1, 16),
                22 => {
                    self.repeats = if [1, 2, 4, 8].contains(&value) {
                        value
                    } else {
                        1
                    }
                }
                23 => self.span_slots = value.clamp(1, 32),
                24 => self.player_slots = value.clamp(1, 32),
                25 => self.thin_override = Some(f32::from(value) / 63.5 - 1.0),
                26 => self.root_override = Some(value.min(11)),
                30 => {
                    self.bars[((beat.rem_euclid(16.0) / 4.0) as usize).min(3)] =
                        value.min(3) as usize
                }
                _ => {}
            },
            EventBody::NoteOn {
                channel: 0,
                note,
                velocity,
                ..
            } if velocity > 0 => {
                let source_loop = note / 32;
                let source_slice = note % 32;
                let Some(slice) = pattern.slice(source_loop as usize, source_slice as usize) else {
                    return false;
                };
                self.voice = Some(MidiVoice {
                    note,
                    placement: StepPlacement {
                        active: true,
                        start_slot: 0,
                        span_slots: self.span_slots.min(self.player_slots),
                        source_loop,
                        source_slice,
                        mode: self.mode,
                        bit_depth: self.bit_depth,
                        repeats: self.repeats,
                        locked: false,
                    },
                    elapsed_beats: 0.0,
                });
                return slice.bass;
            }
            EventBody::NoteOff {
                channel: 0, note, ..
            } => {
                if self.voice.is_some_and(|voice| voice.note == note) {
                    self.voice = None;
                }
            }
            _ => {}
        }
        false
    }
}

#[derive(Clone, Copy, Default)]
struct Audition {
    source_loop: usize,
    position: f64,
    end: u32,
}

const SOURCE_FILES: [&str; 4] = [
    "174_JunglizerAmen_Bar_01.wav",
    "174_JunglizerAmen_Bar_02.wav",
    "174_JunglizerAmen_Bar_03.wav",
    "174_JunglizerAmen_Bar_04.wav",
];
const SOURCE_AUDIO: [&[u8]; 4] = [
    include_bytes!("../sample/174_JunglizerAmen_Bar_01.wav"),
    include_bytes!("../sample/174_JunglizerAmen_Bar_02.wav"),
    include_bytes!("../sample/174_JunglizerAmen_Bar_03.wav"),
    include_bytes!("../sample/174_JunglizerAmen_Bar_04.wav"),
];

impl Junglizer {
    fn load_sources() -> Self {
        let sources: [StereoSample; 4] = std::array::from_fn(|index| {
            StereoSample::from_bytes(SOURCE_AUDIO[index]).unwrap_or_default()
        });
        let mut edit = EditState::default();
        for count in 1..=32 {
            for (index, source) in sources.iter().enumerate() {
                edit.markers[index][count as usize - 1] = edit_state::MarkerSet {
                    valid: true,
                    slices: source.suggested_slices(count),
                };
            }
        }
        edit.touch();
        let live_pattern = edit.clone();
        Self {
            sources,
            edit,
            live_pattern,
            live_version: 0,
            live_cycle: 0,
            live_bars: [0, 1, 2, 3],
            loop_wraps: 0,
            last_transport_beat: None,
            tone: Tone::default(),
            midi: MidiPlayback::default(),
            controls: None,
            audition: None,
        }
    }

    fn sync_controls(&mut self, params: &JunglizerParams, bars: [usize; 4]) {
        let current = ControlValues::read(params);
        let previous = self.controls.replace(current);
        if previous.is_some_and(|before| current.source_slices != before.source_slices)
            && !self.edit.frozen
        {
            let loop_index = current.source_loop as usize;
            self.edit.set_slice_count(loop_index, current.source_slices);
            let frames = self.sources[loop_index].frames.len() as u32;
            self.edit.fill_even_markers(loop_index, frames);
        }
        if previous.is_some_and(|before| current.player_slots != before.player_slots)
            && !self.edit.frozen
        {
            self.edit.set_player_slots(current.player_slots);
        }
        if previous.is_some_and(|before| current.live != before.live) && !self.edit.frozen {
            self.edit.live_enabled = current.live;
            self.edit.touch();
        }
        let Some(before) = previous else { return };
        if current.bass != before.bass && !self.edit.frozen {
            let source_count = self.edit.slice_count(current.source_loop as usize);
            if let Some(set) = self.edit.marker_set_mut(current.source_loop as usize) {
                if current.source_slice < source_count {
                    set.slices[current.source_slice as usize].bass = current.bass;
                    self.edit.touch();
                }
            }
        }
        if current.muted != before.muted && !self.edit.frozen {
            let source_count = self.edit.slice_count(current.source_loop as usize);
            if let Some(set) = self.edit.marker_set_mut(current.source_loop as usize)
                && current.source_slice < source_count
            {
                set.slices[current.source_slice as usize].active = !current.muted;
                self.edit.touch();
            }
        }
        if current.marker_position != before.marker_position
            && !self.edit.frozen
            && current.source_slice > 0
            && current.source_slice < self.edit.slice_count(current.source_loop as usize)
        {
            let loop_index = current.source_loop as usize;
            let index = current.source_slice as usize;
            let frames = self.sources[loop_index].frames.len() as u32;
            let source_count = self.edit.slice_count(loop_index) as usize;
            if let Some(set) = self.edit.marker_set_mut(loop_index) {
                let min = set.slices[index - 1].start.saturating_add(1);
                let max = if index + 1 < source_count {
                    set.slices[index + 1].start.saturating_sub(1)
                } else {
                    frames.saturating_sub(1)
                };
                if min <= max {
                    let frame = (current.marker_position.clamp(0.0, 1.0) * frames as f32) as u32;
                    set.slices[index].start = frame.clamp(min, max);
                    set.slices[index - 1].end = set.slices[index].start;
                    self.edit.touch();
                }
            }
        }
        let edit_changed = current.length != before.length
            || current.mode != before.mode
            || current.bits != before.bits
            || current.repeats != before.repeats
            || current.locked != before.locked;
        if edit_changed && !self.edit.frozen && !self.edit.live_enabled {
            let old = self
                .edit
                .placements
                .iter()
                .find(|item| {
                    item.active
                        && item.start_slot <= current.slot
                        && current.slot < item.start_slot + item.span_slots
                })
                .copied();
            if let Some(old) = old {
                let mut updated = old;
                if current.length != before.length {
                    updated.span_slots = current.length;
                }
                if current.mode != before.mode {
                    updated.mode = current.mode;
                }
                if current.bits != before.bits {
                    updated.bit_depth = current.bits;
                }
                if current.repeats != before.repeats {
                    updated.repeats = current.repeats;
                }
                if current.locked != before.locked {
                    updated.locked = current.locked;
                }
                self.edit.replace(old, updated);
            }
        }
        if current.clear && !before.clear && !self.edit.frozen && !self.edit.live_enabled {
            self.edit.remove_at_slot(current.slot);
        }
        if current.place && !before.place && !self.edit.frozen && !self.edit.live_enabled {
            self.edit.place(StepPlacement {
                active: true,
                start_slot: current.slot,
                span_slots: current
                    .length
                    .min(self.edit.player_slots.saturating_sub(current.slot)),
                source_loop: current.source_loop,
                source_slice: current.source_slice,
                mode: current.mode,
                bit_depth: current.bits,
                repeats: current.repeats,
                locked: current.locked,
            });
        }
        if current.suggest && !before.suggest && !self.edit.frozen && !self.edit.live_enabled {
            suggest::suggest_bar(
                &mut self.edit,
                current.fill_bar as usize,
                current.source_loop as usize,
            );
        }
        if current.lock_bar && !before.lock_bar && !self.edit.frozen {
            let bar = current.fill_bar as usize;
            let start = (bar * self.edit.player_slots as usize + 3) / 4;
            let end = ((bar + 1) * self.edit.player_slots as usize + 3) / 4;
            for item in &mut self.edit.placements {
                if item.active
                    && (item.start_slot as usize) < end
                    && start < item.start_slot as usize + item.span_slots as usize
                {
                    item.locked = true;
                }
            }
            self.edit.touch();
        }
        if current.freeze && !before.freeze {
            let bars = if self.edit.live_enabled {
                self.live_bars.map(|bar| bar as u8)
            } else {
                bars.map(|bar| bar as u8)
            };
            let placements = if self.edit.live_enabled {
                self.live_pattern.placements
            } else {
                self.edit.placements
            };
            self.edit.freeze_phrase(
                placements,
                self.edit.slice_counts(),
                self.edit.player_slots,
                bars,
            );
        }
        if current.unfreeze && !before.unfreeze {
            self.edit.frozen = false;
            self.edit.touch();
        }
        if current.unlock_all && !before.unlock_all && !self.edit.frozen {
            self.edit.unlock_all();
        }
    }

    fn slice_frame(
        &self,
        pattern: &EditState,
        placement: &StepPlacement,
        elapsed_beats: f64,
        tempo: f64,
        player_slots: u8,
    ) -> [f32; 2] {
        let Some(source) = self.sources.get(placement.source_loop as usize) else {
            return [0.0; 2];
        };
        let Some(slice) = pattern.slice(
            placement.source_loop as usize,
            placement.source_slice as usize,
        ) else {
            return [0.0; 2];
        };
        if slice.end <= slice.start || slice.end as usize > source.frames.len() {
            return [0.0; 2];
        }
        let duration = 16.0 * f64::from(placement.span_slots) / f64::from(player_slots);
        let repeat_duration = duration / f64::from(placement.repeats);
        let beat_in_repeat = elapsed_beats.rem_euclid(repeat_duration);
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

    fn frame_at_beat(
        &self,
        pattern: &EditState,
        beat: f64,
        tempo: f64,
        bar_loops: &[usize; 4],
    ) -> [f32; 2] {
        let bar = (beat / 4.0) as usize;
        let source = bar_loops[bar].min(3);
        let beat_in_bar = beat - bar as f64 * 4.0;
        let ghost = [
            self.sources[source].at_beat(beat_in_bar, 0),
            self.sources[source].at_beat(beat_in_bar, 1),
        ];
        let Some(placement) = pattern.placement_at(beat) else {
            return ghost;
        };
        let start = pattern.slot_start(placement.start_slot);
        let placed = self.slice_frame(
            pattern,
            placement,
            beat - start,
            tempo,
            pattern.player_slots,
        );
        let end = 16.0 * f64::from(placement.start_slot + placement.span_slots)
            / f64::from(pattern.player_slots);
        let fade_beats = (tempo / 60.0 * 0.0005).min((end - start) / 8.0);
        let mix = ((beat - start) / fade_beats)
            .min((end - beat) / fade_beats)
            .clamp(0.0, 1.0) as f32;
        [
            ghost[0] * (1.0 - mix) + placed[0] * mix,
            ghost[1] * (1.0 - mix) + placed[1] * mix,
        ]
    }

    fn midi_frame(&self, beat: f64, tempo: f64) -> [f32; 2] {
        let bar = (beat / 4.0) as usize;
        let source = self.midi.bars[bar].min(3);
        let beat_in_bar = beat - bar as f64 * 4.0;
        let ghost = [
            self.sources[source].at_beat(beat_in_bar, 0),
            self.sources[source].at_beat(beat_in_bar, 1),
        ];
        let Some(voice) = self.midi.voice else {
            return ghost;
        };
        let duration =
            16.0 * f64::from(voice.placement.span_slots) / f64::from(self.midi.player_slots);
        if voice.elapsed_beats >= duration {
            return ghost;
        }
        let placed = self.slice_frame(
            &self.edit,
            &voice.placement,
            voice.elapsed_beats,
            tempo,
            self.midi.player_slots,
        );
        let fade = (tempo / 60.0 * 0.0005).min(duration / 8.0);
        let mix = (voice.elapsed_beats / fade)
            .min((duration - voice.elapsed_beats) / fade)
            .clamp(0.0, 1.0) as f32;
        [
            ghost[0] * (1.0 - mix) + placed[0] * mix,
            ghost[1] * (1.0 - mix) + placed[1] * mix,
        ]
    }

    fn bass_onset_between(
        &self,
        pattern: &EditState,
        bars: &[usize; 4],
        previous: f64,
        current: f64,
        include_placements: bool,
    ) -> bool {
        let crosses = |onset: f64| {
            if previous <= current {
                previous < onset && onset <= current
            } else {
                onset > previous || onset <= current
            }
        };
        if include_placements {
            for item in pattern.placements.iter().filter(|item| item.active) {
                if !pattern
                    .slice(item.source_loop as usize, item.source_slice as usize)
                    .is_some_and(|slice| slice.bass)
                {
                    continue;
                }
                let start = pattern.slot_start(item.start_slot);
                let duration = 16.0 * f64::from(item.span_slots) / f64::from(pattern.player_slots);
                for repeat in 0..item.repeats {
                    if crosses(start + f64::from(repeat) * duration / f64::from(item.repeats)) {
                        return true;
                    }
                }
            }
        }
        for bar in 0..4 {
            let source_index = bars[bar].min(3);
            let frame_count = self.sources[source_index].frames.len();
            if frame_count == 0 {
                continue;
            }
            if let Some(set) = pattern.marker_set(source_index) {
                for marker in set.slices[..pattern.slice_count(source_index) as usize]
                    .iter()
                    .filter(|marker| marker.active && marker.bass)
                {
                    let onset =
                        bar as f64 * 4.0 + 4.0 * f64::from(marker.start) / frame_count as f64;
                    if crosses(onset)
                        && (!include_placements || pattern.placement_at(onset).is_none())
                        && (include_placements || self.midi.voice.is_none())
                    {
                        return true;
                    }
                }
            }
        }
        false
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
        events: &EventList,
        context: &mut ProcessContext,
    ) -> ProcessStatus {
        let selected_bars = [
            params.bar_1.value() as usize - 1,
            params.bar_2.value() as usize - 1,
            params.bar_3.value() as usize - 1,
            params.bar_4.value() as usize - 1,
        ];
        state.sync_controls(params, selected_bars);
        let bar_loops = if state.edit.frozen {
            state.edit.frozen_bars.map(usize::from)
        } else {
            selected_bars
        };
        let midi_mode = params.playback.value() != 0;
        let transport = context.transport;
        let beat_step = transport.tempo / (60.0 * context.sample_rate);
        let mut event_iter = events.iter().peekable();
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
            let running = transport.playing && beat_step.is_finite() && beat_step > 0.0;
            let host_beat = transport.position_beats + i as f64 * beat_step;
            let beat = host_beat.rem_euclid(16.0);
            let previous = state
                .last_transport_beat
                .map(|value| value.rem_euclid(16.0))
                .unwrap_or_else(|| (beat - beat_step).rem_euclid(16.0));
            while event_iter
                .peek()
                .is_some_and(|event| event.sample_offset as usize <= i)
            {
                let event = event_iter.next().expect("peeked event");
                if midi_mode && state.midi.handle(&event.body, beat, &state.edit) {
                    state.tone.trigger(
                        state
                            .midi
                            .root_override
                            .unwrap_or(params.root.value() as u8),
                    );
                }
            }
            if running {
                if state
                    .last_transport_beat
                    .is_some_and(|last| host_beat + 0.5 < last)
                {
                    state.loop_wraps = state.loop_wraps.wrapping_add(1);
                }
                let cycle = (host_beat / 16.0).floor() as i64 + state.loop_wraps;
                if !midi_mode
                    && state.edit.live_enabled
                    && (state.live_version != state.edit.version
                        || state.live_cycle != cycle
                        || state.live_bars != bar_loops)
                {
                    state.live_pattern = variation_for_cycle(&state.edit, cycle, bar_loops);
                    state.live_version = state.edit.version;
                    state.live_cycle = cycle;
                    state.live_bars = bar_loops;
                    params
                        .live_snapshot
                        .publish(&state.live_pattern, cycle, bar_loops);
                }
                if midi_mode {
                    frame = state.midi_frame(beat, transport.tempo);
                    if state.bass_onset_between(
                        &state.edit,
                        &state.midi.bars,
                        previous,
                        beat,
                        false,
                    ) {
                        state.tone.trigger(
                            state
                                .midi
                                .root_override
                                .unwrap_or(params.root.value() as u8),
                        );
                    }
                } else {
                    let pattern = if state.edit.live_enabled {
                        &state.live_pattern
                    } else {
                        &state.edit
                    };
                    frame = state.frame_at_beat(pattern, beat, transport.tempo, &bar_loops);
                    if state.bass_onset_between(pattern, &bar_loops, previous, beat, true) {
                        state.tone.trigger(params.root.value() as u8);
                    }
                }
                state.last_transport_beat = Some(host_beat);
            } else {
                state.last_transport_beat = None;
                state.loop_wraps = 0;
            }
            if let Some(voice) = &mut state.midi.voice {
                voice.elapsed_beats += if beat_step.is_finite() && beat_step > 0.0 {
                    beat_step
                } else {
                    174.0 / (60.0 * context.sample_rate)
                };
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
            let thin_thicc = if midi_mode {
                state.midi.thin_override.unwrap_or(params.thin_thicc.read())
            } else {
                params.thin_thicc.read()
            };
            frame = state.tone.process(frame, thin_thicc, context.sample_rate);
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
        let Some(mut edit) = EditState::deserialize(data) else {
            return Err(StateLoadError::Malformed("Junglizer pattern"));
        };
        for source in 0..4 {
            // User and blank slots keep exactly what the project saved.
            if edit.slots[source].kind != SlotKind::Builtin {
                continue;
            }
            for count in 0..32 {
                if !edit.markers[source][count].valid {
                    edit.markers[source][count] = state.edit.markers[source][count];
                }
            }
        }
        state.edit = edit;
        state.live_version = 0;
        state.midi.player_slots = state.edit.player_slots;
        state.controls = None;
        Ok(())
    }

    fn editor(params: Arc<JunglizerParams>) -> Box<dyn Editor> {
        truce_egui::EguiEditor::with_ui(params, (1100, 720), ui::JunglizerUi::new())
            .with_visuals(ui::visuals())
            .into_editor()
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
            ..Junglizer::default()
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
    fn source_slices_sets_the_selected_slot_and_fills_even_markers() {
        use truce::params::Params;
        let mut state = Junglizer::load_sources();
        let frames = state.sources[1].frames.len() as u32;
        let marks = [0_u32, 1, 2, 3].map(|i| SliceMarker {
            start: frames * i / 4,
            end: frames * (i + 1) / 4,
            active: true,
            bass: false,
        });
        state
            .edit
            .load_user(1, "/a.wav", frames, 4, &marks)
            .unwrap();
        let params = JunglizerParams::default();
        params.set_normalized(JunglizerParamsParamId::EditBreak.into(), 1.0 / 3.0);
        state.sync_controls(&params, [0, 1, 2, 3]);
        params.set_normalized(JunglizerParamsParamId::SourceSlices.into(), 11.0 / 31.0);
        state.sync_controls(&params, [0, 1, 2, 3]);
        assert_eq!(state.edit.slice_count(0), 8);
        assert_eq!(state.edit.slice_count(1), 12);
        let set = state.edit.marker_set(1).expect("marker set");
        assert!(set.valid);
        assert_eq!(set.slices[0].start, 0);
        assert_eq!(set.slices[11].end, frames);
        assert!(set.slices[..12].iter().all(|slice| slice.active));
    }

    #[test]
    fn placed_slice_overrides_ghost_then_ghost_resumes() {
        let mut state = Junglizer {
            sources: std::array::from_fn(|index| StereoSample {
                frames: vec![[if index == 0 { 0.2 } else { 0.8 }; 2]; 100],
                sample_rate: 100,
            }),
            edit: EditState::default(),
            ..Junglizer::default()
        };
        state.edit.set_markers(
            1,
            &[SliceMarker {
                start: 0,
                end: 100,
                active: true,
                bass: true,
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
        assert!((state.frame_at_beat(&state.edit, 0.25, 120.0, &bar_loops)[0] - 0.8).abs() < 0.01);
        assert!((state.frame_at_beat(&state.edit, 0.75, 120.0, &bar_loops)[0] - 0.2).abs() < 0.01);
    }

    #[test]
    fn midi_note_uses_selected_source_and_playback_controls() {
        let mut midi = MidiPlayback::default();
        let mut edit = EditState::default();
        edit.set_markers(
            2,
            &[SliceMarker {
                start: 0,
                end: 100,
                active: true,
                bass: true,
            }; 8],
        );
        for (cc, value) in [(20, 1), (21, 8), (22, 4), (23, 3), (24, 16), (30, 2)] {
            midi.handle(
                &EventBody::ControlChange {
                    group: 0,
                    channel: 0,
                    cc,
                    value,
                },
                4.0,
                &edit,
            );
        }
        assert_eq!(midi.bars[1], 2);
        assert!(midi.handle(
            &EventBody::NoteOn {
                group: 0,
                channel: 0,
                note: 2 * 32 + 3,
                velocity: 100,
            },
            4.0,
            &edit,
        ));
        let voice = midi.voice.expect("MIDI slice voice");
        assert_eq!(voice.placement.source_loop, 2);
        assert_eq!(voice.placement.source_slice, 3);
        assert_eq!(voice.placement.bit_depth, 8);
        assert_eq!(voice.placement.repeats, 4);
        assert_eq!(voice.placement.mode, 1);
        midi.handle(
            &EventBody::NoteOff {
                group: 0,
                channel: 0,
                note: 2 * 32 + 3,
                velocity: 0,
            },
            4.5,
            &edit,
        );
        assert!(midi.voice.is_none());
    }
}
