use fltk::{
    enums::Color,
    frame::Frame,
    menu::MenuBar,
    prelude::*,
    text::TextEditor,
    window::Window,
};

pub fn apply_theme(
    editor: &mut TextEditor,
    window: &mut Window,
    menu: &mut MenuBar,
    banner: Option<&mut Frame>,
    is_dark: bool,
) {
    if is_dark {
        // Dark mode colors
        editor.set_color(Color::from_rgb(30, 30, 30));
        editor.set_text_color(Color::from_rgb(220, 220, 220));
        editor.set_cursor_color(Color::from_rgb(255, 255, 255));
        editor.set_selection_color(Color::from_rgb(70, 70, 100));
        editor.set_linenumber_bgcolor(Color::from_rgb(40, 40, 40));
        editor.set_linenumber_fgcolor(Color::from_rgb(150, 150, 150));
        window.set_color(Color::from_rgb(25, 25, 25));
        window.set_label_color(Color::from_rgb(220, 220, 220));
        menu.set_color(Color::from_rgb(35, 35, 35));
        menu.set_text_color(Color::from_rgb(220, 220, 220));
        menu.set_selection_color(Color::from_rgb(60, 60, 60)); // Hover color
        if let Some(b) = banner {
            b.set_color(Color::from_rgb(139, 128, 0)); // Darker yellow/olive
            b.set_label_color(Color::White);
        }
    } else {
        // Light mode colors
        editor.set_color(Color::White);
        editor.set_text_color(Color::Black);
        editor.set_cursor_color(Color::Black);
        editor.set_selection_color(Color::from_rgb(173, 216, 230));
        editor.set_linenumber_bgcolor(Color::from_rgb(240, 240, 240));
        editor.set_linenumber_fgcolor(Color::from_rgb(100, 100, 100));
        window.set_color(Color::from_rgb(240, 240, 240));
        window.set_label_color(Color::Black);
        menu.set_color(Color::from_rgb(240, 240, 240));
        menu.set_text_color(Color::Black);
        menu.set_selection_color(Color::from_rgb(200, 200, 200)); // Hover color
        if let Some(b) = banner {
            b.set_color(Color::from_rgb(255, 250, 205)); // Lemon chiffon
            b.set_label_color(Color::Black);
        }
    }

    editor.redraw();
    window.redraw();
    menu.redraw();
}

/// Set Windows title bar theme (Windows 10 build 1809+)
/// Must be called AFTER window.show() to have a valid HWND
#[cfg(target_os = "windows")]
pub fn set_windows_titlebar_theme(window: &Window, is_dark: bool) {
    use std::mem::size_of;
    use std::ptr::from_ref;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWINDOWATTRIBUTE};

    unsafe {
        let hwnd = HWND(window.raw_handle() as *mut std::ffi::c_void);

        let on: i32 = if is_dark { 1 } else { 0 };

        // Try attribute 20 (Windows 11 / Windows 10 2004+)
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWINDOWATTRIBUTE(20), // DWMWA_USE_IMMERSIVE_DARK_MODE
            from_ref(&on).cast(),
            size_of::<i32>() as u32,
        );

        // Also try attribute 19 (Windows 10 1809-1903)
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWINDOWATTRIBUTE(19),
            from_ref(&on).cast(),
            size_of::<i32>() as u32,
        );
    }
}
