//! Decoded breaks, ready for the audio thread. Built off the audio thread and
//! handed over behind an `Arc`.

use crate::decode::{DecodeError, decode_file_capped};
use crate::edit_state::{LOOP_COUNT, MAX_PATH_BYTES, MAX_STEPS, SliceMarker, path_hash};
use crate::sample::StereoSample;
use crate::tempo::{MAX_BREAK_SECS, Reading, Refusal, read_length};
use crossbeam_queue::ArrayQueue;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, Weak};
use std::thread::{JoinHandle, Thread};

/// The slice count `suggested` markers are computed for.
const SUGGESTED_SLICES: u8 = 8;

pub struct LoadedBreak {
    pub sample: StereoSample,
    pub path_hash: u64,
    #[allow(dead_code)] // shown by the slot controls (Task 8)
    pub reading: Reading,
    pub suggested: [SliceMarker; MAX_STEPS],
}

impl LoadedBreak {
    /// Wraps decoded audio and computes its suggested 8-slice markers. This
    /// allocates; never call it from the audio thread.
    pub fn new(sample: StereoSample, path_hash: u64, reading: Reading) -> Self {
        let suggested = sample.suggested_slices(SUGGESTED_SLICES);
        Self {
            sample,
            path_hash,
            reading,
            suggested,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Purpose {
    Restore,
    Choose,
}

#[derive(Clone)]
pub enum LoadOutcome {
    Loaded(Arc<LoadedBreak>),
    Refused(Refusal),
    Unreadable,
    Missing,
}

#[derive(Clone)]
pub struct SlotResult {
    pub seq: u64,
    pub path_hash: u64,
    pub outcome: LoadOutcome,
}

/// Room for one requested path, filled without allocating.
struct PathBuf32 {
    len: u16,
    bytes: [u8; MAX_PATH_BYTES],
}

impl Default for PathBuf32 {
    fn default() -> Self {
        Self {
            len: 0,
            bytes: [0; MAX_PATH_BYTES],
        }
    }
}

/// The latest request for one slot and purpose. `seq` is written while the
/// path is locked, so the loader always reads a matching pair.
#[derive(Default)]
struct Request {
    path: Mutex<PathBuf32>,
    seq: AtomicU64,
}

/// How many retired breaks can wait for the loader or editor to free them.
const RETIRE_CAPACITY: usize = 16;

struct Inner {
    requests: [[Request; 2]; LOOP_COUNT],
    /// The last request sequence the loader picked up, per slot and purpose.
    taken: [[AtomicU64; 2]; LOOP_COUNT],
    results: [[Mutex<Option<SlotResult>>; 2]; LOOP_COUNT],
    next_seq: AtomicU64,
    retired: ArrayQueue<Arc<LoadedBreak>>,
    loader: OnceLock<Thread>,
    handle: Mutex<Option<JoinHandle<()>>>,
    shutdown: AtomicBool,
}

impl Default for Inner {
    fn default() -> Self {
        let inner = Self {
            requests: Default::default(),
            taken: Default::default(),
            results: Default::default(),
            next_seq: AtomicU64::new(0),
            retired: ArrayQueue::new(RETIRE_CAPACITY),
            loader: OnceLock::new(),
            handle: Mutex::new(None),
            shutdown: AtomicBool::new(false),
        };
        // Where std's `Mutex` wraps a pthread mutex (macOS), it boxes it on
        // first lock. Lock each one the audio thread touches now, so its
        // first `try_lock` there doesn't allocate.
        for slot in 0..LOOP_COUNT {
            for purpose in 0..2 {
                drop(inner.requests[slot][purpose].path.try_lock());
                drop(inner.results[slot][purpose].try_lock());
            }
        }
        inner
    }
}

/// Loads user breaks off the audio thread and hands them to the DSP.
///
/// The editor and the DSP post requests; a loader thread decodes them and
/// publishes one result per slot and purpose. The DSP adopts results with
/// `try_lock` and retires the audio it lets go of to a fixed queue, which
/// the loader and the editor drain, so the audio thread never allocates,
/// frees or blocks.
#[derive(Default)]
pub struct Library {
    inner: Arc<Inner>,
}

impl Drop for Library {
    fn drop(&mut self) {
        self.inner.shutdown.store(true, Ordering::Release);
        if let Some(thread) = self.inner.loader.get() {
            thread.unpark();
        }
    }
}

fn purpose_index(purpose: Purpose) -> usize {
    purpose as usize
}

impl Library {
    /// Spawns the loader thread, once. Never call from the audio thread.
    pub fn start(&self) {
        let mut handle = self.inner.handle.lock().unwrap_or_else(|e| e.into_inner());
        if self.inner.loader.get().is_some() {
            return;
        }
        let weak = Arc::downgrade(&self.inner);
        let spawned = std::thread::Builder::new()
            .name("junglizer-loader".into())
            .spawn(move || loader_main(weak));
        if let Ok(spawned) = spawned {
            let _ = self.inner.loader.set(spawned.thread().clone());
            *handle = Some(spawned);
        }
    }

    /// Posts a load of `path` into `slot`. Allocation-free and non-blocking,
    /// so the DSP can call it: returns `None` if the request slot is busy
    /// (try again later) or the slot or path is invalid.
    pub fn request(&self, slot: usize, path: &[u8], purpose: Purpose) -> Option<u64> {
        if path.is_empty() || path.len() > MAX_PATH_BYTES {
            return None;
        }
        let request = self.inner.requests.get(slot)?.get(purpose_index(purpose))?;
        let mut buf = request.path.try_lock().ok()?;
        buf.bytes[..path.len()].copy_from_slice(path);
        buf.len = path.len() as u16;
        let seq = self.inner.next_seq.fetch_add(1, Ordering::Relaxed) + 1;
        request.seq.store(seq, Ordering::Release);
        drop(buf);
        if let Some(thread) = self.inner.loader.get() {
            thread.unpark();
        }
        Some(seq)
    }

    /// The latest finished load for `slot` and `purpose`. Locks; editor only.
    #[allow(dead_code)] // read by the slot controls (Task 8)
    pub fn result(&self, slot: usize, purpose: Purpose) -> Option<SlotResult> {
        let cell = self.inner.results.get(slot)?.get(purpose_index(purpose))?;
        cell.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// A finished load of the file `path_hash` names in `slot`, from either
    /// purpose, preferring loaded audio. Non-blocking and allocation-free
    /// (it clones an `Arc`), for the audio thread. `None` means nothing
    /// for that path has finished, or a result slot was busy.
    pub fn try_adopt(&self, slot: usize, path_hash: u64) -> Option<LoadOutcome> {
        let mut found = None;
        for cell in self.inner.results.get(slot)? {
            let Ok(guard) = cell.try_lock() else { continue };
            let Some(result) = guard.as_ref().filter(|r| r.path_hash == path_hash) else {
                continue;
            };
            if let LoadOutcome::Loaded(_) = result.outcome {
                return Some(result.outcome.clone());
            }
            found = Some(result.outcome.clone());
        }
        found
    }

    /// The loaded audio of the file `path_hash` names in `slot`, if any.
    /// Locks; editor only.
    #[allow(dead_code)] // drawn by the editor's waveform (Task 8)
    pub fn audio_for(&self, slot: usize, path_hash: u64) -> Option<Arc<LoadedBreak>> {
        self.inner.results.get(slot)?.iter().find_map(|cell| {
            let guard = cell.lock().unwrap_or_else(|e| e.into_inner());
            match guard.as_ref()? {
                SlotResult {
                    path_hash: hash,
                    outcome: LoadOutcome::Loaded(audio),
                    ..
                } if *hash == path_hash => Some(audio.clone()),
                _ => None,
            }
        })
    }

    /// Hands audio the DSP no longer holds to the loader or editor to free.
    /// Gives it back if the queue is full.
    pub fn retire(&self, old: Arc<LoadedBreak>) -> Result<(), Arc<LoadedBreak>> {
        self.inner.retired.push(old)?;
        // Wake the loader to free it now rather than at the next request.
        if let Some(thread) = self.inner.loader.get() {
            thread.unpark();
        }
        Ok(())
    }

    /// The loader body: takes every new request, decodes it and publishes
    /// the result, then frees retired audio. Tests call it directly.
    pub fn run_pending(&self) {
        run_pending(&self.inner);
    }

    /// Frees audio the DSP retired. Call from the loader or the editor.
    pub fn collect_garbage(&self) {
        while let Some(old) = self.inner.retired.pop() {
            drop(old);
        }
    }

    /// The four built-in amen bars, decoded once. First touched in `init`,
    /// so the audio thread only ever reads them.
    pub fn builtin() -> &'static [Arc<LoadedBreak>; LOOP_COUNT] {
        static BUILTIN: OnceLock<[Arc<LoadedBreak>; LOOP_COUNT]> = OnceLock::new();
        BUILTIN.get_or_init(|| {
            std::array::from_fn(|index| {
                let sample =
                    StereoSample::from_bytes(crate::SOURCE_AUDIO[index]).unwrap_or_default();
                Arc::new(LoadedBreak::new(sample, 0, BUILTIN_READING))
            })
        })
    }
}

/// A path that makes the guarded load panic, to test the loader survives.
#[cfg(test)]
const PANIC_FOR_TESTS: &str = "/junglizer/panic-for-tests.wav";

fn is_slash(byte: u8) -> bool {
    byte == b'\\' || byte == b'/'
}

/// Splits a Windows `\\.\` or `\\?\` prefix (either slash) off `path`,
/// returning the namespace character and the rest. `\\.` and `\\?` with
/// nothing after count too.
fn win32_namespace(path: &[u8]) -> Option<(u8, &[u8])> {
    match path {
        [a, b, ns @ (b'.' | b'?')] if is_slash(*a) && is_slash(*b) => Some((*ns, &[])),
        [a, b, ns @ (b'.' | b'?'), c, rest @ ..]
            if is_slash(*a) && is_slash(*b) && is_slash(*c) =>
        {
            Some((*ns, rest))
        }
        _ => None,
    }
}

/// `<letter>:\…` after a `\\?\` prefix.
fn is_drive_path(rest: &[u8]) -> bool {
    matches!(rest, [letter, b':', slash, ..] if letter.is_ascii_alphabetic() && is_slash(*slash))
}

/// `UNC\…` after a `\\?\` prefix.
fn is_unc_path(rest: &[u8]) -> bool {
    matches!(rest, [u, n, c, slash, ..]
        if [*u, *n, *c].eq_ignore_ascii_case(b"UNC") && is_slash(*slash))
}

/// The NT object-manager prefix `\??\`, with either slash.
fn is_nt_prefix(path: &[u8]) -> bool {
    matches!(path, [a, b'?', b'?', b, ..] if is_slash(*a) && is_slash(*b))
}

/// Whether a path chosen in the file dialog may be opened. Device
/// namespaces reach raw disks, volumes and ports, so only the exact
/// backslash `\\?\` prefix is accepted, followed by `<letter>:\` or `UNC\`.
/// Win32 normalizes the forward form `//?/` (resolving `..`, which can walk
/// a drive path into device or UNC space), so it is refused, as are
/// `\\.\`, `//./` and `\??\` in any slash mix.
fn path_allowed(path: &str) -> bool {
    let bytes = path.as_bytes();
    if is_nt_prefix(bytes) {
        return false;
    }
    match win32_namespace(bytes) {
        Some((b'?', rest)) => {
            bytes.starts_with(br"\\?\") && (is_drive_path(rest) || is_unc_path(rest))
        }
        Some(_) => false,
        None => true,
    }
}

/// Whether a path from project state may be opened without asking. Stricter
/// than `path_allowed`; see the comment where it is used for why.
fn restore_allowed(path: &str) -> bool {
    let bytes = path.as_bytes();
    // A drive letter must start an absolute path (`C:\`, `C:/`). A
    // drive-relative `C:CON` or `C:..\x` would hide its first component
    // from the checks below. Strip `C:` so they see the rest.
    let rest = match bytes {
        [letter, b':', ..] if letter.is_ascii_alphabetic() => {
            if !bytes.get(2).is_some_and(|&byte| is_slash(byte)) {
                return false;
            }
            &path[2..]
        }
        _ => path,
    };
    path_allowed(path)
        && win32_namespace(bytes).is_none()
        && !is_nt_prefix(bytes)
        && !is_remote(path)
        && !rest
            .as_bytes()
            .split(|&byte| is_slash(byte))
            .any(|part| part == b"." || part == b"..")
        && !rest.split(['\\', '/']).any(is_dos_device)
        && !has_stray_colon(bytes)
}

/// Whether `path` has a `:` anywhere but right after a leading drive
/// letter (`C:`). Windows ends a device name at `:` (`nul:stream`,
/// `COM1:`), and a later `:` also names an NTFS alternate data stream.
fn has_stray_colon(path: &[u8]) -> bool {
    path.iter()
        .enumerate()
        .any(|(index, &byte)| byte == b':' && !(index == 1 && path[0].is_ascii_alphabetic()))
}

/// Whether a path component names a reserved DOS device on Windows
/// (`COM1`, `nul.wav`, `Lpt1 .wav`, `CONIN$`…). Opening one, even just to
/// stat it, opens the device: a serial or parallel port, or a driver that
/// can stall. The base name is the part before the first `.` or `:`, with
/// trailing spaces and dots trimmed, compared ignoring ASCII case.
fn is_dos_device(component: &str) -> bool {
    const NAMES: [&str; 32] = [
        "CON",
        "PRN",
        "AUX",
        "NUL",
        "CONIN$",
        "CONOUT$",
        "COM0",
        "COM1",
        "COM2",
        "COM3",
        "COM4",
        "COM5",
        "COM6",
        "COM7",
        "COM8",
        "COM9",
        "COM\u{b9}",
        "COM\u{b2}",
        "COM\u{b3}",
        "LPT0",
        "LPT1",
        "LPT2",
        "LPT3",
        "LPT4",
        "LPT5",
        "LPT6",
        "LPT7",
        "LPT8",
        "LPT9",
        "LPT\u{b9}",
        "LPT\u{b2}",
        "LPT\u{b3}",
    ];
    // Windows ends a device name at ':' as well as '.'.
    let base = component.split(['.', ':']).next().unwrap_or("");
    let base = base.trim_end_matches([' ', '.']);
    NAMES.iter().any(|name| base.eq_ignore_ascii_case(name))
}

/// Whether `path` names a network location: `\\server\…`, `//server/…`
/// or `\\?\UNC\…`, with either slash.
fn is_remote(path: &str) -> bool {
    let bytes = path.as_bytes();
    match win32_namespace(bytes) {
        Some((b'?', rest)) => is_unc_path(rest),
        Some(_) => false,
        None => bytes.len() > 2 && is_slash(bytes[0]) && is_slash(bytes[1]),
    }
}

const BUILTIN_READING: Reading = Reading {
    beats: 4,
    bpm: 174.0,
};

fn loader_main(weak: Weak<Inner>) {
    loop {
        let Some(inner) = weak.upgrade() else { return };
        if inner.shutdown.load(Ordering::Acquire) {
            return;
        }
        run_pending(&inner);
        drop(inner);
        std::thread::park();
    }
}

fn run_pending(inner: &Inner) {
    for slot in 0..LOOP_COUNT {
        for purpose in 0..2 {
            let request = &inner.requests[slot][purpose];
            let taken = &inner.taken[slot][purpose];
            if request.seq.load(Ordering::Acquire) == taken.load(Ordering::Relaxed) {
                continue;
            }
            let (seq, path) = {
                let buf = request.path.lock().unwrap_or_else(|e| e.into_inner());
                let seq = request.seq.load(Ordering::Acquire);
                (seq, buf.bytes[..buf.len as usize].to_vec())
            };
            taken.store(seq, Ordering::Relaxed);
            let hash = path_hash(&path);
            let result = SlotResult {
                seq,
                path_hash: hash,
                outcome: load(&path, hash, purpose == Purpose::Restore as usize),
            };
            // Replacing the old result frees it here, on the loader.
            *inner.results[slot][purpose]
                .lock()
                .unwrap_or_else(|e| e.into_inner()) = Some(result);
        }
    }
    while let Some(old) = inner.retired.pop() {
        drop(old);
    }
}

/// Loads one requested file. A panic anywhere in decoding or analysis
/// becomes `Unreadable` for that request instead of killing the loader.
fn load(path: &[u8], hash: u64, restore: bool) -> LoadOutcome {
    std::panic::catch_unwind(|| load_unguarded(path, hash, restore))
        .unwrap_or(LoadOutcome::Unreadable)
}

fn load_unguarded(path: &[u8], hash: u64, restore: bool) -> LoadOutcome {
    let Ok(path) = std::str::from_utf8(path) else {
        return LoadOutcome::Unreadable;
    };
    #[cfg(test)]
    if path == PANIC_FOR_TESTS {
        panic!("a load that panics, for tests");
    }
    // Project paths are opened without asking, and a project can come from
    // anyone, so a restore opens only an ordinary local path:
    // - Never a network path, not even to stat it. On Windows, touching
    //   `\\server\share` makes the OS sign in to that server with the
    //   user's NTLM hash, so a crafted project could harvest it.
    // - Never a namespace path (`\\?\`, `//?/`, `\\.\`, `//./`, `\??\`).
    //   Win32 normalizes the forward forms, so `..` can walk a drive path
    //   into UNC or device space, e.g. `//?/C:/../../UNC/srv/share/x.wav`;
    //   no string allowlist can be trusted there.
    // - Never a path with a `.` or `..` component, for the same reason.
    // - Never a reserved DOS device name in any component (`C:\Breaks\COM1`,
    //   `nul.wav`): `metadata` would open the port or driver before
    //   `is_file` could reject it.
    // - Never a `:` except after a leading drive letter: Windows also ends a
    //   device name at `:` (`C:\Breaks\nul:stream`), and a later `:`
    //   names an NTFS alternate data stream.
    // Our own dialog never saves such paths. A refused path shows as
    // missing and the user can Relink. Choose (a file the user picked in
    // the dialog) is trusted, minus raw devices. Keep these checks before
    // every filesystem call, `metadata` included.
    if restore && !restore_allowed(path) {
        return LoadOutcome::Missing;
    }
    if !path_allowed(path) {
        return LoadOutcome::Unreadable;
    }
    let path = Path::new(path);
    match std::fs::metadata(path) {
        Ok(meta) if meta.is_file() => {}
        Ok(_) => return LoadOutcome::Unreadable,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return LoadOutcome::Missing;
        }
        Err(_) => return LoadOutcome::Unreadable,
    }
    let sample = match decode_file_capped(path, Some(MAX_BREAK_SECS)) {
        Ok(sample) => sample,
        Err(DecodeError::Missing) => return LoadOutcome::Missing,
        Err(DecodeError::TooLong) => return LoadOutcome::Refused(Refusal::TooLong),
        Err(DecodeError::Unreadable | DecodeError::Empty) => return LoadOutcome::Unreadable,
    };
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    match read_length(sample.duration_secs(), name) {
        Ok(reading) => LoadOutcome::Loaded(Arc::new(LoadedBreak::new(sample, hash, reading))),
        Err(refusal) => LoadOutcome::Refused(refusal),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit_state::path_hash;
    use std::path::{Path, PathBuf};

    fn fixture(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name)
    }

    #[test]
    fn choose_publishes_a_loaded_break_with_its_reading() {
        let lib = Library::default();
        let path = fixture("loop_160_4beats_mono16_48k.wav");
        let seq = lib
            .request(0, path.to_str().unwrap().as_bytes(), Purpose::Choose)
            .unwrap();
        lib.run_pending();
        let r = lib.result(0, Purpose::Choose).unwrap();
        assert_eq!(r.seq, seq);
        assert_eq!(r.path_hash, path_hash(path.to_str().unwrap().as_bytes()));
        let LoadOutcome::Loaded(b) = r.outcome else {
            panic!("not loaded")
        };
        assert_eq!(
            (b.reading.beats, (b.reading.bpm * 10.0).round() / 10.0),
            (4, 160.0)
        );
    }

    #[test]
    fn too_long_and_unreadable_files_are_not_loaded() {
        let lib = Library::default();
        lib.request(
            1,
            fixture("not_audio.wav").to_str().unwrap().as_bytes(),
            Purpose::Choose,
        );
        lib.run_pending();
        assert!(matches!(
            lib.result(1, Purpose::Choose).unwrap().outcome,
            LoadOutcome::Unreadable
        ));
    }

    #[test]
    fn missing_and_non_utf8_paths_are_reported() {
        let lib = Library::default();
        lib.request(2, b"/nowhere/gone 174.wav", Purpose::Restore)
            .unwrap();
        lib.request(3, b"/bad/\xff\xfe.wav", Purpose::Restore)
            .unwrap();
        lib.run_pending();
        assert!(matches!(
            lib.result(2, Purpose::Restore).unwrap().outcome,
            LoadOutcome::Missing
        ));
        assert!(matches!(
            lib.result(3, Purpose::Restore).unwrap().outcome,
            LoadOutcome::Unreadable
        ));
        assert!(lib.result(2, Purpose::Choose).is_none());
    }

    #[test]
    fn requests_reject_bad_slots_and_long_paths() {
        let lib = Library::default();
        assert!(lib.request(4, b"/a.wav", Purpose::Choose).is_none());
        assert!(lib.request(0, &[b'a'; 1025], Purpose::Choose).is_none());
        assert!(lib.request(0, b"", Purpose::Choose).is_none());
    }

    #[test]
    fn a_newer_request_replaces_the_pending_one() {
        let lib = Library::default();
        let missing = b"/nowhere/first.wav";
        let path = fixture("loop_160_4beats_mono16_48k.wav");
        let first = lib.request(0, missing, Purpose::Choose).unwrap();
        let second = lib
            .request(0, path.to_str().unwrap().as_bytes(), Purpose::Choose)
            .unwrap();
        assert!(second > first);
        lib.run_pending();
        let r = lib.result(0, Purpose::Choose).unwrap();
        assert_eq!(r.seq, second);
        assert!(matches!(r.outcome, LoadOutcome::Loaded(_)));
    }

    #[test]
    fn try_adopt_and_audio_for_match_the_path() {
        let lib = Library::default();
        let path = fixture("loop_160_4beats_mono16_48k.wav");
        let bytes = path.to_str().unwrap().as_bytes();
        let hash = path_hash(bytes);
        assert!(lib.try_adopt(0, hash).is_none());
        lib.request(0, bytes, Purpose::Restore).unwrap();
        lib.run_pending();
        assert!(lib.try_adopt(0, hash ^ 1).is_none());
        assert!(lib.try_adopt(1, hash).is_none());
        let Some(LoadOutcome::Loaded(b)) = lib.try_adopt(0, hash) else {
            panic!("not adopted")
        };
        assert_eq!(b.path_hash, hash);
        assert!(Arc::ptr_eq(&b, &lib.audio_for(0, hash).unwrap()));
        assert!(lib.audio_for(0, hash ^ 1).is_none());
    }

    #[test]
    fn loaded_choose_wins_over_a_missing_restore() {
        let lib = Library::default();
        let path = fixture("loop_160_4beats_mono16_48k.wav");
        let bytes = path.to_str().unwrap().as_bytes();
        lib.request(0, bytes, Purpose::Choose).unwrap();
        lib.run_pending();
        // Fake a stale Missing restore for the same path.
        *lib.inner.results[0][Purpose::Restore as usize]
            .lock()
            .unwrap() = Some(SlotResult {
            seq: 99,
            path_hash: path_hash(bytes),
            outcome: LoadOutcome::Missing,
        });
        assert!(matches!(
            lib.try_adopt(0, path_hash(bytes)),
            Some(LoadOutcome::Loaded(_))
        ));
    }

    #[test]
    fn retire_queue_holds_sixteen_then_refuses_until_collected() {
        let lib = Library::default();
        let sample = StereoSample {
            frames: vec![[0.0; 2]; 16],
            sample_rate: 44_100,
        };
        let b = Arc::new(LoadedBreak::new(sample, 7, BUILTIN_READING));
        for _ in 0..16 {
            assert!(lib.retire(b.clone()).is_ok());
        }
        assert!(lib.retire(b.clone()).is_err());
        lib.collect_garbage();
        assert!(lib.retire(b.clone()).is_ok());
        lib.collect_garbage();
        assert_eq!(Arc::strong_count(&b), 1);
    }

    #[test]
    fn builtins_are_the_four_amen_bars() {
        let b = Library::builtin();
        assert!(b.iter().all(|x| !x.sample.frames.is_empty()));
        assert!(b.iter().all(|x| x.path_hash == 0 && x.reading.beats == 4));
        assert!(Arc::ptr_eq(&b[0], &Library::builtin()[0]));
    }

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("junglizer-{}-{name}", std::process::id()))
    }

    fn write_wav(path: &Path, secs: u32) {
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 8_000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut w = hound::WavWriter::create(path, spec).unwrap();
        for i in 0..secs * 8_000 {
            w.write_sample(((i % 200) as i16 - 100) * 100).unwrap();
        }
        w.finalize().unwrap();
    }

    fn outcome_of(lib: &Library, path: &[u8]) -> LoadOutcome {
        lib.request(0, path, Purpose::Choose).unwrap();
        lib.run_pending();
        lib.result(0, Purpose::Choose).unwrap().outcome
    }

    #[test]
    fn a_file_longer_than_any_break_is_refused_as_too_long() {
        let lib = Library::default();
        let path = temp_path("long.wav");
        write_wav(&path, 25);
        let outcome = outcome_of(&lib, path.to_str().unwrap().as_bytes());
        std::fs::remove_file(&path).unwrap();
        assert!(matches!(outcome, LoadOutcome::Refused(Refusal::TooLong)));
    }

    #[test]
    fn a_declared_hour_is_refused_without_decoding_it() {
        // 1.5 s of audio whose header claims an hour.
        let mut bytes = std::fs::read(fixture("loop_160_4beats_mono16_48k.wav")).unwrap();
        let data = bytes.windows(4).position(|w| w == b"data").unwrap();
        let hour = 48_000_u32 * 2 * 3_600;
        bytes[4..8].copy_from_slice(&u32::MAX.to_le_bytes()); // RIFF size unknown
        bytes[data + 4..data + 8].copy_from_slice(&hour.to_le_bytes());
        let path = temp_path("hour.wav");
        std::fs::write(&path, &bytes).unwrap();
        let lib = Library::default();
        let outcome = outcome_of(&lib, path.to_str().unwrap().as_bytes());
        std::fs::remove_file(&path).unwrap();
        assert!(matches!(outcome, LoadOutcome::Refused(Refusal::TooLong)));
    }

    #[test]
    fn only_regular_files_are_opened() {
        let lib = Library::default();
        let dir = fixture("");
        assert!(matches!(
            outcome_of(&lib, dir.to_str().unwrap().as_bytes()),
            LoadOutcome::Unreadable
        ));
    }

    #[test]
    fn windows_device_paths_are_refused_and_shares_allowed() {
        for bad in [
            r"\\.\PhysicalDrive0",
            r"\\.\COM1",
            r"\\?\GLOBALROOT\Device\Harddisk0\Partition1\a.wav",
            r"\\?\globalroot\device\a.wav",
            "//./PhysicalDrive0",
            "//?/GlobalRoot/Device/a.wav",
            r"\??\GLOBALROOT\Device\a.wav",
            r"\\?\PhysicalDrive0",
            r"\\?\Volume{abc}\x.wav",
            "//?/Volume{abc}/x.wav",
            r"\\?\CC:\a.wav",
            r"\\?\UNCX\srv\a.wav",
            r"\\?\",
            r"\\?",
            r"\\.",
            "//?/c:/a.wav",
            "//?/C:/../../UNC/srv/share/x.wav",
            "//?/C:/../UNC/srv/share/x.wav",
            r"\\?/C:/../UNC/srv/share/x.wav",
            r"/\?\C:\a.wav",
            "/??/C:/a.wav",
            r"\??/C:/a.wav",
        ] {
            assert!(!path_allowed(bad), "{bad}");
        }
        for good in [
            r"\\server\share\breaks\amen.wav",
            r"C:\Users\todd\amen.wav",
            r"\\?\C:\very\long\amen.wav",
            r"\\?\c:\a.wav",
            r"\\?\UNC\srv\share\a.wav",
            r"\\?\unc\srv\share\a.wav",
            "/Users/todd/amen.wav",
            "/home/todd/.breaks/amen.wav",
        ] {
            assert!(path_allowed(good), "{good}");
        }
        let lib = Library::default();
        assert!(matches!(
            outcome_of(&lib, br"\\.\PhysicalDrive0"),
            LoadOutcome::Unreadable
        ));
    }

    #[test]
    fn restore_refuses_drive_relative_paths() {
        for refused in ["C:CON", "C:nul.wav", "C:x.wav", "C:", r"c:..\x.wav"] {
            assert!(!restore_allowed(refused), "{refused}");
        }
        for allowed in [r"C:\a.wav", "C:/a.wav", r"c:\Breaks\x.wav"] {
            assert!(restore_allowed(allowed), "{allowed}");
        }
        // The rest after `C:\` gets the device and dot checks.
        assert!(!restore_allowed(r"C:\CON"));
        assert!(!restore_allowed(r"C:\..\x.wav"));
        // Choose is unchanged.
        assert!(path_allowed("C:CON"));
    }

    #[test]
    fn restore_refuses_a_colon_anywhere_but_after_a_drive_letter() {
        for refused in [
            r"C:\Breaks\nul:stream",
            r"C:\x\COM1:",
            r"C:\x\a.wav:evil",
            "C:x:y",
            "/x/CON:",
            r"1:\x.wav",
            "CON:x.wav",
            ":x.wav",
        ] {
            assert!(!restore_allowed(refused), "{refused}");
        }
        for allowed in [r"C:\Breaks\a.wav", r"c:\a.wav", "/Users/a/b.wav"] {
            assert!(restore_allowed(allowed), "{allowed}");
        }
        // Choose is unchanged: the user picked the file.
        assert!(path_allowed(r"C:\x\a.wav:evil"));
        // Defense in depth: a device name also ends at ':'.
        assert!(is_dos_device("nul:stream"));
        assert!(is_dos_device("COM1:"));
    }

    #[test]
    fn restore_refuses_dos_device_names_in_any_component() {
        for refused in [
            r"C:\Breaks\COM1",
            r"C:\Breaks\nul.wav",
            r"C:\Breaks\Lpt1 .wav",
            "/x/CONIN$",
            r"C:\a\CON\x.wav",
            r"C:\Breaks\conout$.wav",
            r"C:\Breaks\PRN",
            r"C:\Breaks\aux.wav",
            r"C:\Breaks\COM0.wav",
            r"C:\Breaks\lpt9.wav",
            "C:\\Breaks\\COM\u{b9}.wav",
            "/x/lpt\u{b3}",
            r"C:\Breaks\NUL. .wav",
            r"C:\Breaks\nul.tar.wav",
        ] {
            assert!(!restore_allowed(refused), "{refused}");
            assert!(path_allowed(refused), "Choose still takes {refused}");
        }
        for allowed in [
            r"C:\Breaks\Console.wav",
            r"C:\Breaks\COM10.wav",
            r"C:\Breaks\null.wav",
            "/x/auxiliary.wav",
            r"C:\Breaks\COM.wav",
            "/x/.wav",
            "C:\\Breaks\\COM\u{b4}.wav",
        ] {
            assert!(restore_allowed(allowed), "{allowed}");
        }
    }

    #[test]
    fn restore_refuses_namespaces_dot_components_and_shares() {
        for refused in [
            r"\\?\C:\breaks\a.wav",
            r"\\?\c:\a.wav",
            r"\\?\UNC\srv\share\a.wav",
            "//?/C:/../../UNC/srv/share/x.wav",
            r"\\?\C:\..\UNC\srv\share\x.wav",
            r"\\?/C:/../UNC/srv/share/x.wav",
            r"\\.\PhysicalDrive0",
            "//./COM1",
            r"\??\C:\a.wav",
            "/??/C:/a.wav",
            r"C:\Breaks\..\x.wav",
            "/Users/a/../b/x.wav",
            "/Users/a/./b/x.wav",
            r"C:\Breaks/./x.wav",
            "../x.wav",
            "/Users/a/..",
            r"\\srv\share\loop 174.wav",
            "//srv/share/a.wav",
        ] {
            assert!(!restore_allowed(refused), "{refused}");
        }
        for allowed in [
            r"C:\Breaks\amen 174.wav",
            "/Users/todd/Breaks/amen.wav",
            "/Users/todd/..hidden/a.wav",
            "/Users/todd/a..b/x.wav",
            "/Users/todd/.breaks/a.wav",
        ] {
            assert!(restore_allowed(allowed), "{allowed}");
        }
        // The user picked these in the dialog, so Choose still takes them.
        for chosen in [
            r"C:\Breaks\..\x.wav",
            "/Users/a/../b/x.wav",
            r"\\?\C:\breaks\a.wav",
            r"\\srv\share\loop 174.wav",
        ] {
            assert!(path_allowed(chosen), "{chosen}");
        }
    }

    #[test]
    fn restore_publishes_missing_for_a_dotted_path_that_choose_opens() {
        let lib = Library::default();
        let fixture_path = fixture("loop_160_4beats_mono16_48k.wav");
        let dir = fixture_path.parent().unwrap();
        let dotted = format!(
            "{}/../fixtures/loop_160_4beats_mono16_48k.wav",
            dir.to_str().unwrap()
        );
        lib.request(0, dotted.as_bytes(), Purpose::Restore).unwrap();
        lib.request(0, dotted.as_bytes(), Purpose::Choose).unwrap();
        lib.request(1, b"//?/C:/../../UNC/srv/share/x.wav", Purpose::Restore)
            .unwrap();
        lib.request(1, b"//?/C:/../../UNC/srv/share/x.wav", Purpose::Choose)
            .unwrap();
        lib.run_pending();
        assert!(matches!(
            lib.result(0, Purpose::Restore).unwrap().outcome,
            LoadOutcome::Missing
        ));
        assert!(matches!(
            lib.result(0, Purpose::Choose).unwrap().outcome,
            LoadOutcome::Loaded(_)
        ));
        assert!(matches!(
            lib.result(1, Purpose::Restore).unwrap().outcome,
            LoadOutcome::Missing
        ));
        assert!(matches!(
            lib.result(1, Purpose::Choose).unwrap().outcome,
            LoadOutcome::Unreadable
        ));
    }

    #[test]
    fn network_paths_are_recognized_as_remote() {
        for remote in [
            r"\\srv\share\loop 174.wav",
            "//srv/share/loop.wav",
            r"\\?\UNC\srv\share\a.wav",
            r"\\?\unc\srv\share\a.wav",
            "//?/UNC/srv/share/a.wav",
            r"\/srv\share\a.wav",
        ] {
            assert!(is_remote(remote), "{remote}");
        }
        for local in [
            r"C:\breaks\a.wav",
            r"\\?\C:\breaks\a.wav",
            "/Users/todd/a.wav",
            "relative/a.wav",
            r"\breaks\a.wav",
            "",
        ] {
            assert!(!is_remote(local), "{local}");
        }
    }

    #[test]
    fn restore_never_touches_a_network_path_but_choose_may() {
        // Opening a share from a project would leak the user's NTLM hash.
        let lib = Library::default();
        lib.request(0, br"\\srv\share\loop 174.wav", Purpose::Restore)
            .unwrap();
        lib.run_pending();
        assert!(matches!(
            lib.result(0, Purpose::Restore).unwrap().outcome,
            LoadOutcome::Missing
        ));
        // A real local file spelled with a leading `//`: POSIX resolves it
        // like `/`, but the rule counts it as remote. Restore refuses it
        // without looking; Choose opens and loads it.
        let local = fixture("loop_160_4beats_mono16_48k.wav");
        let remote = format!("/{}", local.to_str().unwrap());
        assert!(is_remote(&remote));
        lib.request(1, remote.as_bytes(), Purpose::Restore).unwrap();
        lib.request(1, remote.as_bytes(), Purpose::Choose).unwrap();
        lib.run_pending();
        assert!(matches!(
            lib.result(1, Purpose::Restore).unwrap().outcome,
            LoadOutcome::Missing
        ));
        assert!(matches!(
            lib.result(1, Purpose::Choose).unwrap().outcome,
            LoadOutcome::Loaded(_)
        ));
    }

    #[test]
    fn a_panicking_load_is_unreadable_and_the_loader_carries_on() {
        let lib = Library::default();
        assert!(matches!(
            outcome_of(&lib, PANIC_FOR_TESTS.as_bytes()),
            LoadOutcome::Unreadable
        ));
        let path = fixture("loop_160_4beats_mono16_48k.wav");
        assert!(matches!(
            outcome_of(&lib, path.to_str().unwrap().as_bytes()),
            LoadOutcome::Loaded(_)
        ));
    }

    #[test]
    fn retiring_wakes_the_loader_to_free_it() {
        let lib = Library::default();
        lib.start();
        let sample = StereoSample {
            frames: vec![[0.0; 2]; 16],
            sample_rate: 44_100,
        };
        let b = Arc::new(LoadedBreak::new(sample, 7, BUILTIN_READING));
        // Let the loader finish a request and go back to sleep first.
        let path = fixture("loop_160_4beats_mono16_48k.wav");
        let seq = lib
            .request(0, path.to_str().unwrap().as_bytes(), Purpose::Choose)
            .unwrap();
        while lib.result(0, Purpose::Choose).is_none_or(|r| r.seq != seq) {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
        assert!(lib.retire(b.clone()).is_ok());
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while Arc::strong_count(&b) > 1 {
            assert!(
                std::time::Instant::now() < deadline,
                "loader never freed it"
            );
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }

    #[test]
    fn started_loader_runs_requests_and_exits_on_drop() {
        let lib = Library::default();
        lib.start();
        lib.start(); // idempotent
        let path = fixture("loop_160_4beats_mono16_48k.wav");
        let seq = lib
            .request(0, path.to_str().unwrap().as_bytes(), Purpose::Choose)
            .unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while lib.result(0, Purpose::Choose).is_none_or(|r| r.seq != seq) {
            assert!(std::time::Instant::now() < deadline, "loader never ran");
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let weak = Arc::downgrade(&lib.inner);
        let handle = lib.inner.handle.lock().unwrap().take().unwrap();
        drop(lib);
        handle.join().unwrap();
        assert!(weak.upgrade().is_none());
    }
}
