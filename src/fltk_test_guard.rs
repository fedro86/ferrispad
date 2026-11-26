//! Guard (T0113): no library unit test builds FLTK objects.
//!
//! libtest runs unit tests on worker threads. On macOS, AppKit aborts the
//! whole test binary as soon as FLTK touches the display off the main
//! thread (T0089), and #177 (T0087) hit exactly that with a `TextBuffer`
//! built through `TabManager::add_untitled`. Linux and Windows don't
//! notice, so this guard reads the sources and fails on every OS.
//!
//! It scans the body of every `#[cfg(test)] mod … { … }` in `src/`. Nested
//! `main_thread_checks` modules are skipped: those run on the main thread
//! through `tests/fltk_main_thread.rs`. Lines are matched against a narrow
//! list of FLTK constructors, of calls that create a `TextBuffer`, and of
//! the dialog components and builders. A plain `fltk::app::channel` or an
//! empty `TabManager::new` is fine.
//!
//! Limit: a helper outside `#[cfg(test)]` that builds FLTK objects and is
//! called from a test is not seen. Keep such helpers in the test module or
//! in `main_thread_checks`.

/// What a unit test must not call: FLTK widget and buffer constructors,
/// the document calls that create a `TextBuffer`, and the dialog
/// components and builders that create widgets.
const FLTK_IN_TESTS: &str = concat!(
    r"TextBuffer::(default|new)|Document::new_\w+|\.add_untitled\(|\.add_from_(file|buffer)\(",
    r"|\bWindow::(new|default)",
    r"|\b(Flex|Group|Button|CheckButton|Frame|TextEditor|TextDisplay|MenuBar|MenuButton",
    r"|Input|IntInput|Scroll|Pack|Tabs|Tree|HoldBrowser|Choice|Progress)::(new|default)\(",
    r"|build_main_window\(|AppState::new\(",
    r"|\btheme\.(input|readonly_input|int_input|text_display|text_editor|choice|list",
    r"|check_button|radio|button|secondary_button|warning_button|success_button",
    r"|progress|header_bar)\(",
    r"|\bbuild_themed_(popup|input|message|choice)\(",
);

/// The `#[cfg(test)]` modules of `source`, each as (first line number,
/// body text), with nested `main_thread_checks` modules blanked out.
fn test_module_bodies(source: &str) -> Vec<(usize, String)> {
    let mut bodies = Vec::new();
    let mut rest = 0;
    while let Some(at) = source[rest..].find("#[cfg(test)]") {
        let attr = rest + at;
        rest = attr + "#[cfg(test)]".len();
        // Only `#[cfg(test)]` on a module, possibly after more attributes.
        let after = source[rest..].trim_start();
        let after = after.trim_start_matches("pub ").trim_start();
        if !after.starts_with("mod ") {
            continue;
        }
        let Some(open) = source[rest..].find('{').map(|i| rest + i) else {
            break;
        };
        let Some(close) = matching_brace(source, open) else {
            break;
        };
        let line = source[..open].lines().count();
        bodies.push((line, without_main_thread_checks(&source[open..=close])));
        rest = close;
    }
    bodies
}

/// The byte index of the `}` closing the `{` at `open`. Braces in string
/// and char literals and in comments are not special-cased; test code that
/// opens more braces in literals than it closes would end the module early.
fn matching_brace(source: &str, open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (i, c) in source[open..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(open + i);
                }
            }
            _ => {}
        }
    }
    None
}

/// `body` with every nested `mod main_thread_checks { … }` replaced by
/// blank lines (line numbers stay right).
fn without_main_thread_checks(body: &str) -> String {
    let mut out = body.to_string();
    while let Some(at) = out.find("mod main_thread_checks") {
        let Some(open) = out[at..].find('{').map(|i| at + i) else {
            break;
        };
        let Some(close) = matching_brace(&out, open) else {
            break;
        };
        let blank: String = out[at..=close]
            .chars()
            .map(|c| if c == '\n' { '\n' } else { ' ' })
            .collect();
        out.replace_range(at..=close, &blank);
    }
    out
}

/// The lines of `source`'s test modules that build FLTK objects, as
/// "line: code".
fn fltk_lines_in_tests(source: &str) -> Vec<String> {
    let pattern = regex_lite::Regex::new(FLTK_IN_TESTS).expect("guard pattern");
    let mut hits = Vec::new();
    for (first_line, body) in test_module_bodies(source) {
        for (n, line) in body.lines().enumerate() {
            let code = line.trim_start();
            if code.starts_with("//") {
                continue;
            }
            if pattern.is_match(code) {
                hits.push(format!("{}: {}", first_line + n, code.trim_end()));
            }
        }
    }
    hits
}

/// T0087's `js_tab` helper as it stood in `mod tests` on ticket/T0087
/// (54b1759) before it moved to `main_thread_checks`: it aborted the macOS
/// test binary.
const T0087_JS_TAB: &str = r#"
#[cfg(test)]
mod tests {
    use super::*;

    fn js_tab(text: &str) -> (TabManager, DocumentId) {
        let (sender, _receiver) = fltk::app::channel::<Message>();
        let mut tabs = TabManager::new(sender);
        let id = tabs.add_untitled();
        let doc = tabs.doc_by_id_mut(id).unwrap();
        doc.buffer.set_text(text);
        (tabs, id)
    }
}
"#;

#[test]
fn guard_flags_t0087_js_tab() {
    let hits = fltk_lines_in_tests(T0087_JS_TAB);
    assert_eq!(hits.len(), 1, "{hits:?}");
    assert!(hits[0].contains("tabs.add_untitled()"), "{hits:?}");
}

#[test]
fn guard_skips_main_thread_checks_and_plain_channels() {
    // The same helper inside a nested main_thread_checks module, and a test
    // that only makes a channel and an empty tab manager: both fine.
    let fixed = T0087_JS_TAB.replace(
        "    fn js_tab",
        "    mod main_thread_checks {\n    fn js_tab",
    ) + "}\n";
    assert_eq!(fltk_lines_in_tests(&fixed), Vec::<String>::new());
    let channel_only = "#[cfg(test)]\nmod tests {\n    fn t() {\n        \
        let (s, _r) = fltk::app::channel::<Message>();\n        \
        let tabs = TabManager::new(s);\n    }\n}\n";
    assert_eq!(fltk_lines_in_tests(channel_only), Vec::<String>::new());
    // Code outside a test module is not the guard's business.
    assert_eq!(
        fltk_lines_in_tests("fn f() { let w = Window::default(); }"),
        Vec::<String>::new()
    );
}

/// T0113: no library unit test builds FLTK objects (they abort the macOS
/// test binary). Such checks belong in a `main_thread_checks` module, run
/// by `tests/fltk_main_thread.rs` on the main thread (T0089).
#[test]
fn lib_unit_tests_build_no_fltk_objects() {
    fn walk(dir: &std::path::Path, hits: &mut Vec<String>) {
        for entry in std::fs::read_dir(dir).expect("src dir") {
            let path = entry.expect("dir entry").path();
            if path.is_dir() {
                walk(&path, hits);
            } else if path.extension().is_some_and(|e| e == "rs")
                && !path.ends_with("fltk_test_guard.rs")
            {
                let source = std::fs::read_to_string(&path).expect("source file");
                for hit in fltk_lines_in_tests(&source) {
                    hits.push(format!("{}:{hit}", path.display()));
                }
            }
        }
    }
    let mut hits = Vec::new();
    walk(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
        &mut hits,
    );
    assert!(
        hits.is_empty(),
        "unit tests build FLTK objects off the main thread (AppKit aborts the macOS \
         test binary). Move these checks into a `main_thread_checks` module and call \
         them from tests/fltk_main_thread.rs (T0089):\n{}",
        hits.join("\n")
    );
}
