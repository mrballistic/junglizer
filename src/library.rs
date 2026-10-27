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

/// Whether the loader may open `path` at all. Windows device namespaces
/// (`\\.\…`, `\\?\GLOBALROOT\…`) reach raw disks and ports, so they are
/// refused, as is the NT `\??\` prefix. UNC shares (`\\server\share\…`) and long-path prefixes are fine.
fn path_allowed(path: &str) -> bool {
    let bytes = path.as_bytes();
    // `\??\` is the NT object-manager prefix, another way to a device.
    if bytes.starts_with(b"\\??\\") {
        return false;
    }
    let slash = |b: u8| b == b'\\' || b == b'/';
    if bytes.len() < 4 || !(slash(bytes[0]) && slash(bytes[1]) && slash(bytes[3])) {
        return true;
    }
    match bytes[2] {
        b'.' => false,
        b'?' => !bytes[4..]
            .get(..10)
            .is_some_and(|word| word.eq_ignore_ascii_case(b"GLOBALROOT")),
        _ => true,
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
                outcome: load(&path, hash),
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
fn load(path: &[u8], hash: u64) -> LoadOutcome {
    std::panic::catch_unwind(|| load_unguarded(path, hash)).unwrap_or(LoadOutcome::Unreadable)
}

fn load_unguarded(path: &[u8], hash: u64) -> LoadOutcome {
    let Ok(path) = std::str::from_utf8(path) else {
        return LoadOutcome::Unreadable;
    };
    #[cfg(test)]
    if path == PANIC_FOR_TESTS {
        panic!("a load that panics, for tests");
    }
    // Project paths are opened without asking, so only ever open a plain
    // file: never a device, a pipe or a directory.
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
        ] {
            assert!(!path_allowed(bad), "{bad}");
        }
        for good in [
            r"\\server\share\breaks\amen.wav",
            r"C:\Users\todd\amen.wav",
            r"\\?\C:\very\long\amen.wav",
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
