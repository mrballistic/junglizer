//! junglizer patch: debug logging. Appends to
//! ~/Library/Logs/Junglizer/debug.log when the `debug-log` feature is on;
//! a no-op otherwise.

#[cfg(feature = "debug-log")]
pub fn write(line: std::fmt::Arguments<'_>) {
    use std::io::Write;
    let Some(home) = std::env::var_os("HOME") else {
        return;
    };
    let dir = std::path::Path::new(&home).join("Library/Logs/Junglizer");
    let _ = std::fs::create_dir_all(&dir);
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("debug.log"))
    {
        let t = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs_f64())
            .unwrap_or_default();
        let _ = writeln!(f, "{t:.3} [{}] truce-egui {line}", std::process::id());
    }
}

#[macro_export]
macro_rules! debug_log {
    ($($arg:tt)*) => {{
        #[cfg(feature = "debug-log")]
        $crate::debug_log::write(format_args!($($arg)*));
    }};
}
