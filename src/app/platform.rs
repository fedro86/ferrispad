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

    // Linux: Try to detect system theme on GNOME
    #[cfg(target_os = "linux")]
    {
        use std::process::Command;

        if let Ok(output) = Command::new("gsettings")
            .args(["get", "org.gnome.desktop.interface", "gtk-theme"])
            .output()
        {
            let theme = String::from_utf8_lossy(&output.stdout).to_lowercase();
            if theme.contains("dark") {
                return true;
            }
        }

        // Try alternative method for other desktop environments
        if let Ok(output) = Command::new("gsettings")
            .args(["get", "org.gnome.desktop.interface", "color-scheme"])
            .output()
        {
            let scheme = String::from_utf8_lossy(&output.stdout);
            if scheme.contains("prefer-dark") {
                return true;
            }
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
