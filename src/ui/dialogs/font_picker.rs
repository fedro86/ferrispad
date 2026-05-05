//! Font picker dialog.
//!
//! Lets the user pick any system font and a size, with a live preview rendered
//! in the chosen font/size. The font catalog is loaded lazily on first open.

use std::cell::RefCell;
use std::rc::Rc;

use fltk::{
    browser::HoldBrowser,
    button::{Button, CheckButton},
    enums::{CallbackTrigger, FrameType},
    frame::Frame,
    input::{Input, IntInput},
    prelude::*,
    text::{TextBuffer, TextDisplay},
    window::Window,
};

use crate::app::domain::settings;
use crate::app::services::font_catalog::{self, FontEntry};

use super::DialogTheme;

const PREVIEW_SAMPLE: &str =
    "fn main() {\n    let x = 42;\n    // 0Oo 1lI iIl\n    println!(\"Hello, {}!\", x);\n}";

const DW: i32 = 640;
const DH: i32 = 460;

/// Show the font picker dialog. Returns `Some((name, size))` if the user
/// confirmed a selection, `None` otherwise.
pub fn show_font_picker(
    parent: &Window,
    current_name: &str,
    current_size: u32,
    theme_bg: (u8, u8, u8),
) -> Option<(String, u32)> {
    let theme = DialogTheme::from_theme_bg(theme_bg);

    let entries = font_catalog::list();
    if entries.is_empty() {
        return None;
    }

    let result: Rc<RefCell<Option<(String, u32)>>> = Rc::new(RefCell::new(None));
    let entries = Rc::new(entries);

    let mut dialog_win = Window::default()
        .with_size(DW, DH)
        .with_label("Choose Font")
        .center_screen();
    dialog_win.set_color(theme.bg);

    // Top row: search input + monospace toggle
    let mut search_label = Frame::default()
        .with_pos(20, 15)
        .with_size(60, 25)
        .with_label("Search:")
        .with_align(fltk::enums::Align::Left | fltk::enums::Align::Inside);
    search_label.set_label_color(theme.text);

    let mut search_input = Input::default().with_pos(85, 15).with_size(220, 25);
    search_input.set_frame(FrameType::FlatBox);
    search_input.set_color(theme.input_bg);
    search_input.set_text_color(theme.text);
    search_input.set_selection_color(theme.button_bg);
    search_input.set_trigger(CallbackTrigger::Changed);

    let mut monospace_toggle = CheckButton::default()
        .with_pos(320, 15)
        .with_size(180, 25)
        .with_label("Monospace only");
    monospace_toggle.set_label_color(theme.text);
    monospace_toggle.set_color(theme.bg);
    monospace_toggle.set_value(true);

    // Left column: font list
    let list_x = 20;
    let list_y = 55;
    let list_w = 280;
    let list_h = 320;
    let mut browser = HoldBrowser::default()
        .with_pos(list_x, list_y)
        .with_size(list_w, list_h);
    browser.set_color(theme.input_bg);
    browser.set_selection_color(theme.button_bg);
    browser.set_frame(FrameType::FlatBox);

    // Right column: size + preview
    let right_x = list_x + list_w + 20;
    let right_w = DW - right_x - 20;

    let mut size_label = Frame::default()
        .with_pos(right_x, list_y)
        .with_size(60, 25)
        .with_label("Size:")
        .with_align(fltk::enums::Align::Left | fltk::enums::Align::Inside);
    size_label.set_label_color(theme.text);

    let mut size_input = IntInput::default()
        .with_pos(right_x + 60, list_y)
        .with_size(70, 25);
    size_input.set_frame(FrameType::FlatBox);
    size_input.set_color(theme.input_bg);
    size_input.set_text_color(theme.text);
    size_input.set_selection_color(theme.button_bg);
    size_input.set_value(&current_size.to_string());

    let mut size_hint = Frame::default()
        .with_pos(right_x + 135, list_y)
        .with_size(150, 25)
        .with_label("(6–96)")
        .with_align(fltk::enums::Align::Left | fltk::enums::Align::Inside);
    size_hint.set_label_color(theme.text_dim);

    let mut preview_label = Frame::default()
        .with_pos(right_x, list_y + 35)
        .with_size(right_w, 20)
        .with_label("Preview:")
        .with_align(fltk::enums::Align::Left | fltk::enums::Align::Inside);
    preview_label.set_label_color(theme.text);

    let preview_y = list_y + 60;
    let preview_h = list_y + list_h - preview_y;
    let mut preview_buffer = TextBuffer::default();
    preview_buffer.set_text(PREVIEW_SAMPLE);
    let mut preview = TextDisplay::default()
        .with_pos(right_x, preview_y)
        .with_size(right_w, preview_h);
    preview.set_buffer(preview_buffer);
    preview.set_color(theme.input_bg);
    preview.set_text_color(theme.text);
    preview.set_frame(FrameType::FlatBox);
    preview.set_text_font(settings::resolve_font(current_name));
    preview.set_text_size(current_size as i32);

    // Bottom button row
    let btn_y = DH - 45;
    let btn_w = 90;
    let btn_h = 30;

    let mut ok_btn = Button::default()
        .with_pos(DW - 2 * btn_w - 30, btn_y)
        .with_size(btn_w, btn_h)
        .with_label("OK");
    ok_btn.set_color(theme.button_bg);
    ok_btn.set_label_color(theme.text);

    let mut cancel_btn = Button::default()
        .with_pos(DW - btn_w - 20, btn_y)
        .with_size(btn_w, btn_h)
        .with_label("Cancel");
    cancel_btn.set_color(theme.button_bg);
    cancel_btn.set_label_color(theme.text);

    dialog_win.end();
    dialog_win.make_resizable(false);
    // Modal so FLTK manages the focus grab/release relative to the parent
    // (the settings dialog, itself modal). Without this, on Wayland the parent
    // stays visually darkened after this dialog hides because the grab is
    // never returned to it.
    dialog_win.make_modal(true);
    dialog_win.show();
    theme.apply_titlebar(&dialog_win);
    dialog_win.resize(
        parent.x() + (parent.w() - DW) / 2,
        parent.y() + (parent.h() - DH) / 2,
        DW,
        DH,
    );

    // Filtered entries cache: maps the displayed-row index (0-based) to the
    // original entries[] index. Updated by repopulate().
    let visible_indices: Rc<RefCell<Vec<usize>>> = Rc::new(RefCell::new(Vec::new()));
    let current_name_owned = current_name.to_string();

    // Initial population
    repopulate(
        &mut browser.clone(),
        &visible_indices,
        &entries,
        &search_input,
        &monospace_toggle,
        &current_name_owned,
    );

    // Hook up search + checkbox to refilter
    {
        let entries = entries.clone();
        let visible_indices = visible_indices.clone();
        let browser = browser.clone();
        let search_input_inner = search_input.clone();
        let monospace_toggle_inner = monospace_toggle.clone();
        let preview = preview.clone();
        let size_input_inner = size_input.clone();
        let current_name_owned = current_name_owned.clone();
        search_input.clone().set_callback(move |_| {
            repopulate(
                &mut browser.clone(),
                &visible_indices,
                &entries,
                &search_input_inner,
                &monospace_toggle_inner,
                &current_name_owned,
            );
            update_preview(
                &mut preview.clone(),
                &browser,
                &visible_indices,
                &entries,
                &size_input_inner,
            );
        });
    }
    {
        let entries = entries.clone();
        let visible_indices = visible_indices.clone();
        let browser = browser.clone();
        let search_input_inner = search_input.clone();
        let monospace_toggle_inner = monospace_toggle.clone();
        let preview = preview.clone();
        let size_input_inner = size_input.clone();
        let current_name_owned = current_name_owned.clone();
        monospace_toggle.clone().set_callback(move |_| {
            repopulate(
                &mut browser.clone(),
                &visible_indices,
                &entries,
                &search_input_inner,
                &monospace_toggle_inner,
                &current_name_owned,
            );
            update_preview(
                &mut preview.clone(),
                &browser,
                &visible_indices,
                &entries,
                &size_input_inner,
            );
        });
    }

    // Browser selection change → update preview
    {
        let entries = entries.clone();
        let visible_indices = visible_indices.clone();
        let preview = preview.clone();
        let size_input_inner = size_input.clone();
        browser.set_callback(move |b| {
            update_preview(
                &mut preview.clone(),
                b,
                &visible_indices,
                &entries,
                &size_input_inner,
            );
        });
    }

    // Size change → update preview
    {
        let entries = entries.clone();
        let visible_indices = visible_indices.clone();
        let preview = preview.clone();
        let browser = browser.clone();
        size_input.set_trigger(CallbackTrigger::Changed);
        size_input.clone().set_callback(move |s| {
            if let Ok(v) = s.value().parse::<i32>() {
                let clamped = v.clamp(6, 96);
                if clamped != v {
                    s.set_value(&clamped.to_string());
                }
            }
            update_preview(
                &mut preview.clone(),
                &browser,
                &visible_indices,
                &entries,
                s,
            );
        });
    }

    // OK collects the result
    {
        let result = result.clone();
        let mut dialog = dialog_win.clone();
        let browser = browser.clone();
        let visible_indices = visible_indices.clone();
        let entries = entries.clone();
        let size_input = size_input.clone();
        ok_btn.set_callback(move |_| {
            let row = browser.value();
            if row < 1 {
                return;
            }
            let indices = visible_indices.borrow();
            let Some(&entry_idx) = indices.get((row - 1) as usize) else {
                return;
            };
            let entry: &FontEntry = &entries[entry_idx];
            let size = size_input.value().parse::<i32>().unwrap_or(16).clamp(6, 96) as u32;
            // Pick the canonical name to persist:
            //   - Built-in monospace (Courier) keeps its bare name
            //   - Custom fonts use their fltk_name (preserves leading-space marker
            //     for proportional fonts; FLTK needs that to look them up)
            let name = if settings::is_legacy_font_name(&entry.display_name) {
                entry.display_name.clone()
            } else {
                entry.fltk_name.clone()
            };
            *result.borrow_mut() = Some((name, size));
            dialog.hide();
        });
    }

    // Cancel + X close without setting the result
    {
        let mut dialog = dialog_win.clone();
        cancel_btn.set_callback(move |_| dialog.hide());
    }
    {
        let mut dialog = dialog_win.clone();
        dialog_win.set_callback(move |_| dialog.hide());
    }

    super::run_dialog(&dialog_win);

    let mut out = result.borrow_mut();
    out.take()
}

/// Refilter the browser based on the search query and monospace toggle.
fn repopulate(
    browser: &mut HoldBrowser,
    visible_indices: &Rc<RefCell<Vec<usize>>>,
    entries: &[FontEntry],
    search_input: &Input,
    monospace_toggle: &CheckButton,
    current_name: &str,
) {
    let query = search_input.value().to_lowercase();
    let monospace_only = monospace_toggle.value();
    browser.clear();
    let mut indices = visible_indices.borrow_mut();
    indices.clear();
    let mut select_row: Option<i32> = None;
    for (i, entry) in entries.iter().enumerate() {
        if monospace_only && !entry.is_monospace {
            continue;
        }
        if !query.is_empty()
            && !entry.display_name.to_lowercase().contains(&query)
            && !entry.fltk_name.to_lowercase().contains(&query)
        {
            continue;
        }
        indices.push(i);
        browser.add(&entry.display_name);
        if entry.fltk_name == current_name || entry.display_name == current_name {
            select_row = Some(indices.len() as i32);
        }
    }
    if let Some(row) = select_row {
        browser.select(row);
    } else if !indices.is_empty() {
        browser.select(1);
    }
}

/// Re-apply the currently-selected font and parsed size to the preview display.
fn update_preview(
    preview: &mut TextDisplay,
    browser: &HoldBrowser,
    visible_indices: &Rc<RefCell<Vec<usize>>>,
    entries: &[FontEntry],
    size_input: &IntInput,
) {
    let row = browser.value();
    if row < 1 {
        return;
    }
    let indices = visible_indices.borrow();
    let Some(&entry_idx) = indices.get((row - 1) as usize) else {
        return;
    };
    let entry = &entries[entry_idx];
    let font = settings::resolve_font(&entry.fltk_name);
    let size = size_input.value().parse::<i32>().unwrap_or(16).clamp(6, 96);
    preview.set_text_font(font);
    preview.set_text_size(size);
    preview.redraw();
}
