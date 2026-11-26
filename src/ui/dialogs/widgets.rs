//! Themed dialog widgets (T0105): the only way dialogs create their inputs,
//! choices, check and radio buttons, buttons, lists and text views.
//!
//! Every constructor sets every themed property, so no dialog can forget one
//! (the Large Files inputs in Settings once selected text in FLTK's default
//! navy and drew a black cursor on a dark field). Inputs, text views and
//! lists share one selection color, [`DialogTheme::list_selection_bg`]
//! (T0097), registered with FerrisPad's contrast rule so the selected text is
//! drawn readable on it. A source-guard test keeps raw constructors and
//! per-widget selection/cursor colors out of the dialogs.
//!
//! The constructors return default-placed widgets, so call sites keep their
//! layout: `theme.input().with_pos(x, y).with_size(w, h)`.

use fltk::{
    browser::HoldBrowser,
    button::{Button, CheckButton, RadioRoundButton},
    enums::{Color, FrameType},
    group::{ColorChooser, Group, Tabs},
    input::{Input, IntInput},
    menu::{Choice, MenuButton},
    misc::Progress,
    prelude::*,
    text::{TextDisplay, TextEditor},
};

use std::ffi::{c_int, c_uint};
use std::sync::Once;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use super::DialogTheme;
use crate::ui::color::{ThemeRgb, push_to_contrast};

/// FLTK box type of every themed text field, text view and list (T0105): a
/// flat fill in the widget's color with a 1 px border in its theme's
/// [`DialogTheme::input_border`]. FLTK passes a box-drawing function only the
/// fill color, so the border is looked up from that color (see
/// [`input_border_for`]); FLTK's own `BORDER_BOX` would draw it black.
pub const INPUT_FRAME: FrameType = FrameType::FreeBoxType;

// fill → border pairs registered by the themes in use (a theme switch adds
// one; four slots hold the light and dark dialogs side by side). Packed as
// `PRESENT | fill << 24 | border`, atomics so the C drawing callback reads
// them without locks or panics.
static INPUT_BORDERS: [AtomicU64; 4] = [
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
];
static NEXT_BORDER_SLOT: AtomicUsize = AtomicUsize::new(0);
const PRESENT: u64 = 1 << 48;

fn pack_rgb((r, g, b): (u8, u8, u8)) -> u64 {
    u64::from(r) << 16 | u64::from(g) << 8 | u64::from(b)
}

fn unpack_rgb(bits: u64) -> (u8, u8, u8) {
    ((bits >> 16) as u8, (bits >> 8) as u8, bits as u8)
}

/// Remember that a field filled with `fill` gets a `border` border.
fn register_input_border(fill: (u8, u8, u8), border: (u8, u8, u8)) {
    let entry = PRESENT | pack_rgb(fill) << 24 | pack_rgb(border);
    if INPUT_BORDERS
        .iter()
        .any(|slot| slot.load(Ordering::Relaxed) == entry)
    {
        return;
    }
    let slot = NEXT_BORDER_SLOT.fetch_add(1, Ordering::Relaxed) % INPUT_BORDERS.len();
    INPUT_BORDERS[slot].store(entry, Ordering::Relaxed);
}

/// The border registered for a field filled with `fill`, if any.
pub fn input_border_for(fill: Color) -> Option<Color> {
    let fill = pack_rgb(fill.to_rgb());
    INPUT_BORDERS.iter().find_map(|slot| {
        let entry = slot.load(Ordering::Relaxed);
        (entry & PRESENT != 0 && (entry >> 24) & 0xFF_FFFF == fill).then(|| {
            let (r, g, b) = unpack_rgb(entry & 0xFF_FFFF);
            Color::from_rgb(r, g, b)
        })
    })
}

/// FLTK's box-drawing callback for [`INPUT_FRAME`], with FLTK's C signature
/// `void (*)(int x, int y, int w, int h, Fl_Color c)`. It runs inside C
/// drawing code: plain drawing and atomic loads only, and being `extern "C"`
/// a panic would abort rather than unwind into FLTK.
extern "C" fn draw_input_frame(x: c_int, y: c_int, w: c_int, h: c_int, c: c_uint) {
    let fill = Color::from_rgbi(c);
    fltk::draw::draw_rect_fill(x, y, w, h, fill);
    // An unregistered fill (e.g. FLTK's inactive variant of a field) still
    // gets a border, derived from the fill itself (3:1 against it).
    let border = input_border_for(fill).unwrap_or_else(|| {
        let fill = ThemeRgb::from_fltk(fill);
        push_to_contrast(fill, fill, 3.0, fill.brightness() < 128).to_fltk()
    });
    fltk::draw::set_draw_color(border);
    fltk::draw::draw_rect(x, y, w, h);
}

/// Register [`draw_input_frame`] for [`INPUT_FRAME`] once.
fn install_input_frame() {
    static INSTALL: Once = Once::new();
    INSTALL.call_once(|| {
        // SAFETY: `draw_input_frame` has exactly the C signature FLTK expects
        // (`Fl_Box_Draw_F`), captures nothing and does not unwind; the frame
        // insets (1, 1, 2, 2) are those of FLTK's BORDER_BOX. Registered
        // directly rather than via `app::set_frame_type_cb`, which transmutes
        // a Rust-ABI fn pointer into the C one (as `ui::contrast::install`).
        unsafe {
            fltk_sys::fl::Fl_set_box_type_cb(
                INPUT_FRAME.as_i32(),
                Some(draw_input_frame),
                1,
                1,
                2,
                2,
            )
        };
    });
}

impl DialogTheme {
    /// The colors every text field shares: input background, text and cursor
    /// in the text color, and the shared, contrast-registered selection.
    fn style_text_field<I: InputExt>(&self, field: &mut I) {
        field.set_color(self.input_bg);
        field.set_text_color(self.text);
        field.set_cursor_color(self.text);
        field.set_selection_color(self.list_selection_bg);
        field.set_label_color(self.text);
        self.set_input_frame(field);
        crate::ui::contrast::register_list_selection(self.list_selection_bg.to_rgb());
    }

    /// [`INPUT_FRAME`] with this theme's border for a field filled with
    /// `input_bg`.
    fn set_input_frame<W: WidgetExt>(&self, widget: &mut W) {
        install_input_frame();
        register_input_border(self.input_bg.to_rgb(), self.input_border.to_rgb());
        widget.set_frame(INPUT_FRAME);
    }

    /// A single-line text input.
    pub fn input(&self) -> Input {
        let mut input = Input::default();
        self.style_text_field(&mut input);
        input
    }

    /// A read-only text input (e.g. the Settings Font field, chosen through a
    /// picker): input colors, its text still selectable for copying, and no
    /// caret, since nothing can be typed there.
    pub fn readonly_input(&self) -> Input {
        let mut input = self.input();
        input.set_readonly(true);
        input.set_cursor_color(self.input_bg);
        input
    }

    /// An integer-only text input.
    pub fn int_input(&self) -> IntInput {
        let mut input = IntInput::default();
        self.style_text_field(&mut input);
        input
    }

    /// A multi-line text view or editor (`TextDisplay` / `TextEditor`): the
    /// same colors as an input.
    fn style_text_view<D: DisplayExt>(&self, view: &mut D) {
        view.set_color(self.input_bg);
        view.set_text_color(self.text);
        view.set_cursor_color(self.text);
        view.set_selection_color(self.list_selection_bg);
        view.set_label_color(self.text);
        self.set_input_frame(view);
        crate::ui::contrast::register_list_selection(self.list_selection_bg.to_rgb());
    }

    /// A read-only multi-line text view.
    pub fn text_display(&self) -> TextDisplay {
        let mut view = TextDisplay::default();
        self.style_text_view(&mut view);
        view
    }

    /// An editable multi-line text view.
    pub fn text_editor(&self) -> TextEditor {
        let mut view = TextEditor::default();
        self.style_text_view(&mut view);
        view
    }

    /// A drop-down choice: input colors, menu highlight in the button color.
    pub fn choice(&self) -> Choice {
        let mut choice = Choice::default();
        choice.set_color(self.input_bg);
        choice.set_text_color(self.text);
        choice.set_selection_color(self.button_bg);
        choice.set_label_color(self.text);
        choice
    }

    /// A check button on the dialog background. Its mark keeps FLTK's own
    /// color: the box interior is FLTK's white `BACKGROUND2`, on which a
    /// theme color could read poorly (the button color did, in light themes).
    pub fn check_button(&self) -> CheckButton {
        let mut check = CheckButton::default();
        check.set_color(self.bg);
        check.set_label_color(self.text);
        check
    }

    /// A round radio button on the dialog background (mark as for
    /// [`Self::check_button`]).
    pub fn radio(&self) -> RadioRoundButton {
        let mut radio = RadioRoundButton::default();
        radio.set_color(self.bg);
        radio.set_label_color(self.text);
        radio
    }

    /// A progress bar: input-colored track, button-colored fill.
    pub fn progress(&self) -> Progress {
        let mut progress = Progress::default();
        progress.set_frame(FrameType::FlatBox);
        progress.set_color(self.input_bg);
        progress.set_selection_color(self.button_bg);
        progress.set_label_color(self.text);
        progress
    }

    /// Tabs: flat frame (so the GTK scheme cannot override the colors),
    /// inactive tabs in the row color, the active one in `tab_active_bg`.
    pub fn style_tabs(&self, tabs: &mut Tabs) {
        tabs.set_frame(FrameType::FlatBox);
        tabs.set_color(self.row_bg);
        tabs.set_selection_color(self.tab_active_bg);
    }

    /// FLTK's color chooser and its own parts (mode choice, value fields,
    /// hue and value boxes).
    pub fn style_color_chooser(&self, chooser: &mut ColorChooser) {
        chooser.set_color(self.bg);
        for i in 0..chooser.children() {
            if let Some(mut part) = chooser.child(i) {
                part.set_color(self.input_bg);
                part.set_label_color(self.text);
                // The value fields select text like every other input; the
                // mode choice highlights its menu in the button color.
                let field = fltk::valuator::ValueInput::from_dyn_widget(&part).is_some();
                part.set_selection_color(if field {
                    self.list_selection_bg
                } else {
                    self.button_bg
                });
            }
        }
        crate::ui::contrast::register_list_selection(self.list_selection_bg.to_rgb());
    }

    /// The text-field colors (background, text, cursor, shared selection)
    /// for an input FLTK builds itself, e.g. the file chooser's filename
    /// field; its frame is left alone.
    pub(crate) fn style_foreign_input<I: InputExt>(&self, field: &mut I) {
        field.set_color(self.input_bg);
        field.set_text_color(self.text);
        field.set_cursor_color(self.text);
        field.set_selection_color(self.list_selection_bg);
        crate::ui::contrast::register_list_selection(self.list_selection_bg.to_rgb());
    }

    /// A section header bar (a group, flat, in the button color).
    pub fn header_bar(&self) -> Group {
        let mut bar = Group::default();
        bar.set_frame(FrameType::FlatBox);
        bar.set_color(self.button_bg);
        bar
    }

    /// A menu button FLTK builds itself (the file chooser's): the secondary
    /// button color, menu highlight in the button color.
    pub(crate) fn style_foreign_menu_button(&self, menu: &mut MenuButton) {
        self.style_foreign_menu(menu);
        menu.set_color(self.secondary_button_bg);
    }

    /// A menu FLTK builds itself (the file chooser's): input colors, menu
    /// highlight in the button color.
    pub(crate) fn style_foreign_menu<M: MenuExt>(&self, menu: &mut M) {
        menu.set_color(self.input_bg);
        menu.set_text_color(self.text);
        menu.set_selection_color(self.button_bg);
    }

    /// Give a dialog list (a browser) the shared list colors: `input_bg`
    /// background and the readable selected row of [`Self::list_selection`],
    /// registered with FerrisPad's contrast rule so FLTK draws the selected
    /// row's text in `list_selection_text` (T0097).
    pub fn style_list<B: WidgetExt>(&self, list: &mut B) {
        list.set_color(self.input_bg);
        list.set_selection_color(self.list_selection_bg);
        crate::ui::contrast::register_list_selection(self.list_selection_bg.to_rgb());
    }

    /// A flat dialog button in `bg` with a `label_color` label.
    fn flat_button(&self, bg: Color, label_color: Color) -> Button {
        let mut button = Button::default();
        button.set_frame(FrameType::RFlatBox);
        button.set_color(bg);
        button.set_label_color(label_color);
        button
    }

    /// A push button (OK, Save, Find, …): flat, in the button color.
    pub fn button(&self) -> Button {
        self.flat_button(self.button_bg, self.text)
    }

    /// A secondary push button (Cancel, Close): flat, a subtle step off the
    /// dialog background, with the normal text color (a dimmed label would
    /// make an active button look disabled).
    pub fn secondary_button(&self) -> Button {
        self.flat_button(self.secondary_button_bg, self.text)
    }

    /// A push button whose action needs care (e.g. "Install Anyway"): the
    /// button color with a [`Self::warning_text`] label.
    pub fn warning_button(&self) -> Button {
        self.flat_button(self.button_bg, self.warning_text())
    }

    /// Warning text color, readable on the dialog in dark and light themes.
    pub fn warning_text(&self) -> Color {
        if self.is_dark() {
            Color::from_rgb(220, 180, 80)
        } else {
            Color::from_rgb(180, 120, 20)
        }
    }

    /// A single-selection list (see [`DialogTheme::style_list`]).
    pub fn list(&self) -> HoldBrowser {
        let mut list = HoldBrowser::default();
        self.style_list(&mut list);
        self.set_input_frame(&mut list);
        list
    }
}

#[cfg(test)]
mod tests {
    /// T0105: dialogs build their themed widgets only through this module,
    /// so none can forget a themed property again. Outside it, the dialog
    /// sources (and the themed file chooser) must not construct these
    /// widgets raw nor set a selection or cursor color by hand.
    #[test]
    fn dialogs_build_widgets_only_through_the_components() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/ui");
        let mut files: Vec<_> = std::fs::read_dir(root.join("dialogs"))
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.extension().is_some_and(|e| e == "rs") && !p.ends_with("widgets.rs"))
            .collect();
        files.push(root.join("file_dialogs.rs"));
        // Any FLTK widget a user can select in or click, built raw…
        let constructor = regex_lite::Regex::new(
            r"\b\w*(Input|Output|Browser|Choice|Button|Display|Editor|Progress|Spinner)::(new|default)\(",
        )
        .unwrap();
        // …or styled by hand.
        let hand_styled = [
            ".set_selection_color(",
            ".set_cursor_color(",
            "button_bg)",
            "set_frame(FrameType::RFlatBox)",
        ];
        let mut hits = Vec::new();
        for file in files {
            let text = std::fs::read_to_string(&file).unwrap();
            // The tests module and comments may name these freely. Only the
            // `mod tests` block is cut, so a `#[cfg(test)]` item elsewhere in
            // the file does not end the scan.
            let code = text.split("#[cfg(test)]\nmod tests").next().unwrap_or("");
            for (n, line) in code.lines().enumerate() {
                if line.trim_start().starts_with("//") {
                    continue;
                }
                if let Some(m) = constructor.find(line) {
                    hits.push(format!("{}:{}: {}", file.display(), n + 1, m.as_str()));
                } else if let Some(pattern) = hand_styled.iter().find(|p| line.contains(*p)) {
                    hits.push(format!("{}:{}: {pattern}", file.display(), n + 1));
                }
            }
        }
        assert!(
            hits.is_empty(),
            "use the DialogTheme components (ui::dialogs::widgets):\n{}",
            hits.join("\n")
        );
    }
}
