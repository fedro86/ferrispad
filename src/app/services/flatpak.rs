//! Flatpak detection, shared by the updater and the Markdown preview.

use std::ffi::OsStr;
use std::path::Path;

/// True when the given environment says FerrisPad runs inside a Flatpak:
/// `FLATPAK_ID` is set (and not empty), or `/.flatpak-info` exists.
pub fn detect(flatpak_id: Option<&OsStr>, info_file_exists: bool) -> bool {
    flatpak_id.is_some_and(|id| !id.is_empty()) || info_file_exists
}

/// True when this process runs inside a Flatpak sandbox.
pub fn is_flatpak() -> bool {
    detect(
        std::env::var_os("FLATPAK_ID").as_deref(),
        Path::new("/.flatpak-info").exists(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flatpak_detection() {
        assert!(detect(Some(OsStr::new("com.ferrispad.FerrisPad")), false));
        assert!(detect(None, true));
        assert!(!detect(None, false));
        assert!(!detect(Some(OsStr::new("")), false));
    }
}
