//! The theme manager's logic (T0080), free of FLTK so it is unit-testable.
//!
//! Like the other controllers, every operation returns
//! [`ThemeManagerAction`]s that the caller applies to the app.
//!
//! Two kinds of change, as the window's hint line states:
//! - **Theme-level** actions (Duplicate, Rename, Delete, Import) are written
//!   to disk at once and are not undone by Cancel.
//! - **Color edits** stay in a working copy (`edits`) and preview live; OK
//!   saves them atomically, Cancel discards them.
//!
//! Built-in themes are read-only here, not only in the UI.

use std::collections::BTreeMap;
use std::path::Path;

use syntect::highlighting::ThemeSettings;

use crate::app::domain::settings::{SyntaxTheme, ThemeRef, default_syntax_theme};
use crate::app::services::custom_themes::{
    CustomTheme, EditorColorField, MAX_THEME_NAME_CHARS, ThemeStore, base_colors, read_tm_theme,
};
use crate::ui::theme::{EditorColors, effective_editor_colors};

/// What the caller must do after a theme manager operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ThemeManagerAction {
    /// Show this error; nothing was changed.
    Error(String),
    /// Select this entry in the list.
    Select(ThemeRef),
    /// The custom themes changed: hand [`ThemeManagerModel::custom_themes`]
    /// to the highlighter and refresh every theme list.
    ThemesChanged,
    /// Point the saved light (`is_dark == false`) or dark slot at `theme`.
    SetSlot { is_dark: bool, theme: ThemeRef },
    /// Save `settings.json`.
    SaveSettings,
    /// Show [`ThemeManagerModel::shown`] in the editor again (live preview).
    Retheme,
}

use ThemeManagerAction as Action;

/// One row of the theme list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThemeEntry {
    pub theme: ThemeRef,
    pub name: String,
    /// Built-in: no rename, delete or color edits.
    pub read_only: bool,
    /// "Reset to base" is possible (a custom theme with `based_on`).
    pub can_reset: bool,
}

/// Parse a `#RRGGBB` / `RRGGBB` hex color (any case, surrounding whitespace
/// ignored). Anything else is `None`. The text is checked to be ASCII before
/// it is sliced, so multi-byte input cannot panic.
pub fn parse_hex_color(text: &str) -> Option<(u8, u8, u8)> {
    let text = text.trim();
    let hex = text.strip_prefix('#').unwrap_or(text);
    if hex.len() != 6 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let channel = |i: usize| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok();
    Some((channel(0)?, channel(2)?, channel(4)?))
}

/// `#rrggbb` for a color.
pub fn format_hex_color((r, g, b): (u8, u8, u8)) -> String {
    format!("#{r:02x}{g:02x}{b:02x}")
}

pub struct ThemeManagerModel {
    store: ThemeStore,
    /// Working copy of the editor settings of custom themes with unsaved
    /// color edits, by id.
    edits: BTreeMap<String, ThemeSettings>,
    /// The saved light/dark slots (settings.json).
    light: ThemeRef,
    dark: ThemeRef,
    is_dark: bool,
    /// The theme the editor shows now: usually the current mode's saved
    /// slot, but Settings may be previewing another one.
    shown: ThemeRef,
}

impl ThemeManagerModel {
    /// A manager over `store`, with the saved light/dark slots, the mode,
    /// and the theme the editor shows now.
    pub fn new(
        store: ThemeStore,
        light: ThemeRef,
        dark: ThemeRef,
        is_dark: bool,
        shown: ThemeRef,
    ) -> Self {
        Self {
            store,
            edits: BTreeMap::new(),
            light,
            dark,
            is_dark,
            shown,
        }
    }

    /// The theme the editor shows (the mode default after a delete removed
    /// the one it showed).
    pub fn shown(&self) -> &ThemeRef {
        &self.shown
    }

    /// The list rows: the built-ins first, then the custom themes by name.
    pub fn entries(&self) -> Vec<ThemeEntry> {
        let builtins = SyntaxTheme::all().iter().map(|t| ThemeEntry {
            theme: ThemeRef::Builtin(*t),
            name: t.display_name().to_string(),
            read_only: true,
            can_reset: false,
        });
        let customs = self.store.list().into_iter().map(|t| ThemeEntry {
            theme: t.theme_ref(),
            name: t.name.clone(),
            read_only: false,
            can_reset: t.based_on.is_some(),
        });
        builtins.chain(customs).collect()
    }

    /// The custom themes as they should be shown now (color edits included).
    pub fn custom_themes(&self) -> Vec<CustomTheme> {
        self.store
            .list()
            .into_iter()
            .map(|t| {
                let mut t = t.clone();
                if let Some(settings) = self.edits.get(&t.id) {
                    t.theme.settings = settings.clone();
                }
                t
            })
            .collect()
    }

    /// The slot references, possibly changed by a delete.
    pub fn slots(&self) -> (&ThemeRef, &ThemeRef) {
        (&self.light, &self.dark)
    }

    /// The editor colors `theme` is shown with (color edits included).
    pub fn colors(&self, theme: &ThemeRef) -> Option<EditorColors> {
        match theme {
            ThemeRef::Builtin(t) => Some(base_colors(*t)),
            ThemeRef::Custom(id) => {
                let custom = self.store.get(id)?;
                let mut theme = custom.theme.clone();
                if let Some(settings) = self.edits.get(id) {
                    theme.settings = settings.clone();
                }
                Some(effective_editor_colors(&theme, false))
            }
        }
    }

    /// Whether `id` is the theme the editor shows.
    fn is_active(&self, id: &str) -> bool {
        matches!(&self.shown, ThemeRef::Custom(shown) if shown == id)
    }

    /// The custom theme id `theme` refers to, or an error action for a
    /// built-in (read-only) or a missing theme.
    fn editable_id<'a>(&self, theme: &'a ThemeRef) -> Result<&'a str, Action> {
        match theme {
            ThemeRef::Builtin(_) => Err(Action::Error(
                "Built-in themes are read-only. Duplicate one to change it.".into(),
            )),
            ThemeRef::Custom(id) if self.store.get(id).is_some() => Ok(id),
            ThemeRef::Custom(_) => Err(Action::Error("This theme no longer exists.".into())),
        }
    }

    /// Check a Duplicate/Rename name: not empty or whitespace-only, at most
    /// 64 characters, unique among custom themes (case-insensitive, `except`
    /// itself when renaming). Returns the trimmed name.
    pub fn validate_name(&self, name: &str, except: Option<&str>) -> Result<String, String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("The name cannot be empty.".into());
        }
        if name.chars().count() > MAX_THEME_NAME_CHARS {
            return Err(format!(
                "The name is longer than {MAX_THEME_NAME_CHARS} characters."
            ));
        }
        let lower = name.to_lowercase();
        let taken = self
            .store
            .list()
            .into_iter()
            .any(|t| Some(t.id.as_str()) != except && t.name.to_lowercase() == lower);
        if taken {
            return Err(format!("A custom theme named \"{name}\" already exists."));
        }
        Ok(name.to_string())
    }

    /// `name`, or `name (2)`, `name (3)`… — the first not used by a custom
    /// theme (case-insensitive).
    fn unique_name(&self, name: &str) -> String {
        let free = |candidate: &str| self.validate_name(candidate, None).is_ok();
        if free(name) {
            return name.to_string();
        }
        (2u32..)
            .map(|n| {
                let suffix = format!(" ({n})");
                let room = MAX_THEME_NAME_CHARS.saturating_sub(suffix.chars().count());
                let base: String = name.chars().take(room).collect();
                format!("{}{suffix}", base.trim_end())
            })
            .find(|candidate| free(candidate))
            .unwrap_or_else(|| name.to_string())
    }

    /// Duplicate `source` (built-in or custom) as a new custom theme `name`.
    pub fn duplicate(&mut self, source: &ThemeRef, name: &str) -> Vec<Action> {
        let name = match self.validate_name(name, None) {
            Ok(name) => name,
            Err(e) => return vec![Action::Error(e)],
        };
        // The copy shows what the user sees, unsaved color edits included.
        let edited = match source {
            ThemeRef::Custom(id) => self.edits.get(id).cloned().and_then(|settings| {
                let mut src = self.store.get(id)?.clone();
                src.theme.settings = settings;
                Some(src)
            }),
            ThemeRef::Builtin(_) => None,
        };
        let result = match edited {
            Some(src) => self.store.add(&name, src.based_on, src.theme),
            None => self.store.clone_from(source, &name),
        };
        match result {
            Ok(id) => vec![Action::ThemesChanged, Action::Select(ThemeRef::Custom(id))],
            Err(e) => vec![Action::Error(format!("Could not duplicate the theme: {e}"))],
        }
    }

    /// Rename a custom theme. The id and every reference stay the same.
    pub fn rename(&mut self, theme: &ThemeRef, name: &str) -> Vec<Action> {
        let id = match self.editable_id(theme) {
            Ok(id) => id,
            Err(e) => return vec![e],
        };
        let name = match self.validate_name(name, Some(id)) {
            Ok(name) => name,
            Err(e) => return vec![Action::Error(e)],
        };
        match self.store.rename(id, &name) {
            Ok(()) => vec![Action::ThemesChanged, Action::Select(theme.clone())],
            Err(e) => vec![Action::Error(format!("Could not rename the theme: {e}"))],
        }
    }

    /// Delete a custom theme. Slots that used it fall back to their mode's
    /// default built-in, and the settings are saved, so no reference dangles.
    pub fn delete(&mut self, theme: &ThemeRef) -> Vec<Action> {
        let id = match self.editable_id(theme) {
            Ok(id) => id.to_string(),
            Err(e) => return vec![e],
        };
        if let Err(e) = self.store.delete(&id) {
            return vec![Action::Error(format!("Could not delete the theme: {e}"))];
        }
        self.edits.remove(&id);
        let mut actions = vec![Action::ThemesChanged];
        let mut slot_changed = false;
        for is_dark in [false, true] {
            let slot = if is_dark {
                &mut self.dark
            } else {
                &mut self.light
            };
            if *slot == *theme {
                *slot = default_syntax_theme(is_dark).into();
                actions.push(Action::SetSlot {
                    is_dark,
                    theme: slot.clone(),
                });
                slot_changed = true;
            }
        }
        if slot_changed {
            actions.push(Action::SaveSettings);
        }
        if self.shown == *theme {
            self.shown = default_syntax_theme(self.is_dark).into();
            actions.push(Action::Retheme);
        }
        actions
    }

    /// Import a `.tmTheme` file as a custom theme without a base. Files over
    /// 1 MiB, unreadable or malformed files are rejected and nothing is
    /// created. A name already in use gets " (2)", " (3)"…
    pub fn import(&mut self, path: &Path) -> Vec<Action> {
        let theme = match read_tm_theme(path) {
            Ok(theme) => theme,
            Err(e) => return vec![Action::Error(format!("Could not import the theme: {e}"))],
        };
        let name = theme
            .name
            .clone()
            .filter(|n| !n.trim().is_empty())
            .or_else(|| path.file_stem().map(|s| s.to_string_lossy().into_owned()))
            .unwrap_or_else(|| "Imported theme".to_string());
        let name: String = name.trim().chars().take(MAX_THEME_NAME_CHARS).collect();
        let name = self.unique_name(name.trim());
        match self.store.add(&name, None, theme) {
            Ok(id) => vec![Action::ThemesChanged, Action::Select(ThemeRef::Custom(id))],
            Err(e) => vec![Action::Error(format!("Could not import the theme: {e}"))],
        }
    }

    /// The working settings of custom theme `id` (created on first edit).
    fn working_settings(&mut self, id: &str) -> Option<&mut ThemeSettings> {
        if !self.edits.contains_key(id) {
            let settings = self.store.get(id)?.theme.settings.clone();
            self.edits.insert(id.to_string(), settings);
        }
        self.edits.get_mut(id)
    }

    /// Preview actions after a color edit of `id`: only the active theme
    /// touches the editor.
    fn after_edit(&self, id: &str) -> Vec<Action> {
        if self.is_active(id) {
            vec![Action::ThemesChanged, Action::Retheme]
        } else {
            Vec::new()
        }
    }

    /// Change one editor color of a custom theme (working copy).
    pub fn set_color(
        &mut self,
        theme: &ThemeRef,
        field: EditorColorField,
        rgb: (u8, u8, u8),
    ) -> Vec<Action> {
        let id = match self.editable_id(theme) {
            Ok(id) => id,
            Err(e) => return vec![e],
        };
        let Some(settings) = self.working_settings(id) else {
            return vec![Action::Error("This theme no longer exists.".into())];
        };
        field.set(settings, rgb);
        self.after_edit(id)
    }

    /// Reset one editor color of a custom theme to its base (working copy).
    pub fn reset_color(&mut self, theme: &ThemeRef, field: EditorColorField) -> Vec<Action> {
        let id = match self.editable_id(theme) {
            Ok(id) => id,
            Err(e) => return vec![e],
        };
        let Some(base) = self.store.get(id).and_then(|t| t.based_on) else {
            return vec![Action::Error(
                "This theme has no base theme to reset to.".into(),
            )];
        };
        let Some(settings) = self.working_settings(id) else {
            return vec![Action::Error("This theme no longer exists.".into())];
        };
        field.set(settings, field.of(&base_colors(base)));
        self.after_edit(id)
    }

    /// OK: save every edited theme (atomically, one file each).
    pub fn ok(&mut self) -> Vec<Action> {
        let edited = !self.edits.is_empty();
        let mut actions = Vec::new();
        for (id, settings) in std::mem::take(&mut self.edits) {
            let saved = self
                .store
                .set_settings(&id, settings)
                .and_then(|()| self.store.save(&id));
            if let Err(e) = saved {
                actions.push(Action::Error(format!("Could not save the theme: {e}")));
            }
        }
        if edited {
            actions.push(Action::ThemesChanged);
            actions.push(Action::Retheme);
        }
        actions
    }

    /// Cancel: drop the color edits of this session; the files are untouched.
    /// Theme-level changes (Duplicate, Rename, Delete, Import) stay.
    pub fn cancel(&mut self) -> Vec<Action> {
        let active_edited = self.edits.keys().any(|id| self.is_active(id));
        let edited = !self.edits.is_empty();
        self.edits.clear();
        match (edited, active_edited) {
            (_, true) => vec![Action::ThemesChanged, Action::Retheme],
            (true, false) => vec![Action::ThemesChanged],
            (false, false) => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::color::wcag_contrast;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::{TempDir, tempdir};

    const DARK_DEFAULT: SyntaxTheme = SyntaxTheme::Base16OceanDark;
    const LIGHT_DEFAULT: SyntaxTheme = SyntaxTheme::Base16OceanLight;

    fn fixture(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name)
    }

    /// A model over an empty themes dir, defaults in both slots, dark mode.
    fn model() -> (TempDir, ThemeManagerModel) {
        let dir = tempdir().unwrap();
        let store = ThemeStore::empty(dir.path().to_path_buf());
        let model = ThemeManagerModel::new(
            store,
            LIGHT_DEFAULT.into(),
            DARK_DEFAULT.into(),
            true,
            DARK_DEFAULT.into(),
        );
        (dir, model)
    }

    /// The custom theme selected by `actions`.
    fn selected(actions: &[ThemeManagerAction]) -> ThemeRef {
        actions
            .iter()
            .find_map(|a| match a {
                ThemeManagerAction::Select(t) => Some(t.clone()),
                _ => None,
            })
            .unwrap_or_else(|| panic!("no Select in {actions:?}"))
    }

    fn is_error(actions: &[ThemeManagerAction]) -> bool {
        matches!(actions, [ThemeManagerAction::Error(_)])
    }

    /// File names and contents of a directory, to prove "nothing changed".
    fn snapshot(dir: &Path) -> Vec<(String, Vec<u8>)> {
        let mut files: Vec<(String, Vec<u8>)> = fs::read_dir(dir)
            .unwrap()
            .map(|e| {
                let e = e.unwrap();
                (
                    e.file_name().to_string_lossy().into_owned(),
                    fs::read(e.path()).unwrap(),
                )
            })
            .collect();
        files.sort();
        files
    }

    #[test]
    fn theme_manager_builtins_are_read_only() {
        let (dir, mut m) = model();
        let before = snapshot(dir.path());
        let builtin = ThemeRef::Builtin(SyntaxTheme::SolarizedDark);
        assert!(is_error(&m.rename(&builtin, "Mine")));
        assert!(is_error(&m.delete(&builtin)));
        assert!(is_error(&m.set_color(
            &builtin,
            EditorColorField::Background,
            (1, 2, 3)
        )));
        assert!(is_error(
            &m.reset_color(&builtin, EditorColorField::Background)
        ));
        assert_eq!(snapshot(dir.path()), before);
        assert_eq!(
            m.colors(&builtin),
            Some(base_colors(SyntaxTheme::SolarizedDark))
        );
        assert!(m.entries().iter().filter(|e| e.read_only).count() == SyntaxTheme::all().len());
    }

    #[test]
    fn theme_manager_delete_in_use_resets_slots() {
        let (dir, mut m) = model();
        let theme = selected(&m.duplicate(&DARK_DEFAULT.into(), "Mine"));
        // Both slots use it (a theme can serve light and dark), and it shows.
        m.light = theme.clone();
        m.dark = theme.clone();
        m.shown = theme.clone();
        let actions = m.delete(&theme);
        assert!(actions.contains(&ThemeManagerAction::SetSlot {
            is_dark: false,
            theme: LIGHT_DEFAULT.into()
        }));
        assert!(actions.contains(&ThemeManagerAction::SetSlot {
            is_dark: true,
            theme: DARK_DEFAULT.into()
        }));
        assert!(actions.contains(&ThemeManagerAction::SaveSettings));
        assert!(actions.contains(&ThemeManagerAction::Retheme));
        assert_eq!(m.slots(), (&LIGHT_DEFAULT.into(), &DARK_DEFAULT.into()));
        assert_eq!(m.shown(), &DARK_DEFAULT.into());
        assert!(snapshot(dir.path()).is_empty(), "the file is gone");
        // Deleting a theme no slot uses touches no slot.
        let other = selected(&m.duplicate(&DARK_DEFAULT.into(), "Other"));
        let actions = m.delete(&other);
        assert_eq!(actions, vec![ThemeManagerAction::ThemesChanged]);
    }

    #[test]
    fn theme_manager_rename_validation() {
        let (dir, mut m) = model();
        let first = selected(&m.duplicate(&DARK_DEFAULT.into(), "First"));
        let second = selected(&m.duplicate(&DARK_DEFAULT.into(), "Second"));
        let before = snapshot(dir.path());
        let long = "x".repeat(65);
        for bad in ["", "   ", long.as_str(), "FIRST", " first "] {
            assert!(
                is_error(&m.rename(&second, bad)),
                "{bad:?} must be rejected"
            );
        }
        assert_eq!(snapshot(dir.path()), before, "nothing written");
        // Valid: renaming to its own name in another case, or a new name.
        assert!(!is_error(&m.rename(&second, "SECOND")));
        let actions = m.rename(&second, "  Night  ");
        assert_eq!(selected(&actions), second, "the id is kept");
        let names: Vec<String> = m.entries().into_iter().map(|e| e.name).collect();
        assert!(names.contains(&"Night".to_string()));
        assert!(names.contains(&"First".to_string()));
        let ThemeRef::Custom(id) = &second else {
            unreachable!()
        };
        assert!(dir.path().join(format!("{id}.json")).exists());
        assert_eq!(
            m.validate_name(&"y".repeat(64), None).map(|n| n.len()),
            Ok(64)
        );
        let _ = first;
    }

    #[test]
    fn theme_manager_cancel_discards_color_edits() {
        let (dir, mut m) = model();
        let theme = selected(&m.duplicate(&DARK_DEFAULT.into(), "Mine"));
        m.dark = theme.clone(); // active in dark mode
        m.shown = theme.clone();
        let before_files = snapshot(dir.path());
        let before = m.colors(&theme).unwrap();
        for (field, rgb) in [
            (EditorColorField::Background, (1, 1, 1)),
            (EditorColorField::Selection, (2, 2, 2)),
            (EditorColorField::CurrentLine, (3, 3, 3)),
        ] {
            let actions = m.set_color(&theme, field, rgb);
            assert!(
                actions.contains(&ThemeManagerAction::Retheme),
                "live preview"
            );
        }
        assert_eq!(m.colors(&theme).unwrap().background, (1, 1, 1));
        let actions = m.cancel();
        assert!(actions.contains(&ThemeManagerAction::ThemesChanged));
        assert!(actions.contains(&ThemeManagerAction::Retheme));
        assert_eq!(
            m.colors(&theme).unwrap(),
            before,
            "previous colors restored"
        );
        assert_eq!(snapshot(dir.path()), before_files, "file byte-identical");
    }

    #[test]
    fn theme_manager_edit_inactive_theme_does_not_touch_editor() {
        let (_dir, mut m) = model();
        let theme = selected(&m.duplicate(&DARK_DEFAULT.into(), "Unused"));
        assert!(
            m.set_color(&theme, EditorColorField::Background, (9, 9, 9))
                .is_empty()
        );
        assert_eq!(m.cancel(), vec![ThemeManagerAction::ThemesChanged]);
    }

    #[test]
    fn theme_manager_ok_persists_color_edits() {
        let (dir, mut m) = model();
        let theme = selected(&m.duplicate(&DARK_DEFAULT.into(), "Mine"));
        m.set_color(&theme, EditorColorField::Background, (10, 20, 30));
        m.set_color(&theme, EditorColorField::Caret, (200, 100, 0));
        m.reset_color(&theme, EditorColorField::Caret);
        let actions = m.ok();
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a, ThemeManagerAction::Error(_))),
            "{actions:?}"
        );
        let (store, warnings) = ThemeStore::load(dir.path().to_path_buf());
        assert!(warnings.is_empty());
        let ThemeRef::Custom(id) = &theme else {
            unreachable!()
        };
        let colors = effective_editor_colors(&store.get(id).unwrap().theme, false);
        assert_eq!(colors.background, (10, 20, 30));
        assert_eq!(
            colors.caret,
            base_colors(DARK_DEFAULT).caret,
            "reset to base"
        );
        // A later Cancel does not undo what OK saved.
        assert!(m.cancel().is_empty());
        assert_eq!(m.colors(&theme).unwrap().background, (10, 20, 30));
    }

    #[test]
    fn theme_manager_import_valid() {
        let (_dir, mut m) = model();
        let theme = selected(&m.import(&fixture("import_ok.tmTheme")));
        let entry = m.entries().into_iter().find(|e| e.theme == theme).unwrap();
        assert_eq!(entry.name, "Imported OK");
        assert!(!entry.read_only);
        assert!(!entry.can_reset, "imports have no base");
        let colors = m.colors(&theme).unwrap();
        assert_eq!(colors.background, (0x10, 0x20, 0x30));
        assert_eq!(colors.text, (0xE0, 0xE4, 0xE8));
        assert_eq!(colors.caret, (0xFF, 0xCC, 0x00));
        assert_eq!(colors.selection, (0x33, 0x55, 0x77));
        assert_eq!(colors.current_line, (0x18, 0x28, 0x3A));
        // Importing it again gets a distinct name.
        let again = selected(&m.import(&fixture("import_ok.tmTheme")));
        let name = m
            .entries()
            .into_iter()
            .find(|e| e.theme == again)
            .unwrap()
            .name;
        assert_eq!(name, "Imported OK (2)");
        // Reset is refused for an import, without panicking.
        assert!(is_error(
            &m.reset_color(&theme, EditorColorField::Selection)
        ));
    }

    #[test]
    fn theme_manager_import_rejects_bad_files() {
        let (dir, mut m) = model();
        let work = tempdir().unwrap();
        let huge = work.path().join("huge.tmTheme");
        fs::write(&huge, vec![b' '; 2 * 1024 * 1024]).unwrap();
        let binary = work.path().join("picture.tmTheme");
        fs::write(
            &binary,
            b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR\x00\x00\x00\x10\xff\xfe",
        )
        .unwrap();
        for bad in [
            fixture("import_malformed.tmTheme"),
            huge,
            binary,
            work.path().to_path_buf(),
        ] {
            let actions = m.import(&bad);
            assert!(is_error(&actions), "{bad:?}: {actions:?}");
        }
        assert!(snapshot(dir.path()).is_empty(), "nothing created");
        assert_eq!(m.entries().len(), SyntaxTheme::all().len());
    }

    #[test]
    fn theme_manager_import_missing_colors_uses_fallback() {
        let (_dir, mut m) = model();
        let theme = selected(&m.import(&fixture("import_no_background.tmTheme")));
        let colors = m.colors(&theme).unwrap();
        let ratio = wcag_contrast(colors.text, colors.background);
        assert!(
            ratio >= 4.5,
            "text {:?} on {:?}: {ratio:.2}",
            colors.text,
            colors.background
        );
        assert_eq!(colors.caret, (0, 0, 0), "the colors the file has are kept");
    }

    #[test]
    fn theme_manager_import_missing_foreground_normalized() {
        let (_dir, mut m) = model();
        let theme = selected(&m.import(&fixture("import_no_background.tmTheme")));
        let custom = m
            .custom_themes()
            .into_iter()
            .find(|t| t.theme_ref() == theme)
            .unwrap();
        let effective = effective_editor_colors(&custom.theme, false);
        let default = syntect::highlighting::Highlighter::new(&custom.theme).get_default();
        assert_eq!(
            (
                default.foreground.r,
                default.foreground.g,
                default.foreground.b
            ),
            effective.text
        );
        assert_eq!(
            (
                default.background.r,
                default.background.g,
                default.background.b
            ),
            effective.background
        );
    }

    #[test]
    fn theme_manager_hex_color_parse() {
        assert_eq!(parse_hex_color("#1e1e1e"), Some((0x1e, 0x1e, 0x1e)));
        assert_eq!(parse_hex_color("1E1E1E"), Some((0x1e, 0x1e, 0x1e)));
        assert_eq!(parse_hex_color("  #AbCdEf  "), Some((0xab, 0xcd, 0xef)));
        for bad in [
            "",
            "#",
            "#12345",
            "#1234567",
            "#gggggg",
            "rgb(1,2,3)",
            "#ééé",
            "#12345€",
            "##123456",
            "+12345",
        ] {
            assert_eq!(parse_hex_color(bad), None, "{bad:?}");
        }
        assert_eq!(format_hex_color((0x0a, 0xbc, 0xff)), "#0abcff");
        assert_eq!(
            parse_hex_color(&format_hex_color((1, 2, 3))),
            Some((1, 2, 3))
        );
    }

    #[test]
    fn theme_manager_duplicate_keeps_unsaved_edits_and_validates_name() {
        let (_dir, mut m) = model();
        let theme = selected(&m.duplicate(&DARK_DEFAULT.into(), "Mine"));
        m.set_color(&theme, EditorColorField::Background, (7, 7, 7));
        let copy = selected(&m.duplicate(&theme, "Copy"));
        assert_eq!(m.colors(&copy).unwrap().background, (7, 7, 7));
        assert!(is_error(&m.duplicate(&theme, "mine")), "duplicate name");
        assert!(is_error(&m.duplicate(&theme, "  ")), "empty name");
    }
}
