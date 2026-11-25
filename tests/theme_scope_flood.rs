//! A crafted theme file must not exhaust syntect's process-global scope
//! repository (T0079). Its own test binary: if a flood ever got through, it
//! would poison every other test sharing the process.

use std::fs;
use std::path::Path;

use ferris_pad::app::services::custom_themes::ThemeStore;
use syntect::easy::HighlightLines;
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;
use tempfile::tempdir;

/// syntect stops interning at u16::MAX - 2 atoms; 66 000 distinct ones
/// overflow it on their own.
const FLOOD_ATOMS: usize = 66_000;

fn fixture() -> serde_json::Value {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/custom_theme_v1.json");
    serde_json::from_str(&fs::read_to_string(fixture).unwrap()).unwrap()
}

/// Write `json` as theme `id`, load the themes dir and assert the file was
/// skipped with one warning.
fn assert_skipped(id: &str, json: &serde_json::Value) {
    let bytes = serde_json::to_vec(json).unwrap();
    assert!(
        bytes.len() < 1024 * 1024,
        "the flood fits under the 1 MiB cap"
    );
    let dir = tempdir().unwrap();
    fs::write(dir.path().join(format!("{id}.json")), &bytes).unwrap();
    let (store, warnings) = ThemeStore::load(dir.path().to_path_buf());
    assert!(store.get(id).is_none(), "{id} must be skipped");
    assert_eq!(warnings.len(), 1, "{warnings:?}");
}

/// One test, so the floods run in order and the final syntect check sees
/// whatever they left in the process-global scope repository.
#[test]
fn custom_theme_atom_flood_rejected() {
    // 1. One scope string of 66 000 distinct dot-separated atoms.
    let mut json = fixture();
    let flood: Vec<String> = (0..FLOOD_ATOMS).map(|i| format!("q{i}")).collect();
    json["theme"]["scopes"][0]["scope"]["selectors"][0]["path"]["scopes"][0] =
        serde_json::Value::String(flood.join("."));
    assert_skipped("t-flood", &json);

    // 2. Combinatorial: 66 000 distinct atoms that each contain spaces
    //    ("c a j b e"), from a 10-letter alphabet, 8 atoms per scope string.
    //    In a JSON theme each string is ONE syntect scope, so the spaces are
    //    part of the atoms: counting space-separated tokens would see only
    //    10 atoms.
    let letters = b"abcdefghij";
    let atom = |i: usize| -> String {
        (0..5u32)
            .map(|k| (letters[(i / 10usize.pow(k)) % 10] as char).to_string())
            .collect::<Vec<_>>()
            .join(" ")
    };
    let indices: Vec<usize> = (0..FLOOD_ATOMS).collect();
    let scopes: Vec<serde_json::Value> = indices
        .chunks(8)
        .map(|chunk| {
            let atoms: Vec<String> = chunk.iter().map(|&i| atom(i)).collect();
            serde_json::Value::String(atoms.join("."))
        })
        .collect();
    let mut json = fixture();
    json["theme"]["scopes"][0]["scope"]["selectors"][0]["path"]["scopes"] =
        serde_json::Value::Array(scopes);
    assert_skipped("t-combinatorial", &json);

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
