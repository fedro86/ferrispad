---
id: T0086
title: Stop a manual run of the release workflow from publishing a release
status: review          # todo | review | done
created: 2026-11-26
severity: minor
area: process
depends-on: []
issue: 175
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
- `release.yml`, the three build jobs: on a non-tag ref, name the artifacts
  `<Cargo.toml version>-dev.<short SHA>` instead of the branch-derived
  "version" (`refs/heads/…`, with slashes, which breaks the Linux `.deb`
  rename). The tag path stays byte-identical. *(Added after the red run,
  orchestrator.)*
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

## What was done (2026-11-26)

Decision on the open note (orchestrator): **signing is tag-only too.** The
release key should only sign binaries that go into a published release; a
branch run's signatures would carry a branch-derived "version" nobody needs.

- `release.yml`: new first job `release-gate` (no checkout, no secrets).
  `publish=true` only when `github.ref_type == 'tag'` **and** the tag name
  matches the `on.push.tags` patterns, mirrored as the bash regex
  `^(v[^/]*|[0-9]+\.[0-9]+\.[0-9]+(-[^/]*)?)$`. Otherwise it is
  `publish=false` with a `::notice::` saying the run is build-only.
  - `sign-binaries`: `needs` + `release-gate`;
    `if: always() && needs.release-gate.outputs.publish == 'true'`.
  - `create-release`: the same.
  - `flatpark`: `needs: [release-gate, create-release]`;
    `if: publish == 'true' && !contains(github.ref_name, '-')`.
  - The four build jobs (Linux amd64/arm64, Windows, macOS) are unchanged
    and still upload their artifacts on any run.
- Tag path, unchanged in behaviour: on a version tag `publish=true`, so the
  three gated jobs keep their old conditions (`always()` for sign and
  create, no `-` for FlatPark). The only difference is the extra
  `release-gate` job (a few seconds).
- Regex checked locally against: `0.9.10-rc.2`, `0.9.9`, `v1.0` and
  `1.2.3-beta.1` → publish; `master`, `ticket/T0086`, `1.2`, `1.2.3.4` and
  `1.2.3-x/y` → build only.
- Build jobs (Linux ×2, Windows, macOS), "Get version from tag": on a tag
  the same two lines as before; otherwise
  `VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)-dev.${GITHUB_SHA::7}"`,
  e.g. `0.9.10-rc.2-dev.7ea980d`. That is a valid file name on every
  platform. On a branch it also lands in the macOS `Info.plist` version
  strings, which nothing validates at build time. `sign-binaries` and
  `create-release` keep the old step verbatim, since they now run only on
  tags.
- `docs/guides/RELEASE_PROCESS.md` → *Manual Trigger*: says what a branch
  run does (artifacts only, unsigned, no release, no FlatPark; the
  auto-updater would refuse unsigned binaries) and that a manual run on a
  version tag behaves like the tag push.
- `actionlint`: no findings from this change. The 9 `SC2086` info-level
  notes it prints are all in unchanged steps and identical on `master`
  (checked on master's file). They are left alone so the tag path stays
  byte-identical.

### Red run (unchanged workflow, `ticket/T0086` == master 7ea980d)

`gh workflow run release.yml --ref ticket/T0086`: run **45866964468**, never
from master. Conclusion: **failure**.

| Job | Result | Why |
|---|---|---|
| build-linux amd64 / arm64 | failure | `mv: cannot move 'target/debian/ferrispad_0.9.10~rc.2-1_amd64.deb' to 'FerrisPad-vrefs/heads/ticket/T0086-ubuntu-amd64.deb': No such file or directory`; the branch-derived "version" `refs/heads/ticket/T0086` has slashes |
| build-macos | failure | the same cause in "Prepare raw binary (universal)": `cp: FerrisPad-vrefs/heads/ticket/T0086-macos-universal: No such file or directory` |
| build-windows | success | uploaded `windows-bin` and `windows-zip`, unsigned. **Not to be used.** |
| sign-binaries | failure | it decoded the release key, then "Download macOS binary" failed (`Artifact not found for name: macos-bin`), so **"Sign binaries" was skipped: nothing was signed** |
| create-release | failure | `⚠️ GitHub Releases requires a tag` (the only guard today) |
| flatpark | skipped | `needs: create-release` failed |

No release and no tag were created (`gh release list` still ends at
`0.9.10-rc.2`). This confirms the Goal: the guard was a third-party
action's internal check, the branch build itself was broken, and a branch
run already reached the signing step with the release key loaded.
