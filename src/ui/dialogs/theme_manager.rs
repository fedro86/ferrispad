//! "Manage themes…" window (T0080): list the built-in (read-only) and custom
//! editor themes; Duplicate, Rename, Delete, Import .tmTheme; edit the
//! editor colors of a custom theme with live preview.
//!
//! Built like the Plugin Manager: [`DialogTheme`] for every color, the themed
//! title bar, `run_dialog_with`, and the shared themed prompts. The logic is
//! [`ThemeManagerModel`]; this file only turns clicks into model calls and
//! hands the returned actions to the caller.

use crate::ui::color::fltk_rgb;
use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

use fltk::{
    app,
    browser::HoldBrowser,
    button::Button,
    enums::{Align, Color, FrameType},
    frame::Frame,
    prelude::*,
    window::Window,
};

use super::color_picker::show_color_picker;
use super::{
    DialogTheme, SCROLLBAR_SIZE, label_width, max_label_width, show_themed_choice,
    show_themed_input, show_themed_message,
};
use crate::app::controllers::theme_manager::{
    ThemeEntry, ThemeManagerAction, ThemeManagerModel, format_hex_color,
};
use crate::app::domain::settings::ThemeRef;
use crate::app::services::custom_themes::EditorColorField;
use crate::ui::file_dialogs::native_open_dialog;
use crate::ui::scrollbar::style_scrollbars;

// Heights and gaps are fixed; every width is measured from the text it
// shows (`label_width`), so the layout follows the installed fonts.
const DIALOG_HEIGHT: i32 = 480;
const PADDING: i32 = 15;
const GAP: i32 = 8;
const MIN_LIST_WIDTH: i32 = 250;
const MIN_BUTTON_WIDTH: i32 = 85;
const BUTTON_HEIGHT: i32 = 28;
const ROW_HEIGHT: i32 = 34;
const SWATCH_W: i32 = 36;
const HEX_SIZE: i32 = 12;
const HINT_SIZE: i32 = 11;
/// Hex colors made of the widest hex digits, for measuring.
const WIDE_HEX: [&str; 3] = ["#888888", "#dddddd", "#bbbbbb"];

const HINT: &str = "Color edits are saved on OK and discarded on Cancel.\n\
                    Duplicate, Rename, Delete and Import are saved at once.";

/// Display label of each editable color.
fn field_label(field: EditorColorField) -> &'static str {
    match field {
        EditorColorField::Background => "Background",
        EditorColorField::Text => "Text",
        EditorColorField::Caret => "Caret",
        EditorColorField::Selection => "Selection",
        EditorColorField::LineNumberFg => "Line numbers",
        EditorColorField::LineNumberBg => "Line number bg",
        EditorColorField::CurrentLine => "Current line",
    }
}

/// Tooltip of the color swatches (also how the UI tests find them).
pub const SWATCH_TOOLTIP: &str = "Current color";

/// A color swatch with a 1 px border in the dialog's dimmed text color, so
/// a color close to the dialog background (or equal to it) stays visible.
/// FLTK's `BorderBox` alone draws a black border, invisible on dark themes.
fn style_swatch(swatch: &mut Frame, theme: &DialogTheme) {
    swatch.set_frame(FrameType::BorderBox);
    swatch.set_tooltip(SWATCH_TOOLTIP);
    let border = theme.text_dim;
    swatch.draw(move |f| {
        fltk::draw::draw_rect_fill(f.x(), f.y(), f.w(), f.h(), f.color());
        fltk::draw::draw_rect_with_color(f.x(), f.y(), f.w(), f.h(), border);
    });
}

/// `name` as a browser line shown literally. Theme names are free text, and
/// FLTK's browsers read a leading `@` as a formatting code and a tab as a
/// column break. `@.` is FLTK's documented "print the rest of the line, look
/// for no more `@` codes"; tabs become spaces. (Turning formatting off with
/// a NUL format character trips a fltk-rs debug assertion.) Rows are always
/// mapped to themes by index, never by their text.
pub(crate) fn literal_browser_label(name: &str) -> String {
    format!("@.{}", name.replace('\t', " "))
}

/// One color row: swatch, hex text, Pick… and Reset buttons.
#[derive(Clone)]
struct ColorRow {
    field: EditorColorField,
    swatch: Frame,
    hex: Frame,
    pick: Button,
    reset: Button,
}

/// Widgets and shared state the callbacks need. Everything is a cheap
/// handle (`Rc` or an FLTK widget pointer), so it is cloned into callbacks.
#[derive(Clone)]
struct Ctx {
    theme_bg: (u8, u8, u8),
    model: Rc<RefCell<ThemeManagerModel>>,
    /// Actions for the caller, handed over after each event.
    pending: Rc<RefCell<Vec<ThemeManagerAction>>>,
    selected: Rc<RefCell<Option<ThemeRef>>>,
    browser: HoldBrowser,
    rename: Button,
    delete: Button,
    title: Frame,
    note: Frame,
    error: Frame,
    rows: Rc<Vec<ColorRow>>,
}

impl Ctx {
    /// The selected entry (the first one when nothing valid is selected).
    fn selected_entry(&self) -> Option<ThemeEntry> {
        let entries = self.model.borrow().entries();
        let selected = self.selected.borrow().clone();
        entries
            .iter()
            .find(|e| Some(&e.theme) == selected.as_ref())
            .or_else(|| entries.first())
            .cloned()
    }

    /// Apply a model result: show errors inline, follow `Select`, queue the
    /// rest for the caller, and redraw the window.
    fn handle(&self, actions: Vec<ThemeManagerAction>) {
        let mut error = String::new();
        for action in &actions {
            match action {
                ThemeManagerAction::Error(message) => error.clone_from(message),
                ThemeManagerAction::Select(theme) => {
                    *self.selected.borrow_mut() = Some(theme.clone());
                }
                _ => {}
            }
        }
        // Messages may quote theme names: no `@` label symbols.
        self.error.clone().set_label(&error.replace('@', "@@"));
        self.pending
            .borrow_mut()
            .extend(actions.into_iter().filter(|a| {
                !matches!(
                    a,
                    ThemeManagerAction::Error(_) | ThemeManagerAction::Select(_)
                )
            }));
        self.refresh_list();
    }

    /// Rebuild the list, keep the selection, then refresh the details.
    fn refresh_list(&self) {
        let entries = self.model.borrow().entries();
        let selected = self.selected_entry().map(|e| e.theme);
        let mut browser = self.browser.clone();
        browser.clear();
        for entry in &entries {
            let label = if entry.read_only {
                format!("{}  (built-in)", entry.name)
            } else {
                entry.name.clone()
            };
            browser.add(&literal_browser_label(&label));
        }
        if let Some(line) = entries
            .iter()
            .position(|e| Some(&e.theme) == selected.as_ref())
        {
            browser.select(line as i32 + 1);
        }
        *self.selected.borrow_mut() = selected;
        self.refresh_details();
    }

    /// Show the selected theme's colors and enable what it allows.
    fn refresh_details(&self) {
        let Some(entry) = self.selected_entry() else {
            return;
        };
        let colors = self.model.borrow().colors(&entry.theme);
        let (mut rename, mut delete) = (self.rename.clone(), self.delete.clone());
        let (mut title, mut note) = (self.title.clone(), self.note.clone());
        title.set_label(&format!("Editor colors: {}", entry.name.replace('@', "@@")));
        if entry.read_only {
            rename.deactivate();
            delete.deactivate();
            note.set_label("Built-in themes are read-only. Duplicate one to edit its colors.");
        } else {
            rename.activate();
            delete.activate();
            note.set_label(if entry.can_reset {
                ""
            } else {
                "Imported theme: no base theme to reset to."
            });
        }
        for row in self.rows.iter() {
            let mut row = row.clone();
            if let Some(colors) = colors {
                let rgb = row.field.of(&colors);
                row.swatch.set_color(fltk_rgb(rgb.0, rgb.1, rgb.2));
                row.hex.set_label(&format_hex_color(rgb));
            }
            if entry.read_only {
                row.pick.deactivate();
            } else {
                row.pick.activate();
            }
            if entry.can_reset {
                row.reset.activate();
            } else {
                row.reset.deactivate();
            }
            row.swatch.redraw();
        }
        if let Some(mut window) = self.browser.window() {
            window.redraw();
        }
    }
}

fn button(x: i32, y: i32, w: i32, label: &str, theme: &DialogTheme) -> Button {
    theme
        .button()
        .with_pos(x, y)
        .with_size(w, BUTTON_HEIGHT)
        .with_label(label)
}

/// Show the theme manager over `model`. `apply` receives the model and the
/// actions to carry out (re-theme, settings, theme list) after each event,
/// while the window is still open, like the Settings dialog's preview.
pub fn show_theme_manager(
    theme_bg: (u8, u8, u8),
    model: ThemeManagerModel,
    mut apply: impl FnMut(&ThemeManagerModel, Vec<ThemeManagerAction>),
) {
    let theme = DialogTheme::from_theme_bg(theme_bg);
    // A live preview re-themes the main window and sets FLTK's global
    // foreground/background. This window keeps its own colors while open
    // (list text and popups use the foreground), and the dialog behind it
    // gets its globals back when it closes.
    let behind_fg = Color::Foreground.to_rgb();
    let behind_bg = Color::Background.to_rgb();
    let own_fg = theme.text_rgb();
    app::foreground(own_fg.0, own_fg.1, own_fg.2);

    let (mut dialog, ctx) = build_window(theme_bg, model);
    dialog.show();
    theme.apply_titlebar(&dialog);

    let (model, pending) = (ctx.model.clone(), ctx.pending.clone());
    let mut dialog_redraw = dialog.clone();
    super::run_dialog_with(&dialog, || {
        let actions: Vec<ThemeManagerAction> = pending.borrow_mut().drain(..).collect();
        if actions.is_empty() {
            return;
        }
        apply(&model.borrow(), actions);
        // Keep this window in its own colors, whatever the preview set.
        app::foreground(own_fg.0, own_fg.1, own_fg.2);
        app::background(behind_bg.0, behind_bg.1, behind_bg.2);
        dialog_redraw.redraw();
    });
    app::foreground(behind_fg.0, behind_fg.1, behind_fg.2);
    app::background(behind_bg.0, behind_bg.1, behind_bg.2);
}

/// Build the theme manager window with its callbacks wired, without showing
/// it, and return it with its theme list. For the UI construction tests
/// (`tests/ui_dialogs.rs`); [`show_theme_manager`] is the real entry point.
pub fn build_theme_manager(
    theme_bg: (u8, u8, u8),
    model: ThemeManagerModel,
) -> (Window, HoldBrowser) {
    let (dialog, ctx) = build_window(theme_bg, model);
    (dialog, ctx.browser)
}

/// Build the window and wire its callbacks, without showing it.
fn build_window(theme_bg: (u8, u8, u8), model: ThemeManagerModel) -> (Window, Ctx) {
    let theme = DialogTheme::from_theme_bg(theme_bg);

    // ---- Measured layout: every width comes from the text it shows ----
    let size = app::font_size();
    let action_w = max_label_width(
        &["Duplicate…", "Rename…", "Delete", "Import .tmTheme…"],
        size,
    );
    let list_w = (2 * action_w + GAP).max(MIN_LIST_WIDTH);
    let action_w = (list_w - GAP) / 2;
    let field_labels: Vec<&str> = EditorColorField::all()
        .iter()
        .map(|&f| field_label(f))
        .collect();
    let label_col = max_label_width(&field_labels, size);
    // Hex digits differ in width; measure the widest ones.
    let hex_col = max_label_width(&WIDE_HEX, HEX_SIZE);
    let pick_w = label_width("Pick…", size);
    let reset_w = label_width("Reset", size);
    let right_w = label_col + SWATCH_W + hex_col + pick_w + reset_w + 4 * GAP;
    let ok_w = max_label_width(&["OK", "Cancel"], size).max(MIN_BUTTON_WIDTH);
    let hint_w = HINT
        .lines()
        .map(|line| label_width(line, HINT_SIZE))
        .max()
        .unwrap_or(0);
    let right_x = PADDING * 2 + list_w;
    let dialog_w =
        (right_x + right_w + PADDING).max(PADDING + hint_w + PADDING + 2 * ok_w + GAP + PADDING);

    // Built top-level even if another group is current (T0058).
    let top_level = super::TopLevelScope::new();
    let mut dialog = Window::default()
        .with_size(dialog_w, DIALOG_HEIGHT)
        .with_label("Manage Themes")
        .center_screen();
    dialog.make_modal(true);
    dialog.set_color(theme.bg);

    // ---- Left: theme list and theme-level actions ----
    let mut list_label = Frame::default()
        .with_pos(PADDING, PADDING)
        .with_size(list_w, 20)
        .with_label("Themes")
        .with_align(Align::Left | Align::Inside);
    list_label.set_label_color(theme.text);

    let list_y = PADDING + 25;
    let list_h = 300;
    let mut browser = theme
        .list()
        .with_pos(PADDING, list_y)
        .with_size(list_w, list_h);
    // Item text uses FLTK's foreground, the dialog text color here.
    browser.set_scrollbar_size(SCROLLBAR_SIZE);
    style_scrollbars(&browser, theme.scroll_track, theme.scroll_thumb);

    let second_x = PADDING + action_w + GAP;
    let row1 = list_y + list_h + 10;
    let mut duplicate_btn = button(PADDING, row1, action_w, "Duplicate…", &theme);
    let mut rename_btn = button(second_x, row1, action_w, "Rename…", &theme);
    let row2 = row1 + BUTTON_HEIGHT + 8;
    let mut delete_btn = button(PADDING, row2, action_w, "Delete", &theme);
    let mut import_btn = button(second_x, row2, action_w, "Import .tmTheme…", &theme);
    import_btn.set_tooltip("Import a TextMate .tmTheme file as a custom theme");

    // ---- Right: editor colors of the selected theme ----
    let mut title = Frame::default()
        .with_pos(right_x, PADDING)
        .with_size(right_w, 20)
        .with_align(Align::Left | Align::Inside | Align::Clip);
    title.set_label_color(theme.text);

    let swatch_x = right_x + label_col + GAP;
    let hex_x = swatch_x + SWATCH_W + GAP;
    let pick_x = hex_x + hex_col + GAP;
    let reset_x = pick_x + pick_w + GAP;
    let mut rows = Vec::new();
    for (i, &field) in EditorColorField::all().iter().enumerate() {
        let y = list_y + i as i32 * ROW_HEIGHT;
        let mut label = Frame::default()
            .with_pos(right_x, y)
            .with_size(label_col, 26)
            .with_label(field_label(field))
            .with_align(Align::Left | Align::Inside);
        label.set_label_color(theme.text);
        let mut swatch = Frame::default()
            .with_pos(swatch_x, y + 2)
            .with_size(SWATCH_W, 22);
        style_swatch(&mut swatch, &theme);
        let mut hex = Frame::default()
            .with_pos(hex_x, y)
            .with_size(hex_col, 26)
            .with_align(Align::Left | Align::Inside);
        hex.set_label_color(theme.text_dim);
        hex.set_label_size(HEX_SIZE);
        let pick = button(pick_x, y, pick_w, "Pick…", &theme);
        let mut reset = button(reset_x, y, reset_w, "Reset", &theme);
        reset.set_tooltip("Reset to the base theme's color");
        rows.push(ColorRow {
            field,
            swatch,
            hex,
            pick,
            reset,
        });
    }

    let below_rows = list_y + EditorColorField::all().len() as i32 * ROW_HEIGHT + 4;
    let mut note = Frame::default()
        .with_pos(right_x, below_rows)
        .with_size(right_w, 36)
        .with_align(Align::Left | Align::Top | Align::Inside | Align::Wrap);
    note.set_label_color(theme.text_dim);
    note.set_label_size(12);
    let mut error = Frame::default()
        .with_pos(right_x, below_rows + 38)
        .with_size(right_w, 36)
        .with_align(Align::Left | Align::Top | Align::Inside | Align::Wrap);
    error.set_label_color(theme.error_color());
    error.set_label_size(12);

    // ---- Bottom: hint, OK / Cancel ----
    let btn_y = DIALOG_HEIGHT - PADDING - BUTTON_HEIGHT;
    let cancel_x = dialog_w - PADDING - ok_w;
    let ok_x = cancel_x - GAP - ok_w;
    let mut hint = Frame::default()
        .with_pos(PADDING, btn_y - 4)
        .with_size(ok_x - 2 * PADDING, 36)
        .with_label(HINT)
        .with_align(Align::Left | Align::Inside | Align::Wrap);
    hint.set_label_color(theme.text_dim);
    hint.set_label_size(HINT_SIZE);
    let mut cancel_btn = button(cancel_x, btn_y, ok_w, "Cancel", &theme);
    let mut ok_btn = button(ok_x, btn_y, ok_w, "OK", &theme);

    dialog.end();
    drop(top_level);

    let ctx = Ctx {
        theme_bg,
        model: Rc::new(RefCell::new(model)),
        pending: Rc::new(RefCell::new(Vec::new())),
        selected: Rc::new(RefCell::new(None)),
        browser: browser.clone(),
        rename: rename_btn.clone(),
        delete: delete_btn.clone(),
        title,
        note,
        error,
        rows: Rc::new(rows),
    };
    // Open on the theme the editor shows (falls back to the first entry
    // when that is a missing custom theme).
    let shown = ctx.model.borrow().shown().clone();
    *ctx.selected.borrow_mut() = Some(shown);
    ctx.refresh_list();

    // ---- Callbacks ----
    let c = ctx.clone();
    browser.set_callback(move |b| {
        let line = b.value();
        if line < 1 {
            return;
        }
        let entries = c.model.borrow().entries();
        if let Some(entry) = entries.get(line as usize - 1) {
            *c.selected.borrow_mut() = Some(entry.theme.clone());
            c.error.clone().set_label("");
            c.refresh_details();
        }
    });

    let c = ctx.clone();
    duplicate_btn.set_callback(move |_| {
        let Some(entry) = c.selected_entry() else {
            return;
        };
        let default = format!("{} copy", entry.name);
        if let Some(name) = show_themed_input(
            c.theme_bg,
            "Duplicate Theme",
            "Name of the new theme:",
            &default,
        ) {
            let actions = c.model.borrow_mut().duplicate(&entry.theme, &name);
            c.handle(actions);
        }
    });

    let c = ctx.clone();
    rename_btn.set_callback(move |_| {
        let Some(entry) = c.selected_entry() else {
            return;
        };
        if let Some(name) = show_themed_input(c.theme_bg, "Rename Theme", "New name:", &entry.name)
        {
            let actions = c.model.borrow_mut().rename(&entry.theme, &name);
            c.handle(actions);
        }
    });

    let c = ctx.clone();
    delete_btn.set_callback(move |_| {
        let Some(entry) = c.selected_entry() else {
            return;
        };
        let message = format!(
            "Delete the theme \"{}\"? This cannot be undone.",
            entry.name
        );
        // b0 (rightmost) = Delete; b1 = Cancel, the Enter default.
        if show_themed_choice(c.theme_bg, "Delete Theme", &message, "Delete", "Cancel", "")
            == Some(0)
        {
            let actions = c.model.borrow_mut().delete(&entry.theme);
            c.handle(actions);
        }
    });

    let c = ctx.clone();
    import_btn.set_callback(move |_| {
        let Some(path) = native_open_dialog(None, c.theme_bg) else {
            return;
        };
        let actions = c.model.borrow_mut().import(Path::new(&path));
        // A failed import gets a clear dialog, not only the inline line.
        if let Some(ThemeManagerAction::Error(message)) = actions
            .iter()
            .find(|a| matches!(a, ThemeManagerAction::Error(_)))
        {
            show_themed_message(c.theme_bg, "Import Theme", message);
        }
        c.handle(actions);
    });

    for row in ctx.rows.iter() {
        let (mut pick, mut reset) = (row.pick.clone(), row.reset.clone());
        let field = row.field;
        let c = ctx.clone();
        pick.set_callback(move |_| {
            let Some(entry) = c.selected_entry() else {
                return;
            };
            let Some(current) = c.model.borrow().colors(&entry.theme) else {
                return;
            };
            let title = format!("{} Color", field_label(field));
            if let Some(rgb) = show_color_picker(c.theme_bg, &title, field.of(&current)) {
                let actions = c.model.borrow_mut().set_color(&entry.theme, field, rgb);
                c.handle(actions);
            }
        });
        let c = ctx.clone();
        reset.set_callback(move |_| {
            let Some(entry) = c.selected_entry() else {
                return;
            };
            let actions = c.model.borrow_mut().reset_color(&entry.theme, field);
            c.handle(actions);
        });
    }

    let (c, mut dialog_ok) = (ctx.clone(), dialog.clone());
    ok_btn.set_callback(move |_| {
        let actions = c.model.borrow_mut().ok();
        let failed = actions
            .iter()
            .any(|a| matches!(a, ThemeManagerAction::Error(_)));
        c.handle(actions);
        if !failed {
            dialog_ok.hide();
        }
    });

    let (c, mut dialog_cancel) = (ctx.clone(), dialog.clone());
    cancel_btn.set_callback(move |_| {
        let actions = c.model.borrow_mut().cancel();
        c.handle(actions);
        dialog_cancel.hide();
    });

    // Escape / close button: same as Cancel.
    let c = ctx.clone();
    dialog.set_callback(move |w| {
        let actions = c.model.borrow_mut().cancel();
        c.handle(actions);
        w.hide();
    });

    (dialog, ctx)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literal_browser_label_disables_formatting() {
        for name in ["@b", "@C1x", "@@", "", "normal", "@.already"] {
            assert_eq!(literal_browser_label(name), format!("@.{name}"));
        }
        assert_eq!(literal_browser_label("a\tb"), "@.a b");
    }

    /// No code may turn off a browser's format or column character with
    /// '\0': fltk-rs asserts against it in debug builds.
    #[test]
    fn no_nul_browser_format_or_column_char_in_src() {
        let needles = [
            format!("set_format_char('{}0')", '\\'),
            format!("set_column_char('{}0')", '\\'),
        ];
        let mut stack = vec![std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src")];
        let mut offenders = Vec::new();
        while let Some(path) = stack.pop() {
            if path.is_dir() {
                for entry in std::fs::read_dir(&path).unwrap() {
                    stack.push(entry.unwrap().path());
                }
            } else if path.extension().is_some_and(|e| e == "rs") {
                let text = std::fs::read_to_string(&path).unwrap();
                if needles.iter().any(|n| text.contains(n.as_str())) {
                    offenders.push(path.display().to_string());
                }
            }
        }
        assert!(offenders.is_empty(), "{offenders:?}");
    }
}
