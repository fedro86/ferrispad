pub mod find;
pub mod goto_line;
pub mod settings_dialog;
pub mod about;
pub mod update;

use fltk::{app, prelude::*, window::Window};

/// Run a dialog's event loop, automatically closing the dialog if the app
/// is quitting (e.g. user clicks X on the main window while a dialog is open).
pub fn run_dialog(dialog: &Window) {
    while dialog.shown() {
        app::wait();
        if app::should_program_quit() {
            let mut d = dialog.clone();
            d.hide();
        }
    }
}
