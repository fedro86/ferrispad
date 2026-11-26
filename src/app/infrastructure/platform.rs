/// Copy text to the system clipboard, using the correct mechanism per platform.
///
/// On Wayland, `fltk::app::copy2()` only sets the X11 selection (via XWayland),
/// which doesn't persist once focus leaves the app. We detect Wayland and pipe
/// through `wl-copy` instead. On X11 and other platforms, `copy2` works fine.
pub fn copy_to_clipboard(text: &str) {
    #[cfg(target_os = "linux")]
    {
        if std::env::var("WAYLAND_DISPLAY").is_ok() {
            // Wayland: pipe text to wl-copy
            use std::io::Write;
            use std::process::{Command, Stdio};

            if let Ok(mut child) = Command::new("wl-copy")
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
            {
                if let Some(mut stdin) = child.stdin.take() {
                    let _ = stdin.write_all(text.as_bytes());
                }
                let _ = child.wait();
                return;
            }
            // Fall through to FLTK if wl-copy is not available
            eprintln!("[clipboard] wl-copy not found, falling back to fltk::app::copy2");
        }
    }
    fltk::app::copy2(text);
}

// paste_from_clipboard removed: use fltk::app::paste_text() + Event::Paste instead.
// Synchronous clipboard reads deadlock on Wayland.

pub fn detect_system_dark_mode() -> bool {
    // Windows: Check registry for dark mode preference
    #[cfg(target_os = "windows")]
    {
        use winreg::RegKey;
        use winreg::enums::HKEY_CURRENT_USER;

        if let Ok(hkcu) = RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize")
        {
            // AppsUseLightTheme: 0 = dark mode, 1 = light mode
            if let Ok(value) = hkcu.get_value::<u32, _>("AppsUseLightTheme") {
                return value == 0;
            }
        }
    }

    // Linux: the XDG desktop portal first, then GNOME's gsettings (T0088).
    #[cfg(target_os = "linux")]
    {
        use crate::app::services::system_appearance::prefers_dark;
        if prefers_dark(read_portal_color_scheme(), gsettings_prefers_dark) {
            return true;
        }
    }

    // macOS: Check AppleInterfaceStyle
    #[cfg(target_os = "macos")]
    {
        use std::process::Command;

        if let Ok(output) = Command::new("defaults")
            .args(["read", "-g", "AppleInterfaceStyle"])
            .output()
        {
            if output.status.success() {
                let style = String::from_utf8_lossy(&output.stdout).to_lowercase();
                if style.contains("dark") {
                    return true;
                }
            }
        }
    }

    // Default to light mode if detection fails
    false
}

/// Ask the XDG desktop portal for `org.freedesktop.appearance` /
/// `color-scheme` with one `gdbus call` (1 s D-Bus timeout). `gdbus` ships
/// with the same glib package as `gsettings`. `None` when there is no session
/// bus, no portal, or an unreadable answer.
#[cfg(target_os = "linux")]
fn read_portal_color_scheme() -> Option<crate::app::services::system_appearance::ColorScheme> {
    use crate::app::services::system_appearance::{parse_portal_reply, session_bus_reachable};
    use std::process::{Command, Output, Stdio};

    // Without a bus GIO would autolaunch a dbus-daemon; never do that here.
    let runtime_bus = std::env::var_os("XDG_RUNTIME_DIR")
        .is_some_and(|dir| std::path::Path::new(&dir).join("bus").exists());
    let address = std::env::var("DBUS_SESSION_BUS_ADDRESS").ok();
    if !session_bus_reachable(address.as_deref(), runtime_bus) {
        return None;
    }

    let call = |method: &str| -> Option<Output> {
        Command::new("gdbus")
            .args([
                "call",
                "--session",
                "--timeout",
                "1",
                "--dest",
                "org.freedesktop.portal.Desktop",
                "--object-path",
                "/org/freedesktop/portal/desktop",
                "--method",
                method,
                "org.freedesktop.appearance",
                "color-scheme",
            ])
            .stdin(Stdio::null())
            .output()
            .ok()
    };
    let out = call("org.freedesktop.portal.Settings.ReadOne")?;
    if out.status.success() {
        return parse_portal_reply(&String::from_utf8_lossy(&out.stdout));
    }
    // Portal Settings interface v1 has only the (deprecated) Read method.
    if String::from_utf8_lossy(&out.stderr).contains("UnknownMethod") {
        let out = call("org.freedesktop.portal.Settings.Read")?;
        if out.status.success() {
            return parse_portal_reply(&String::from_utf8_lossy(&out.stdout));
        }
    }
    None
}

/// GNOME's `gtk-theme` / `color-scheme` keys, as before T0088. Fails to
/// "light" when `gsettings` is missing or cannot see the user's dconf.
#[cfg(target_os = "linux")]
fn gsettings_prefers_dark() -> bool {
    use crate::app::services::system_appearance::gsettings_prefers_dark as decide;
    use std::process::{Command, Stdio};

    let get = |key: &str| {
        Command::new("gsettings")
            .args(["get", "org.gnome.desktop.interface", key])
            .stdin(Stdio::null())
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
            .unwrap_or_default()
    };
    let gtk_theme = get("gtk-theme");
    // Same short-circuit as before: color-scheme only when the theme is light.
    decide(&gtk_theme, "") || decide("", &get("color-scheme"))
}
