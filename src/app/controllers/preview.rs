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

/// Directory for all FerrisPad preview files.
fn preview_dir() -> std::path::PathBuf {
    std::env::temp_dir().join("ferrispad_preview")
}

pub struct PreviewController {
    /// Set of temp file paths created during this session.
    temp_files: HashSet<std::path::PathBuf>,
}

impl PreviewController {
    pub fn new() -> Self {
        // Ensure preview directory exists
        let dir = preview_dir();
        let _ = fs::create_dir_all(&dir);

        Self {
            temp_files: HashSet::new(),
        }
    }

    /// Generate a unique temp file path for a document.
    /// Uses file path hash for saved files (stable across sessions),
    /// or doc_id for untitled documents.
    fn temp_path_for_doc(file_path: Option<&str>, doc_id: u64) -> std::path::PathBuf {
        let id = match file_path {
            Some(path) => {
                // Hash the file path for stable naming across sessions
                let mut hasher = DefaultHasher::new();
                path.hash(&mut hasher);
                hasher.finish()
            }
            None => doc_id, // Untitled documents use doc_id
        };
        preview_dir().join(format!("preview_{:016x}.html", id))
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
    /// Shows an error dialog if the file is not markdown or browser launch fails.
    pub fn preview_in_browser(
        &mut self,
        file_path: Option<&str>,
        doc_id: u64,
        text: &str,
        dark_mode: bool,
    ) {
        if !Self::is_markdown_file(file_path) {
            fltk::dialog::message_default(
                "Preview is only available for Markdown files (.md, .markdown, .mdown)",
            );
            return;
        }

        let raw_html = Self::render_markdown(text);
        let base_dir = file_path
            .and_then(|p| Path::new(p).parent())
            .map(|p| p.to_path_buf());
        let wrapped = wrap_html_for_preview(&raw_html, dark_mode, base_dir.as_deref());

        if let Err(e) = self.open_in_browser(file_path, doc_id, &wrapped) {
            fltk::dialog::alert_default(&e);
        }
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
        let temp_path = Self::temp_path_for_doc(file_path, doc_id);

        fs::write(&temp_path, html).map_err(|e| format!("Failed to write preview file: {}", e))?;

        self.temp_files.insert(temp_path.clone());

        open::that(&temp_path).map_err(|e| format!("Failed to open browser: {}", e))
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
        let temp_path = Self::temp_path_for_doc(file_path, doc_id);

        // Only update if preview file exists (from this session or a previous one)
        if !self.temp_files.contains(&temp_path) && !temp_path.exists() {
            return Ok(());
        }

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
        let _ = fs::remove_dir(preview_dir());
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
    fn test_is_markdown_file() {
        assert!(PreviewController::is_markdown_file(Some("test.md")));
        assert!(PreviewController::is_markdown_file(Some("test.markdown")));
        assert!(PreviewController::is_markdown_file(Some("test.mdown")));
        assert!(!PreviewController::is_markdown_file(Some("test.txt")));
        assert!(!PreviewController::is_markdown_file(None));
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
