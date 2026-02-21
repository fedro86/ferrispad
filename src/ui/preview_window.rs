//! GTK-based preview window with WRY WebView for Markdown rendering.
//!
//! This module provides a separate GTK window containing a WebView for
//! rendering Markdown with full HTML5/CSS3 support. It works on both
//! X11 and Wayland display servers.
//!
//! The WebView is lazily initialized on first use to avoid ~100MB memory
//! overhead when the preview feature is not used.

#[cfg(not(target_os = "windows"))]
use gtk::glib;
#[cfg(not(target_os = "windows"))]
use gtk::prelude::*;
#[cfg(not(target_os = "windows"))]
use wry::{WebView, WebViewBuilder, WebViewBuilderExtUnix};
#[cfg(not(target_os = "windows"))]
use std::fs;

/// A separate GTK window containing a WebView for Markdown preview.
/// The window and WebView are lazily initialized on first use.
#[cfg(not(target_os = "windows"))]
pub struct PreviewWindow {
    /// GTK window - created lazily on first show()
    window: Option<gtk::Window>,
    /// WRY WebView - created lazily with the window
    webview: Option<WebView>,
    /// Path to temp file used for serving HTML content.
    temp_path: std::path::PathBuf,
    is_visible: bool,
}

#[cfg(not(target_os = "windows"))]
impl PreviewWindow {
    /// Create a new preview window handle. The actual GTK window and WebView
    /// are not created until first use (lazy initialization).
    pub fn new() -> Self {
        let temp_path = std::env::temp_dir().join("ferrispad_preview.html");

        Self {
            window: None,
            webview: None,
            temp_path,
            is_visible: false,
        }
    }

    /// Initialize the GTK window and WebView if not already done.
    /// Called automatically by show() and set_html().
    fn ensure_initialized(&mut self) -> Result<(), String> {
        if self.webview.is_some() {
            return Ok(());
        }

        // Initialize GTK on first use (lazy to avoid ~35MB overhead if preview unused)
        if gtk::init().is_err() {
            return Err("Failed to initialize GTK".to_string());
        }

        // Create the GTK window
        let window = gtk::Window::new(gtk::WindowType::Toplevel);
        window.set_title("FerrisPad Preview");
        window.set_default_size(600, 800);
        window.set_decorated(true);
        window.set_deletable(true);

        // When user closes preview window, just hide it (don't destroy)
        window.connect_delete_event(|win, _| {
            win.hide();
            glib::Propagation::Stop
        });

        // Create a Box container that expands to fill the window
        let vbox = gtk::Box::new(gtk::Orientation::Vertical, 0);
        vbox.set_hexpand(true);
        vbox.set_vexpand(true);
        window.add(&vbox);

        // Write default HTML to temp file first
        let _ = fs::write(&self.temp_path, Self::default_html());
        let initial_url = format!("file://{}", self.temp_path.display());

        // Build WebView and navigate to temp file URL.
        // Using file:// URLs allows relative paths in the HTML to resolve correctly.
        let webview = WebViewBuilder::new()
            .with_transparent(false)
            .with_url(&initial_url)
            .build_gtk(&vbox)
            .map_err(|e| format!("Failed to create WebView: {}", e))?;

        // The WebView's underlying widget needs to expand
        if let Some(widget) = vbox.children().first() {
            widget.set_hexpand(true);
            widget.set_vexpand(true);
        }

        vbox.show_all();
        self.window = Some(window);
        self.webview = Some(webview);
        Ok(())
    }

    /// Show the preview window. Initializes GTK/WebView on first call.
    pub fn show(&mut self) {
        if let Err(e) = self.ensure_initialized() {
            eprintln!("Warning: Failed to initialize preview WebView: {}", e);
            return;
        }
        if let Some(ref window) = self.window {
            window.show_all();
        }
        self.is_visible = true;
    }

    /// Hide the preview window.
    pub fn hide(&mut self) {
        if let Some(ref window) = self.window {
            window.hide();
        }
        self.is_visible = false;
    }

    /// Check if the window is currently visible.
    #[allow(dead_code)]
    pub fn is_visible(&self) -> bool {
        self.is_visible
    }

    /// Toggle visibility.
    #[allow(dead_code)]
    pub fn toggle(&mut self) {
        if self.is_visible {
            self.hide();
        } else {
            self.show();
        }
    }

    /// Set the HTML content to display. Initializes WebView on first call.
    /// Writes HTML to temp file and navigates to it, allowing file:// URLs to resolve.
    pub fn set_html(&mut self, html: &str) {
        if let Err(e) = self.ensure_initialized() {
            eprintln!("Warning: Failed to initialize preview WebView: {}", e);
            return;
        }

        // Write HTML to temp file
        if fs::write(&self.temp_path, html).is_err() {
            return;
        }

        // Navigate to the temp file URL
        if let Some(ref wv) = self.webview {
            let url = format!("file://{}", self.temp_path.display());
            let _ = wv.load_url(&url);
        }
    }

    /// Update the window position to be next to the main FLTK window.
    /// Places the preview window to the right of the main window.
    pub fn sync_position(&self, main_x: i32, main_y: i32, main_w: i32, main_h: i32) {
        if let Some(ref window) = self.window {
            // Position preview window to the right of main window
            let preview_x = main_x + main_w + 10; // 10px gap
            window.move_(preview_x, main_y);
            window.resize(main_w / 2, main_h);
        }
    }

    /// Default HTML shown when preview is empty.
    fn default_html() -> &'static str {
        r#"<!DOCTYPE html>
<html>
<head>
    <meta charset="UTF-8">
    <style>
        body {
            font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
            display: flex;
            justify-content: center;
            align-items: center;
            height: 100vh;
            margin: 0;
            background: #f5f5f5;
            color: #666;
        }
    </style>
</head>
<body>
    <p>Open a Markdown file to see preview</p>
</body>
</html>"#
    }
}

#[cfg(not(target_os = "windows"))]
impl Default for PreviewWindow {
    fn default() -> Self {
        Self::new()
    }
}

// Windows stub - WRY on Windows doesn't need GTK
#[cfg(target_os = "windows")]
pub struct PreviewWindow;

#[cfg(target_os = "windows")]
impl PreviewWindow {
    pub fn new() -> Self {
        Self
    }

    pub fn show(&mut self) {}
    pub fn hide(&mut self) {}
    pub fn is_visible(&self) -> bool { false }
    pub fn toggle(&mut self) {}
    pub fn set_html(&mut self, _html: &str) {}
    pub fn sync_position(&self, _x: i32, _y: i32, _w: i32, _h: i32) {}
}

#[cfg(target_os = "windows")]
impl Default for PreviewWindow {
    fn default() -> Self {
        Self::new()
    }
}
