use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use pulldown_cmark::{Options, Parser, html};

/// A single image that needs resizing.
pub struct ImageResizeTask {
    /// Original src attribute value (relative or absolute).
    pub original_src: String,
    /// Absolute path to the source image on disk.
    pub absolute_path: PathBuf,
    /// Target width (may be computed from height + aspect ratio).
    pub target_width: u32,
    /// Target height (may be computed from width + aspect ratio).
    pub target_height: u32,
    /// Path where the resized copy will be written.
    pub temp_path: PathBuf,
}

/// Progress state for chunked image resizing.
pub struct ChunkedImageResize {
    pub tasks: Vec<ImageResizeTask>,
    pub progress: usize,
    /// The raw HTML from phase 1 (with src attrs to be rewritten).
    pub phase1_html: String,
    /// Directory containing the markdown file.
    pub md_dir: PathBuf,
}

/// Result from processing one image.
pub enum ImageResizeProgress {
    /// Still processing: (done, total).
    InProgress(usize, usize),
    /// All done. Contains final HTML with rewritten src attrs.
    Done(String),
}

pub struct PreviewController {
    pub enabled: bool,
    pub chunked_resize: Option<ChunkedImageResize>,
}

impl PreviewController {
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled,
            chunked_resize: None,
        }
    }

    /// Render markdown text to raw HTML (no font wrapping, no img stripping).
    /// Font wrapping is done later in state.rs after image processing.
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

    /// Write rendered HTML to a temp file next to the markdown file.
    /// Returns the temp file path, or None if writing failed.
    pub fn write_preview_file(html: &str, md_path: &str) -> Option<String> {
        let md = Path::new(md_path);
        let dir = md.parent()?;
        let temp_path = dir.join(".ferrispad-preview.html");
        fs::write(&temp_path, html).ok()?;
        Some(temp_path.to_string_lossy().to_string())
    }

    /// Remove the temp preview file if it exists.
    pub fn cleanup_preview_file(md_path: &str) {
        if let Some(dir) = Path::new(md_path).parent() {
            let temp_path = dir.join(".ferrispad-preview.html");
            let _ = fs::remove_file(temp_path);
        }
    }

    /// Toggle preview state. Returns new enabled state.
    pub fn toggle(&mut self) -> bool {
        self.enabled = !self.enabled;
        self.enabled
    }

    /// Start chunked image resizing. Returns true if there are tasks to process.
    pub fn start_image_resize(
        &mut self,
        phase1_html: String,
        tasks: Vec<ImageResizeTask>,
        md_dir: PathBuf,
    ) -> bool {
        if tasks.is_empty() {
            self.chunked_resize = None;
            return false;
        }
        self.chunked_resize = Some(ChunkedImageResize {
            tasks,
            progress: 0,
            phase1_html,
            md_dir,
        });
        true
    }

    /// Process the next image in the resize queue.
    /// Returns None if no resize is in progress.
    pub fn process_next_image(&mut self) -> Option<ImageResizeProgress> {
        let state = self.chunked_resize.as_mut()?;
        let total = state.tasks.len();

        if state.progress < total {
            let task = &state.tasks[state.progress];
            resize_image(task);
            state.progress += 1;

            if state.progress < total {
                return Some(ImageResizeProgress::InProgress(state.progress, total));
            }
        }

        // All done — rewrite sources and return final HTML
        let final_html = rewrite_img_sources(&state.phase1_html, &state.tasks, &state.md_dir);
        self.chunked_resize = None;
        Some(ImageResizeProgress::Done(final_html))
    }
}

/// Wrap HTML in HelpView-compatible font tags.
pub fn wrap_html_for_helpview(html: &str) -> String {
    format!("<font face=\"Helvetica\" size=\"4\">{}</font>", html)
}

/// Get the temp directory for resized images.
pub fn temp_image_dir() -> PathBuf {
    let dir = std::env::temp_dir().join("ferrispad-images");
    let _ = fs::create_dir_all(&dir);
    dir
}

/// Remove the entire temp image directory.
pub fn cleanup_temp_images() {
    let dir = std::env::temp_dir().join("ferrispad-images");
    let _ = fs::remove_dir_all(dir);
}

/// Parse `<img>` tags from HTML. Returns:
/// - Phase-1 HTML with width/height stripped (for immediate display)
/// - List of resize tasks for images that have width/height attributes
///
/// Skips http/https URLs (only processes local files).
pub fn extract_resize_tasks(html: &str, md_dir: &Path) -> (String, Vec<ImageResizeTask>) {
    let mut phase1_html = String::with_capacity(html.len());
    let mut tasks = Vec::new();
    let mut rest = html;

    while let Some(img_start) = rest.find("<img ") {
        phase1_html.push_str(&rest[..img_start]);

        let tag_content = &rest[img_start..];
        let tag_end = tag_content.find('>').unwrap_or(tag_content.len() - 1) + 1;
        let tag = &tag_content[..tag_end];
        let self_closing = tag.ends_with("/>");

        let src = extract_attr(tag, "src");
        let alt = extract_attr(tag, "alt");
        let width = extract_attr(tag, "width");
        let height = extract_attr(tag, "height");

        // Build phase-1 tag (stripped of width/height/style)
        phase1_html.push_str("<img");
        if let Some(s) = &src {
            phase1_html.push_str(&format!(" src=\"{}\"", s));
        }
        if let Some(a) = &alt {
            phase1_html.push_str(&format!(" alt=\"{}\"", a));
        }
        if self_closing {
            phase1_html.push_str("/>");
        } else {
            phase1_html.push('>');
        }

        // Create resize task if we have src + at least one dimension + local file
        if let Some(ref src_val) = src {
            let is_remote = src_val.starts_with("http://") || src_val.starts_with("https://");
            let has_dimensions = width.is_some() || height.is_some();

            if !is_remote && has_dimensions {
                let abs_path = if Path::new(src_val).is_absolute() {
                    PathBuf::from(src_val)
                } else {
                    md_dir.join(src_val)
                };

                if abs_path.exists() {
                    let w: Option<u32> = width.and_then(|v| v.parse().ok());
                    let h: Option<u32> = height.and_then(|v| v.parse().ok());

                    if let Some((tw, th)) = resolve_dimensions(&abs_path, w, h) {
                        let temp_path = compute_temp_path(&abs_path, tw, th);
                        tasks.push(ImageResizeTask {
                            original_src: src_val.to_string(),
                            absolute_path: abs_path,
                            target_width: tw,
                            target_height: th,
                            temp_path,
                        });
                    }
                }
            }
        }

        rest = &rest[img_start + tag_end..];
    }

    phase1_html.push_str(rest);
    (phase1_html, tasks)
}

/// Resolve target dimensions, computing missing dimension from aspect ratio.
/// Returns None if dimensions are invalid or image can't be read.
fn resolve_dimensions(image_path: &Path, width: Option<u32>, height: Option<u32>) -> Option<(u32, u32)> {
    match (width, height) {
        (Some(w), Some(h)) if w > 0 && h > 0 => Some((w, h)),
        (Some(w), None) if w > 0 => {
            // Read actual dimensions to compute proportional height
            let (orig_w, orig_h) = read_image_dimensions(image_path)?;
            if orig_w == 0 { return None; }
            let h = (w as f64 / orig_w as f64 * orig_h as f64).round() as u32;
            if h == 0 { return None; }
            Some((w, h))
        }
        (None, Some(h)) if h > 0 => {
            let (orig_w, orig_h) = read_image_dimensions(image_path)?;
            if orig_h == 0 { return None; }
            let w = (h as f64 / orig_h as f64 * orig_w as f64).round() as u32;
            if w == 0 { return None; }
            Some((w, h))
        }
        _ => None,
    }
}

/// Read image dimensions without loading full pixel data.
fn read_image_dimensions(path: &Path) -> Option<(u32, u32)> {
    image::image_dimensions(path).ok()
}

/// Compute a hash-based temp path for a resized image.
pub fn compute_temp_path(absolute_src: &Path, width: u32, height: u32) -> PathBuf {
    let canonical = absolute_src
        .canonicalize()
        .unwrap_or_else(|_| absolute_src.to_path_buf());

    let mut hasher = DefaultHasher::new();
    canonical.to_string_lossy().hash(&mut hasher);
    width.hash(&mut hasher);
    height.hash(&mut hasher);
    let hash = hasher.finish();

    let ext = absolute_src
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("png");

    temp_image_dir().join(format!("{:016x}.{}", hash, ext))
}

/// Check if a cached temp file exists and is newer than the source.
fn is_cached(task: &ImageResizeTask) -> bool {
    let temp_meta = match fs::metadata(&task.temp_path) {
        Ok(m) => m,
        Err(_) => return false,
    };
    let src_meta = match fs::metadata(&task.absolute_path) {
        Ok(m) => m,
        Err(_) => return false,
    };

    let temp_mtime = temp_meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
    let src_mtime = src_meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);

    temp_mtime >= src_mtime
}

/// Resize a single image and save to the temp path.
/// Silently skips on any failure (image shows at native size).
fn resize_image(task: &ImageResizeTask) {
    // Skip if already cached
    if is_cached(task) {
        return;
    }

    let img = match image::open(&task.absolute_path) {
        Ok(img) => img,
        Err(_) => return,
    };

    let resized = img.resize_exact(
        task.target_width,
        task.target_height,
        image::imageops::FilterType::Lanczos3,
    );

    let _ = resized.save(&task.temp_path);
}

/// Rewrite `<img src>` attributes in HTML to point to resized temp files.
/// Only rewrites images that have matching resize tasks with existing temp files.
fn rewrite_img_sources(html: &str, tasks: &[ImageResizeTask], md_dir: &Path) -> String {
    // Build lookup from original src -> temp absolute path
    let mut src_map: std::collections::HashMap<String, &PathBuf> = std::collections::HashMap::new();
    for task in tasks {
        if task.temp_path.exists() {
            src_map.insert(task.original_src.clone(), &task.temp_path);
        }
    }

    if src_map.is_empty() {
        return html.to_string();
    }

    let mut result = String::with_capacity(html.len());
    let mut rest = html;

    while let Some(img_start) = rest.find("<img ") {
        result.push_str(&rest[..img_start]);

        let tag_content = &rest[img_start..];
        let tag_end = tag_content.find('>').unwrap_or(tag_content.len() - 1) + 1;
        let tag = &tag_content[..tag_end];
        let self_closing = tag.ends_with("/>");

        let src = extract_attr(tag, "src");
        let alt = extract_attr(tag, "alt");

        result.push_str("<img");

        if let Some(ref s) = src {
            if let Some(temp_path) = src_map.get(s.as_str()) {
                // Rewrite to absolute temp path
                result.push_str(&format!(" src=\"{}\"", temp_path.to_string_lossy()));
            } else {
                // No resize task — resolve relative path to absolute for HelpView
                let abs = if Path::new(s.as_str()).is_absolute() {
                    s.to_string()
                } else {
                    md_dir.join(s.as_str()).to_string_lossy().to_string()
                };
                result.push_str(&format!(" src=\"{}\"", abs));
            }
        }
        if let Some(ref a) = alt {
            result.push_str(&format!(" alt=\"{}\"", a));
        }
        if self_closing {
            result.push_str("/>");
        } else {
            result.push('>');
        }

        rest = &rest[img_start + tag_end..];
    }

    result.push_str(rest);
    result
}

/// Extract an attribute value from an HTML tag string.
fn extract_attr(tag: &str, attr_name: &str) -> Option<String> {
    let lower = tag.to_lowercase();
    let pattern = format!("{}=\"", attr_name);
    let pos = lower.find(&pattern)?;
    let value_start = pos + pattern.len();
    let value_end = tag[value_start..].find('"')? + value_start;
    Some(tag[value_start..value_end].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_extract_resize_tasks_strips_attrs() {
        let input = r#"<img src="logo.png" alt="Logo" width="200" style="border: 1px;"/>"#;
        let (phase1, _tasks) = extract_resize_tasks(input, Path::new("/nonexistent"));
        assert_eq!(phase1, r#"<img src="logo.png" alt="Logo"/>"#);
    }

    #[test]
    fn test_extract_resize_tasks_preserves_other_html() {
        let input = r#"<h1>Title</h1><img src="a.png" width="100"><p>text</p>"#;
        let (phase1, _tasks) = extract_resize_tasks(input, Path::new("/nonexistent"));
        assert_eq!(phase1, r#"<h1>Title</h1><img src="a.png"><p>text</p>"#);
    }

    #[test]
    fn test_extract_resize_tasks_no_img() {
        let input = "<h1>No images</h1><p>text</p>";
        let (phase1, tasks) = extract_resize_tasks(input, Path::new("/nonexistent"));
        assert_eq!(phase1, input);
        assert!(tasks.is_empty());
    }

    #[test]
    fn test_extract_resize_tasks_skips_http() {
        let input = r#"<img src="https://example.com/img.png" width="200">"#;
        let (_, tasks) = extract_resize_tasks(input, Path::new("/tmp"));
        assert!(tasks.is_empty());
    }

    #[test]
    fn test_compute_temp_path_different_sizes() {
        let path = Path::new("/tmp/test.png");
        let p1 = compute_temp_path(path, 100, 100);
        let p2 = compute_temp_path(path, 200, 200);
        assert_ne!(p1, p2);
    }

    #[test]
    fn test_compute_temp_path_preserves_extension() {
        let path = Path::new("/tmp/test.jpg");
        let result = compute_temp_path(path, 100, 100);
        assert_eq!(result.extension().unwrap(), "jpg");
    }

    #[test]
    fn test_resolve_dimensions_both() {
        let result = resolve_dimensions(Path::new("/nonexistent"), Some(200), Some(100));
        assert_eq!(result, Some((200, 100)));
    }

    #[test]
    fn test_resolve_dimensions_zero() {
        let result = resolve_dimensions(Path::new("/nonexistent"), Some(0), Some(100));
        assert_eq!(result, None);
    }

    #[test]
    fn test_rewrite_img_sources() {
        let tasks = vec![ImageResizeTask {
            original_src: "logo.png".to_string(),
            absolute_path: PathBuf::from("/tmp/logo.png"),
            target_width: 200,
            target_height: 100,
            temp_path: PathBuf::from("/tmp/ferrispad-images/abc.png"),
        }];

        // Create the temp file so it "exists"
        let dir = Path::new("/tmp/ferrispad-images");
        let _ = fs::create_dir_all(dir);
        let _ = fs::write(&tasks[0].temp_path, b"fake");

        let input = r#"<img src="logo.png" alt="Logo">"#;
        let result = rewrite_img_sources(input, &tasks, Path::new("/tmp"));
        assert_eq!(
            result,
            r#"<img src="/tmp/ferrispad-images/abc.png" alt="Logo">"#
        );

        // Cleanup
        let _ = fs::remove_file(&tasks[0].temp_path);
    }

    #[test]
    fn test_wrap_html_for_helpview() {
        let html = "<p>Hello</p>";
        let result = wrap_html_for_helpview(html);
        assert!(result.starts_with("<font face=\"Helvetica\""));
        assert!(result.contains("<p>Hello</p>"));
    }
}
