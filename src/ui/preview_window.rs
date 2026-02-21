//! GTK-based preview window with WRY WebView for Markdown rendering.
//!
//! This module provides a separate GTK window containing a WebView for
//! rendering Markdown with full HTML5/CSS3 support. It works on both
//! X11 and Wayland display servers.

#[cfg(not(target_os = "windows"))]
use gtk::glib;
#[cfg(not(target_os = "windows"))]
use gtk::prelude::*;
#[cfg(not(target_os = "windows"))]
use wry::{WebView, WebViewBuilder, WebViewBuilderExtUnix};
#[cfg(not(target_os = "windows"))]
use std::fs;

/// A separate GTK window containing a WebView for Markdown preview.
#[cfg(not(target_os = "windows"))]
pub struct PreviewWindow {
    window: gtk::Window,
    webview: Option<WebView>,
    /// Path to temp file used for serving HTML content.
    temp_path: std::path::PathBuf,
    is_visible: bool,
}

#[cfg(not(target_os = "windows"))]
impl PreviewWindow {
    /// Create a new preview window. The window is hidden by default.
    pub fn new() -> Self {
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

        // Create temp file path for serving HTML content
        let temp_path = std::env::temp_dir().join("ferrispad_preview.html");

        Self {
            window,
            webview: None,
            temp_path,
            is_visible: false,
        }
    }

    /// Initialize the WebView inside the window.
    /// Must be called after gtk::init() and before show().
    pub fn init_webview(&mut self) -> Result<(), String> {
        if self.webview.is_some() {
            return Ok(());
        }

        // Create a Box container that expands to fill the window
        let vbox = gtk::Box::new(gtk::Orientation::Vertical, 0);
        vbox.set_hexpand(true);
        vbox.set_vexpand(true);
        self.window.add(&vbox);

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
        self.webview = Some(webview);
        Ok(())
    }

    /// Show the preview window.
    pub fn show(&mut self) {
        self.window.show_all();
        self.is_visible = true;
    }

    /// Hide the preview window.
    pub fn hide(&mut self) {
        self.window.hide();
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

    /// Set the HTML content to display.
    /// Writes HTML to temp file and navigates to it, allowing file:// URLs to resolve.
    pub fn set_html(&self, html: &str) {
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
        // Position preview window to the right of main window
        let preview_x = main_x + main_w + 10; // 10px gap
        self.window.move_(preview_x, main_y);
        self.window.resize(main_w / 2, main_h);
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

    pub fn init_webview(&mut self) -> Result<(), String> {
        // TODO: Implement Windows WebView2 support
        Ok(())
    }

    pub fn show(&mut self) {}
    pub fn hide(&mut self) {}
    pub fn is_visible(&self) -> bool { false }
    pub fn toggle(&mut self) {}
    pub fn set_html(&self, _html: &str) {}
    pub fn sync_position(&self, _x: i32, _y: i32, _w: i32, _h: i32) {}
}

#[cfg(target_os = "windows")]
impl Default for PreviewWindow {
    fn default() -> Self {
        Self::new()
    }
}
