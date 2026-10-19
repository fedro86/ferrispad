pub mod color;
pub mod diagnostic_panel;
pub mod dialogs;
pub mod divider;
pub mod editor_container;
pub mod file_dialogs;
pub mod layout_shortcut;
pub mod main_window;
pub mod menu;
pub mod scrollbar;
pub mod split_panel;
pub mod start_page;
pub mod status_bar;
pub mod tab_bar;
pub mod terminal_panel;
pub mod theme;
pub mod toast;
pub mod tree_panel;

/// Serializes the unit tests that construct FLTK widgets: FLTK's "current
/// group" is process-global, so widgets built on parallel test threads could
/// land in each other's groups.
#[cfg(test)]
pub(crate) fn fltk_widget_test_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}
