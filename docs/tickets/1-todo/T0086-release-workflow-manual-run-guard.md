---
id: T0086
title: Stop a manual run of the release workflow from publishing a release
status: todo            # todo | review | done
created: 2026-11-26
severity: minor
area: process
depends-on: []
issue: ~
---

## Goal

`release.yml` also runs on `workflow_dispatch`. Started by hand from a branch
(for example `master`), it builds and **signs** every platform with the
release key, and every job derives the version as `${GITHUB_REF#refs/tags/}`,
which on a branch is the whole ref (`refs/heads/master`). The run then fails
in `create-release`: `softprops/action-gh-release` v3 throws "GitHub Releases
requires a tag" when the ref is not a tag and neither `tag_name` nor `draft`
is set (`src/run.ts`). Because `flatpark` has `needs: create-release`
without `always()`, it is skipped.

So today nothing is published, but only by accident: the only guard is an
internal check of a third-party action, the run ends red, and the
`flatpark` condition (`!contains(github.ref_name, '-')`) would pass for a
branch like `master`. Adding `tag_name:` or `draft:` to the release step one
day would remove the guard silently. `docs/guides/RELEASE_PROCESS.md` →
*Manual Trigger* also says a manual run "will create draft releases you can
test", which is not what happens.

After this ticket, the workflow itself decides: a manual run on a branch
builds and uploads the binaries as workflow artifacts and ends green, and
only a version tag can publish a release or notify FlatPark.

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

Start the workflow by hand on the `ticket/T0086` branch **before** changing
`release.yml` (`gh workflow run release.yml --ref ticket/T0086`). Expect the
run to fail in `create-release` with "GitHub Releases requires a tag", with
`flatpark` skipped and no release created. Record the run ID here. This is
safe: it is the behaviour described in the Goal.

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
  tagging; T0085 added a Windows build to `ci.yml` instead. At first the
  manual run looked able to publish a branch-named release and notify
  FlatPark; reading `action-gh-release` showed it fails first.
- `sign-binaries` also runs on a manual run and signs with the release key.
  The signatures carry the branch-derived version and are only uploaded as
  artifacts once `create-release` is skipped. Decide during implementation
  whether signing should also be tag-only.
