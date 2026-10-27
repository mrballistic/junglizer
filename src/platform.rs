//! Per-machine locations and handing things to the operating system.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// The empty file that records the first-launch prompt was answered on
/// this machine, or `None` when there's no config folder.
pub fn first_run_flag() -> Option<PathBuf> {
    dirs::config_dir().map(|dir| dir.join("Junglizer").join("first-run-0.9.1"))
}

/// Where MIDI exports go: the user's music folder (or `~/Music`) plus
/// `Junglizer/Exports`. `None` when there's no home folder.
pub fn export_dir() -> Option<PathBuf> {
    let music = dirs::audio_dir().or_else(|| dirs::home_dir().map(|home| home.join("Music")))?;
    Some(music.join("Junglizer").join("Exports"))
}

/// The label for the reveal button, named for this system's file manager.
pub const REVEAL_LABEL: &str = if cfg!(target_os = "macos") {
    "Show in Finder"
} else if cfg!(target_os = "windows") {
    "Show in Explorer"
} else {
    "Show in Files"
};

/// The command that reveals `path` in the file manager. macOS and Windows
/// select the file; Linux has no portable select, so it opens the parent
/// folder. No shell is involved.
pub fn reveal_command(path: &Path) -> (&'static str, Vec<OsString>) {
    if cfg!(target_os = "macos") {
        ("open", vec![OsString::from("-R"), path.into()])
    } else if cfg!(target_os = "windows") {
        let mut arg = OsString::from("/select,");
        arg.push(path);
        ("explorer", vec![arg])
    } else {
        let parent = path.parent().unwrap_or(path);
        ("xdg-open", vec![parent.into()])
    }
}

/// Reveals `path` in the file manager, spawning and returning at once.
pub fn reveal(path: &Path) {
    let (program, args) = reveal_command(path);
    spawn_detached(program, args);
}

fn spawn_detached(program: &str, args: Vec<OsString>) {
    if let Ok(mut child) = std::process::Command::new(program).args(args).spawn() {
        std::thread::spawn(move || child.wait());
    }
}

/// The system opener and its arguments for `url`. No shell is involved,
/// so nothing in the URL is interpreted.
pub fn open_url_command(url: &str) -> (&'static str, Vec<OsString>) {
    let program = if cfg!(target_os = "macos") {
        "open"
    } else if cfg!(target_os = "windows") {
        "explorer"
    } else {
        "xdg-open"
    };
    (program, vec![OsString::from(url)])
}

/// Opens `url` in the default browser. Spawns and returns at once; a
/// failure to start the opener is ignored. The opener exits quickly, and a
/// background thread reaps it so it doesn't linger as a zombie.
pub fn open_url(url: &str) {
    let (program, args) = open_url_command(url);
    spawn_detached(program, args);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_dir_ends_in_junglizer_exports() {
        assert!(
            export_dir()
                .unwrap()
                .ends_with(Path::new("Junglizer").join("Exports"))
        );
    }

    #[test]
    fn reveal_command_matches_this_os() {
        let (program, args) = reveal_command(Path::new("/tmp/x.mid"));
        #[cfg(target_os = "macos")]
        {
            assert_eq!(program, "open");
            assert_eq!(args[0], "-R");
        }
        #[cfg(target_os = "windows")]
        {
            assert_eq!(program, "explorer");
            assert!(args[0].to_string_lossy().starts_with("/select,"));
        }
        #[cfg(target_os = "linux")]
        {
            assert_eq!(program, "xdg-open");
            assert_eq!(args[0], "/tmp");
        }
    }

    #[test]
    fn open_url_uses_the_system_opener_without_a_shell() {
        let url = "https://kansamples.com/blogs/free-drum-and-bass-sample-packs";
        let (program, args) = open_url_command(url);
        let expected = if cfg!(target_os = "macos") {
            "open"
        } else if cfg!(target_os = "windows") {
            "explorer"
        } else {
            "xdg-open"
        };
        assert_eq!(program, expected);
        assert_eq!(args, vec![OsString::from(url)]);
    }

    #[test]
    fn the_first_run_flag_lives_in_the_junglizer_config_folder() {
        if let Some(flag) = first_run_flag() {
            assert!(flag.ends_with("Junglizer/first-run-0.9.1"));
            assert_eq!(
                flag.parent().unwrap().parent(),
                dirs::config_dir().as_deref()
            );
        }
    }
}
