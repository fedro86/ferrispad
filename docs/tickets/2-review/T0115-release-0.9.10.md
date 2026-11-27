---
id: T0115
title: Prepare the 0.9.10 stable release
status: review          # todo | review | done
created: 2026-11-27
severity: ~
area: process
depends-on: [T0114]
issue: 231
---

## Goal

Ship 0.9.10 as the stable release, from the state verified as 0.9.10-rc.3,
with no code change. Stable users come from 0.9.9 and see only the
`## [0.9.10]` section, because `release.yml` extracts that one section as the
release notes. So the section consolidates rc.1, rc.2 and rc.3 into one list
that describes the final state.

## In scope

- Version `0.9.10` in `Cargo.toml`, `Cargo.lock` and
  `scripts/build-releases.sh`, the same files `bump-version.sh` edits. The
  edits are made without its auto-commit or tag.
- `CHANGELOG.md`: one `## [0.9.10]` section (Security, Added, Changed,
  Fixed, Note) for users coming from 0.9.9. The three rc sections stay below
  it unchanged, as history.
- Check what a stable release needs beyond an RC (`RELEASE_PROCESS.md`, the
  README, the FlatPark packaging).

## Out of scope

- Creating and pushing the tag. The orchestrator does that after this
  ticket lands on `master`.
- Any code change. The 0.9.11 PRs stay on hold until 0.9.10 ships.
- The start-page card: the rc.3 entries already list the release's main
  features, so they stay.

## Where 0.9.10 must appear (checked 2026-11-27)

| Channel | How it picks "latest" | Stable tag effect |
|---------|-----------------------|-------------------|
| GitHub Release | `create-release`: no `-(alpha\|beta\|rc)` in the tag, so `prerelease: false` | **normal release; becomes `/releases/latest`** |
| Release gate (T0086) | `release-gate`: `publish=true` for a version tag | publishes |
| FlatPark webhook | `flatpark` job: `publish == 'true' && !contains(github.ref_name, '-')` | **runs**; record its result |
| FlatPark twice-daily check | `resolve-update.sh`: `GET /releases/latest` | resolves 0.9.10 |
| In-app updater, stable channel | `GET /releases/latest` | **offers 0.9.10** to 0.9.9 users |
| In-app updater, beta channel | `GET /releases` (includes pre-releases) | offers 0.9.10 to rc users |
| ferrispad.com | `GET /releases/latest` (fedro86/fedro-websites#15) | **shows 0.9.10** |

`is_newer_version` uses `semver`: `0.9.9 < 0.9.10-rc.1 < 0.9.10-rc.3 <
0.9.10`. So 0.9.9 users and rc testers are both offered 0.9.10.

## How to test

1. `awk '/^## \[0.9.10\]/{f=1;next} /^## \[/{f=0} f' CHANGELOG.md` prints
   the consolidated section. This is the same extraction `release.yml` does.
   The rc sections still extract unchanged.
2. `nix develop -c cargo build`, then start `target/debug/FerrisPad` with a
   temporary `XDG_CONFIG_HOME`. The start page shows `v0.9.10`.
3. The gates pass.
4. After the tag (`VERSION=0.9.10; git tag -a $VERSION -m "Release
   $VERSION"; git push origin $VERSION`):
   - `release-gate` reports `publish=true`;
   - the GitHub Release `0.9.10` is **not** a pre-release and has the
     Linux amd64/arm64 (raw + `.deb`), Windows (`.exe` + `.zip`) and macOS
     (raw + `.dmg`) assets, plus the `.sig` files of the four updater
     binaries;
   - the `flatpark` job **runs**, and its result is recorded here;
   - `curl -s https://api.github.com/repos/fedro86/ferrispad/releases/latest | jq -r .tag_name`
     prints `0.9.10`;
   - www.ferrispad.com/download offers 0.9.10;
   - a 0.9.9 build on the stable channel is offered 0.9.10.

## Acceptance criteria

- [ ] The user approved the CHANGELOG text and the release.
- [x] Version 0.9.10 in `Cargo.toml`, `Cargo.lock` and
      `build-releases.sh`.
- [x] The `[0.9.10]` section extracts with the `awk` above, and the rc
      sections are unchanged.
- [ ] After the tag: a normal (non-pre-release) release with all assets
      signed; `/releases/latest`, the website and the stable updater on
      0.9.10; the `flatpark` job result recorded.

## Affected files

- `Cargo.toml`, `Cargo.lock`, `scripts/build-releases.sh`: version
- `CHANGELOG.md`: the `[0.9.10]` section

## What was done (2026-11-27)

- Version `0.9.10` in `Cargo.toml`, `Cargo.lock` (the `FerrisPad` package
  entry) and `scripts/build-releases.sh`. Edited directly, so
  `bump-version.sh` neither committed nor tagged.
- `CHANGELOG.md` `## [0.9.10] - 2026-11-27`: one section for users coming
  from 0.9.9, consolidated from rc.1, rc.2 and rc.3. It extracts as 39
  lines. The three rc sections are byte-identical to `master`: `diff` of
  each extracted section is empty, and the diff removes no line.
- **Security:** rc.3's five entries, verbatim.
- **Added:** rc.1's three entries, verbatim.
- **Changed:**
  - Selection: "every syntax theme" is now "every built-in syntax theme".
    Custom themes (T0079/T0080) can set their own selection color, so the
    old claim would no longer hold.
  - FLTK 1.5.24, from rc.2: the X11 text offset is now compared with 0.9.9,
    not with rc.1.
- **Fixed:** rc.1's two fixes and rc.3's fixes, ordered by impact.
  - The T0099 line is kept exactly as narrowed in rc.3.
- **Dropped,** because they fix something 0.9.9 never had:
  - rc.2's "Windows Build" (0.9.9 had Windows builds);
  - rc.3's "Black in the Theme Manager" (T0098: "The bug shipped in
    rc.1/rc.2 with the theme manager").
- **Kept,** because the tickets show the bugs older than 0.9.10:
  - T0087 ("older than T0081");
  - T0099 ("pre-existing", the 0.9.9 session format);
  - T0103, T0091, T0093, T0107: causes in code that predates the rc work;
  - T0105: the Large Files inputs in 0.9.9's `settings_dialog.rs` set no
    selection color;
  - T0097: 0.9.9's lists already used `button_bg` on `input_bg`.
- **Note:** rc.3's T0102 note, verbatim.
- **Review (a3) and the user's decision.** a3 found that the rc.3 T0099
  line overclaims. After window B closes, window A's next session save
  (an auto-save, at most every 30 s while the session is dirty) merges B's
  documents and takes ownership of `session.json`. A's following save,
  including its quit save, skips the merge and drops B's documents, and
  the orphan cleanup deletes their temp files. a3 reproduced this on
  0c44351 with a throwaway unit test. It is the T0101 class.
  - The user chose to keep T0101 in 0.9.11.
  - In `[0.9.10]` only (rc.3 stays as published):
    - the Fixed line now says what T0099 fixed: windows no longer
      mistake each other's untitled documents for one because they share
      a name;
    - a known-issue Note was added.
  - The orchestrator's draft of the note said "for a minute or more". That
    was wrong: it can take seconds.
  - a3's corrections, confirmed in the code (`mark_dirty` in
    `dispatch.rs`; `is_dirty || !has_path` in `session.rs`):
    - the trigger is any session-dirtying action (editing, opening,
      saving or closing files);
    - modified named files and the closed window's tabs are affected too.
  - The note uses a3's approved wording.
- **Intro:** no release-candidate wording.
- UI names checked in the code: "Manage themes…", "View/Toggle Current
  Line Highlight", Settings "Save".
- **What a stable release needs beyond an RC.** Checked:
  - `RELEASE_PROCESS.md` → *Version Bump Checklist*: CHANGELOG first, then
    the version files. `bump-version.sh` also edits the README, but the
    README has no version-specific link or text (`grep` for `0.9.` finds
    nothing), so there is nothing to change there.
  - The website reads `/releases/latest` and needs no edit.
  - `packaging/flatpark/…metainfo.xml` and the manifest are the **mirror**
    of the FlatPark registry copy (T0071). They still say 0.9.8 because
    FlatPark's own update job prepends the `<release>` and re-pins the
    URLs in the registry (T0047). 0.9.9 did not edit them either, so this
    release leaves them alone too.
  - So nothing beyond the version files and the CHANGELOG.
- Start page unchanged. Its rc.3 entries list the release's main features.
  Xvfb, Noto Sans, light and dark: `ferrispad-wt/uitest-T0115/start-{Light,Dark}-16.png`
  show `v0.9.10` and all six entries inside the card.
