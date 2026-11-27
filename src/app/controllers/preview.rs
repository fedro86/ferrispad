//! Preview controller for Markdown rendering.
//!
//! This module handles Markdown to HTML conversion and opens the preview
//! in the user's default browser. This approach uses zero memory in FerrisPad
//! and works on all platforms.

use pulldown_cmark::{Options, Parser, html};
use std::borrow::Cow;
use std::collections::HashSet;
use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::Path;

/// Name of the preview folder inside the chosen base directory.
const PREVIEW_DIR_NAME: &str = "ferrispad_preview";

/// Directory for the preview files, chosen from where FerrisPad runs.
///
/// In a Flatpak the temp dir is the sandbox's private `/tmp`, which the host
/// browser (reached through the OpenURI portal) cannot read, so the preview
/// goes to the app's cache dir: `~/.var/app/<id>/cache` is the same path on
/// the host. Everywhere else, and in a Flatpak without a cache dir, it stays
/// in the temp dir.
fn preview_dir_for(
    in_flatpak: bool,
    cache_dir: Option<&Path>,
    temp_dir: &Path,
) -> std::path::PathBuf {
    match cache_dir {
        Some(cache) if in_flatpak => cache.join(PREVIEW_DIR_NAME),
        _ => temp_dir.join(PREVIEW_DIR_NAME),
    }
}

/// The preview directory for this process.
fn default_preview_dir() -> std::path::PathBuf {
    preview_dir_for(
        crate::app::services::updater::is_flatpak(),
        dirs::cache_dir().as_deref(),
        &std::env::temp_dir(),
    )
}

/// Preview files older than this are leftovers of a crashed or killed run.
/// Newer ones may still be in use by another FerrisPad window.
const STALE_PREVIEW_AGE: std::time::Duration = std::time::Duration::from_secs(24 * 60 * 60);

/// True for a name FerrisPad gives its preview files: `preview_<16 hex>.html`.
fn is_preview_file_name(name: &std::ffi::OsStr) -> bool {
    let Some(name) = name.to_str() else {
        return false;
    };
    name.strip_prefix("preview_")
        .and_then(|rest| rest.strip_suffix(".html"))
        .is_some_and(|id| id.len() == 16 && id.bytes().all(|b| b.is_ascii_hexdigit()))
}

/// Remove preview files older than [`STALE_PREVIEW_AGE`] from `dir`: the
/// leftovers of a crashed or killed run, which `cleanup` never saw. Only
/// regular files named like a preview, directly inside `dir`; symlinks and
/// subdirectories are skipped. Any error is ignored.
fn sweep_stale_previews(dir: &Path, now: std::time::SystemTime) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        if !is_preview_file_name(&entry.file_name()) {
            continue;
        }
        let path = entry.path();
        // symlink_metadata: a symlink is seen as a link, never followed.
        let Ok(meta) = fs::symlink_metadata(&path) else {
            continue;
        };
        if !meta.file_type().is_file() {
            continue;
        }
        let stale = meta
            .modified()
            .ok()
            .and_then(|mtime| now.duration_since(mtime).ok())
            .is_some_and(|age| age > STALE_PREVIEW_AGE);
        if stale {
            let _ = fs::remove_file(&path);
        }
    }
}

/// Create the preview directory if needed and check that it is safe to write
/// into: on Unix, a real directory (not a symlink) owned by this user, with
/// mode 0700. Anything else is refused, so nothing is written or swept there.
/// On Windows the per-user `%TEMP%` is used as is.
fn prepare_preview_dir(dir: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(dir)?;
        // symlink_metadata: a symlink is seen as a link, never followed.
        let meta = fs::symlink_metadata(dir)?;
        if !meta.file_type().is_dir() {
            return Err(std::io::Error::other(
                "the preview folder is not a real directory",
            ));
        }
        if meta.uid() != nix::unistd::geteuid().as_raw() {
            return Err(std::io::Error::other(
                "the preview folder belongs to another user",
            ));
        }
        if meta.permissions().mode() & 0o777 != 0o700 {
            fs::set_permissions(dir, fs::Permissions::from_mode(0o700))?;
        }
        Ok(())
    }
    #[cfg(not(unix))]
    {
        fs::create_dir_all(dir)
    }
}

/// Why a browser preview did not open — shown to the user as a popup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreviewNotice {
    /// The active document is not a Markdown file (an info notice).
    NotMarkdown,
    /// Writing the preview file or launching the browser failed.
    OpenFailed(String),
}

impl PreviewNotice {
    /// Popup title.
    pub fn title(&self) -> &'static str {
        match self {
            Self::NotMarkdown => "Preview",
            Self::OpenFailed(_) => "Preview Failed",
        }
    }

    /// Popup message.
    pub fn message(&self) -> String {
        match self {
            Self::NotMarkdown => {
                "Preview is only available for Markdown files (.md, .markdown, .mdown)".to_string()
            }
            Self::OpenFailed(e) => e.clone(),
        }
    }
}

pub struct PreviewController {
    /// Directory the preview files are written to, chosen once.
    dir: std::path::PathBuf,
    /// Set of temp file paths created during this session.
    temp_files: HashSet<std::path::PathBuf>,
}

impl PreviewController {
    pub fn new() -> Self {
        Self::with_dir(default_preview_dir())
    }

    /// A controller that writes its preview files to `dir`. Once, at
    /// startup and with no timer, it drops what a crashed run left there,
    /// unless the dir was refused.
    fn with_dir(dir: std::path::PathBuf) -> Self {
        if prepare_preview_dir(&dir).is_ok() {
            sweep_stale_previews(&dir, std::time::SystemTime::now());
        }

        Self {
            dir,
            temp_files: HashSet::new(),
        }
    }

    /// Generate a unique temp file path for a document.
    /// Uses file path hash for saved files (stable across sessions),
    /// or doc_id for untitled documents.
    fn temp_path_for_doc(&self, file_path: Option<&str>, doc_id: u64) -> std::path::PathBuf {
        let id = match file_path {
            Some(path) => {
                // Hash the file path for stable naming across sessions
                let mut hasher = DefaultHasher::new();
                path.hash(&mut hasher);
                hasher.finish()
            }
            None => doc_id, // Untitled documents use doc_id
        };
        self.dir.join(format!("preview_{:016x}.html", id))
    }

    /// Render markdown text to HTML.
    pub fn render_markdown(text: &str) -> String {
        let mut options = Options::empty();
        options.insert(Options::ENABLE_TABLES);
        options.insert(Options::ENABLE_STRIKETHROUGH);
        options.insert(Options::ENABLE_TASKLISTS);

        let parser = Parser::new_ext(text, options);
        let mut html_output = String::new();
        html::push_html(&mut html_output, parser);

        html_output
    }

    /// Check if a file path points to a markdown file.
    pub fn is_markdown_file(path: Option<&str>) -> bool {
        match path {
            Some(p) => {
                let lower = p.to_lowercase();
                lower.ends_with(".md") || lower.ends_with(".markdown") || lower.ends_with(".mdown")
            }
            None => false,
        }
    }

    /// Render and open a markdown document in the default browser.
    /// Returns the notice to show the user if the file is not markdown or the
    /// browser launch fails (the caller shows it in a themed popup).
    pub fn preview_in_browser(
        &mut self,
        file_path: Option<&str>,
        doc_id: u64,
        text: &str,
        dark_mode: bool,
    ) -> Result<(), PreviewNotice> {
        if !Self::is_markdown_file(file_path) {
            return Err(PreviewNotice::NotMarkdown);
        }

        let raw_html = Self::render_markdown(text);
        let base_dir = file_path
            .and_then(|p| Path::new(p).parent())
            .map(|p| p.to_path_buf());
        let wrapped = wrap_html_for_preview(&raw_html, dark_mode, base_dir.as_deref());

        self.open_in_browser(file_path, doc_id, &wrapped)
            .map_err(PreviewNotice::OpenFailed)
    }

    /// Open markdown preview in the default browser for a specific document.
    /// Uses file_path for stable naming (survives app restarts).
    /// Returns Ok(()) if successful, Err with message otherwise.
    pub fn open_in_browser(
        &mut self,
        file_path: Option<&str>,
        doc_id: u64,
        html: &str,
    ) -> Result<(), String> {
        let temp_path = self.write_preview(file_path, doc_id, html)?;
        open::that(&temp_path).map_err(|e| format!("Failed to open browser: {}", e))
    }

    /// Write a document's preview file, creating the directory if needed, and
    /// track it for cleanup. Returns the file's path.
    fn write_preview(
        &mut self,
        file_path: Option<&str>,
        doc_id: u64,
        html: &str,
    ) -> Result<std::path::PathBuf, String> {
        let temp_path = self.temp_path_for_doc(file_path, doc_id);

        // Ensure preview directory exists (may have been cleaned since app
        // start) and is safe to write into.
        prepare_preview_dir(&self.dir)
            .map_err(|e| format!("Failed to write preview file: {}", e))?;

        fs::write(&temp_path, html).map_err(|e| format!("Failed to write preview file: {}", e))?;

        self.temp_files.insert(temp_path.clone());
        Ok(temp_path)
    }

    /// Write HTML to the temp file for a specific document without opening browser.
    /// Used for updating preview when saving markdown files.
    /// Only updates if a preview exists (either opened this session or from a previous session).
    pub fn write_html(
        &mut self,
        file_path: Option<&str>,
        doc_id: u64,
        html: &str,
    ) -> Result<(), String> {
        let temp_path = self.temp_path_for_doc(file_path, doc_id);

        // Only update if preview file exists (from this session or a previous one)
        if !self.temp_files.contains(&temp_path) && !temp_path.exists() {
            return Ok(());
        }
        prepare_preview_dir(&self.dir)
            .map_err(|e| format!("Failed to write preview file: {}", e))?;

        fs::write(&temp_path, html).map_err(|e| format!("Failed to write preview file: {}", e))?;

        // Track for cleanup (in case it was from a previous session)
        self.temp_files.insert(temp_path.clone());

        Ok(())
    }

    /// Clean up all temp files created during this session.
    pub fn cleanup(&self) {
        for path in &self.temp_files {
            let _ = fs::remove_file(path);
        }
        // Try to remove the directory (will only succeed if empty)
        let _ = fs::remove_dir(&self.dir);
    }
}

impl Default for PreviewController {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for PreviewController {
    fn drop(&mut self) {
        self.cleanup();
    }
}

/// Allowed URL schemes for links and resources in preview HTML.
/// Any URL with a scheme not in this list is neutralized to prevent
/// protocol-handler attacks (e.g. SMB, ms-msdt, search-ms).
const ALLOWED_SCHEMES: &[&str] = &["https://", "http://", "file://", "data:", "mailto:"];

/// Check whether a URL uses an allowed scheme or is a relative/anchor reference.
fn is_safe_url(url: &str) -> bool {
    // Fragment-only links (#section) are always safe
    if url.starts_with('#') {
        return true;
    }
    // Absolute paths are safe (resolved to file://)
    if url.starts_with('/') {
        return true;
    }
    // Check against allowed schemes
    if ALLOWED_SCHEMES.iter().any(|s| url.starts_with(s)) {
        return true;
    }
    // No colon before the first slash means it's a relative path (safe)
    // e.g. "images/logo.png", "docs/readme.md"
    // A colon before any slash means it's an unknown scheme (unsafe)
    // e.g. "smb://...", "ms-msdt:/...", "custom:..."
    match url.find(':') {
        Some(colon_pos) => match url.find('/') {
            Some(slash_pos) => slash_pos < colon_pos, // relative path like "dir/file:name"
            None => false,                            // "scheme:payload" with no slash
        },
        None => true, // No colon at all — relative path
    }
}

/// Convert relative paths in src/href attributes to absolute file:// URLs.
/// Neutralizes URLs with disallowed protocol schemes to prevent RCE attacks.
fn resolve_relative_paths<'a>(html: &'a str, base_dir: &Path) -> Cow<'a, str> {
    let re_src = regex_lite::Regex::new(r#"(src|href)="([^"]+)""#).unwrap();

    if !re_src.is_match(html) {
        return Cow::Borrowed(html);
    }

    let base_str = base_dir.to_string_lossy();
    let result = re_src.replace_all(html, |caps: &regex_lite::Captures| {
        let attr = &caps[1];
        let url = &caps[2];

        // Neutralize URLs with disallowed schemes
        if !is_safe_url(url) {
            // Replace with a safe fragment that explains why the link was blocked
            return format!(r##"{}="#" title="Blocked: unsafe protocol""##, attr);
        }

        // Block data: URIs in href attributes (XSS via data:text/html navigation).
        // data: is safe in src attributes (inline images/media) but dangerous in
        // links because the browser navigates to a new document without CSP.
        if attr == "href" && url.starts_with("data:") {
            return format!(r##"{}="#" title="Blocked: data URI in link""##, attr);
        }

        // Preserve allowed absolute URLs
        if ALLOWED_SCHEMES.iter().any(|s| url.starts_with(s))
            || url.starts_with('/')
            || url.starts_with('#')
        {
            format!(r#"{}="{}""#, attr, url)
        } else {
            // Relative path — resolve against base directory
            format!(r#"{}="file://{}/{}""#, attr, base_str, url)
        }
    });

    Cow::Owned(result.into_owned())
}

/// Wrap HTML in a full HTML5 document with CSS styling.
/// If `base_dir` is provided, relative image/video paths will be resolved to absolute file:// URLs.
pub fn wrap_html_for_preview(html: &str, dark_mode: bool, base_dir: Option<&Path>) -> String {
    let (bg, fg, code_bg, border, link) = if dark_mode {
        ("#1e1e1e", "#d4d4d4", "#2d2d2d", "#444", "#569cd6")
    } else {
        ("#ffffff", "#333333", "#f4f4f4", "#ddd", "#0066cc")
    };

    let resolved_html = match base_dir {
        Some(dir) => resolve_relative_paths(html, dir),
        None => Cow::Borrowed(html),
    };

    format!(
        r#"<!DOCTYPE html>
<html>
<head>
    <meta charset="UTF-8">
    <meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src 'unsafe-inline'; img-src file: data: https: http:; media-src file:; font-src file:;">
    <title>FerrisPad Preview</title>
    <style>
        body {{
            font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
            line-height: 1.6;
            max-width: 800px;
            margin: 0 auto;
            padding: 20px;
            background: {bg};
            color: {fg};
        }}
        h1, h2, h3, h4, h5, h6 {{
            margin-top: 1.5em;
            margin-bottom: 0.5em;
        }}
        h1 {{ font-size: 2em; border-bottom: 1px solid {border}; padding-bottom: 0.3em; }}
        h2 {{ font-size: 1.5em; border-bottom: 1px solid {border}; padding-bottom: 0.3em; }}
        code {{
            background: {code_bg};
            padding: 2px 6px;
            border-radius: 3px;
            font-family: "Consolas", "Monaco", monospace;
            font-size: 0.9em;
        }}
        pre {{
            background: {code_bg};
            padding: 16px;
            overflow-x: auto;
            border-radius: 6px;
        }}
        pre code {{
            background: transparent;
            padding: 0;
        }}
        table {{
            border-collapse: collapse;
            width: 100%;
            margin: 1em 0;
        }}
        th, td {{
            border: 1px solid {border};
            padding: 8px 12px;
            text-align: left;
        }}
        th {{
            background: {code_bg};
        }}
        blockquote {{
            border-left: 4px solid {border};
            margin: 1em 0;
            padding-left: 16px;
            color: {fg};
            opacity: 0.8;
        }}
        img {{
            max-width: 100%;
            height: auto;
        }}
        a {{
            color: {link};
        }}
        hr {{
            border: none;
            border-top: 1px solid {border};
            margin: 2em 0;
        }}
        ul, ol {{
            padding-left: 2em;
        }}
        li {{
            margin: 0.3em 0;
        }}
        /* Task list styling */
        input[type="checkbox"] {{
            margin-right: 0.5em;
        }}
    </style>
</head>
<body>
{resolved_html}
</body>
</html>"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_dir_in_flatpak_uses_the_cache_dir() {
        assert_eq!(
            preview_dir_for(true, Some(Path::new("/c")), Path::new("/t")),
            Path::new("/c/ferrispad_preview")
        );
    }

    #[test]
    fn preview_dir_outside_flatpak_is_unchanged() {
        assert_eq!(
            preview_dir_for(false, Some(Path::new("/c")), Path::new("/t")),
            Path::new("/t/ferrispad_preview")
        );
    }

    #[test]
    fn preview_dir_in_flatpak_without_cache_dir_falls_back() {
        assert_eq!(
            preview_dir_for(true, None, Path::new("/t")),
            Path::new("/t/ferrispad_preview")
        );
    }

    #[test]
    fn sweep_removes_only_old_preview_files() {
        use std::time::{Duration, SystemTime};
        let scratch = tempfile::tempdir().unwrap();
        let dir = scratch.path().join(PREVIEW_DIR_NAME);
        fs::create_dir(&dir).unwrap();
        let now = SystemTime::now();
        let old = now - Duration::from_secs(25 * 60 * 60);
        let touch = |path: &Path, mtime: SystemTime| {
            fs::write(path, "x").unwrap();
            fs::File::options()
                .write(true)
                .open(path)
                .unwrap()
                .set_modified(mtime)
                .unwrap();
        };

        let old_preview = dir.join("preview_0123456789abcdef.html");
        let new_preview = dir.join("preview_fedcba9876543210.html");
        let old_foreign = dir.join("notes.html");
        let old_bad_hex = dir.join("preview_0123.html");
        let subdir = dir.join("preview_aaaaaaaaaaaaaaaa.html");
        let outside = scratch.path().join("preview_bbbbbbbbbbbbbbbb.html");
        touch(&old_preview, old);
        touch(&new_preview, now);
        touch(&old_foreign, old);
        touch(&old_bad_hex, old);
        touch(&outside, old);
        fs::create_dir(&subdir).unwrap();
        let inner = subdir.join("preview_cccccccccccccccc.html");
        touch(&inner, old);
        #[cfg(unix)]
        let link = {
            let link = dir.join("preview_dddddddddddddddd.html");
            std::os::unix::fs::symlink(&outside, &link).unwrap();
            link
        };

        sweep_stale_previews(&dir, now);

        assert!(!old_preview.exists(), "an old preview file is removed");
        assert!(new_preview.exists(), "a recent preview may be in use");
        assert!(old_foreign.exists(), "only the preview naming is touched");
        assert!(old_bad_hex.exists(), "only the preview naming is touched");
        assert!(inner.exists(), "no recursion");
        assert!(outside.exists(), "nothing outside the dir");
        #[cfg(unix)]
        assert!(
            fs::symlink_metadata(&link).is_ok(),
            "a symlink is skipped, not removed or followed"
        );
    }

    #[test]
    fn sweep_of_a_missing_dir_is_a_no_op() {
        let scratch = tempfile::tempdir().unwrap();
        sweep_stale_previews(
            &scratch.path().join("missing"),
            std::time::SystemTime::now(),
        );
    }

    #[cfg(unix)]
    #[test]
    fn existing_open_dir_is_made_private() {
        use std::os::unix::fs::PermissionsExt;
        let scratch = tempfile::tempdir().unwrap();
        let dir = scratch.path().join(PREVIEW_DIR_NAME);
        fs::create_dir(&dir).unwrap();
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o777)).unwrap();
        let preview = PreviewController::with_dir(dir.clone());
        let mode = fs::metadata(&dir).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o700, "a pre-existing open preview dir is tightened");
        drop(preview);
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_preview_dir_is_refused() {
        use std::time::{Duration, SystemTime};
        let scratch = tempfile::tempdir().unwrap();
        let target = scratch.path().join("elsewhere");
        fs::create_dir(&target).unwrap();
        let stale = target.join("preview_0123456789abcdef.html");
        fs::write(&stale, "x").unwrap();
        fs::File::options()
            .write(true)
            .open(&stale)
            .unwrap()
            .set_modified(SystemTime::now() - Duration::from_secs(25 * 60 * 60))
            .unwrap();
        let dir = scratch.path().join(PREVIEW_DIR_NAME);
        std::os::unix::fs::symlink(&target, &dir).unwrap();

        let mut preview = PreviewController::with_dir(dir.clone());
        let err = preview
            .write_preview(Some("/notes/a.md"), 1, "<p>one</p>")
            .unwrap_err();
        assert!(err.starts_with("Failed to write preview file"), "{err}");
        let written: Vec<_> = fs::read_dir(&target)
            .unwrap()
            .flatten()
            .map(|e| e.file_name())
            .collect();
        assert_eq!(written, vec![stale.file_name().unwrap().to_owned()]);
        assert!(stale.exists(), "no sweep in a refused dir");
        drop(preview);
        assert!(
            fs::symlink_metadata(&dir).is_ok(),
            "the link itself is left alone"
        );
    }

    /// FLATPAK_ID alone is not a Flatpak: a host process can inherit it, e.g.
    /// from VS Code's Flatpak (`com.visualstudio.code`). The preview uses the
    /// updater's rule, `/.flatpak-info` only. The test re-runs itself with
    /// only that variable set.
    #[test]
    fn flatpak_id_alone_is_not_a_flatpak() {
        use crate::app::services::updater::is_flatpak;
        const PROBE: &str = "FERRISPAD_FLATPAK_ID_PROBE";
        if std::env::var_os(PROBE).is_some() {
            assert!(!is_flatpak(), "FLATPAK_ID alone counted as a Flatpak");
            return;
        }
        if Path::new("/.flatpak-info").exists() {
            return; // inside a real sandbox: nothing to show here
        }
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "app::controllers::preview::tests::flatpak_id_alone_is_not_a_flatpak",
                "--test-threads=1",
            ])
            .env("FLATPAK_ID", "com.visualstudio.code")
            .env(PROBE, "1")
            .status()
            .unwrap();
        assert!(status.success(), "FLATPAK_ID alone counted as a Flatpak");
    }

    #[test]
    fn cleanup_removes_files_and_dir() {
        let scratch = tempfile::tempdir().unwrap();
        let dir = scratch.path().join("cache").join(PREVIEW_DIR_NAME);
        let mut preview = PreviewController::with_dir(dir.clone());
        assert!(dir.is_dir());

        let path = preview
            .write_preview(Some("/notes/a.md"), 1, "<p>one</p>")
            .unwrap();
        assert!(path.starts_with(&dir));
        // The live refresh on save rewrites the same file.
        preview
            .write_html(Some("/notes/a.md"), 1, "<p>two</p>")
            .unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "<p>two</p>");

        preview.cleanup();
        assert!(!path.exists());
        assert!(!dir.exists());
    }

    #[cfg(unix)]
    #[test]
    fn preview_dir_is_private_to_the_user() {
        use std::os::unix::fs::PermissionsExt;
        let scratch = tempfile::tempdir().unwrap();
        let dir = scratch.path().join("cache").join(PREVIEW_DIR_NAME);
        let preview = PreviewController::with_dir(dir.clone());
        let mode = fs::metadata(&dir).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o700);
        drop(preview);
    }

    #[test]
    fn test_is_markdown_file() {
        assert!(PreviewController::is_markdown_file(Some("test.md")));
        assert!(PreviewController::is_markdown_file(Some("test.markdown")));
        assert!(PreviewController::is_markdown_file(Some("test.mdown")));
        assert!(!PreviewController::is_markdown_file(Some("test.txt")));
        assert!(!PreviewController::is_markdown_file(None));
    }

    // T0024: the controller returns the notice instead of opening an unthemed
    // FLTK popup itself; a non-Markdown file never reaches the browser.
    #[test]
    fn preview_of_non_markdown_returns_the_info_notice() {
        let mut preview = PreviewController::new();
        assert_eq!(
            preview.preview_in_browser(Some("notes.txt"), 1, "text", true),
            Err(PreviewNotice::NotMarkdown)
        );
        assert_eq!(
            preview.preview_in_browser(None, 2, "text", false),
            Err(PreviewNotice::NotMarkdown)
        );
    }

    #[test]
    fn preview_notice_titles_and_messages() {
        assert_eq!(PreviewNotice::NotMarkdown.title(), "Preview");
        assert!(PreviewNotice::NotMarkdown.message().contains(".md"));
        let failed = PreviewNotice::OpenFailed("no browser".to_string());
        assert_eq!(failed.title(), "Preview Failed");
        assert_eq!(failed.message(), "no browser");
    }

    #[test]
    fn test_render_markdown() {
        let html = PreviewController::render_markdown("# Hello\n\nWorld");
        assert!(html.contains("<h1>Hello</h1>"));
        assert!(html.contains("<p>World</p>"));
    }

    #[test]
    fn test_wrap_html_light() {
        let html = wrap_html_for_preview("<p>Test</p>", false, None);
        assert!(html.contains("<!DOCTYPE html>"));
        assert!(html.contains("background: #ffffff"));
        assert!(html.contains("<p>Test</p>"));
    }

    #[test]
    fn test_wrap_html_dark() {
        let html = wrap_html_for_preview("<p>Test</p>", true, None);
        assert!(html.contains("background: #1e1e1e"));
    }

    #[test]
    fn test_resolve_relative_paths() {
        let html = r#"<img src="images/logo.png"><a href="docs/readme.md">Link</a>"#;
        let result = resolve_relative_paths(html, Path::new("/home/user/project"));
        assert!(result.contains(r#"src="file:///home/user/project/images/logo.png""#));
        assert!(result.contains(r#"href="file:///home/user/project/docs/readme.md""#));
    }

    #[test]
    fn test_resolve_keeps_absolute_paths() {
        let html = r#"<img src="https://example.com/img.png"><img src="/absolute/path.png">"#;
        let result = resolve_relative_paths(html, Path::new("/home/user"));
        assert!(result.contains(r#"src="https://example.com/img.png""#));
        assert!(result.contains(r#"src="/absolute/path.png""#));
    }

    // --- CVE-2026-20841 mitigation tests ---

    #[test]
    fn test_is_safe_url() {
        // Safe URLs
        assert!(is_safe_url("https://example.com"));
        assert!(is_safe_url("http://example.com"));
        assert!(is_safe_url("file:///tmp/test.html"));
        assert!(is_safe_url("data:image/png;base64,abc"));
        assert!(is_safe_url("mailto:user@example.com"));
        assert!(is_safe_url("#section"));
        assert!(is_safe_url("/absolute/path.png"));
        assert!(is_safe_url("relative/path.png"));
        assert!(is_safe_url("image.png"));
        assert!(is_safe_url("dir/file:with-colon.txt")); // colon after slash = relative

        // Unsafe URLs — protocol handler attacks
        assert!(!is_safe_url("smb://attacker/share/payload.exe"));
        assert!(!is_safe_url("ms-msdt:/id"));
        assert!(!is_safe_url("search-ms:query=attacker"));
        assert!(!is_safe_url("vbscript:MsgBox"));
        assert!(!is_safe_url("javascript:alert(1)"));
        assert!(!is_safe_url("custom:anything"));
    }

    #[test]
    fn test_blocks_smb_links() {
        let html = r#"<a href="smb://attacker/share/malware.exe">Click</a>"#;
        let result = resolve_relative_paths(html, Path::new("/home/user"));
        assert!(!result.contains("smb://"));
        assert!(result.contains(r##"href="#""##));
        assert!(result.contains("Blocked: unsafe protocol"));
    }

    #[test]
    fn test_blocks_ms_msdt_protocol() {
        let html = r#"<a href="ms-msdt:/id">Exploit</a>"#;
        let result = resolve_relative_paths(html, Path::new("/home/user"));
        assert!(!result.contains("ms-msdt"));
        assert!(result.contains("Blocked: unsafe protocol"));
    }

    #[test]
    fn test_blocks_search_ms_protocol() {
        let html = r#"<a href="search-ms:query=attacker&amp;crumb=location:\\attacker">Search</a>"#;
        let result = resolve_relative_paths(html, Path::new("/home/user"));
        assert!(!result.contains("search-ms"));
        assert!(result.contains("Blocked: unsafe protocol"));
    }

    #[test]
    fn test_blocks_javascript_protocol() {
        let html = r#"<a href="javascript:alert(document.cookie)">XSS</a>"#;
        let result = resolve_relative_paths(html, Path::new("/home/user"));
        assert!(!result.contains("javascript:"));
        assert!(result.contains("Blocked: unsafe protocol"));
    }

    #[test]
    fn test_blocks_data_uri_in_href() {
        // data:text/html in links enables XSS — must be blocked
        let html = r#"<a href="data:text/html,<script>alert(1)</script>">XSS</a>"#;
        let result = resolve_relative_paths(html, Path::new("/home/user"));
        assert!(!result.contains("data:text/html"));
        assert!(result.contains("Blocked: data URI in link"));
    }

    #[test]
    fn test_allows_data_uri_in_img_src() {
        // data: in img src is safe (inline images)
        let html = r#"<img src="data:image/png;base64,iVBOR">"#;
        let result = resolve_relative_paths(html, Path::new("/home/user"));
        assert!(result.contains("data:image/png;base64,iVBOR"));
    }

    #[test]
    fn test_allows_mailto_links() {
        let html = r#"<a href="mailto:user@example.com">Email</a>"#;
        let result = resolve_relative_paths(html, Path::new("/home/user"));
        assert!(result.contains("mailto:user@example.com"));
    }

    #[test]
    fn test_allows_fragment_links() {
        let html = r##"<a href="#section-2">Jump</a>"##;
        let result = resolve_relative_paths(html, Path::new("/home/user"));
        assert!(result.contains(r##"href="#section-2""##));
    }

    #[test]
    fn test_csp_header_present() {
        let html = wrap_html_for_preview("<p>Test</p>", false, None);
        assert!(html.contains("Content-Security-Policy"));
        assert!(html.contains("default-src 'none'"));
    }
}
