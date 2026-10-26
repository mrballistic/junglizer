//! Standard MIDI File export for a four-bar Junglizer phrase.
//!
//! Channel 1 notes 0..127 address source loop × slice. CC 20..24 describe
//! the following note's playback. CC 25/26 carry Thin↔Thicc and root, and
//! CC 30 selects each bar's ghost loop. All timing is 960 ticks per beat.

use crate::edit_state::EditState;
use std::fs;
use std::io;
use std::path::PathBuf;

const TPQ: u32 = 960;

#[derive(Clone, Copy)]
struct MidiEvent {
    tick: u32,
    priority: u8,
    bytes: [u8; 3],
}

fn cc(events: &mut Vec<MidiEvent>, tick: u32, number: u8, value: u8, priority: u8) {
    events.push(MidiEvent {
        tick,
        priority,
        bytes: [0xb0, number, value],
    });
}

fn vlq(mut value: u32, out: &mut Vec<u8>) {
    let mut bytes = [0_u8; 5];
    let mut index = 4;
    bytes[index] = (value & 0x7f) as u8;
    while {
        value >>= 7;
        value != 0
    } {
        index -= 1;
        bytes[index] = (value & 0x7f) as u8 | 0x80;
    }
    out.extend_from_slice(&bytes[index..]);
}

pub fn build_midi(pattern: &EditState, bars: [u8; 4], thin_thicc: f32, root: u8) -> Vec<u8> {
    let mut events = Vec::with_capacity(220);
    cc(&mut events, 0, 24, pattern.player_slots, 0);
    cc(
        &mut events,
        0,
        25,
        ((thin_thicc.clamp(-1.0, 1.0) + 1.0) * 63.5).round() as u8,
        0,
    );
    cc(&mut events, 0, 26, root.min(11), 0);
    for (bar, source) in bars.into_iter().enumerate() {
        cc(&mut events, bar as u32 * 4 * TPQ, 30, source.min(3), 0);
    }
    for placement in pattern.placements.iter().filter(|item| item.active) {
        let start = 16 * TPQ * u32::from(placement.start_slot) / u32::from(pattern.player_slots);
        let end = 16 * TPQ * u32::from(placement.start_slot + placement.span_slots)
            / u32::from(pattern.player_slots);
        let note = placement.source_loop * 32 + placement.source_slice;
        cc(&mut events, start, 20, placement.mode, 2);
        cc(&mut events, start, 21, placement.bit_depth, 2);
        cc(&mut events, start, 22, placement.repeats, 2);
        cc(&mut events, start, 23, placement.span_slots, 2);
        events.push(MidiEvent {
            tick: start,
            priority: 3,
            bytes: [0x90, note, 100],
        });
        events.push(MidiEvent {
            tick: end,
            priority: 1,
            bytes: [0x80, note, 0],
        });
    }
    events.sort_by_key(|event| (event.tick, event.priority));
    let mut track = Vec::with_capacity(events.len() * 5 + 64);
    // A track name helps distinguish the file in Logic's import browser.
    track.extend_from_slice(&[0, 0xff, 0x03, 0x09]);
    track.extend_from_slice(b"Junglizer");
    let mut previous_tick = 0;
    for event in events {
        vlq(event.tick - previous_tick, &mut track);
        track.extend_from_slice(&event.bytes);
        previous_tick = event.tick;
    }
    vlq(16 * TPQ - previous_tick, &mut track);
    track.extend_from_slice(&[0xff, 0x2f, 0]);

    let mut bytes = Vec::with_capacity(track.len() + 22);
    bytes.extend_from_slice(b"MThd");
    bytes.extend_from_slice(&6_u32.to_be_bytes());
    bytes.extend_from_slice(&0_u16.to_be_bytes());
    bytes.extend_from_slice(&1_u16.to_be_bytes());
    bytes.extend_from_slice(&(TPQ as u16).to_be_bytes());
    bytes.extend_from_slice(b"MTrk");
    bytes.extend_from_slice(&(track.len() as u32).to_be_bytes());
    bytes.extend_from_slice(&track);
    bytes
}

pub fn export_to_music(
    pattern: &EditState,
    bars: [u8; 4],
    thin_thicc: f32,
    root: u8,
) -> io::Result<PathBuf> {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "home folder"))?;
    let directory = home.join("Music/Junglizer/Exports");
    fs::create_dir_all(&directory)?;
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(io::Error::other)?
        .as_nanos();
    let path = directory.join(format!("Junglizer-{timestamp}.mid"));
    fs::write(&path, build_midi(pattern, bars, thin_thicc, root))?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit_state::StepPlacement;

    #[test]
    fn exported_midi_has_four_bar_end_and_slice_notes() {
        let mut pattern = EditState::default();
        pattern.place(StepPlacement {
            active: true,
            start_slot: 4,
            span_slots: 2,
            source_loop: 2,
            source_slice: 7,
            mode: 1,
            bit_depth: 9,
            repeats: 2,
            locked: true,
        });
        let bytes = build_midi(&pattern, [3, 2, 1, 0], 0.5, 7);
        assert_eq!(&bytes[..4], b"MThd");
        assert_eq!(&bytes[14..18], b"MTrk");
        assert!(bytes.windows(3).any(|window| window == [0x90, 71, 100]));
        assert!(bytes.windows(3).any(|window| window == [0x80, 71, 0]));
        assert!(bytes.ends_with(&[0xff, 0x2f, 0]));
    }
}
