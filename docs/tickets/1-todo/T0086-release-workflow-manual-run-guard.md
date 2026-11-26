---
id: T0086
title: Stop a manual run of the release workflow from publishing a release
status: todo            # todo | review | done
created: 2026-11-26
severity: moderate
area: process
depends-on: []
issue: ~
---

## Goal

`release.yml` also runs on `workflow_dispatch`. Started by hand from a branch
(for example `master`), it builds every platform and then publishes a real,
non-draft GitHub Release named after the branch, and the `flatpark` job
notifies FlatPark. The version every job derives is
`${GITHUB_REF#refs/tags/}`, which on a branch is the whole ref
(`refs/heads/master`). `create-release` uses `prerelease: false` and
`draft: false`, and the `flatpark` guard `!contains(github.ref_name, '-')`
passes for `master`. A test run could therefore reach the in-app updater's
stable channel and FlatPark users with a broken version string.
`docs/guides/RELEASE_PROCESS.md` → *Manual Trigger* says the same action
"will create draft releases you can test", which is not what the workflow
does. After this ticket, a manual run can build and sign binaries for
testing, but only a pushed version tag can publish a release or notify
FlatPark.

## In scope

- `release.yml`: `create-release` runs only when `github.ref_type == 'tag'`
  (and its tag matches the version patterns already in `on.push.tags`). A
  manual run from a branch still builds every platform and uploads the
  workflow artifacts.
- `release.yml`: the `flatpark` job requires a tag ref as well as no `-` in
  its name, so it never runs for a branch, whatever the branch is called.
- `docs/guides/RELEASE_PROCESS.md` → *Manual Trigger*: describe what a
  manual run does (artifacts on the run page, no release, no FlatPark), and
  that a manual run on a tag behaves like the tag push.

## Out of scope

- Changing what a tag push does (pre-release detection, signing, assets).
- `ci.yml` and `build-deps-check.yml`: their `workflow_dispatch` publishes
  nothing.
- Removing `workflow_dispatch` from `release.yml`. A test build of all
  platforms without a tag stays useful.

## How to test

There is no local runner for GitHub Actions, so the check is on GitHub, on
the `ticket/T0086` branch, before merge.

### Red check (current `master`)

Read `release.yml` on `master`: for `github.ref = refs/heads/master`,
`create-release` has `if: always()` and `flatpark` has
`!contains('master', '-')` = true, so both would run. Record this reasoning
here. Do **not** start a manual run on `master` to prove it, since that
would publish a release.

### On the branch

1. `actionlint .github/workflows/release.yml` (via
   `nix shell nixpkgs#actionlint`) reports nothing.
2. Start the workflow by hand on `ticket/T0086`:
   `gh workflow run release.yml --ref ticket/T0086`.
3. Expect:
   - the build and sign jobs succeed and the run page lists the binaries
     as artifacts;
   - `create-release` and `flatpark` are **skipped**;
   - `gh release list --limit 3` shows no new release;
   - `gh api repos/fedro86/ferrispad/git/refs/tags | jq -r '.[].ref' | tail -3`
     shows no new tag.
4. The next real tag (the following release) still publishes normally; note
   it in this ticket when it happens.

## Acceptance criteria

- [ ] A manual run of `release.yml` on a branch publishes no release and
      does not run the `flatpark` job.
- [ ] The workflow artifacts of that run contain the built binaries.
- [ ] A tag push still creates the release (pre-release for `-rc` tags) and
      runs `flatpark` only for tags without `-`.
- [ ] `RELEASE_PROCESS.md` describes the manual run as it actually behaves.
- [ ] `actionlint` is clean; the cargo gates still pass (no Rust change).

## Affected files

- `.github/workflows/release.yml` — `if:` on `create-release` and `flatpark`
- `docs/guides/RELEASE_PROCESS.md` — *Manual Trigger* section

## Notes

- Found while fixing the missing Windows build of 0.9.10-rc.1 (T0085). A
  manual run was considered as a way to test the Windows build before
  tagging, and dropped because of this hazard; T0085 added a Windows build
  to `ci.yml` instead.
- `sign-binaries` also runs on a manual run and signs with the release key.
  The signatures carry the branch-derived version and are only uploaded as
  artifacts once `create-release` is skipped. Decide during implementation
  whether signing should also be tag-only.
