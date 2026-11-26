//! A crafted `.tmTheme` must not exhaust syntect's process-global scope
//! repository through the theme manager's import (T0080). Its own test
//! binary, so a flood that got through could not poison other tests.

use std::fs;

use ferris_pad::app::controllers::theme_manager::{ThemeManagerAction, ThemeManagerModel};
use ferris_pad::app::domain::settings::SyntaxTheme;
use ferris_pad::app::services::custom_themes::ThemeStore;
use syntect::easy::HighlightLines;
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;
use tempfile::tempdir;

/// syntect stops interning at u16::MAX - 2 atoms.
const FLOOD_ATOMS: usize = 66_000;

/// A `.tmTheme` whose only rule has `selector` as its scope. Generated here
/// rather than committed: the flood is ~0.5 MB of text.
fn tm_theme(selector: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0">
<dict>
	<key>name</key><string>Flood</string>
	<key>settings</key>
	<array>
		<dict><key>settings</key><dict><key>background</key><string>#000000</string></dict></dict>
		<dict>
			<key>scope</key><string>{selector}</string>
			<key>settings</key><dict><key>foreground</key><string>#FF0000</string></dict>
		</dict>
	</array>
</dict>
</plist>
"#
    )
}

#[test]
fn theme_manager_import_atom_flood_rejected() {
    let themes = tempdir().unwrap();
    let store = ThemeStore::empty(themes.path().to_path_buf());
    let mut model = ThemeManagerModel::new(
        store,
        SyntaxTheme::Base16OceanLight.into(),
        SyntaxTheme::Base16OceanDark.into(),
        true,
        SyntaxTheme::Base16OceanDark.into(),
    );
    let work = tempdir().unwrap();

    // 1. One scope of 66 000 atoms; 2. 66 000 one-atom scopes in one
    //    selector (`a, b, c…`), each well within syntect's 8-atom limit.
    let one_scope: Vec<String> = (0..FLOOD_ATOMS).map(|i| format!("q{i}")).collect();
    let many_scopes: Vec<String> = (0..FLOOD_ATOMS).map(|i| format!("w{i}")).collect();
    for (name, selector) in [
        ("one.tmTheme", one_scope.join(".")),
        ("many.tmTheme", many_scopes.join(", ")),
    ] {
        let path = work.path().join(name);
        let text = tm_theme(&selector);
        assert!(text.len() < 1024 * 1024, "{name} fits under the 1 MiB cap");
        fs::write(&path, text).unwrap();
        let actions = model.import(&path);
        assert!(
            matches!(actions.as_slice(), [ThemeManagerAction::Error(_)]),
            "{name}: {actions:?}"
        );
    }
    assert_eq!(
        fs::read_dir(themes.path()).unwrap().count(),
        0,
        "nothing created"
    );

    // syntect still loads its own syntaxes and highlights afterwards.
    let syntaxes = SyntaxSet::load_defaults_newlines();
    let theme_set = ThemeSet::load_defaults();
    let rust = syntaxes.find_syntax_by_extension("rs").unwrap();
    let mut lines = HighlightLines::new(rust, &theme_set.themes["base16-ocean.dark"]);
    let ranges = lines
        .highlight_line("fn main() { let x = 1; }\n", &syntaxes)
        .unwrap();
    assert!(ranges.len() > 1);
}

/// Nesting far beyond any real theme. Dropping (or recursively walking) a
/// plist tree this deep overflows the stack.
const DEEP_NESTING: usize = 65_000;

/// A `.tmTheme` (< 1 MiB) of 65 000 nested arrays must be rejected without
/// building the tree: the test runs on a deliberately small stack (a Windows
/// main thread has 1 MiB).
#[test]
fn theme_manager_import_deep_nesting_rejected() {
    let work = tempdir().unwrap();
    let path = work.path().join("deep.tmTheme");
    let text = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<plist version=\"1.0\">{}{}</plist>\n",
        "<array>".repeat(DEEP_NESTING),
        "</array>".repeat(DEEP_NESTING)
    );
    assert!(text.len() < 1024 * 1024, "fits under the 1 MiB cap");
    fs::write(&path, text).unwrap();

    let themes = tempdir().unwrap();
    let themes_dir = themes.path().to_path_buf();
    let actions = std::thread::Builder::new()
        .stack_size(256 * 1024)
        .spawn(move || {
            let mut model = ThemeManagerModel::new(
                ThemeStore::empty(themes_dir),
                SyntaxTheme::Base16OceanLight.into(),
                SyntaxTheme::Base16OceanDark.into(),
                true,
                SyntaxTheme::Base16OceanDark.into(),
            );
            model.import(&path)
        })
        .unwrap()
        .join()
        .expect("the import thread must not crash");
    assert!(
        matches!(actions.as_slice(), [ThemeManagerAction::Error(e)] if e.contains("nested too deeply")),
        "{actions:?}"
    );
    assert_eq!(
        fs::read_dir(themes.path()).unwrap().count(),
        0,
        "nothing created"
    );
}

/// Import `bytes` as a `.tmTheme`, on a deliberately small stack, and assert
/// it is an error naming `reason` that creates nothing.
fn assert_import_rejected(name: &str, bytes: &[u8], reason: &str) {
    assert!(bytes.len() < 1024 * 1024, "{name} fits under the 1 MiB cap");
    let work = tempdir().unwrap();
    let path = work.path().join(name);
    fs::write(&path, bytes).unwrap();
    let themes = tempdir().unwrap();
    let themes_dir = themes.path().to_path_buf();
    let actions = std::thread::Builder::new()
        .stack_size(256 * 1024)
        .spawn(move || {
            let mut model = ThemeManagerModel::new(
                ThemeStore::empty(themes_dir),
                SyntaxTheme::Base16OceanLight.into(),
                SyntaxTheme::Base16OceanDark.into(),
                true,
                SyntaxTheme::Base16OceanDark.into(),
            );
            model.import(&path)
        })
        .unwrap()
        .join()
        .expect("the import thread must not crash");
    assert!(
        matches!(actions.as_slice(), [ThemeManagerAction::Error(e)] if e.contains(reason)),
        "{name}: expected an error about {reason:?}, got {actions:?}"
    );
    assert_eq!(
        fs::read_dir(themes.path()).unwrap().count(),
        0,
        "{name}: nothing created"
    );
}

/// The shape check covers binary plists too: nesting just past the limit,
/// and a flat array with more values than the event cap.
#[test]
fn theme_manager_import_binary_plist_shape_rejected() {
    let mut nested = plist::Value::Boolean(true);
    for _ in 0..40 {
        nested = plist::Value::Array(vec![nested]);
    }
    let mut deep = Vec::new();
    nested.to_writer_binary(&mut deep).unwrap();
    assert_import_rejected("deep-binary.tmTheme", &deep, "nested too deeply");

    let flat = plist::Value::Array(vec![plist::Value::Boolean(true); 200_001]);
    let mut wide = Vec::new();
    flat.to_writer_binary(&mut wide).unwrap();
    assert_import_rejected("wide-binary.tmTheme", &wide, "more than 200000 entries");
}
