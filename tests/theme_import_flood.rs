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
