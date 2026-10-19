//! File → Open / Save As dialogs.
//!
//! On Linux/BSD, FLTK's `Fl_Native_File_Chooser` picks its backend at run
//! time (`Fl_Native_File_Chooser_GTK.cxx`): zenity if `OPTION_FNFC_USES_ZENITY`
//! is on (off by default), then GTK if `dlopen("libgtk-3.so.0")` works, then
//! its own `Fl_File_Chooser`. That last fallback is drawn with FLTK's global
//! colours (black text on a navy selection bar in light themes) and cannot be
//! reached through `NativeFileChooser`. A binary built in the Nix dev shell
//! gets it on any desktop, because the Nix loader cannot see the system GTK
//! (T0046). So when GTK cannot be loaded we pick the backend ourselves: zenity
//! when it is installed, else the same `Fl_File_Chooser`, built here so it
//! follows the theme. Where GTK loads, nothing changes.

use std::sync::OnceLock;

use fltk::{
    app,
    browser::FileBrowser,
    dialog::{FileChooser, FileChooserType, NativeFileChooser, NativeFileChooserType},
    enums::{Color, FrameType},
    group::Group,
    input::FileInput,
    menu::{Choice, MenuButton},
    prelude::*,
};

use crate::ui::dialogs::DialogTheme;

/// The `app::Option` that FLTK reads as `OPTION_FNFC_USES_ZENITY`.
///
/// fltk-rs 1.5.14's `app::Option` leaves out FLTK 1.4's
/// `OPTION_FNFC_USES_ZENITY` (5) and `OPTION_FNFC_USES_KDIALOG` (6), so its
/// later variants are off: `FnfcUsesZenity` is 7, which FLTK reads as
/// `OPTION_PRINTER_USES_GTK`, and `PrinterUsesGtk` is 5 — the zenity option.
const FNFC_USES_ZENITY: app::Option = app::Option::PrinterUsesGtk;

/// Which file dialog File → Open / Save As shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Backend {
    /// `NativeFileChooser` as is: GTK on Linux/BSD, the OS dialog elsewhere.
    Native,
    /// `NativeFileChooser` with FLTK's zenity backend switched on.
    Zenity,
    /// Our own `Fl_File_Chooser`, styled from the theme.
    Themed,
}

/// GTK first (unchanged behaviour where it loads), then zenity, then the
/// themed FLTK chooser. The zenity probe runs only when GTK is missing.
fn pick_backend(gtk_loadable: bool, zenity_found: impl FnOnce() -> bool) -> Backend {
    if gtk_loadable {
        Backend::Native
    } else if zenity_found() {
        Backend::Zenity
    } else {
        Backend::Themed
    }
}

/// Probed once, at the first file dialog (not at startup).
fn backend() -> Backend {
    static BACKEND: OnceLock<Backend> = OnceLock::new();
    *BACKEND.get_or_init(|| pick_backend(gtk_loadable(), zenity_found))
}

/// Whether FLTK's GTK probe will succeed: the same `dlopen` from the same
/// binary, so the same loader search path. GTK3 first, then GTK2, as FLTK does.
#[cfg(all(unix, not(target_os = "macos")))]
fn gtk_loadable() -> bool {
    use std::ffi::{c_char, c_int, c_void};
    unsafe extern "C" {
        fn dlopen(filename: *const c_char, flag: c_int) -> *mut c_void;
    }
    const RTLD_LAZY: c_int = 0x0001;
    const RTLD_GLOBAL: c_int = 0x0100;
    [c"libgtk-3.so.0", c"libgtk-x11-2.0.so.0"]
        .iter()
        .any(|name| {
            // SAFETY: `name` is a NUL-terminated literal. On success this runs
            // GTK's initialisers, exactly as FLTK's own probe does right after
            // with the same flags; the handle is deliberately never closed
            // (FLTK keeps the same library loaded for the process lifetime).
            !unsafe { dlopen(name.as_ptr(), RTLD_LAZY | RTLD_GLOBAL) }.is_null()
        })
}

/// macOS and Windows always have a native dialog.
#[cfg(not(all(unix, not(target_os = "macos"))))]
fn gtk_loadable() -> bool {
    true
}

/// FLTK's own check: `zenity --version` prints something.
fn zenity_found() -> bool {
    std::process::Command::new("zenity")
        .arg("--version")
        .stderr(std::process::Stdio::null())
        .output()
        .is_ok_and(|out| out.status.success() && !out.stdout.trim_ascii().is_empty())
}

/// FLTK hands the start folder to zenity as `--filename=<dir>`, and zenity
/// opens *inside* it only when the path ends with `/` (else it opens the
/// parent with the folder selected).
fn zenity_folder(directory: &str) -> String {
    if directory.ends_with('/') {
        directory.to_string()
    } else {
        format!("{directory}/")
    }
}

/// Background and text colour of a selected row in the themed chooser's file
/// list: the selection colour and text colour every other themed dialog list
/// uses (session picker, font picker).
fn selected_row_colors(theme: &DialogTheme) -> (Color, Color) {
    (theme.button_bg, theme.text)
}

/// Show the chooser and return the chosen paths (empty when cancelled).
fn choose(
    title: &str,
    native_type: NativeFileChooserType,
    themed_type: FileChooserType,
    directory: Option<&str>,
    theme_bg: (u8, u8, u8),
) -> Vec<String> {
    match backend() {
        Backend::Native => show_native(title, native_type, directory),
        Backend::Zenity => {
            app::set_option(FNFC_USES_ZENITY, true);
            let folder = directory.map(zenity_folder);
            show_native(title, native_type, folder.as_deref())
        }
        Backend::Themed => show_themed(title, themed_type, directory, theme_bg),
    }
}

fn show_native(
    title: &str,
    chooser_type: NativeFileChooserType,
    directory: Option<&str>,
) -> Vec<String> {
    let mut chooser = NativeFileChooser::new(chooser_type);
    chooser.set_title(title);
    if let Some(dir) = directory {
        chooser.set_directory(&dir.to_string()).ok();
    }
    chooser.show();
    chooser
        .filenames()
        .into_iter()
        .filter(|p| !p.as_os_str().is_empty())
        .map(|p| p.to_string_lossy().to_string())
        .collect()
}

/// FLTK's fallback chooser, built by us so its widgets can be styled; the
/// show/wait/read sequence mirrors `Fl_Native_File_Chooser_FLTK_Driver::show`.
fn show_themed(
    title: &str,
    chooser_type: FileChooserType,
    directory: Option<&str>,
    theme_bg: (u8, u8, u8),
) -> Vec<String> {
    let theme = DialogTheme::from_theme_bg(theme_bg);
    let mut chooser = FileChooser::new(directory.unwrap_or("."), "*", chooser_type, title);
    let mut window = chooser.window();
    window.set_color(theme.bg);
    window.set_label_color(theme.text);
    style_children(&window, &theme);
    // The file list (`Fl_File_Chooser::color` / `textcolor`).
    chooser.set_color(theme.input_bg);
    chooser.set_text_color(selected_row_colors(&theme).1);
    chooser.show();
    while chooser.shown() {
        app::wait();
        if app::should_program_quit() {
            // `hide` alone would keep the typed/selected name as the result.
            chooser.hide();
            return Vec::new();
        }
    }
    (1..=chooser.count())
        .filter_map(|i| chooser.value(i))
        .filter(|p| !p.is_empty())
        .collect()
}

/// Theme every widget under `group`. Per-widget colours only: FLTK's global
/// colours (also used by every other window) are left alone.
fn style_children(group: &impl GroupExt, theme: &DialogTheme) {
    for i in 0..group.children() {
        let Some(mut child) = group.child(i) else {
            continue;
        };
        // Keep colours FLTK picked on purpose (the red error message).
        if child.label_color() == Color::Foreground {
            child.set_label_color(theme.text);
        }
        if let Some(mut list) = FileBrowser::from_dyn_widget(&child) {
            // Its background and text colour are set through the chooser.
            list.set_selection_color(selected_row_colors(theme).0);
        } else if let Some(mut input) = FileInput::from_dyn_widget(&child) {
            // The path-segment buttons above the Filename field are drawn
            // with this box type; no box keeps them clickable but invisible.
            input.set_down_frame(FrameType::NoBox);
            input.set_color(theme.input_bg);
            input.set_text_color(theme.text);
            input.set_selection_color(theme.button_bg);
        } else if let Some(mut menu) = Choice::from_dyn_widget(&child) {
            menu.set_color(theme.input_bg);
            menu.set_text_color(theme.text);
            menu.set_selection_color(theme.button_bg);
        } else if let Some(mut menu) = MenuButton::from_dyn_widget(&child) {
            menu.set_color(theme.secondary_button_bg);
            menu.set_text_color(theme.text);
            menu.set_selection_color(theme.button_bg);
        } else {
            child.set_color(theme.bg);
        }
        if let Some(inner) = Group::from_dyn_widget(&child) {
            style_children(&inner, theme);
        }
    }
}

pub fn native_open_dialog(directory: Option<&str>, theme_bg: (u8, u8, u8)) -> Option<String> {
    choose(
        "Open File",
        NativeFileChooserType::BrowseFile,
        FileChooserType::Single,
        directory,
        theme_bg,
    )
    .into_iter()
    .next()
}

pub fn native_open_multi_dialog(directory: Option<&str>, theme_bg: (u8, u8, u8)) -> Vec<String> {
    choose(
        "Open Files",
        NativeFileChooserType::BrowseMultiFile,
        FileChooserType::Multi,
        directory,
        theme_bg,
    )
}

pub fn native_save_dialog(directory: Option<&str>, theme_bg: (u8, u8, u8)) -> Option<String> {
    choose(
        "Save As",
        NativeFileChooserType::BrowseSaveFile,
        FileChooserType::Create,
        directory,
        theme_bg,
    )
    .into_iter()
    .next()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::domain::settings::SyntaxTheme;

    /// FLTK's `fl_luminance` (sRGB channels raised to 2.4).
    fn fltk_luminance((r, g, b): (u8, u8, u8)) -> f64 {
        let ch = |c: u8| (c as f64 / 255.0).powf(2.4);
        0.212_672_9 * ch(r) + 0.715_152_2 * ch(g) + 0.072_175 * ch(b)
    }

    /// FLTK's `fl_lightness`: CIELAB L* (0..100).
    fn fltk_lightness(c: (u8, u8, u8)) -> f64 {
        let y = fltk_luminance(c);
        if y <= 216.0 / 24389.0 {
            y * (24389.0 / 27.0)
        } else {
            y.powf(1.0 / 3.0) * 116.0 - 16.0
        }
    }

    /// The text colour `Fl_File_Browser::item_draw` really uses on a selected
    /// row: `fl_contrast(textcolor(), selection_color())` in FLTK's default
    /// CIELAB mode, at the contrast level `main.rs` sets.
    fn fltk_drawn_text(fg: (u8, u8, u8), bg: (u8, u8, u8)) -> (u8, u8, u8) {
        let level = f64::from(crate::ui::theme::FLTK_CONTRAST_LEVEL);
        let (lfg, lbg) = (fltk_lightness(fg), fltk_lightness(bg));
        if (lfg - lbg).abs() >= level {
            fg
        } else if lbg > 50.0 {
            (0, 0, 0)
        } else {
            (255, 255, 255)
        }
    }

    /// WCAG 2 relative luminance.
    fn wcag_luminance((r, g, b): (u8, u8, u8)) -> f64 {
        let ch = |c: u8| {
            let c = c as f64 / 255.0;
            if c <= 0.039_28 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * ch(r) + 0.7152 * ch(g) + 0.0722 * ch(b)
    }

    fn wcag_contrast(a: (u8, u8, u8), b: (u8, u8, u8)) -> f64 {
        let (la, lb) = (wcag_luminance(a), wcag_luminance(b));
        (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
    }

    /// The selected row of the fallback chooser must be readable in every
    /// syntax theme: the text FLTK actually draws on the selection bar meets
    /// WCAG AA (4.5:1). Failed on FLTK's default pair (black on navy, T0046).
    #[test]
    fn fallback_selected_row_is_readable_in_every_theme() {
        let mut failures = Vec::new();
        for &syntax in SyntaxTheme::all() {
            let theme = DialogTheme::from_theme_bg(syntax.background());
            let (bg, fg) = selected_row_colors(&theme);
            let (bg, fg) = (bg.to_rgb(), fg.to_rgb());
            let drawn = fltk_drawn_text(fg, bg);
            let ratio = wcag_contrast(drawn, bg);
            if ratio < 4.5 {
                failures.push(format!(
                    "{syntax:?}: text {drawn:?} on selection {bg:?} has contrast {ratio:.2}:1"
                ));
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    /// Where GTK loads the dialog is unchanged and zenity is not even probed;
    /// zenity comes next, our themed chooser last.
    #[test]
    fn backend_prefers_gtk_then_zenity_then_themed() {
        let mut probed = false;
        let backend = pick_backend(true, || {
            probed = true;
            true
        });
        assert_eq!(backend, Backend::Native);
        assert!(!probed, "zenity must not be probed when GTK loads");
        assert_eq!(pick_backend(false, || true), Backend::Zenity);
        assert_eq!(pick_backend(false, || false), Backend::Themed);
    }

    #[test]
    fn zenity_folder_ends_with_one_slash() {
        assert_eq!(zenity_folder("/home/me/src"), "/home/me/src/");
        assert_eq!(zenity_folder("/home/me/src/"), "/home/me/src/");
        assert_eq!(zenity_folder("/"), "/");
    }

    /// FLTK 1.4's `Fl::Fl_Option` (FL/Fl.H) numbers `OPTION_FNFC_USES_ZENITY`
    /// 5. If fltk-rs fixes its `app::Option` numbering this fails: switch
    /// `FNFC_USES_ZENITY` to the variant that is 5 then.
    #[test]
    fn zenity_option_has_fltks_number() {
        assert_eq!(FNFC_USES_ZENITY as i32, 5);
    }
}
