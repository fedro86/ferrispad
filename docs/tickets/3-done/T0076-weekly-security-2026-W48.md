---
id: T0076
title: Weekly security hardening 2026-W48 — land the five pending Dependabot bumps
status: done            # todo | review | done
created: 2026-11-25
severity: ~
area: security
depends-on: []
issue: ~                # not opened yet: this run was local-only (no push / no gh)
---

## Goal

Weekly security pass for ISO week 2026-W48. Five Dependabot PRs (#144–#148,
opened 2026-11-11, green on Linux CI) are pending. Review each supply-chain
diff (versions, sources, checksums, new transitive crates, build scripts) and
land them as one reviewed change, so the dependency tree stays current without
five separate merges.

## In scope

- `Cargo.lock` / `Cargo.toml`: the five Dependabot changes (applied with
  `git cherry-pick -n`, so they land as one commit on the ticket branch, not
  one commit per PR):
  - `serde` / `serde_core` / `serde_derive` 1.0.228 → 1.0.229 — PR #148
    (pulls in **`syn 3.0.10`** next to the existing `syn 2.0.106`)
  - `similar` 3.0.0 → 3.2.1 — PR #147
  - `serde_json` 1.0.149 → 1.0.154 — PR #146
  - `sha2` 0.11.0 → 0.11.1 — PR #145
  - `objc2` 0.5.2 → 0.6.5 (**macOS-only**, breaking minor) — PR #144
- `src/ui/theme.rs`: adapt the macOS-only `msg_send!` calls to objc2 0.6
  syntax (comma between `selector: arg` pairs). objc2 0.6 marks the
  comma-less form `#[deprecated]`, which would be a warning — and a
  `-D warnings` clippy failure — on macOS.

## Out of scope

- Other dependency upgrades not proposed by those PRs.
- Raising / declaring an MSRV (`rust-version`) in `Cargo.toml` — see Notes.
- Closing the Dependabot PRs, pushing, opening the PR, a release.

## How to test

### Automated (Linux)

```bash
nix develop -c cargo fetch --locked        # checksums in Cargo.lock verified
nix develop -c cargo test --locked
nix develop -c cargo clippy --locked --all-targets --all-features -- -D warnings
nix develop -c cargo fmt --check
nix develop -c cargo tree --locked -i syn@3.0.10   # only serde_derive uses syn 3
```

All green, zero warnings. Diff-heavy suites to eyeball: anything exercising
`similar` (diff review / MCP proposed edits) and `serde_json`
(session/settings round-trip, `cargo test --test session_roundtrip`).

### macOS (objc2 0.6 — Linux CI does not compile this code)

On a Mac (or the `macos-latest` release runner):

```bash
rustup target add aarch64-apple-darwin x86_64-apple-darwin
cargo clippy --locked --target aarch64-apple-darwin --all-targets -- -D warnings
cargo build --locked --target x86_64-apple-darwin
cargo run --locked
```

1. Expect: no `deprecated` warning from `objc2::__missing_comma`, no errors.
2. Launch FerrisPad: the title bar takes the theme background colour and a
   centred title label in the theme foreground colour (`set_macos_titlebar_color`).
3. Open a file, edit it (dirty marker), save: the custom title label updates
   each time (`update_macos_title_label`).
4. Switch the syntax theme (light ↔ dark): title bar colours follow, no
   duplicate title label, traffic-light buttons still clickable.
5. Run a **debug** build too: objc2 verifies message encodings at runtime when
   `debug_assertions` are on, so an encoding mismatch would panic there.

## Acceptance criteria

- [x] `Cargo.lock` resolves `serde 1.0.229`, `similar 3.2.1`,
      `serde_json 1.0.154`, `sha2 0.11.1`, `objc2 0.6.5`; every package
      source is `registry+https://github.com/rust-lang/crates.io-index`.
- [x] Every new checksum matches the crates.io index entry (verified below).
- [x] macOS build is warning-free and the title-bar feature works (recipe above).
- [x] `cargo test`, `cargo clippy --all-targets --all-features`, and
      `cargo fmt --check` all pass with zero warnings.

## Affected files

- `Cargo.toml` — `objc2 = "0.6"` (macOS target section)
- `Cargo.lock` — resolved versions
- `src/ui/theme.rs` — `msg_send!` comma syntax (macOS-only code)
- `.github/workflows/macos-gates.yml` — new macOS gate, on PRs to `master`
  + manual trigger (local proposal, see below)

## Notes

- Origin: Dependabot PRs #144, #145, #146, #147, #148 (2026-11-11).
- MSRV: `sha2 0.11.1` declares `rust-version = 1.88` and genuinely needs it
  (safe `target_feature` intrinsics; fails with E0133/E0658 on 1.85.1). This
  is **not** a new floor: `time 0.3.47`, already on `master`, also requires
  1.88. CI uses `dtolnay/rust-toolchain@stable`, so CI is unaffected; only a
  local toolchain < 1.88 breaks. Worth an explicit `rust-version = "1.88"`
  in a follow-up ticket.
- `syn 3`: `serde_derive 1.0.229` moved to `syn ^3`, so the tree now builds
  two `syn` majors (slightly longer clean build; no runtime/binary effect —
  proc-macro only).

## What was done (2026-11-25, local machine, not pushed)

### Supply-chain review

- All 230 `[[package]]` sources in the resulting `Cargo.lock` are the
  crates.io registry; no git / path / alternate-registry sources.
- New checksums cross-checked against `https://index.crates.io` — all match,
  none yanked: `serde 1.0.229` `4148590a…`, `serde_core 1.0.229` `67dca2c9…`,
  `serde_derive 1.0.229` `e7a5d712…`, `syn 3.0.10` `4ae9ccbe…`,
  `serde_json 1.0.154` `e7e9cc8b…`, `similar 3.2.1` `d2f8bec0…`,
  `sha2 0.11.1` `47d7069b…`, `objc2 0.6.5` `08849bbd…`.
  `cargo fetch --locked` then downloaded and verified every crate.
- New transitive crates: only `syn 3.0.10` (author dtolnay, repo
  `dtolnay/syn`, `build = false`, deps proc-macro2/quote/unicode-ident).
  Removed: `objc-sys 0.3.5` (merged into `objc2` in 0.6).
- Build scripts: `serde`/`serde_core`/`serde_json` build.rs are the usual
  upstream cfg probes (rustc version, target arch) — no network, no
  filesystem access outside `OUT_DIR`. `objc2 0.6.5` build.rs only sets
  `target_abi_macabi` / `target_simulator` cfgs. `serde_derive`, `syn`,
  `similar`, `sha2`: `build = false`.

### objc2 0.6 source audit

All `objc2` use is in `src/ui/theme.rs` behind `#[cfg(target_os = "macos")]`
(`set_macos_titlebar_color`, `update_macos_title_label`, the `NSRect`
`Encode` impl), called from `main.rs` / `state.rs` under the same cfg. APIs
used — `runtime::AnyObject`, `class!`, `msg_send!` with raw-pointer
receivers/returns, `encode::{Encode, Encoding}` — all still exist in 0.6.
The one break: comma-less multi-argument `msg_send!` is `#[deprecated]` in
0.6 (`objc2::__missing_comma_between_args`), hit at 4 call sites
(2× `colorWithRed:green:blue:alpha:`, `systemFontOfSize:weight:`,
`addSubview:positioned:relativeTo:`). Commas added. **Not compiled**: no
`rustup`, no Apple target std, no macOS SDK/linker on this machine.

### Gates on this machine

`nix`, `rustup`, `cargo clippy`, `cargo fmt`, `cmake` and `make` are not
installed; system `cargo`/`rustc` 1.85.1 is too old for the tree (sha2 0.11.1
and time 0.3.47 need 1.88). `cargo test` fails before compiling FerrisPad
(rust-version check; with `--ignore-rust-version`, `fltk-sys` needs cmake and
`tikv-jemalloc-sys` needs make). Isolated `cargo check` of `serde 1.0.229`
(+ `serde_derive`/`syn 3`), `serde_json 1.0.154`, `similar 3.2.1` succeeds on
1.85.1. The full gates were **not** run green here.

### Commits (first machine — superseded, see *Gates run* below)

The pre-commit hook (`.claude/hooks/pre-commit-gates.sh`) blocked
`git commit` (`nix: command not found`). It was not bypassed. The changes
sit **uncommitted** in the working tree of `ticket/T0076-weekly-security-2026-W48`
(Dependabot changes applied with `git cherry-pick -n`). Next step, on a machine
with the Nix dev shell: run the gates, commit, open the issue + draft PR, move
the ticket to `2-review/`.

## Gates run (2026-11-25, Nix dev shell, second machine)

Nix 2.36.0, `nix develop` on `flake.nix` (nixos-unstable): rustc 1.97.1,
cargo 1.97.0, clippy 0.1.97. Run from the repo root with the staged changes
in the working tree:

| Command | Exit | Result |
|---|---|---|
| `nix develop -c cargo fmt --check` | 0 | clean |
| `nix develop -c cargo clippy --locked --all-targets --all-features -- -D warnings` | 0 | zero warnings |
| `nix develop -c cargo test --locked` | 0 | 552 passed, 0 failed, 4 ignored (unit + integration + doctests) |
| `nix develop -c cargo fetch --locked` | 0 | every checksum in `Cargo.lock` verified |
| `nix develop -c cargo tree --locked -i syn@3.0.10` | 0 | only `serde_derive 1.0.229` depends on `syn 3` |

The only `warning:` line in the logs is Nix's own `Git tree … is dirty`.

**macOS not compiled here either.** The Nix `rustc` knows the
`*-apple-darwin` targets but ships only the `x86_64-unknown-linux-gnu` std, and
there is no `rustup`, Apple SDK or linker (`xcrun`/`ld64`) in the dev shell:
`cargo check --locked --target aarch64-apple-darwin` fails immediately with
`E0463: can't find crate for core`. The objc2 0.6 migration in `theme.rs`
still needs the macOS recipe above (a Mac or the `macos-latest` runner)
before this ticket can move to `3-done/`.

Moved to `2-review/` and committed on `ticket/T0076-weekly-security-2026-W48`
(pre-commit hook ran the gates again). Deliberately **not** done on this run,
by the user's instruction: push, GitHub issue (`issue:` stays `~`), draft PR,
closing the Dependabot PRs #144–#148. Those are the remaining steps of the
`1-todo/ → 2-review/` transition in `work-sequence.md`.

## Proposed macOS gate workflow (2026-11-25, local only, not pushed)

The macOS recipe above has no safe runner today:

- `ci.yml` (has `workflow_dispatch`) runs only on `ubuntu-latest` and
  `ubuntu-24.04-arm`; its `on.push` covers `master` only, so `ticket/*`
  pushes start nothing.
- `build-deps-check.yml` is Debian/Ubuntu only.
- `release.yml` has a `build-macos` job on `macos-latest` and a
  `workflow_dispatch` trigger, but it **must not be launched** for this:
  `create-release` runs with `if: always()` and would publish a real
  (non-draft) GitHub Release, and `sign-binaries` signs with `SIGNING_KEY`.

So this commit adds `.github/workflows/macos-gates.yml` as a **local
proposal**: `workflow_dispatch` only (no `push` / `pull_request`, so existing
CI behaviour is unchanged), one `macos-14` (arm64) job running
`cargo build --locked`, `cargo clippy --locked --all-targets --all-features
-- -D warnings` and `cargo test --locked`. Same actions as `ci.yml`
(`actions/checkout@v7`, `dtolnay/rust-toolchain@stable` + clippy,
`Swatinem/rust-cache@v2`, `CARGO_TERM_COLOR`), `permissions: contents: read`,
no release, no signing, no secrets. `ci.yml`, `release.yml` and
`build-deps-check.yml` are untouched.

Not pushed and never run. Caveat: GitHub only offers `workflow_dispatch` for
a workflow file that exists on the default branch, so to gate this branch the
file must first reach `master` (e.g. split into its own small ticket); then
`gh workflow run macos-gates.yml --ref ticket/T0076-weekly-security-2026-W48`.
It covers recipe step 1 (warning-free arm64 debug build/clippy/test of the
objc2 0.6 calls in `theme.rs`; not `x86_64-apple-darwin`); the visual
title-bar checks (steps 2–5) and the runtime encoding check still need a
real Mac.

### Update: `pull_request` trigger (2026-11-25, local only, not pushed)

`macos-gates.yml` now also has `on.pull_request` with `branches: [master]`
(alongside `workflow_dispatch`). Jobs, commands, permissions and the
`macos-14` runner are unchanged; `ci.yml`, `release.yml` and
`build-deps-check.yml` are still untouched. Unlike `workflow_dispatch`, a
`pull_request` workflow is read from the PR's merge ref, so it does **not**
need to be on `master` first: opening the draft PR for this branch against
`master` runs the macOS gate on this very change. This removes the
"split into its own ticket" caveat above. Not pushed, no PR opened, never run.

### Update: `macos-latest` runner (2026-11-25, local only, not pushed)

`macos-gates.yml` now uses `runs-on: macos-latest` instead of `macos-14`, the
same label as `release.yml`'s `build-macos` job. The job name drops "arm64"
(`build + clippy + test (macos)`) because the image behind `macos-latest` is
moved by GitHub over time; it is currently Apple Silicon (arm64). Triggers,
commands and permissions are unchanged; no other workflow is touched.

## Brought up to date with master + first real macOS gate (2026-11-26)

### Merge with master (7ea980d: T0077–T0085, fltk 1.5.24)

`origin/master` merged into the branch (merge commit 1e3b2dd, no rebase, no
force-push). No textual conflict; `Cargo.lock` was checked rather than
trusted: its delta against master is byte-identical to the branch's original
delta (serde/serde_core/serde_derive 1.0.229 + syn 3.0.10 via serde_derive,
similar 3.2.1, serde_json 1.0.154, sha2 0.11.1, objc2 0.6.5, objc-sys gone),
on top of master's fltk/fltk-sys 1.5.24 set, and `cargo tree --locked`
accepts it unchanged (nothing else moved). Gates on the merge (Nix dev shell,
Xvfb, `FERRISPAD_REQUIRE_UI_TESTS=1`): fmt, clippy `-D warnings`, full suite
(lib 581 passed, every integration binary green).

### Pre-existing macOS defects exposed by the first real macOS run

The 2026-11-25 macOS run (45684116682) never got a runner (queue timeout), so
run 45857601075 was the first time the gate ran at all. Its clippy found two
macOS-only defects that were already on master, fixed in 608fb44:

- `DialogTheme::apply_titlebar` (`src/ui/dialogs/mod.rs`): `window` unused on
  macOS → explicit, commented no-op for macOS.
- `detect_system_dark_mode` macOS branch (`src/app/infrastructure/platform.rs`):
  `clippy::collapsible_if` → let chain.

`cargo test` on macOS then aborted (SIGABRT) the whole lib test binary. A
temporary per-test diagnostic step (01ff0f4, 96101d9: every test in its own
process) listed **all** macOS failures in one run (45861376873); there are
exactly three, all pre-existing on master and unrelated to the bumps:

| Test | Failure | Follow-up |
|---|---|---|
| `ui::dialogs::tests::popup_is_top_level` | SIGABRT: AppKit `nextEventMatchingMask should only be called from the Main Thread!` (FLTK widgets built on a libtest worker thread) | T0089 |
| `ui::scrollbar::tests::child_index_invariant_matches_bundled_fltk` | same abort | T0089 |
| `app::plugins::api::tests::test_lua_create_dir_and_rename` | `create_dir should succeed: Path outside project root` | T0090 |

Every other test (lib 579 + all integration binaries) passes on macOS.
`macos-gates.yml` now skips exactly these three by name (`--exact --skip`),
with a comment pointing at T0089/T0090; the list may only shrink, and any new
macOS failure still turns the job red. The diagnostic step is gone.

### CI on the final head

All green on a9ad0f0 (the code head; later commits are docs-only):
`ci.yml` run 45862294769 — fmt + clippy + test, test (linux arm64),
build (windows x64); `build-deps-check.yml` run 45862294742 — README-deps
build; `macos-gates.yml` run 45862294938 — build + clippy + test (macos),
with the three named skips.

Still pending user verification on a Mac: the objc2 0.6 recipe above (title
bar colours/title, debug build encoding check).

## User verification

2026-11-26, by the user on Fedora GNOME (Wayland) with the rc.3 integration build `verify/rc3` (checklist VERIFICA-RC3.md); T0076 also on macOS (the title bar follows the theme at startup, the title updates on edit and save, no unexpected quit).
