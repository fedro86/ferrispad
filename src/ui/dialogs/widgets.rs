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

use super::DialogTheme;

impl DialogTheme {
    /// The colors every text field shares: input background, text and cursor
    /// in the text color, and the shared, contrast-registered selection.
    fn style_text_field<I: InputExt>(&self, field: &mut I) {
        field.set_color(self.input_bg);
        field.set_text_color(self.text);
        field.set_cursor_color(self.text);
        field.set_selection_color(self.list_selection_bg);
        field.set_label_color(self.text);
        field.set_frame(FrameType::FlatBox);
        crate::ui::contrast::register_list_selection(self.list_selection_bg.to_rgb());
    }

    /// A single-line text input.
    pub fn input(&self) -> Input {
        let mut input = Input::default();
        self.style_text_field(&mut input);
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
        view.set_frame(FrameType::FlatBox);
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
        list.set_frame(FrameType::FlatBox);
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
