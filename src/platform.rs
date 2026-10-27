//! Per-machine locations and handing things to the operating system.

use std::ffi::OsString;
use std::path::PathBuf;

/// The empty file that records the first-launch prompt was answered on
/// this machine, or `None` when there's no config folder.
pub fn first_run_flag() -> Option<PathBuf> {
    dirs::config_dir().map(|dir| dir.join("Junglizer").join("first-run-0.9.1"))
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
    if let Ok(mut child) = std::process::Command::new(program).args(args).spawn() {
        std::thread::spawn(move || child.wait());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
