//! T0111: only a real version tag starts, signs or publishes a release.
//!
//! The release convention is `X.Y.Z` or `X.Y.Z-(alpha|beta|rc).N`, without a
//! `v` prefix and without leading zeros. Three places decide it, and this test
//! reads each from its file:
//! - the `release-gate` regex in `release.yml`, which decides whether a run
//!   may sign, publish a release and notify FlatPark (the source of truth);
//! - the version check in `scripts/bump-version.sh` (must be the same regex);
//! - the `on.push.tags` filters, GitHub globs that cannot rule out leading
//!   zeros: a pre-filter that must accept every valid version.
//!
//! The bash `=~` regexes are evaluated with `regex-lite`; for this syntax
//! (anchors, groups, alternation, classes, `*`/`+`/`?`) both agree.

use regex_lite::Regex;

const RELEASE_YML: &str = include_str!("../.github/workflows/release.yml");
const BUMP_VERSION_SH: &str = include_str!("../scripts/bump-version.sh");

/// Real versions: every form the convention allows.
const VALID: &[&str] = &[
    "0.0.0",
    "0.9.11",
    "1.0.0",
    "10.20.30",
    "0.9.10-rc.2",
    "0.1.8-rc.7",
    "1.0.0-alpha.1",
    "1.0.0-beta.10",
    "1.0.0-rc.0",
];

/// Not versions. Each must be rejected by the gate and the bump script.
const INVALID: &[&str] = &[
    "v1.0.0",
    "vandal",
    "v",
    "1.2",
    "1.2.3.4",
    "1.2.3-",
    "1.2.3-rc",
    "1.2.3-rc.",
    "1.2.3-foo.1",
    "1.2.3-RC.1",
    "1.2.3-rc.1-x",
    "1.2.3-rc.1.2",
    "1.2.3 ",
    "latest",
    "release/1.2.3",
    // Leading zeros: the trigger's globs cannot tell these apart, the gate
    // and the bump script must (`LEADING_ZEROS`).
    "01.2.3",
    "1.02.3",
    "1.2.03",
    "1.2.3-rc.01",
];

/// The invalid tags only a leading zero tells apart: the trigger may start a
/// (build-only) run for them, the gate must still refuse to publish.
const LEADING_ZEROS: &[&str] = &["01.2.3", "1.02.3", "1.2.03", "1.2.3-rc.01"];

/// The regex of the first `[[ "<var>" =~ <regex> ]]` test on `var` in `text`.
fn bash_regex(text: &str, var: &str) -> String {
    let needle = format!("\"${var}\" =~ ");
    let line = text
        .lines()
        .find(|l| l.contains(&needle))
        .unwrap_or_else(|| panic!("no `{needle}` test found"));
    let rest = &line[line.find(&needle).unwrap() + needle.len()..];
    let end = rest.find(" ]]").expect("unterminated [[ ... ]]");
    rest[..end].to_string()
}

/// The `on.push.tags` filters of the workflow.
fn trigger_globs(yml: &str) -> Vec<String> {
    let mut lines = yml.lines().skip_while(|l| l.trim_end() != "    tags:");
    lines.next().expect("no on.push.tags in release.yml");
    lines
        .map_while(|l| l.strip_prefix("      - "))
        .map(|g| g.trim().trim_matches('\'').trim_matches('"').to_string())
        .collect()
}

/// A GitHub Actions filter pattern as an anchored regex: `*` is any run of
/// characters but `/`, `**` any run, `+` / `?` repeat the preceding character
/// or class, `[...]` is a class, `\` escapes, everything else is literal.
fn glob_to_regex(glob: &str) -> Regex {
    let mut re = String::from("^");
    let mut chars = glob.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '*' if chars.peek() == Some(&'*') => {
                chars.next();
                re.push_str(".*");
            }
            '*' => re.push_str("[^/]*"),
            '+' | '?' => re.push(c),
            '[' => {
                re.push('[');
                for c in chars.by_ref() {
                    re.push(c);
                    if c == ']' {
                        break;
                    }
                }
            }
            '\\' => {
                if let Some(next) = chars.next() {
                    re.push_str(&regex_lite::escape(&next.to_string()));
                }
            }
            _ => re.push_str(&regex_lite::escape(&c.to_string())),
        }
    }
    re.push('$');
    Regex::new(&re).unwrap_or_else(|e| panic!("glob {glob:?} → {re:?}: {e}"))
}

#[test]
fn release_tags_accept_only_versions() {
    let gate = bash_regex(RELEASE_YML, "REF_NAME");
    let bump = bash_regex(BUMP_VERSION_SH, "NEW_VERSION");
    let globs = trigger_globs(RELEASE_YML);
    assert!(!globs.is_empty(), "no trigger filters read");
    let gate_re = Regex::new(&gate).unwrap();
    let bump_re = Regex::new(&bump).unwrap();
    let triggers: Vec<Regex> = globs.iter().map(|g| glob_to_regex(g)).collect();
    let triggered = |tag: &str| triggers.iter().any(|r| r.is_match(tag));

    let mut failures = Vec::new();
    if gate != bump {
        failures.push(format!(
            "release-gate and bump-version.sh use different rules:\n  gate: {gate}\n  bump: {bump}"
        ));
    }
    for tag in VALID {
        if !gate_re.is_match(tag) {
            failures.push(format!("gate rejects version {tag:?}"));
        }
        if !bump_re.is_match(tag) {
            failures.push(format!("bump-version.sh rejects version {tag:?}"));
        }
        if !triggered(tag) {
            failures.push(format!("no trigger filter starts a release for {tag:?}"));
        }
    }
    for tag in INVALID {
        if gate_re.is_match(tag) {
            failures.push(format!("gate would sign and publish {tag:?}"));
        }
        if bump_re.is_match(tag) {
            failures.push(format!("bump-version.sh accepts {tag:?}"));
        }
        if !LEADING_ZEROS.contains(tag) && triggered(tag) {
            failures.push(format!("a trigger filter starts a release for {tag:?}"));
        }
    }
    assert!(
        failures.is_empty(),
        "trigger filters {globs:?}\n{}",
        failures.join("\n")
    );
}

/// The glob translation itself, on GitHub's documented examples.
#[test]
fn glob_semantics() {
    let m = |g: &str, s: &str| glob_to_regex(g).is_match(s);
    assert!(m("v[12].[0-9]+.[0-9]+", "v1.10.1"));
    assert!(!m("v[12].[0-9]+.[0-9]+", "v3.1.1"));
    assert!(m("v2*", "v2.0.0"));
    assert!(!m("v2*", "release/v2"));
    assert!(m("**", "release/v2"));
    assert!(m("[0-9]+.[0-9]+.[0-9]+-*", "1.2.3-"));
    assert!(!m("[0-9]+.[0-9]+.[0-9]+", "1.2"));
}
