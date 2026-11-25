//! A crafted theme file must not exhaust syntect's process-global scope
//! repository (T0079). Its own test binary: if the flood ever got through,
//! it would poison every other test sharing the process.

use std::fs;
use std::path::Path;

use ferris_pad::app::services::custom_themes::ThemeStore;
use syntect::easy::HighlightLines;
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;
use tempfile::tempdir;

/// syntect stops interning at u16::MAX - 2 atoms; 66 000 distinct ones in a
/// single scope string overflow it on their own.
const FLOOD_ATOMS: usize = 66_000;

#[test]
fn custom_theme_atom_flood_rejected() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/custom_theme_v1.json");
    let mut json: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(fixture).unwrap()).unwrap();
    let flood: Vec<String> = (0..FLOOD_ATOMS).map(|i| format!("q{i}")).collect();
    json["theme"]["scopes"][0]["scope"]["selectors"][0]["path"]["scopes"][0] =
        serde_json::Value::String(flood.join("."));
    let bytes = serde_json::to_vec(&json).unwrap();
    assert!(
        bytes.len() < 1024 * 1024,
        "the flood fits under the 1 MiB cap"
    );

    let dir = tempdir().unwrap();
    fs::write(dir.path().join("t-flood.json"), &bytes).unwrap();
    let (store, warnings) = ThemeStore::load(dir.path().to_path_buf());
    assert!(store.get("t-flood").is_none(), "the flood file is skipped");
    assert_eq!(warnings.len(), 1, "{warnings:?}");

    // syntect still loads its own syntaxes and highlights afterwards.
    let syntaxes = SyntaxSet::load_defaults_newlines();
    let themes = ThemeSet::load_defaults();
    let rust = syntaxes.find_syntax_by_extension("rs").unwrap();
    let mut lines = HighlightLines::new(rust, &themes.themes["base16-ocean.dark"]);
    let ranges = lines
        .highlight_line("fn main() { let x = 1; }\n", &syntaxes)
        .unwrap();
    assert!(ranges.len() > 1);
}
