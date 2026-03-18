//! Community/manual plugin install review dialog.
//!
//! Shows plugin details, permissions, scan warnings, and disclaimer
//! before allowing the user to proceed with installation.

use fltk::{
    button::Button,
    enums::{Align, Color, Font, FrameType},
    frame::Frame,
    group::{Pack, PackType, Scroll},
    prelude::*,
    window::Window,
};
use std::cell::RefCell;
use std::rc::Rc;

use super::DialogTheme;

const DIALOG_WIDTH: i32 = 460;
const PADDING: i32 = 15;

/// Data for the community install review dialog.
pub struct CommunityInstallReview {
    /// Plugin name as shown in the registry or archive.
    pub plugin_name: String,
    /// Semantic version string.
    pub version: String,
    /// Plugin author (empty string if unknown).
    pub author: String,
    /// URL the plugin was fetched from.
    pub source_url: String,
    /// Permission strings declared in `plugin.toml`.
    pub permissions: Vec<String>,
    /// `true` when installing from a local archive (not from the community index).
    pub is_manual: bool,
    /// `true` when the plugin registers an `on_text_changed` hook.
    pub has_text_change_hook: bool,
    /// Warnings produced by the pre-install source scan.
    pub scan_warnings: Vec<String>,
}

/// Show the community install review dialog.
///
/// Returns `true` if the user clicks "Install Anyway", `false` if cancelled.
pub fn show_community_install_dialog(review: &CommunityInstallReview, theme: &DialogTheme) -> bool {
    // ── Calculate dynamic height based on content ──
    let mut content_height = 0i32;

    // Header + separator + spacer
    content_height += 26 + 1 + 8;

    // Plugin info (name, author, source) + spacer
    content_height += 18 * 3 + 8;

    // Permissions section header + items
    content_height += 18; // "Permissions requested:" label
    if review.permissions.is_empty() {
        content_height += 16;
    } else {
        content_height += 16 * review.permissions.len() as i32;
    }

    // Text change hook warning
    if review.has_text_change_hook {
        content_height += 4 + 32;
    }

    // Scan warnings
    if !review.scan_warnings.is_empty() {
        content_height += 4 + 18; // spacer + header
        content_height += 16 * review.scan_warnings.len() as i32;
    }

    // Separator + spacer + disclaimer
    content_height += 8 + 1 + 8 + 50;

    // Manual extra warning
    if review.is_manual {
        content_height += 18;
    }

    // Pack spacing (approximate)
    content_height += 40;

    let dialog_height = (content_height + PADDING * 2 + 50).clamp(300, 550);

    let result = Rc::new(RefCell::new(false));

    let mut dialog = Window::default()
        .with_size(DIALOG_WIDTH, dialog_height)
        .with_label("Install Plugin")
        .center_screen();
    dialog.make_modal(true);
    dialog.set_color(theme.bg);

    let mut scroll = Scroll::default()
        .with_pos(0, 0)
        .with_size(DIALOG_WIDTH, dialog_height - 50);
    scroll.set_color(theme.bg);
    theme.style_scroll(&mut scroll);

    let mut pack = Pack::default()
        .with_pos(PADDING, PADDING)
        .with_size(DIALOG_WIDTH - PADDING * 2, 0);
    pack.set_type(PackType::Vertical);
    pack.set_spacing(4);

    let content_width = DIALOG_WIDTH - PADDING * 2;

    // ── Header ──
    let header_text = if review.is_manual {
        "Install Unverified Plugin"
    } else {
        "Install Community Plugin"
    };
    let warning_color = if theme.is_dark() {
        Color::from_rgb(220, 180, 80)
    } else {
        Color::from_rgb(180, 120, 20)
    };
    let mut header = Frame::default()
        .with_size(content_width, 26)
        .with_label(&format!("\u{26A0}  {}", header_text));
    header.set_label_color(warning_color);
    header.set_label_font(Font::HelveticaBold);
    header.set_label_size(14);
    header.set_align(Align::Left | Align::Inside);

    // Thin separator
    let mut sep1 = Frame::default().with_size(content_width, 1);
    sep1.set_frame(FrameType::FlatBox);
    sep1.set_color(theme.text_dim);

    // Spacer
    Frame::default().with_size(content_width, 8);

    // ── Plugin info ──
    let mut name_label = Frame::default()
        .with_size(content_width, 18)
        .with_label(&format!(
            "Plugin:  {} v{}",
            review.plugin_name, review.version
        ));
    name_label.set_label_color(theme.text);
    name_label.set_align(Align::Left | Align::Inside);
    name_label.set_label_size(12);

    let author_display = if review.author.is_empty() {
        "Unknown"
    } else {
        &review.author
    };
    let mut author_label = Frame::default()
        .with_size(content_width, 18)
        .with_label(&format!("Author:  {}", author_display));
    author_label.set_label_color(theme.text);
    author_label.set_align(Align::Left | Align::Inside);
    author_label.set_label_size(12);

    let source_display = review
        .source_url
        .strip_prefix("https://")
        .unwrap_or(&review.source_url);
    let mut source_label = Frame::default()
        .with_size(content_width, 18)
        .with_label(&format!("Source:  {}", source_display));
    source_label.set_label_color(theme.text);
    source_label.set_align(Align::Left | Align::Inside);
    source_label.set_label_size(12);

    // Spacer
    Frame::default().with_size(content_width, 8);

    // ── Permissions ──
    let mut perms_header = Frame::default()
        .with_size(content_width, 18)
        .with_label("Permissions requested:");
    perms_header.set_label_color(theme.text);
    perms_header.set_align(Align::Left | Align::Inside);
    perms_header.set_label_font(Font::HelveticaBold);
    perms_header.set_label_size(12);

    if review.permissions.is_empty() {
        let mut no_perms = Frame::default()
            .with_size(content_width, 16)
            .with_label("  No special permissions requested (sandbox only)");
        no_perms.set_label_color(theme.text_dim);
        no_perms.set_align(Align::Left | Align::Inside);
        no_perms.set_label_size(11);
    } else {
        for perm in &review.permissions {
            let mut perm_label = Frame::default()
                .with_size(content_width, 16)
                .with_label(&format!("  \u{2022} {}", perm));
            perm_label.set_label_color(warning_color);
            perm_label.set_align(Align::Left | Align::Inside);
            perm_label.set_label_size(11);
        }
    }

    // ── Text change hook warning ──
    if review.has_text_change_hook {
        Frame::default().with_size(content_width, 4);
        let mut hook_warn = Frame::default()
            .with_size(content_width, 32)
            .with_label(concat!(
                "\u{26A0} This plugin monitors every keystroke (on_text_changed).\n",
                "This may affect editor performance and gives the plugin access to all typed content."
            ));
        hook_warn.set_label_color(warning_color);
        hook_warn.set_align(Align::Left | Align::Inside | Align::Wrap);
        hook_warn.set_label_size(11);
    }

    // ── Scan warnings ──
    if !review.scan_warnings.is_empty() {
        Frame::default().with_size(content_width, 4);
        let mut scan_header = Frame::default()
            .with_size(content_width, 18)
            .with_label("Scan warnings:");
        scan_header.set_label_color(warning_color);
        scan_header.set_align(Align::Left | Align::Inside);
        scan_header.set_label_font(Font::HelveticaBold);
        scan_header.set_label_size(12);

        for warning in &review.scan_warnings {
            let mut warn_label = Frame::default()
                .with_size(content_width, 16)
                .with_label(&format!("  \u{2022} {}", warning));
            warn_label.set_label_color(warning_color);
            warn_label.set_align(Align::Left | Align::Inside);
            warn_label.set_label_size(11);
        }
    }

    // ── Separator ──
    Frame::default().with_size(content_width, 8);
    let mut sep2 = Frame::default().with_size(content_width, 1);
    sep2.set_frame(FrameType::FlatBox);
    sep2.set_color(theme.text_dim);
    Frame::default().with_size(content_width, 8);

    // ── Disclaimer ──
    let mut disclaimer = Frame::default()
        .with_size(content_width, 50)
        .with_label(concat!(
            "This plugin is NOT verified by FerrisPad. It has not been\n",
            "reviewed for safety or correctness. You are responsible for\n",
            "any effects of installing and running this plugin."
        ));
    disclaimer.set_label_color(theme.text_dim);
    disclaimer.set_align(Align::Left | Align::Inside | Align::Wrap);
    disclaimer.set_label_size(11);

    if review.is_manual {
        let mut manual_warn = Frame::default().with_size(content_width, 18).with_label(
            "This plugin is not listed in the community index. No integrity checks are available.",
        );
        manual_warn.set_label_color(warning_color);
        manual_warn.set_align(Align::Left | Align::Inside | Align::Wrap);
        manual_warn.set_label_size(11);
    }

    pack.end();
    scroll.end();

    // ── Buttons ──
    let btn_y = dialog_height - 42;

    let mut cancel_btn = Button::default()
        .with_pos(PADDING, btn_y)
        .with_size(100, 30)
        .with_label("Cancel");
    cancel_btn.set_frame(FrameType::RFlatBox);
    cancel_btn.set_color(theme.button_bg);
    cancel_btn.set_label_color(theme.text);

    let mut install_btn = Button::default()
        .with_pos(DIALOG_WIDTH - PADDING - 120, btn_y)
        .with_size(120, 30)
        .with_label("Install Anyway");
    install_btn.set_frame(FrameType::RFlatBox);
    install_btn.set_color(theme.button_bg);
    install_btn.set_label_color(warning_color);

    dialog.end();

    // ── Callbacks ──
    let mut dialog_cancel = dialog.clone();
    cancel_btn.set_callback(move |_| {
        dialog_cancel.hide();
    });

    let result_install = Rc::clone(&result);
    let mut dialog_install = dialog.clone();
    install_btn.set_callback(move |_| {
        *result_install.borrow_mut() = true;
        dialog_install.hide();
    });

    dialog.set_callback(move |w| {
        w.hide();
    });

    dialog.show();
    super::run_dialog(&dialog);

    *result.borrow()
}
