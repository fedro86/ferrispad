---
id: T0036
title: Add cargo fmt/clippy/test gates to CI
status: done
created: 2026-08-24
severity: moderate
area: process
depends-on: []
issue: 31
---

## Goal

CI runs only `release.yml` and `build-deps-check.yml`; nothing runs
`cargo test` / `clippy` / `fmt` on push or PR. That absence is why the reactive
`#[allow(...)]`s and the copy-paste drift documented across these tickets could
accumulate unseen. A quality gate in CI is what stops all of the above from
re-accreting after it's fixed. This is the meta-ticket: land it early so every
subsequent ticket's fix is protected.

## In scope

- Add a CI workflow (e.g. `.github/workflows/ci.yml`) that, on push/PR, runs:
  `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`,
  and `cargo test` — with the Linux build deps installed (reuse the list from
  `build-deps-check.yml` / the README).
- Consider a `[lints]` section in `Cargo.toml` to make the clippy bar explicit.

## Out of scope

- Cross-platform CI (macOS/Windows test runners) — nice-to-have follow-up.
- Coverage tooling.

## How to test

### Recipe

Open a PR that deliberately introduces a clippy warning and a fmt violation;
assert the new CI job fails on both. Then a clean PR passes.

- Before: no such job exists.
- After: the job blocks warnings/format drift on every PR.

## Acceptance criteria

- [x] CI runs fmt + clippy (`-D warnings`) + test on push/PR.
- [x] Build deps installed so the job actually compiles FLTK.
- [~] A warning/format violation fails the job; a clean tree passes — verifiable
      only on GitHub after push (see below); validated locally as far as possible.

## Affected files

- `.github/workflows/ci.yml` (new — this diff IS committed).
- `Cargo.toml` — `[lints]` section deliberately **not** added (see decision).

## Notes

- Origin: both audits (the shared root cause). NOTE: this ticket edits committed
  CI files — unlike the code tickets, its diff is meant to be committed once the
  team decides to adopt the workflow; the `docs/tickets/` + `.claude/` workspace
  itself stays gitignored per the current trial.

## Outcome (2-review)

New workflow `.github/workflows/ci.yml`, one job `gates` running the exact
pre-commit-hook checks in cheapest-first order:

1. `cargo fmt --check`
2. `cargo clippy --locked --all-targets --all-features -- -D warnings`
3. `cargo test --locked`

- **Triggers:** `push` to `master`, every `pull_request`, and manual
  `workflow_dispatch`. `concurrency: cancel-in-progress` so a newer push
  supersedes an in-flight run.
- **FLTK deps:** the same apt list `release.yml` uses to build the Linux binary
  (X11 + Wayland/xkbcommon/dbus), so the job actually links FLTK with the
  `use-wayland` feature.
- **Toolchain/cache:** `dtolnay/rust-toolchain@stable` with `rustfmt, clippy`
  components (matches `release.yml`), plus `Swatinem/rust-cache@v2`.
- **`--locked`** on the first dep-touching step (clippy) also fails the job on
  `Cargo.lock` drift.

### Decision: no `[lints]` section

The ticket floats a `Cargo.toml [lints]` table as optional. A crate-level
`deny(warnings)` would make **`cargo build` itself** fail on any warning during
local iteration, which hurts the edit loop. The CI `-- -D warnings` flag already
enforces the bar at the gate without penalising local dev, so `[lints]` is
omitted. Easy to add later if the team wants it.

### Verification

CI behaviour can't be exercised from a dev shell, so this was validated as far as
is locally possible:

- `ci.yml` parses as valid YAML.
- Its three commands are byte-for-byte the gates that pass green locally right
  now (`fmt --check`, `clippy --locked … -D warnings`, `test --locked` — all
  exit 0 in the Nix shell).
- `cargo build --locked` exits 0, so `Cargo.lock` is in sync (the T0016 `windows`
  feature needed no lockfile change) and `--locked` won't spuriously fail CI.
- The apt list is copied from `release.yml`, already proven to build FLTK in CI.

**Post-push checks (on GitHub):** the commit that adds `ci.yml` triggers the
`push: master` run — watch it with `gh run watch` to confirm it goes green on the
real tree. To confirm it *fails* on drift, open a throwaway PR that adds a fmt
violation and a clippy warning and check the job turns red (the ticket's recipe).

## How to verify (reviewer recipe)

```bash
# Locally: the three commands the job runs all pass.
nix develop -c cargo fmt --check
nix develop -c cargo clippy --locked --all-targets --all-features -- -D warnings
nix develop -c cargo test --locked
# YAML sanity:
python3 -c "import yaml; yaml.safe_load(open('.github/workflows/ci.yml'))"
# After push: watch the triggered run
gh run watch
```
