---
id: T0076
title: Weekly security hardening 2026-W48 — land the five pending Dependabot bumps
status: review          # todo | review | done
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

- [ ] `Cargo.lock` resolves `serde 1.0.229`, `similar 3.2.1`,
      `serde_json 1.0.154`, `sha2 0.11.1`, `objc2 0.6.5`; every package
      source is `registry+https://github.com/rust-lang/crates.io-index`.
- [ ] Every new checksum matches the crates.io index entry (verified below).
- [ ] macOS build is warning-free and the title-bar feature works (recipe above).
- [ ] `cargo test`, `cargo clippy --all-targets --all-features`, and
      `cargo fmt --check` all pass with zero warnings.

## Affected files

- `Cargo.toml` — `objc2 = "0.6"` (macOS target section)
- `Cargo.lock` — resolved versions
- `src/ui/theme.rs` — `msg_send!` comma syntax (macOS-only code)

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
