//! Preview controller for Markdown rendering.
//!
//! This module handles Markdown to HTML conversion and provides CSS styling
//! for the GTK/WRY WebView preview window.

use pulldown_cmark::{Options, Parser, html};
use std::path::Path;
use std::borrow::Cow;

pub struct PreviewController {
    pub enabled: bool,
}

impl PreviewController {
    pub fn new(enabled: bool) -> Self {
        Self { enabled }
    }

    /// Render markdown text to raw HTML.
    pub fn render_markdown(text: &str) -> String {
        let mut options = Options::empty();
        options.insert(Options::ENABLE_TABLES);
        options.insert(Options::ENABLE_STRIKETHROUGH);

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
                lower.ends_with(".md")
                    || lower.ends_with(".markdown")
                    || lower.ends_with(".mdown")
            }
            None => false,
        }
    }

    /// Toggle preview state. Returns new enabled state.
    pub fn toggle(&mut self) -> bool {
        self.enabled = !self.enabled;
        self.enabled
    }
}

/// Convert relative paths in src/href attributes to absolute file:// URLs.
/// This is needed because WebKitGTK doesn't honor <base> tags for file:// URLs.
fn resolve_relative_paths<'a>(html: &'a str, base_dir: &Path) -> Cow<'a, str> {
    // Match src="..." and href="..." attributes
    let re_src = regex_lite::Regex::new(r#"(src|href)="([^"]+)""#).unwrap();

    if !re_src.is_match(html) {
        return Cow::Borrowed(html);
    }

    let base_str = base_dir.to_string_lossy();
    let result = re_src.replace_all(html, |caps: &regex_lite::Captures| {
        let attr = &caps[1];
        let path = &caps[2];

        // Skip absolute URLs and paths
        if path.starts_with("https://")
            || path.starts_with("http://")
            || path.starts_with("file://")
            || path.starts_with("data:")
            || path.starts_with('/')
        {
            // Return unchanged
            format!(r#"{}="{}""#, attr, path)
        } else {
            // Convert relative path to absolute file:// URL
            format!(r#"{}="file://{}/{}""#, attr, base_str, path)
        }
    });

    Cow::Owned(result.into_owned())
}

/// Wrap HTML in a full HTML5 document with CSS styling for WebView preview.
/// If `base_dir` is provided, relative image/video paths will be resolved to absolute file:// URLs.
pub fn wrap_html_for_webview(html: &str, dark_mode: bool, base_dir: Option<&Path>) -> String {
    let (bg, fg, code_bg, border, link) = if dark_mode {
        ("#1e1e1e", "#d4d4d4", "#2d2d2d", "#444", "#569cd6")
    } else {
        ("#ffffff", "#333333", "#f4f4f4", "#ddd", "#0066cc")
    };

    // Resolve relative paths to absolute file:// URLs
    let resolved_html = match base_dir {
        Some(dir) => resolve_relative_paths(html, dir),
        None => Cow::Borrowed(html),
    };

    format!(r#"<!DOCTYPE html>
<html>
<head>
    <meta charset="UTF-8">
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
    </style>
</head>
<body>
{resolved_html}
</body>
</html>"#)
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
    fn test_wrap_html_for_webview_light() {
        let html = wrap_html_for_webview("<p>Test</p>", false, None);
        assert!(html.contains("<!DOCTYPE html>"));
        assert!(html.contains("background: #ffffff"));
        assert!(html.contains("<p>Test</p>"));
    }

    #[test]
    fn test_wrap_html_for_webview_dark() {
        let html = wrap_html_for_webview("<p>Test</p>", true, None);
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
        // Should not modify absolute or http URLs
        assert!(result.contains(r#"src="https://example.com/img.png""#));
        assert!(result.contains(r#"src="/absolute/path.png""#));
    }

    #[test]
    fn test_wrap_html_resolves_paths() {
        let html = r#"<img src="assets/img.png">"#;
        let result = wrap_html_for_webview(html, false, Some(Path::new("/home/user/docs")));
        assert!(result.contains(r#"src="file:///home/user/docs/assets/img.png""#));
    }
}
