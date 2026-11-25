//! Custom editor themes (T0079).
//!
//! A custom theme is a syntect [`Theme`] — the same type the built-in themes
//! are — persisted with serde as `<config>/ferrispad/themes/<id>.json`:
//!
//! ```json
//! { "format": 1, "name": "My Dark", "based_on": "Base16OceanDark", "theme": { ... } }
//! ```
//!
//! - `based_on` names the built-in it was cloned from (its settings name), so
//!   a single color can be reset to the original; `null` for imports.
//! - `format` allows a future migration.
//!
//! Themes are loaded once at startup (no directory watching: 0% CPU idle).
//! Theme files can be shared and hand-edited, so loading treats them as
//! untrusted: a bad file is skipped with a warning, never a panic.
//! Several instances may edit the same theme file: the last write wins.

use std::collections::BTreeMap;
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use syntect::highlighting::{Color, Theme, ThemeSet, ThemeSettings};

use crate::app::domain::settings::{
    CUSTOM_THEME_ID_MAX_LEN, SyntaxTheme, ThemeRef, is_valid_custom_theme_id,
};
use crate::ui::theme::{EditorColors, derived_editor_colors, effective_editor_colors};

use super::scope_atoms::{ScopeAtomBudget, with_process_atom_budget};

/// The only theme file format this build reads and writes.
pub const THEME_FILE_FORMAT: u32 = 1;

/// Theme files larger than this are skipped without being read.
pub const MAX_THEME_FILE_BYTES: u64 = 1024 * 1024;

/// Display names are cut to this many characters.
pub const MAX_THEME_NAME_CHARS: usize = 64;

/// Display name used when a name is empty or whitespace-only.
const UNNAMED_THEME: &str = "Custom theme";

/// Slug used for the id when a name has no ASCII letter or digit.
const FALLBACK_SLUG: &str = "theme";

/// Longest slug taken from a name, leaving room for a `-<n>` suffix inside
/// [`CUSTOM_THEME_ID_MAX_LEN`].
const MAX_SLUG_LEN: usize = CUSTOM_THEME_ID_MAX_LEN - 8;

/// Every string in a JSON value, depth-first (bounded by serde_json's own
/// recursion limit when the value was parsed).
fn json_strings<'a>(value: &'a serde_json::Value, out: &mut Vec<&'a str>) {
    match value {
        serde_json::Value::String(s) => out.push(s),
        serde_json::Value::Array(items) => items.iter().for_each(|v| json_strings(v, out)),
        serde_json::Value::Object(map) => map.values().for_each(|v| json_strings(v, out)),
        _ => {}
    }
}

/// Give a theme the foreground and background its editor colors use when it
/// has none: syntect paints unscoped text with `settings.foreground`, or
/// black when it is missing, whatever the background.
pub fn fill_default_colors(theme: &mut Theme) {
    let colors = effective_editor_colors(theme, false);
    let s = &mut theme.settings;
    s.foreground.get_or_insert(opaque(colors.text));
    s.background.get_or_insert(opaque(colors.background));
}

/// The on-disk shape of a theme file.
#[derive(Debug, Serialize, Deserialize)]
struct ThemeFile {
    format: u32,
    name: String,
    based_on: Option<String>,
    theme: Theme,
}

/// A loaded custom theme.
#[derive(Debug, Clone, PartialEq)]
pub struct CustomTheme {
    /// File-name-safe id, fixed at creation (`t-[a-z0-9-]{1,64}`).
    pub id: String,
    /// Free-text display name; renaming never changes the id.
    pub name: String,
    /// The built-in this theme was cloned from, for "reset to base".
    pub based_on: Option<SyntaxTheme>,
    pub theme: Theme,
}

impl CustomTheme {
    /// The reference stored in `settings.json` for this theme.
    pub fn theme_ref(&self) -> ThemeRef {
        ThemeRef::Custom(self.id.clone())
    }
}

/// One editable editor color of a custom theme, and the TextMate
/// `theme.settings` field that stores it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorColorField {
    Background,
    Text,
    Caret,
    Selection,
    LineNumberFg,
    LineNumberBg,
    CurrentLine,
}

impl EditorColorField {
    pub fn all() -> &'static [Self] {
        &[
            Self::Background,
            Self::Text,
            Self::Caret,
            Self::Selection,
            Self::LineNumberFg,
            Self::LineNumberBg,
            Self::CurrentLine,
        ]
    }

    /// This field's value in `colors`.
    pub fn of(self, colors: &EditorColors) -> (u8, u8, u8) {
        match self {
            Self::Background => colors.background,
            Self::Text => colors.text,
            Self::Caret => colors.caret,
            Self::Selection => colors.selection,
            Self::LineNumberFg => colors.line_number_fg,
            Self::LineNumberBg => colors.line_number_bg,
            Self::CurrentLine => colors.current_line,
        }
    }

    /// Store `rgb` (opaque) as this field of `settings`.
    pub fn set(self, settings: &mut ThemeSettings, rgb: (u8, u8, u8)) {
        *self.slot(settings) = Some(opaque(rgb));
    }

    fn slot(self, settings: &mut ThemeSettings) -> &mut Option<Color> {
        match self {
            Self::Background => &mut settings.background,
            Self::Text => &mut settings.foreground,
            Self::Caret => &mut settings.caret,
            Self::Selection => &mut settings.selection,
            Self::LineNumberFg => &mut settings.gutter_foreground,
            Self::LineNumberBg => &mut settings.gutter,
            Self::CurrentLine => &mut settings.line_highlight,
        }
    }
}

/// Why a theme operation failed.
#[derive(Debug)]
pub enum ThemeError {
    /// No custom theme with this id is loaded.
    NotFound(String),
    /// The theme has no `based_on`, so there is nothing to reset to.
    NoBase,
    /// The source theme of a clone does not exist.
    UnknownSource(String),
    /// The id or the path it maps to is not inside the themes directory.
    UnsafePath(String),
    Io(io::Error),
    Json(serde_json::Error),
}

impl fmt::Display for ThemeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound(id) => write!(f, "custom theme '{id}' not found"),
            Self::NoBase => write!(f, "this theme has no base theme to reset to"),
            Self::UnknownSource(key) => write!(f, "theme '{key}' not found"),
            Self::UnsafePath(what) => write!(f, "refusing theme path outside themes/: {what}"),
            Self::Io(e) => write!(f, "theme file error: {e}"),
            Self::Json(e) => write!(f, "theme file format error: {e}"),
        }
    }
}

impl std::error::Error for ThemeError {}

impl From<io::Error> for ThemeError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<serde_json::Error> for ThemeError {
    fn from(e: serde_json::Error) -> Self {
        Self::Json(e)
    }
}

/// `<config>/ferrispad/themes`.
pub fn themes_dir() -> PathBuf {
    let mut path = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
    path.push("ferrispad");
    path.push("themes");
    path
}

/// The display name stored for `name`: trimmed, at most
/// [`MAX_THEME_NAME_CHARS`] characters, never empty.
pub fn normalize_name(name: &str) -> String {
    let trimmed: String = name.trim().chars().take(MAX_THEME_NAME_CHARS).collect();
    let trimmed = trimmed.trim();
    if trimmed.is_empty() {
        UNNAMED_THEME.to_string()
    } else {
        trimmed.to_string()
    }
}

/// `[a-z0-9-]` slug of `name`: ASCII letters/digits lowercased, every other
/// run of characters one `-`, no leading/trailing `-`, never empty.
fn slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            if out.len() >= MAX_SLUG_LEN {
                break;
            }
            out.push(c.to_ascii_lowercase());
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    let out = out.trim_end_matches('-');
    if out.is_empty() {
        FALLBACK_SLUG.to_string()
    } else {
        out.to_string()
    }
}

fn opaque((r, g, b): (u8, u8, u8)) -> Color {
    Color { r, g, b, a: 0xFF }
}

/// The colors "reset to base" restores for a theme cloned from `base`: the
/// base built-in's effective editor colors.
pub fn base_colors(base: SyntaxTheme) -> EditorColors {
    derived_editor_colors(base.background(), base.foreground())
}

/// The loaded custom themes and the directory they live in.
pub struct ThemeStore {
    dir: PathBuf,
    themes: BTreeMap<String, CustomTheme>,
}

impl ThemeStore {
    /// An empty store for `dir` (nothing read).
    pub fn empty(dir: PathBuf) -> Self {
        Self {
            dir,
            themes: BTreeMap::new(),
        }
    }

    /// Load every valid theme file in `dir`. Returns the store and one
    /// warning per skipped file (bad JSON, wrong types, unsupported format,
    /// over 1 MiB, not a regular file, or an unsafe file name). Files without
    /// a `.json` extension (including leftover `.tmp` files) are ignored.
    /// A missing directory is simply empty.
    pub fn load(dir: PathBuf) -> (Self, Vec<String>) {
        with_process_atom_budget(|budget| Self::load_with_budget(dir, budget))
    }

    /// [`Self::load`] against an explicit scope-atom budget.
    pub fn load_with_budget(dir: PathBuf, budget: &mut ScopeAtomBudget) -> (Self, Vec<String>) {
        let mut store = Self::empty(dir);
        let mut warnings = Vec::new();
        let Ok(entries) = fs::read_dir(&store.dir) else {
            return (store, warnings);
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            match load_theme_file(&path, budget) {
                Ok(theme) => {
                    store.themes.insert(theme.id.clone(), theme);
                }
                Err(reason) => {
                    warnings.push(format!("[theme] skipping {}: {reason}", path.display()))
                }
            }
        }
        (store, warnings)
    }

    /// The themes, sorted by name (case-insensitive), then id.
    pub fn list(&self) -> Vec<&CustomTheme> {
        let mut list: Vec<&CustomTheme> = self.themes.values().collect();
        list.sort_by(|a, b| (a.name.to_lowercase(), &a.id).cmp(&(b.name.to_lowercase(), &b.id)));
        list
    }

    pub fn get(&self, id: &str) -> Option<&CustomTheme> {
        self.themes.get(id)
    }

    /// All themes, consuming the store.
    pub fn into_themes(self) -> Vec<CustomTheme> {
        self.themes.into_values().collect()
    }

    /// Create a custom theme named `name` as a copy of `source` (built-in or
    /// custom), save it, and return its id. Every editor color field is
    /// filled with the source's *effective* color, so the clone looks exactly
    /// like what the user was seeing; syntax scopes are copied untouched.
    pub fn clone_from(&mut self, source: &ThemeRef, name: &str) -> Result<String, ThemeError> {
        let (mut theme, colors, based_on) = match source {
            ThemeRef::Builtin(builtin) => {
                let mut set = ThemeSet::load_defaults();
                let theme = set
                    .themes
                    .remove(builtin.theme_key())
                    .ok_or_else(|| ThemeError::UnknownSource(builtin.theme_key().to_string()))?;
                // The same colors "reset to base" restores.
                let colors = derived_editor_colors(builtin.background(), builtin.foreground());
                (theme, colors, Some(*builtin))
            }
            ThemeRef::Custom(id) => {
                let src = self
                    .themes
                    .get(id)
                    .ok_or_else(|| ThemeError::UnknownSource(id.clone()))?;
                let colors = effective_editor_colors(&src.theme, false);
                (src.theme.clone(), colors, src.based_on)
            }
        };
        for &field in EditorColorField::all() {
            field.set(&mut theme.settings, field.of(&colors));
        }
        self.add(name, based_on, theme)
    }

    /// Add `theme` as a new custom theme named `name`, save it, and return
    /// its fresh id. Nothing is kept if saving fails.
    pub fn add(
        &mut self,
        name: &str,
        based_on: Option<SyntaxTheme>,
        mut theme: Theme,
    ) -> Result<String, ThemeError> {
        let name = normalize_name(name);
        theme.name = Some(name.clone());
        fill_default_colors(&mut theme);
        let id = self.new_id(&name);
        self.themes.insert(
            id.clone(),
            CustomTheme {
                id: id.clone(),
                name,
                based_on,
                theme,
            },
        );
        if let Err(e) = self.save(&id) {
            self.themes.remove(&id);
            return Err(e);
        }
        Ok(id)
    }

    /// Replace the editor/UI settings of theme `id` in memory (call
    /// [`Self::save`] to keep them). Name and scopes are untouched.
    pub fn set_settings(&mut self, id: &str, settings: ThemeSettings) -> Result<(), ThemeError> {
        let theme = self
            .themes
            .get_mut(id)
            .ok_or_else(|| ThemeError::NotFound(id.to_string()))?;
        theme.theme.settings = settings;
        Ok(())
    }

    /// A fresh, unique id for `name` (unique among loaded themes and files
    /// already in the directory).
    fn new_id(&self, name: &str) -> String {
        let base = format!("t-{}", slug(name));
        let taken =
            |id: &str| self.themes.contains_key(id) || self.dir.join(format!("{id}.json")).exists();
        if !taken(&base) {
            return base;
        }
        (2u32..)
            .map(|n| format!("{base}-{n}"))
            .find(|id| !taken(id))
            .unwrap_or(base)
    }

    /// `<dir>/<id>.json`, refusing anything that would not sit directly in
    /// the (canonicalized) themes directory. Fails closed.
    fn theme_path(&self, id: &str) -> Result<PathBuf, ThemeError> {
        if !is_valid_custom_theme_id(id) {
            return Err(ThemeError::UnsafePath(id.to_string()));
        }
        let dir = self.dir.canonicalize()?;
        let path = dir.join(format!("{id}.json"));
        if path.parent() != Some(dir.as_path()) {
            return Err(ThemeError::UnsafePath(path.display().to_string()));
        }
        Ok(path)
    }

    /// Write theme `id` to disk atomically (temp file + rename).
    pub fn save(&self, id: &str) -> Result<(), ThemeError> {
        let theme = self
            .themes
            .get(id)
            .ok_or_else(|| ThemeError::NotFound(id.to_string()))?;
        fs::create_dir_all(&self.dir)?;
        let path = self.theme_path(id)?;
        let file = ThemeFile {
            format: THEME_FILE_FORMAT,
            name: theme.name.clone(),
            based_on: theme.based_on.map(|b| b.setting_name().to_string()),
            theme: theme.theme.clone(),
        };
        let json = serde_json::to_vec_pretty(&file)?;

        // `.tmp` is not `.json`, so a leftover temp file is never loaded. The
        // pid keeps two instances saving the same theme from sharing (and
        // deleting) one temp file: the last complete write wins.
        let tmp = path.with_extension(format!("json.{}.tmp", std::process::id()));
        match fs::remove_file(&tmp) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e.into()),
            _ => {}
        }
        // `create_new` refuses an existing path, symlinks included.
        let mut out = OpenOptions::new().write(true).create_new(true).open(&tmp)?;
        let written = out.write_all(&json).and_then(|()| out.sync_all());
        drop(out);
        if let Err(e) = written.and_then(|()| fs::rename(&tmp, &path)) {
            let _ = fs::remove_file(&tmp);
            return Err(e.into());
        }
        Ok(())
    }

    /// Change the display name of `id` and save it. The id, the file name
    /// and every `custom:<id>` reference stay the same.
    pub fn rename(&mut self, id: &str, name: &str) -> Result<(), ThemeError> {
        let theme = self
            .themes
            .get_mut(id)
            .ok_or_else(|| ThemeError::NotFound(id.to_string()))?;
        let name = normalize_name(name);
        theme.theme.name = Some(name.clone());
        theme.name = name;
        self.save(id)
    }

    /// Remove theme `id` from the store and delete its file. Refuses a path
    /// that is not a regular file directly inside the themes directory.
    pub fn delete(&mut self, id: &str) -> Result<(), ThemeError> {
        if !self.themes.contains_key(id) {
            return Err(ThemeError::NotFound(id.to_string()));
        }
        let path = self.theme_path(id)?;
        match fs::symlink_metadata(&path) {
            Ok(meta) if meta.is_file() => fs::remove_file(&path)?,
            Ok(_) => return Err(ThemeError::UnsafePath(path.display().to_string())),
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
        self.themes.remove(id);
        Ok(())
    }

    /// Set one editor color of `id` in memory (call [`Self::save`] to keep it).
    pub fn set_color(
        &mut self,
        id: &str,
        field: EditorColorField,
        rgb: (u8, u8, u8),
    ) -> Result<(), ThemeError> {
        let theme = self
            .themes
            .get_mut(id)
            .ok_or_else(|| ThemeError::NotFound(id.to_string()))?;
        field.set(&mut theme.theme.settings, rgb);
        Ok(())
    }

    /// Reset one editor color of `id` to its `based_on` theme's effective
    /// color, in memory. Fails with [`ThemeError::NoBase`] for a theme that
    /// has no base (an import).
    pub fn reset_color(&mut self, id: &str, field: EditorColorField) -> Result<(), ThemeError> {
        let theme = self
            .themes
            .get_mut(id)
            .ok_or_else(|| ThemeError::NotFound(id.to_string()))?;
        let base = theme.based_on.ok_or(ThemeError::NoBase)?;
        field.set(&mut theme.theme.settings, field.of(&base_colors(base)));
        Ok(())
    }
}

/// Deepest plist nesting an imported `.tmTheme` may have. A real theme
/// needs 4–6 (plist → dict → settings array → rule dict → settings dict).
/// Anything deeper is refused before a `plist::Value` tree is built:
/// dropping (or walking) a tree nested tens of thousands of levels deep
/// overflows the stack, and a 1 MiB file can nest 65 000 arrays.
const MAX_PLIST_DEPTH: usize = 32;

/// Most plist events (values, collection starts and ends) an imported
/// `.tmTheme` may have, so a flat but huge file cannot make the import do
/// unbounded work either. Real themes have a few thousand.
const MAX_PLIST_EVENTS: usize = 200_000;

// The shape check runs through plist's stable serde API: plist reads events
// iteratively, and serde descends one level per nested value only as far
// as this visitor allows, so the check needs a bounded stack and builds no
// tree. (plist's raw event reader is behind an unstable feature.) serde's
// `Deserialize` takes no state, so the counters are thread-local and reset
// for every check.
thread_local! {
    static PLIST_DEPTH: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    static PLIST_EVENTS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// A plist value whose shape is checked and then discarded.
struct PlistShape;

impl PlistShape {
    fn event<E: serde::de::Error>() -> Result<(), E> {
        let events = PLIST_EVENTS.with(|n| {
            n.set(n.get() + 1);
            n.get()
        });
        if events > MAX_PLIST_EVENTS {
            return Err(E::custom(format!(
                "the file has more than {MAX_PLIST_EVENTS} entries"
            )));
        }
        Ok(())
    }

    /// Enter a collection: one more level, refused past the limit.
    fn enter<E: serde::de::Error>() -> Result<(), E> {
        Self::event()?;
        let depth = PLIST_DEPTH.with(|d| {
            d.set(d.get() + 1);
            d.get()
        });
        if depth > MAX_PLIST_DEPTH {
            return Err(E::custom("the file is nested too deeply"));
        }
        Ok(())
    }

    fn leave() {
        PLIST_DEPTH.with(|d| d.set(d.get().saturating_sub(1)));
    }
}

impl<'de> Deserialize<'de> for PlistShape {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(PlistShapeVisitor)
    }
}

struct PlistShapeVisitor;

impl<'de> serde::de::Visitor<'de> for PlistShapeVisitor {
    type Value = PlistShape;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a property list")
    }

    fn visit_bool<E: serde::de::Error>(self, _: bool) -> Result<PlistShape, E> {
        PlistShape::event().map(|()| PlistShape)
    }
    fn visit_i64<E: serde::de::Error>(self, _: i64) -> Result<PlistShape, E> {
        PlistShape::event().map(|()| PlistShape)
    }
    fn visit_u64<E: serde::de::Error>(self, _: u64) -> Result<PlistShape, E> {
        PlistShape::event().map(|()| PlistShape)
    }
    fn visit_f64<E: serde::de::Error>(self, _: f64) -> Result<PlistShape, E> {
        PlistShape::event().map(|()| PlistShape)
    }
    fn visit_str<E: serde::de::Error>(self, _: &str) -> Result<PlistShape, E> {
        PlistShape::event().map(|()| PlistShape)
    }
    fn visit_bytes<E: serde::de::Error>(self, _: &[u8]) -> Result<PlistShape, E> {
        PlistShape::event().map(|()| PlistShape)
    }
    fn visit_unit<E: serde::de::Error>(self) -> Result<PlistShape, E> {
        PlistShape::event().map(|()| PlistShape)
    }

    fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut seq: A) -> Result<PlistShape, A::Error> {
        PlistShape::enter()?;
        while seq.next_element::<PlistShape>()?.is_some() {}
        PlistShape::leave();
        Ok(PlistShape)
    }

    fn visit_map<A: serde::de::MapAccess<'de>>(self, mut map: A) -> Result<PlistShape, A::Error> {
        PlistShape::enter()?;
        while map.next_key::<PlistShape>()?.is_some() {
            map.next_value::<PlistShape>()?;
        }
        PlistShape::leave();
        Ok(PlistShape)
    }
}

/// Check a plist's shape before any tree is built (XML and binary alike):
/// nesting at most [`MAX_PLIST_DEPTH`], at most [`MAX_PLIST_EVENTS`] values.
/// Stops at the first violation or parse error.
fn check_plist_shape(bytes: &[u8]) -> Result<(), String> {
    PLIST_DEPTH.with(|d| d.set(0));
    PLIST_EVENTS.with(|n| n.set(0));
    plist::from_bytes::<PlistShape>(bytes)
        .map(|_| ())
        .map_err(|e| format!("not a valid .tmTheme file ({e})"))
}

/// Every string stored under a `scope` key anywhere in `value` — the only
/// place syntect reads scope selectors from a `.tmTheme` (each one a
/// selector text, see `scope_atoms::tm_selector_scopes`).
fn plist_scope_strings<'a>(
    value: &'a plist::Value,
    depth: usize,
    out: &mut Vec<&'a str>,
) -> Result<(), String> {
    if depth > MAX_PLIST_DEPTH {
        return Err("the file is nested too deeply".into());
    }
    match value {
        plist::Value::Dictionary(dict) => {
            for (key, item) in dict {
                if let (true, plist::Value::String(scope)) = (key == "scope", item) {
                    out.push(scope);
                } else {
                    plist_scope_strings(item, depth + 1, out)?;
                }
            }
        }
        plist::Value::Array(items) => {
            for item in items {
                plist_scope_strings(item, depth + 1, out)?;
            }
        }
        _ => {}
    }
    Ok(())
}

/// Read a `.tmTheme` file for import, treating it as untrusted. In order:
/// 1. over 1 MiB (checked before reading, and the read is capped) or not a
///    regular file → error;
/// 2. streaming shape check ([`check_plist_shape`]): depth and event count,
///    before any tree exists;
/// 3. parsed with `plist`, and its scopes checked against the process-wide
///    [`ScopeAtomBudget`] before syntect sees them;
/// 4. parsed by syntect; a missing foreground/background is filled in.
pub fn read_tm_theme(path: &Path) -> Result<Theme, String> {
    let meta = fs::metadata(path).map_err(|e| e.to_string())?;
    if !meta.is_file() {
        return Err("not a regular file".into());
    }
    let too_big = || {
        format!(
            "the file is larger than {} KiB",
            MAX_THEME_FILE_BYTES / 1024
        )
    };
    if meta.len() > MAX_THEME_FILE_BYTES {
        return Err(too_big());
    }
    let mut bytes = Vec::new();
    File::open(path)
        .and_then(|f| f.take(MAX_THEME_FILE_BYTES + 1).read_to_end(&mut bytes))
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_THEME_FILE_BYTES {
        return Err(too_big());
    }
    check_plist_shape(&bytes)?;
    let plist = plist::Value::from_reader(io::Cursor::new(&bytes))
        .map_err(|e| format!("not a valid .tmTheme file ({e})"))?;
    let mut scopes = Vec::new();
    plist_scope_strings(&plist, 0, &mut scopes)?;
    with_process_atom_budget(|budget| budget.admit_tm_selectors(scopes))?;
    let mut theme = ThemeSet::load_from_reader(&mut io::Cursor::new(bytes))
        .map_err(|e| format!("not a valid .tmTheme file ({e})"))?;
    fill_default_colors(&mut theme);
    Ok(theme)
}

/// Read and validate one theme file; the error is a human-readable reason.
fn load_theme_file(path: &Path, budget: &mut ScopeAtomBudget) -> Result<CustomTheme, String> {
    let id = path
        .file_stem()
        .and_then(|s| s.to_str())
        .filter(|id| is_valid_custom_theme_id(id))
        .ok_or("file name is not a valid theme id")?
        .to_string();
    let meta = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if !meta.is_file() {
        return Err("not a regular file".into());
    }
    if meta.len() > MAX_THEME_FILE_BYTES {
        return Err(format!("larger than {MAX_THEME_FILE_BYTES} bytes"));
    }
    let mut bytes = Vec::new();
    File::open(path)
        .and_then(|f| f.take(MAX_THEME_FILE_BYTES + 1).read_to_end(&mut bytes))
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_THEME_FILE_BYTES {
        return Err(format!("larger than {MAX_THEME_FILE_BYTES} bytes"));
    }
    // Plain JSON first: syntect must not see the scopes before they pass
    // the atom budget (see `scope_atoms`). Every string under
    // `theme.scopes` is one syntect `Scope`.
    let value: serde_json::Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    if value.get("format").and_then(serde_json::Value::as_u64) != Some(u64::from(THEME_FILE_FORMAT))
    {
        return Err("unsupported or missing format".into());
    }
    let mut selectors = Vec::new();
    if let Some(scopes) = value.get("theme").and_then(|t| t.get("scopes")) {
        json_strings(scopes, &mut selectors);
    }
    budget.admit_scopes(selectors)?;
    let mut file: ThemeFile = serde_json::from_value(value).map_err(|e| e.to_string())?;
    fill_default_colors(&mut file.theme);
    Ok(CustomTheme {
        id,
        name: normalize_name(&file.name),
        based_on: file
            .based_on
            .as_deref()
            .and_then(SyntaxTheme::from_setting_name),
        theme: file.theme,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::color::wcag_contrast;
    use tempfile::tempdir;

    fn builtin(theme: SyntaxTheme) -> Theme {
        ThemeSet::load_defaults()
            .themes
            .remove(theme.theme_key())
            .expect("built-in theme")
    }

    /// Names of the entries in `dir`, sorted.
    fn dir_entries(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    fn rgb(c: Option<Color>) -> Option<(u8, u8, u8)> {
        c.map(|c| (c.r, c.g, c.b))
    }

    #[test]
    fn custom_theme_roundtrip() {
        let dir = tempdir().unwrap();
        let mut store = ThemeStore::empty(dir.path().to_path_buf());
        let source = ThemeRef::Builtin(SyntaxTheme::SolarizedDark);
        let id = store.clone_from(&source, "My Solarized").unwrap();
        store
            .set_color(&id, EditorColorField::Selection, (1, 2, 3))
            .unwrap();
        store
            .set_color(&id, EditorColorField::LineNumberBg, (4, 5, 6))
            .unwrap();
        store.save(&id).unwrap();

        let (loaded, warnings) = ThemeStore::load(dir.path().to_path_buf());
        assert!(warnings.is_empty(), "{warnings:?}");
        let original = store.get(&id).unwrap();
        let reloaded = loaded.get(&id).expect("theme reloads");
        assert_eq!(reloaded, original);
        assert_eq!(reloaded.name, "My Solarized");
        assert_eq!(reloaded.based_on, Some(SyntaxTheme::SolarizedDark));
        assert_eq!(rgb(reloaded.theme.settings.selection), Some((1, 2, 3)));
        assert_eq!(rgb(reloaded.theme.settings.gutter), Some((4, 5, 6)));
        // Syntax scopes are carried over untouched.
        assert_eq!(
            reloaded.theme.scopes,
            builtin(SyntaxTheme::SolarizedDark).scopes
        );
    }

    /// The committed v1 file must keep loading: a syntect upgrade that changes
    /// `Theme`'s serde shape fails here instead of silently dropping themes.
    #[test]
    fn custom_theme_fixture_still_loads() {
        let fixture =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/custom_theme_v1.json");
        let dir = tempdir().unwrap();
        fs::copy(&fixture, dir.path().join("t-fixture-v1.json")).unwrap();
        let (store, warnings) = ThemeStore::load(dir.path().to_path_buf());
        assert!(warnings.is_empty(), "{warnings:?}");
        let theme = store.get("t-fixture-v1").expect("fixture loads");
        assert_eq!(theme.name, "Fixture v1");
        assert_eq!(theme.based_on, Some(SyntaxTheme::Base16OceanDark));
        let s = &theme.theme.settings;
        assert_eq!(rgb(s.background), Some((16, 24, 32)));
        assert_eq!(rgb(s.foreground), Some((192, 197, 206)));
        assert_eq!(rgb(s.selection), Some((200, 80, 40)));
        assert_eq!(rgb(s.gutter), Some((24, 32, 40)));
        assert_eq!(theme.theme.scopes.len(), 3);
        let colors = effective_editor_colors(&theme.theme, false);
        assert_eq!(colors.background, (16, 24, 32));
        assert_eq!(colors.selection, (200, 80, 40));
    }

    #[test]
    fn custom_theme_bad_files_are_skipped() {
        let dir = tempdir().unwrap();
        let mut store = ThemeStore::empty(dir.path().to_path_buf());
        let good = store
            .clone_from(&ThemeRef::Builtin(SyntaxTheme::Base16OceanDark), "Good")
            .unwrap();
        let p = |name: &str| dir.path().join(name);
        fs::write(p("t-garbage.json"), b"\x00\xffnot json{{{").unwrap();
        fs::write(
            p("t-wrong-type.json"),
            br#"{"format": 1, "name": 5, "based_on": null, "theme": {}}"#,
        )
        .unwrap();
        fs::write(
            p("t-future.json"),
            fs::read_to_string(p(&format!("{good}.json")))
                .unwrap()
                .replacen("\"format\": 1", "\"format\": 99", 1),
        )
        .unwrap();
        // 2 MiB of otherwise valid-looking JSON whitespace.
        let mut big = vec![b' '; 2 * 1024 * 1024];
        big.extend_from_slice(b"{}");
        fs::write(p("t-huge.json"), big).unwrap();
        fs::write(p("notes.txt"), b"hello").unwrap();
        fs::write(p("t-leftover.json.tmp"), b"partial").unwrap();
        fs::create_dir(p("t-dir.json")).unwrap();
        fs::write(p("Bad Name.json"), b"{}").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(p(&format!("{good}.json")), p("t-link.json")).unwrap();

        let (loaded, warnings) = ThemeStore::load(dir.path().to_path_buf());
        let ids: Vec<&str> = loaded.list().iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids, vec![good.as_str()], "only the valid theme loads");
        // One warning per bad .json entry (garbage, wrong type, future
        // format, huge, directory, bad name, + symlink on unix).
        let expected = if cfg!(unix) { 7 } else { 6 };
        assert_eq!(warnings.len(), expected, "{warnings:#?}");
        assert!(
            warnings
                .iter()
                .any(|w| w.contains("t-huge.json") && w.contains("larger"))
        );
    }

    #[test]
    fn custom_theme_ids_are_safe() {
        let dir = tempdir().unwrap();
        let mut store = ThemeStore::empty(dir.path().to_path_buf());
        let long = "x".repeat(500);
        let names = [
            "../../x",
            "a/b",
            "a\\b",
            "CON",
            "",
            "   ",
            long.as_str(),
            "🦀🦀",
            "Same",
            "Same",
        ];
        let source = ThemeRef::Builtin(SyntaxTheme::InspiredGitHub);
        let mut ids = Vec::new();
        for name in names {
            let id = store.clone_from(&source, name).unwrap();
            assert!(is_valid_custom_theme_id(&id), "{name:?} -> {id:?}");
            let body = id.strip_prefix("t-").unwrap();
            assert!((1..=64).contains(&body.len()), "{id}");
            assert!(
                body.bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
            );
            ids.push(id);
        }
        let mut unique = ids.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), ids.len(), "ids are unique: {ids:?}");
        let mut expected: Vec<String> = ids.iter().map(|id| format!("{id}.json")).collect();
        expected.sort();
        assert_eq!(dir_entries(dir.path()), expected, "nothing written outside");
        assert!(!dir.path().parent().unwrap().join("x.json").exists());
        // Display names are free text, normalized.
        assert_eq!(store.get(&ids[0]).unwrap().name, "../../x");
        assert_eq!(store.get(&ids[4]).unwrap().name, "Custom theme");
        assert_eq!(
            store.get(&ids[6]).unwrap().name.chars().count(),
            MAX_THEME_NAME_CHARS
        );
    }

    #[test]
    fn custom_theme_save_and_delete_refuse_unsafe_ids() {
        let dir = tempdir().unwrap();
        let mut store = ThemeStore::empty(dir.path().to_path_buf());
        let id = store
            .clone_from(&ThemeRef::Builtin(SyntaxTheme::Base16OceanLight), "Ok")
            .unwrap();
        // Plant an entry under an unsafe key (as a corrupted state would).
        let mut evil = store.get(&id).unwrap().clone();
        evil.id = "../evil".into();
        store.themes.insert(evil.id.clone(), evil);
        assert!(matches!(
            store.save("../evil"),
            Err(ThemeError::UnsafePath(_))
        ));
        assert!(matches!(
            store.delete("../evil"),
            Err(ThemeError::UnsafePath(_))
        ));
        assert!(!dir.path().parent().unwrap().join("evil.json").exists());
        store.delete(&id).unwrap();
        assert!(dir_entries(dir.path()).is_empty());
    }

    #[test]
    fn custom_theme_rename_keeps_id() {
        let dir = tempdir().unwrap();
        let mut store = ThemeStore::empty(dir.path().to_path_buf());
        let id = store
            .clone_from(&ThemeRef::Builtin(SyntaxTheme::Base16MochaDark), "My Dark")
            .unwrap();
        let reference = store.get(&id).unwrap().theme_ref();
        store.rename(&id, "Night").unwrap();
        assert_eq!(dir_entries(dir.path()), vec![format!("{id}.json")]);
        let (loaded, _) = ThemeStore::load(dir.path().to_path_buf());
        let theme = loaded.get(&id).unwrap();
        assert_eq!(theme.name, "Night");
        assert_eq!(theme.theme_ref(), reference);
        assert_eq!(reference.setting_value(), format!("custom:{id}"));
    }

    #[test]
    fn custom_theme_reset_color_restores_base() {
        let dir = tempdir().unwrap();
        let mut store = ThemeStore::empty(dir.path().to_path_buf());
        let base = SyntaxTheme::Base16OceanDark;
        let id = store.clone_from(&ThemeRef::Builtin(base), "Mine").unwrap();
        let base_colors = derived_editor_colors(base.background(), base.foreground());
        for &field in EditorColorField::all() {
            store.set_color(&id, field, (9, 9, 9)).unwrap();
            store.reset_color(&id, field).unwrap();
            let colors = effective_editor_colors(&store.get(&id).unwrap().theme, false);
            assert_eq!(field.of(&colors), field.of(&base_colors), "{field:?}");
        }
        // No base (an import): reset is an error, not a panic.
        store.themes.get_mut(&id).unwrap().based_on = None;
        assert!(matches!(
            store.reset_color(&id, EditorColorField::Selection),
            Err(ThemeError::NoBase)
        ));
        assert!(matches!(
            store.reset_color("t-missing", EditorColorField::Selection),
            Err(ThemeError::NotFound(_))
        ));
    }

    /// What you see is what you clone: for every built-in, the clone's
    /// effective colors equal the source's.
    #[test]
    fn clone_looks_identical_to_source() {
        let dir = tempdir().unwrap();
        let mut store = ThemeStore::empty(dir.path().to_path_buf());
        for &theme in SyntaxTheme::all() {
            let id = store
                .clone_from(&theme.into(), theme.display_name())
                .unwrap();
            let clone = effective_editor_colors(&store.get(&id).unwrap().theme, false);
            let source = effective_editor_colors(&builtin(theme), true);
            assert_eq!(clone, source, "{theme:?}");
            // And a clone of that clone too.
            let id2 = store.clone_from(&ThemeRef::Custom(id), "again").unwrap();
            let clone2 = effective_editor_colors(&store.get(&id2).unwrap().theme, false);
            assert_eq!(clone2, source, "{theme:?} (clone of clone)");
        }
    }

    /// The editor colors FerrisPad applied to a built-in theme before T0079
    /// (`apply_syntax_theme_colors`), reimplemented verbatim as the reference.
    fn pre_t0079_colors(bg: (u8, u8, u8), fg: (u8, u8, u8)) -> [(u8, u8, u8); 4] {
        let avg = |c: (u8, u8, u8)| (c.0 as u32 + c.1 as u32 + c.2 as u32) / 3;
        if avg(bg) < 128 {
            let text = if avg(fg) < 128 { (220, 220, 220) } else { fg };
            let gutter = (
                bg.0.saturating_add(15),
                bg.1.saturating_add(15),
                bg.2.saturating_add(15),
            );
            [text, (255, 255, 255), gutter, (150, 150, 150)]
        } else {
            let text = if avg(fg) >= 128 { (30, 30, 30) } else { fg };
            let gutter = (
                bg.0.saturating_sub(15),
                bg.1.saturating_sub(15),
                bg.2.saturating_sub(15),
            );
            [text, (0, 0, 0), gutter, (100, 100, 100)]
        }
    }

    #[test]
    fn builtin_effective_colors_unchanged() {
        for &theme in SyntaxTheme::all() {
            let syntect = builtin(theme);
            // The hard-coded background/foreground match syntect's.
            assert_eq!(
                rgb(syntect.settings.background),
                Some(theme.background()),
                "{theme:?}"
            );
            assert_eq!(
                rgb(syntect.settings.foreground),
                Some(theme.foreground()),
                "{theme:?}"
            );
            let c = effective_editor_colors(&syntect, true);
            assert_eq!(c.background, theme.background());
            assert_eq!(
                [c.text, c.caret, c.line_number_bg, c.line_number_fg],
                pre_t0079_colors(theme.background(), theme.foreground()),
                "{theme:?}"
            );
            assert_eq!(
                c.selection,
                crate::ui::theme::editor_selection_color(theme.background()),
                "{theme:?}: selection is T0078's"
            );
        }
    }

    #[test]
    fn effective_colors_fill_missing_fields() {
        let mut cases = vec![Theme::default()];
        // Only a foreground (dark, then light), no background.
        for fg in [(40, 40, 40), (230, 230, 230)] {
            let mut t = Theme::default();
            t.settings.foreground = Some(opaque(fg));
            cases.push(t);
        }
        // Only a background (dark, then light), no foreground.
        for bg in [(10, 10, 10), (250, 250, 250)] {
            let mut t = Theme::default();
            t.settings.background = Some(opaque(bg));
            cases.push(t);
        }
        for theme in cases {
            let c = effective_editor_colors(&theme, false);
            let ratio = wcag_contrast(c.text, c.background);
            assert!(
                ratio >= 4.5,
                "{:?}: text {:?} on {:?} is {ratio:.2}:1",
                theme.settings.foreground,
                c.text,
                c.background
            );
            // Missing fields are the derivation, never black-on-black.
            let derived = derived_editor_colors(c.background, c.text);
            assert_eq!(c.selection, derived.selection);
            assert_eq!(c.line_number_bg, derived.line_number_bg);
        }
    }

    fn fixture_value() -> serde_json::Value {
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/custom_theme_v1.json");
        serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
    }

    /// The fixture with its scopes replaced by one rule per selector text.
    fn theme_with_scopes(selectors: &[String]) -> serde_json::Value {
        let mut value = fixture_value();
        let template = value["theme"]["scopes"][0].clone();
        let rules: Vec<serde_json::Value> = selectors
            .iter()
            .map(|sel| {
                let mut rule = template.clone();
                rule["scope"]["selectors"][0]["path"]["scopes"] = serde_json::json!([sel]);
                rule
            })
            .collect();
        value["theme"]["scopes"] = serde_json::Value::Array(rules);
        value
    }

    fn write_json(dir: &Path, id: &str, value: &serde_json::Value) {
        fs::write(
            dir.join(format!("{id}.json")),
            serde_json::to_vec(value).unwrap(),
        )
        .unwrap();
    }

    /// A scope with more parts than syntect allows is rejected before
    /// syntect parses (and interns) it.
    #[test]
    fn custom_theme_scope_with_too_many_atoms_rejected() {
        let dir = tempdir().unwrap();
        write_json(
            dir.path(),
            "t-nine",
            &theme_with_scopes(&["n1.n2.n3.n4.n5.n6.n7.n8.n9".to_string()]),
        );
        write_json(
            dir.path(),
            "t-eight",
            &theme_with_scopes(&["e1.e2.e3.e4.e5.e6.e7.e8".to_string()]),
        );
        let mut budget = ScopeAtomBudget::default();
        let (store, warnings) = ThemeStore::load_with_budget(dir.path().to_path_buf(), &mut budget);
        assert!(store.get("t-nine").is_none());
        assert!(store.get("t-eight").is_some());
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert!(warnings[0].contains("t-nine") && warnings[0].contains("more than 8"));
    }

    /// Many files that each look fine cannot add up to a flood.
    #[test]
    fn custom_theme_atom_budget_is_cumulative() {
        let dir = tempdir().unwrap();
        for file in 0..3 {
            let scopes: Vec<String> = (0..25).map(|i| format!("cum{file}x{i}")).collect();
            write_json(
                dir.path(),
                &format!("t-cum{file}"),
                &theme_with_scopes(&scopes),
            );
        }
        // Each file adds 25 new atoms: two fit in 60, the third does not.
        let mut budget = ScopeAtomBudget::new(40, 60);
        let (store, warnings) = ThemeStore::load_with_budget(dir.path().to_path_buf(), &mut budget);
        assert_eq!(store.list().len(), 2, "{warnings:?}");
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("together exceed 60"), "{warnings:?}");
    }

    /// Without a foreground, syntect would paint plain text black on any
    /// background; loading fills in the effective text color.
    #[test]
    fn custom_theme_missing_foreground_normalized() {
        let mut value = fixture_value();
        value["theme"]["settings"]["foreground"] = serde_json::Value::Null;
        value["theme"]["settings"]["background"] =
            serde_json::json!({"r": 30, "g": 30, "b": 30, "a": 255});
        let dir = tempdir().unwrap();
        write_json(dir.path(), "t-nofg", &value);
        let mut budget = ScopeAtomBudget::default();
        let (store, warnings) = ThemeStore::load_with_budget(dir.path().to_path_buf(), &mut budget);
        assert!(warnings.is_empty(), "{warnings:?}");
        let theme = &store.get("t-nofg").unwrap().theme;
        let effective = effective_editor_colors(theme, false);
        let drawn = syntect::highlighting::Highlighter::new(theme)
            .get_default()
            .foreground;
        assert_eq!((drawn.r, drawn.g, drawn.b), effective.text);
        assert!(wcag_contrast(effective.text, (30, 30, 30)) >= 4.5);
    }

    /// A deeply nested theme file is rejected by serde_json's default
    /// recursion limit (128) while parsing, so no deep tree is ever built,
    /// walked or dropped: it is safe even on a small stack.
    #[test]
    fn custom_theme_deeply_nested_json_rejected() {
        let dir = tempdir().unwrap();
        let depth = 100_000;
        let json = format!(
            r#"{{"format": 1, "name": "deep", "based_on": null, "theme": {{"scopes": {}{}}}}}"#,
            "[".repeat(depth),
            "]".repeat(depth)
        );
        fs::write(dir.path().join("t-deep.json"), json).unwrap();
        let path = dir.path().to_path_buf();
        let (loaded, warnings) = std::thread::Builder::new()
            .stack_size(256 * 1024)
            .spawn(move || {
                let mut budget = ScopeAtomBudget::default();
                let (store, warnings) = ThemeStore::load_with_budget(path, &mut budget);
                (store.list().len(), warnings)
            })
            .unwrap()
            .join()
            .expect("loading must not crash");
        assert_eq!(loaded, 0);
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert!(warnings[0].contains("recursion limit"), "{warnings:?}");
    }

    #[test]
    fn translucent_theme_colors_are_composited_over_the_background() {
        let mut t = Theme::default();
        t.settings.background = Some(opaque((0, 0, 0)));
        t.settings.line_highlight = Some(Color {
            r: 255,
            g: 255,
            b: 255,
            a: 0x80,
        });
        let c = effective_editor_colors(&t, false);
        assert_eq!(c.current_line, (128, 128, 128));
    }
}
