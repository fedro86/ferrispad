//! FLTK widget tests that must run on the process **main thread** (T0089).
//!
//! On macOS, FLTK's first `Fl::lock()` opens the display through AppKit,
//! which aborts the whole process when that happens off the main thread
//! (`nextEventMatchingMask should only be called from the Main Thread!`).
//! libtest runs every test on a worker thread, so these checks live in the
//! library (`main_thread_checks` modules) and this `harness = false` binary
//! runs them one after another from `main()`.
//!
//! It mimics libtest's command line where it matters, so `cargo test` keeps
//! working across all binaries: positional filters (substring, or exact with
//! `--exact`), `--skip <pattern>`, `--list`, and libtest-style output. Other
//! flags are accepted and ignored.
//!
//! Display: macOS and Windows always have one. On Linux the checks need
//! `DISPLAY`/`WAYLAND_DISPLAY` (CI runs them under `xvfb-run`); without one
//! they are skipped with a note, unless `FERRISPAD_REQUIRE_UI_TESTS=1`, which
//! turns a missing display into a failure.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::process::ExitCode;

use ferris_pad::app::controllers::highlight::main_thread_checks as highlight;
use ferris_pad::app::controllers::session::main_thread_checks as session;
use ferris_pad::app::controllers::tabs::main_thread_checks as tabs;
use ferris_pad::app::domain::document::main_thread_checks as document;
use ferris_pad::ui::dialogs::main_thread_checks as dialogs;
use ferris_pad::ui::menu::main_thread_checks as menu;
use ferris_pad::ui::scrollbar::main_thread_checks as scrollbar;

const CHECKS: &[(&str, fn())] = &[
    ("popup_is_top_level", dialogs::popup_is_top_level),
    (
        "child_index_invariant_matches_bundled_fltk",
        scrollbar::child_index_invariant_matches_bundled_fltk,
    ),
    (
        "long_line_clear_annotations_follows_the_plan",
        highlight::long_line_clear_annotations_follows_the_plan,
    ),
    (
        "dirty_flip_is_reported_once",
        document::dirty_flip_is_reported_once,
    ),
    (
        "untitled_first_new_after_startup_is_untitled",
        tabs::untitled_first_new_after_startup_is_untitled,
    ),
    (
        "untitled_reuses_lowest_free_number",
        tabs::untitled_reuses_lowest_free_number,
    ),
    (
        "untitled_saved_doc_frees_its_number",
        tabs::untitled_saved_doc_frees_its_number,
    ),
    (
        "tab_label_follows_paths_and_closes",
        tabs::tab_label_follows_paths_and_closes,
    ),
    (
        "untitled_full_restore_numbers_from_one",
        session::untitled_full_restore_numbers_from_one,
    ),
    (
        "recent_files_open_recent_sits_under_open",
        menu::recent_files_open_recent_sits_under_open,
    ),
    (
        "recent_files_open_recent_labels_are_whole",
        menu::recent_files_open_recent_labels_are_whole,
    ),
];

/// libtest flags that take a value (so the value is not read as a filter).
const FLAGS_WITH_VALUE: &[&str] = &[
    "--skip",
    "--test-threads",
    "--format",
    "--color",
    "--logfile",
    "--shuffle-seed",
    "-Z",
];

#[derive(Default)]
struct Args {
    filters: Vec<String>,
    skips: Vec<String>,
    exact: bool,
    list: bool,
    /// libtest's `--ignored`: run only `#[ignore]`d tests. None of these
    /// checks is ignored, so it selects nothing.
    ignored_only: bool,
}

fn parse_args(args: impl IntoIterator<Item = String>) -> Args {
    let mut parsed = Args::default();
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--exact" => parsed.exact = true,
            "--list" => parsed.list = true,
            "--ignored" => parsed.ignored_only = true,
            "--skip" => parsed.skips.extend(args.next()),
            flag if flag.starts_with("--skip=") => {
                parsed.skips.push(flag["--skip=".len()..].to_string());
            }
            flag if FLAGS_WITH_VALUE.contains(&flag) => {
                args.next();
            }
            // Everything else is ignored, including the other `--flag=value`
            // forms and `--include-ignored` (which runs everything, as here).
            flag if flag.starts_with('-') => {}
            filter => parsed.filters.push(filter.to_string()),
        }
    }
    parsed
}

fn matches(name: &str, pattern: &str, exact: bool) -> bool {
    if exact {
        name == pattern
    } else {
        name.contains(pattern)
    }
}

fn selected(name: &str, args: &Args) -> bool {
    !args.ignored_only
        && (args.filters.is_empty() || args.filters.iter().any(|f| matches(name, f, args.exact)))
        && !args.skips.iter().any(|s| matches(name, s, args.exact))
}

fn display_available() -> bool {
    if cfg!(any(target_os = "macos", target_os = "windows")) {
        return true;
    }
    let has_display = ["DISPLAY", "WAYLAND_DISPLAY"]
        .iter()
        .any(|var| std::env::var_os(var).is_some_and(|v| !v.is_empty()));
    if !has_display {
        assert!(
            std::env::var("FERRISPAD_REQUIRE_UI_TESTS").as_deref() != Ok("1"),
            "UI tests required but no display (set DISPLAY, e.g. run under xvfb-run)"
        );
        eprintln!("skipping FLTK main-thread checks: no DISPLAY or WAYLAND_DISPLAY");
    }
    has_display
}

fn main() -> ExitCode {
    let args = parse_args(std::env::args().skip(1));
    let chosen: Vec<_> = CHECKS
        .iter()
        .filter(|(name, _)| selected(name, &args))
        .collect();

    if args.list {
        for (name, _) in &chosen {
            println!("{name}: test");
        }
        return ExitCode::SUCCESS;
    }

    let filtered_out = CHECKS.len() - chosen.len();
    if !chosen.is_empty() && !display_available() {
        println!(
            "\nrunning {n} tests\n\ntest result: ok. 0 passed; 0 failed; {n} ignored; 0 measured; {filtered_out} filtered out\n",
            n = chosen.len()
        );
        return ExitCode::SUCCESS;
    }

    println!("\nrunning {} tests", chosen.len());
    let mut failed = Vec::new();
    for (name, check) in &chosen {
        let ok = catch_unwind(AssertUnwindSafe(check)).is_ok();
        println!("test {name} ... {}", if ok { "ok" } else { "FAILED" });
        if !ok {
            failed.push(*name);
        }
    }
    if !failed.is_empty() {
        println!("\nfailures:");
        for name in &failed {
            println!("    {name}");
        }
    }
    println!(
        "\ntest result: {}. {} passed; {} failed; 0 ignored; 0 measured; {filtered_out} filtered out\n",
        if failed.is_empty() { "ok" } else { "FAILED" },
        chosen.len() - failed.len(),
        failed.len(),
    );
    if failed.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
