---
id: T0111
title: Only a real version tag starts, signs or publishes a release
status: review          # todo | review | done
created: 2026-11-26
severity: minor
area: process
depends-on: [T0086]
issue: 211
---

## Goal

`release.yml` decides what a version tag is in two places, and
`scripts/bump-version.sh` in a third. All three accept tags that are not
versions in this repo's convention:
- the `on.push.tags` filters: `v*` (so `vandal` starts a release build),
  and `[0-9]+.[0-9]+.[0-9]+-*` (so `1.2.3-` and `1.2.3-anything`);
- the `release-gate` regex (T0086), which decides whether a run may sign
  with the release key, publish a GitHub release and notify FlatPark:
  `v<anything>`, `01.2.3`, `1.2.3-`;
- `bump-version.sh`: `01.2.3`, `1.2.3-foo.1`.

A pushed tag such as `vandal` or `1.2.3-` is signed with the release key
and published as a release. Only a real version may get there.

The convention, which every existing tag follows, is `X.Y.Z` or
`X.Y.Z-(alpha|beta|rc).N`. There is no `v` prefix and no leading zeros.

## In scope

- One strict rule for "release version":
  `^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(-(alpha|beta|rc)\.(0|[1-9][0-9]*))?$`.
- The `release-gate` regex and `bump-version.sh` use exactly that rule;
  a test checks that the two strings are identical.
- `on.push.tags` becomes four filters: `X.Y.Z` and the three pre-release
  forms. GitHub's filter syntax is a glob (`[0-9]+`), so it cannot rule out
  leading zeros without dozens of patterns. The trigger is a cheap
  pre-filter, tested to accept every valid version; the gate decides
  publishing.
- A table test: tag names → accepted or rejected, checked against the gate
  regex, the `bump-version.sh` regex and the trigger filters, all read from
  the files themselves.
- `RELEASE_PROCESS.md` states the convention and where it is enforced.

## What was done

- `release.yml`, `on.push.tags`: `v*` and `X.Y.Z-*` are gone. There are
  four filters: `[0-9]+.[0-9]+.[0-9]+` plus `-alpha.[0-9]+`, `-beta.[0-9]+`
  and `-rc.[0-9]+`.
- `release-gate` and `bump-version.sh` use the same strict regex, and a
  comment in each points to the other and to the test.
- `RELEASE_PROCESS.md`:
  - a "Version tags are enforced" paragraph;
  - the manual-run section points to it;
  - the troubleshooting example no longer uses a `v` tag.
- actionlint had 9 `SC2086` infos, already present on `ticket/T0086`:
  `>> $GITHUB_OUTPUT` and `-size ${size}x${size}` without quotes. They are
  now quoted, so actionlint is clean. The behaviour is the same.

## Out of scope

- The rest of the release workflow (jobs, signing, artifact names).
- Re-tagging or deleting existing tags: all of them already match.
- Other pre-release labels (`-dev`, `-pre`): not in use.

## How to test

### Regression test

```bash
nix develop -c cargo test --test release_tags
```

- Before the fix (`ticket/T0086`, a7be0fa): it FAILS. The two regexes
  differ, and the gate would sign and publish 15 of the 19 non-versions,
  for example:
  ```text
  release-gate and bump-version.sh use different rules:
    gate: ^(v[^/]*|[0-9]+\.[0-9]+\.[0-9]+(-[^/]*)?)$
    bump: ^[0-9]+\.[0-9]+\.[0-9]+(-[a-z]+\.[0-9]+)?$
  gate would sign and publish "vandal"
  a trigger filter starts a release for "vandal"
  gate would sign and publish "1.2.3-"
  gate would sign and publish "1.2.3-foo.1"
  bump-version.sh accepts "1.2.3-foo.1"
  gate would sign and publish "01.2.3"
  bump-version.sh accepts "01.2.3"
  ```
- After: it passes. `glob_semantics` pins the glob translation on GitHub's
  documented examples.
- Cross-checks outside the test:
  - bash `[[ =~ ]]` with the regex taken from `release.yml` gives the same
    verdict as the table for all 26 names;
  - `bump-version.sh -y` exits 1 for `vandal`, `01.2.3`, `1.2.3-` and
    `1.2.3-foo.1`, and changes no file;
  - all 36 existing tags (`0.1.0` … `0.9.10-rc.2`) match the new rule.
- Gates: 660 passed, clippy `-D warnings` and fmt clean.

### Lint

```bash
nix run nixpkgs#actionlint -- .github/workflows/release.yml
```

### Manual run (no publish)

Run `release.yml` from the Actions tab (or with `gh workflow run`) on
`ticket/T0111`. Expect:
- `release-gate` prints "not a version tag";
- every build job is green;
- `sign-binaries`, `create-release` and `flatpark` are skipped.

Done:
[run 45904265691](https://github.com/fedro86/ferrispad/actions/runs/45904265691)
on 0962449, `gh workflow run release.yml --ref ticket/T0111`. The run
**succeeded**:
- `release-gate`: notice "branch 'ticket/T0111' is not a version tag:
  build only, no signing, no release, no FlatPark";
- `build-linux` (amd64, arm64), `build-windows` and `build-macos`:
  success, with 8 unsigned artifacts uploaded;
- `sign-binaries`, `create-release` and `flatpark`: **skipped**.

A real tag push cannot be tried without publishing. The table test covers
which tags would start a run and which would publish.

## Acceptance criteria

- [x] The gate and `bump-version.sh` accept exactly `X.Y.Z` and
      `X.Y.Z-(alpha|beta|rc).N` without leading zeros; the test pins that
      the two regexes are the same string.
- [x] The trigger filters accept every valid version, and reject `v*`,
      `1.2.3-`, `1.2.3-foo.1` and `1.2.3-rc`.
- [x] Every existing tag in the repo is still a valid version (36/36).
- [x] actionlint is clean.
- [x] The manual run is green and publishes nothing.
- [x] Regression test added; it failed before the fix.
- [x] `cargo test`, `cargo clippy --all-targets --all-features`, and
      `cargo fmt --check` all pass with zero warnings.
- [ ] Pending user verification.

## Affected files

- `.github/workflows/release.yml` — `on.push.tags`, `release-gate`.
- `scripts/bump-version.sh` — the version check.
- `tests/release_tags.rs` — new.
- `docs/guides/RELEASE_PROCESS.md` — the convention.

## Notes

- From ferrispad-a3's T0086 review. ID reserved by the orchestrator
  (ferrispad-5a), target 0.9.11. Stacked on T0086, which adds the
  `release-gate`.
