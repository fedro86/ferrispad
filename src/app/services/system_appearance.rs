//! The desktop's light/dark preference on Linux (T0088). Pure parsing and
//! decision only; `infrastructure::platform::detect_system_dark_mode` runs the
//! `gdbus` / `gsettings` commands and feeds their output in here.
//!
//! The XDG desktop portal (`org.freedesktop.appearance` / `color-scheme`) is
//! asked first because it answers correctly where `gsettings` cannot see the
//! user's dconf database (inside `nix develop`, under a custom
//! `XDG_CONFIG_HOME`, in sandboxes). `gsettings` is the fallback.

/// The portal's `org.freedesktop.appearance` `color-scheme` value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorScheme {
    /// `0`, and any value the spec does not define ("treat as 0").
    NoPreference,
    /// `1`.
    PreferDark,
    /// `2`.
    PreferLight,
}

/// Longest reply worth looking at; anything longer is not a color-scheme.
const MAX_REPLY_LEN: usize = 256;

/// Parse the stdout of `gdbus call … org.freedesktop.portal.Settings.ReadOne`
/// (`(<uint32 1>,)`) or of the older `Settings.Read`, whose value is wrapped
/// in one more variant (`(<<uint32 1>>,)`). `None` for anything else.
pub fn parse_portal_reply(stdout: &str) -> Option<ColorScheme> {
    if stdout.len() > MAX_REPLY_LEN {
        return None;
    }
    let inner = stdout.trim().strip_prefix('(')?.strip_suffix(",)")?.trim();
    let opens = inner.len() - inner.trim_start_matches('<').len();
    let closes = inner.len() - inner.trim_end_matches('>').len();
    if !(1..=2).contains(&opens) || opens != closes {
        return None;
    }
    let value = inner[opens..inner.len() - closes]
        .trim()
        .strip_prefix("uint32 ")?
        .trim();
    Some(match value.parse::<u32>().ok()? {
        1 => ColorScheme::PreferDark,
        2 => ColorScheme::PreferLight,
        _ => ColorScheme::NoPreference,
    })
}

/// Today's `gsettings` rule: a `gtk-theme` whose name contains "dark", or a
/// `color-scheme` of `'prefer-dark'`. Arguments are the raw command stdouts.
pub fn gsettings_prefers_dark(gtk_theme: &str, color_scheme: &str) -> bool {
    gtk_theme.to_lowercase().contains("dark") || color_scheme.contains("prefer-dark")
}

/// The desktop prefers dark. An explicit portal preference decides;
/// `gsettings` is only consulted (lazily) when the portal is missing,
/// unreadable or has no preference.
pub fn prefers_dark(portal: Option<ColorScheme>, gsettings: impl FnOnce() -> bool) -> bool {
    match portal {
        Some(ColorScheme::PreferDark) => true,
        Some(ColorScheme::PreferLight) => false,
        Some(ColorScheme::NoPreference) | None => gsettings(),
    }
}

/// Whether asking the session bus is safe: an explicit
/// `DBUS_SESSION_BUS_ADDRESS`, or the standard `$XDG_RUNTIME_DIR/bus` socket.
/// Without either, GIO would try to *autolaunch* a bus daemon, a side effect
/// a theme lookup must not have; so is an address list that names the
/// `autolaunch:` transport anywhere (GIO tries the entries in order).
pub fn session_bus_reachable(bus_address: Option<&str>, runtime_bus_exists: bool) -> bool {
    match bus_address.map(str::trim).filter(|a| !a.is_empty()) {
        Some(address) => !address
            .split(';')
            .any(|entry| entry.trim_start().starts_with("autolaunch:")),
        None => runtime_bus_exists,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GS_LIGHT: (&str, &str) = ("'Adwaita'\n", "'default'\n");

    fn gs((theme, scheme): (&str, &str)) -> bool {
        gsettings_prefers_dark(theme, scheme)
    }

    /// The bug: portal says dark, gsettings (nix memory backend) says
    /// 'default' → System default stayed light.
    #[test]
    fn system_appearance_portal_dark_beats_gsettings_default() {
        let portal = parse_portal_reply("(<uint32 1>,)\n");
        assert_eq!(portal, Some(ColorScheme::PreferDark));
        assert!(prefers_dark(portal, || gs(GS_LIGHT)), "portal dark → dark");
    }

    #[test]
    fn system_appearance_portal_light_is_light_without_asking_gsettings() {
        let portal = parse_portal_reply("(<uint32 2>,)");
        assert!(!prefers_dark(portal, || panic!("gsettings must not run")));
        let dark = prefers_dark(parse_portal_reply("(<uint32 1>,)"), || {
            panic!("gsettings must not run")
        });
        assert!(dark);
    }

    #[test]
    fn system_appearance_falls_back_to_gsettings() {
        let dark_gs = ("'Adwaita'\n", "'prefer-dark'\n");
        let dark_theme = ("'Adwaita-dark'\n", "'default'\n");
        for portal in [None, Some(ColorScheme::NoPreference)] {
            assert!(prefers_dark(portal, || gs(dark_gs)), "{portal:?}");
            assert!(prefers_dark(portal, || gs(dark_theme)), "{portal:?}");
            assert!(!prefers_dark(portal, || gs(GS_LIGHT)), "{portal:?}");
            assert!(!prefers_dark(portal, || gs(("", ""))), "{portal:?}");
        }
    }

    #[test]
    fn system_appearance_parses_read_one_and_read_replies() {
        use ColorScheme::*;
        assert_eq!(parse_portal_reply("(<uint32 0>,)"), Some(NoPreference));
        assert_eq!(parse_portal_reply("(<uint32 1>,)\n"), Some(PreferDark));
        assert_eq!(parse_portal_reply("(<uint32 2>,)"), Some(PreferLight));
        // Settings.Read (portal interface v1) nests one more variant.
        assert_eq!(parse_portal_reply("(<<uint32 1>>,)\n"), Some(PreferDark));
        assert_eq!(parse_portal_reply("  (<<uint32 2>>,)  "), Some(PreferLight));
        // Values the spec does not define are "no preference".
        assert_eq!(parse_portal_reply("(<uint32 7>,)"), Some(NoPreference));
    }

    #[test]
    fn system_appearance_rejects_garbage_replies() {
        let long = format!("(<uint32 {}>,)", "1".repeat(MAX_REPLY_LEN));
        for bad in [
            "",
            "(",
            ",)",
            "()",
            "(<>,)",
            "(<uint32>,)",
            "(<uint32 >,)",
            "(<uint32 -1>,)",
            "(<uint32 1>>,)",
            "(<<<uint32 1>>>,)",
            "(<int32 1>,)",
            "(<'prefer-dark'>,)",
            "(<uint32 99999999999>,)",
            "Error: GDBus.Error:org.freedesktop.DBus.Error.ServiceUnknown",
            "(<uint32 é>,)",
            long.as_str(),
        ] {
            assert_eq!(parse_portal_reply(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn system_appearance_session_bus_reachable() {
        assert!(session_bus_reachable(
            Some("unix:path=/run/user/1000/bus"),
            false
        ));
        assert!(session_bus_reachable(None, true));
        assert!(!session_bus_reachable(None, false));
        assert!(!session_bus_reachable(Some("  "), false));
        // An explicit autolaunch: address would spawn a daemon.
        assert!(!session_bus_reachable(Some("autolaunch:"), true));
        assert!(!session_bus_reachable(
            Some("unix:path=/nonexistent;autolaunch:scope=x"),
            true
        ));
    }
}
