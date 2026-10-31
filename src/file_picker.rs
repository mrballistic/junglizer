//! The Choose… and Relink… file dialog.
//!
//! On macOS the panel is non-modal: Logic hosts the editor out of process
//! (AUHostingService), and a blocking `runModal` there tears down the remote
//! view and crashes the hosting service. The panel reports back through a
//! [`Pick`] the editor polls each frame. Windows and Linux keep rfd's
//! blocking dialog, which is done before [`pick_audio_file`] returns.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// A file dialog that may still be open.
pub(crate) struct Pick(Arc<Mutex<Option<Option<PathBuf>>>>);

impl Pick {
    fn new() -> Self {
        Self(Arc::new(Mutex::new(None)))
    }

    /// `None` while the dialog is open; once it closes, the chosen file,
    /// or `Some(None)` if the user cancelled.
    pub(crate) fn take(&self) -> Option<Option<PathBuf>> {
        self.0.lock().map_or(Some(None), |mut done| done.take())
    }
}

/// Opens the dialog for one of `extensions`, repainting `ctx` when it
/// closes so the editor picks up the result.
#[cfg(target_os = "macos")]
pub(crate) fn pick_audio_file(extensions: &[&str], ctx: &egui::Context) -> Pick {
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSModalPanelWindowLevel, NSModalResponse, NSModalResponseOK, NSOpenPanel};
    use objc2_foundation::{NSArray, NSString};

    let pick = Pick::new();
    // The editor draws on the main thread; without it there's no panel.
    let Some(mtm) = MainThreadMarker::new() else {
        if let Ok(mut done) = pick.0.lock() {
            *done = Some(None);
        }
        return pick;
    };
    let panel = NSOpenPanel::openPanel(mtm);
    panel.setCanChooseFiles(true);
    panel.setCanChooseDirectories(false);
    panel.setAllowsMultipleSelection(false);
    let types: Vec<_> = extensions.iter().map(|e| NSString::from_str(e)).collect();
    #[allow(deprecated)]
    panel.setAllowedFileTypes(Some(&NSArray::from_retained_slice(&types)));

    let result = Arc::clone(&pick.0);
    let ctx = ctx.clone();
    let chosen = panel.clone();
    let handler = block2::RcBlock::new(move |response: NSModalResponse| {
        let path = (response == NSModalResponseOK)
            .then(|| chosen.URL())
            .flatten()
            .and_then(|url| url.path())
            .map(|path| PathBuf::from(path.to_string()));
        if let Ok(mut done) = result.lock() {
            *done = Some(path);
        }
        ctx.request_repaint();
    });
    // Logic's hosting service is a background process, so a panel at the
    // normal level opens behind Logic's (floating) plug-in windows. Lift it
    // above them and bring it forward without activating the service.
    panel.setLevel(NSModalPanelWindowLevel);
    panel.beginWithCompletionHandler(&handler);
    panel.orderFrontRegardless();
    pick
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn pick_audio_file(extensions: &[&str], _ctx: &egui::Context) -> Pick {
    let pick = Pick::new();
    let path = rfd::FileDialog::new()
        .add_filter("Audio", extensions)
        .pick_file();
    if let Ok(mut done) = pick.0.lock() {
        *done = Some(path);
    }
    pick
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_until_the_dialog_reports_then_taken_once() {
        let pick = Pick::new();
        assert_eq!(pick.take(), None);
        *pick.0.lock().unwrap() = Some(Some(PathBuf::from("/a.wav")));
        assert_eq!(pick.take(), Some(Some(PathBuf::from("/a.wav"))));
        assert_eq!(pick.take(), None);
    }

    #[test]
    fn a_cancel_is_reported() {
        let pick = Pick::new();
        *pick.0.lock().unwrap() = Some(None);
        assert_eq!(pick.take(), Some(None));
    }
}
