---
id: T0042
title: Bump actions/checkout off the deprecated Node 20 runtime (and sweep other actions)
status: done
created: 2026-08-26
severity: minor
area: process
depends-on: []
issue: 53
---

## Goal

Every CI run now prints a deprecation annotation: `actions/checkout@v4` targets
Node 20, which GitHub is retiring — the runner force-runs it on Node 24 for now
and will eventually stop. `checkout@v4` is used across all three workflows
(`ci.yml`, `release.yml`, `build-deps-check.yml`). Bump it (and any sibling
action in the same boat) so the annotation clears and the workflows keep running
once Node 20 is removed.

## In scope

- Bump `actions/checkout@v4` to a Node 24 major in `ci.yml`, `release.yml`, and
  `build-deps-check.yml`.
- Quick sweep of the other pinned actions and bump any whose current major is
  Node-20/EOL to the latest that targets Node 24 — candidates:
  `softprops/action-gh-release@v1`, `Swatinem/rust-cache@v2`,
  `actions/upload-artifact`, `download-artifact`, `cache`.

## Out of scope

- Any behavioural change to what the workflows do (deps, gates, release steps).
- `dtolnay/rust-toolchain@stable` — composite/shell action, no Node runtime.
- The pre-existing shellcheck `SC2086` infos `actionlint` reports inside the
  `run:` scripts of `release.yml` / `build-deps-check.yml` (unquoted
  `$GITHUB_OUTPUT` / `$APT_PACKAGES`). They are untouched by this ticket.

## What was done

Audit made at implementation time (2026-10-19) with
`gh api repos/<owner>/<action>/releases/latest` and the `runs.using` field of
each tag's `action.yml`:

| Action | Was | `runs.using` (was) | Latest release | Now | `runs.using` (now) |
|---|---|---|---|---|---|
| `actions/checkout` | `@v4` | node20 | v7.0.1 | `@v7` | node24 |
| `actions/upload-artifact` | `@v4` | node20 | v7.0.2 | `@v7` | node24 |
| `actions/download-artifact` | `@v4` | node20 | v8.0.2 | `@v8` | node24 |
| `actions/cache` | `@v4` | node20 | v6.1.0 | `@v6` | node24 |
| `softprops/action-gh-release` | `@v1` | **node16** | v3.0.3 | `@v3` | node24 |
| `Swatinem/rust-cache` | `@v2` | node24 (v2 → v2.9.2) | v2.9.2 | unchanged | node24 |
| `dtolnay/rust-toolchain` | `@stable` | composite | — | unchanged | composite |

- Deviation from the original ticket text: it proposed `checkout@v5` and
  `action-gh-release@v2`, and assumed `upload/download-artifact@v4` and
  `cache@v4` were already the newest majors. None of that holds any more:
  `action-gh-release@v2` is itself still Node 20 (v3.0.0 is the Node 24 move),
  and the artifact/cache actions have since shipped Node 24 majors while `@v4`
  stays on node20. Following the ticket's own note ("confirm each bumped
  action's latest major at implementation time"), every action was bumped to its
  **latest** major; all pins remain floating major tags of the same trusted
  publishers (no `@main`).
- Breaking-change review of every skipped major (release notes), against how
  the workflows use each action:
  - `checkout` v5 (node24), v6 (credentials persisted to a separate file),
    v7 (refuses fork-PR checkout under `pull_request_target`/`workflow_run`,
    ESM). None of our workflows uses those triggers or inspects `.git/config`
    credentials. The `sign-binaries` cross-repo checkout of
    `fedro86/ferrispad-plugins` uses only `repository` + `path`, unchanged.
  - `upload-artifact` v5/v6 (node24), v7 (optional `archive: false` direct
    uploads, ESM). We keep the default `archive: true`, so artifacts are still
    zipped and named by `name:`.
  - `download-artifact` v5 (path change **only** for single downloads by
    `artifact-ids`), v6/v7 (node24), v8 (skips unzipping non-zipped artifacts;
    digest mismatch now fails the job by default). We download by `name:` into
    `binaries/`, and the "download all" step into `artifacts/<name>/` — both
    layouts unchanged. All our uploads are zipped. A digest mismatch failing the
    release is the secure, desired behaviour for signed binaries.
  - `cache` v5 (node24), v6 (ESM). Inputs `path`/`key` unchanged.
  - `action-gh-release` v2 (node20), v3 (node24, otherwise same as latest 2.x).
    Inputs used (`name`, `body_path`, `draft`, `prerelease`, `files`) all still
    exist with the same meaning in v3's `action.yml`.
- All Node 24 majors need runner >= 2.327.1; GitHub-hosted runners satisfy that
  (also inside the `debian:trixie` container job, which uses the runner's own
  Node).

## How to test

CI-config change — verified with `actionlint` locally and on GitHub, not with
`cargo test` (no Rust code changed; the Rust gates still run as usual).

```bash
# Workflow schema + known-action inputs check (shellcheck/pyflakes off: the
# remaining shellcheck infos are pre-existing, see Out of scope) -> exit 0:
nix run nixpkgs#actionlint -- -shellcheck= -pyflakes= \
  .github/workflows/ci.yml .github/workflows/release.yml .github/workflows/build-deps-check.yml
# Every workflow still parses as YAML:
nix develop -c python3 -c "import yaml,sys; [yaml.safe_load(open(f)) for f in sys.argv[1:]]; print('yaml ok')" .github/workflows/*.yml
# No Node-20 pin left:
grep -rnE "checkout@v4|upload-artifact@v4|download-artifact@v4|cache@v4|action-gh-release@v1" .github/workflows/   # -> none
# CI on this branch's PR: green, no "Node.js 20 is deprecated" annotation:
gh run list --branch ticket/T0042
gh run view <id>          # the ANNOTATIONS block must not mention Node.js 20
```

- Before: the `ci` run on `master` shows `! Node.js 20 is deprecated …
  actions/checkout@v4`.
- After: the `CI` and `Build Deps Check` runs on the T0042 PR are green with no
  Node-20 deprecation annotation (see *Verification results*).

### Only verifiable at the next real release

`release.yml` runs only on a pushed tag; no tag was pushed for this ticket.
`actionlint` validates its syntax and the inputs of every action, but the
following can only be confirmed by the next release run:

1. `upload-artifact@v7` / `download-artifact@v8` round-trip: the
   `sign-binaries` job finds the binaries under `binaries/`, and the
   `create-release` job finds them under `artifacts/<name>/*`.
2. `actions/cache@v6` restores/saves the signer build cache.
3. `action-gh-release@v3` creates the release with the CHANGELOG notes, the
   correct pre-release flag, and **all** binaries + `.sig` files attached.
4. No Node-20/Node-16 deprecation annotation on any job of the release run.

Check: after the next tag push, `gh run view <release-run-id>` (all jobs green,
no Node deprecation annotations) and `gh release view <tag>` (every binary and
its `.sig` attached).

## Verification results

- `actionlint -shellcheck= -pyflakes=` on all three workflows: exit 0.
- YAML parse: ok. `grep` for old pins: none.
- Rust gates (unchanged code): `fmt --check` and `clippy -D warnings` clean;
  `cargo test` green (368 unit tests + integration suites). Note: under heavy
  machine load (load avg ~35 from parallel builds) the timing test
  `terminal::grid::tests::line_feed_flood_trims_in_bounded_time` failed its 2s
  bound once; it passed on rerun at normal load. Pre-existing and unrelated
  to this ticket.
- Before (`master`, CI run 39250611639): annotation `! Node.js 20 is
  deprecated. The following actions target Node.js 20 but are being forced to
  run on Node.js 24: actions/checkout@v4.`
- After (PR #58): `CI` run 39255778099 and `Build Deps Check` run
  39255778104 (the `debian:trixie` container job) are both green. The only
  annotation left is GitHub's unrelated notice that `ubuntu-latest` is
  migrating to Ubuntu 26. No Node.js 20 deprecation annotation.

## Acceptance criteria

- [x] No `actions/checkout@v4` in any workflow; bumped to `@v7` (latest major,
      Node 24).
- [x] Other Node-20/EOL actions bumped where a newer major exists
      (`upload-artifact@v7`, `download-artifact@v8`, `cache@v6`,
      `action-gh-release@v3`); `rust-cache@v2` already Node 24.
- [x] All workflow YAML still valid; the `ci` run is green with no Node-20
      deprecation annotation.

## Affected files

- `.github/workflows/ci.yml`
- `.github/workflows/release.yml`
- `.github/workflows/build-deps-check.yml`

## Notes

- Origin: deprecation annotation surfaced on the first `ci.yml` run (T0036).
  Ref: https://github.blog/changelog/2025-09-19-deprecation-of-node-20-on-github-actions-runners/
- Pin to exact majors the project already trusts; don't switch to unversioned
  `@main`. Confirm each bumped action's latest major at implementation time.
