---
id: T0118
title: The FlatPark app page lists custom editor themes
status: done            # todo | review | done
created: 2026-11-27
severity: ~
area: process
depends-on: []
issue: 236
---

## Goal

FerrisPad's page in the FlatPark catalog does not mention custom editor
themes, created in the app or imported from `.tmTheme` files (the theme
manager, shipped in 0.9.10). The page is built from the AppStream metainfo in
FlatPark's registry. Add them to its feature list, with the wording the user
approved.

The Markdown preview in the browser (Ctrl+M) was planned as a second line, but
it does not work in the Flatpak (see *Markdown preview line: dropped*), so it
is not listed.

## In scope

- `packaging/flatpark/com.ferrispad.FerrisPad/`, the mirror of
  `registry/com.ferrispad.FerrisPad/` in flatpark/flatpark:
  - first, bring the mirror in line with the registry. FlatPark's update bot
    has changed the registry since our last sync;
  - then, in the metainfo `<description>` `<ul>`, replace "Light and dark
    themes" with `Light and dark themes, plus your own editor themes: create
    them in the app or import .tmTheme files`.
- The same metainfo change prepared for flatpark/flatpark (commit and patch).
  It is **queued for the 0.9.11 release**, not submitted now (see *FlatPark
  submission: queued for 0.9.11*).

## Out of scope

- The website (fedro86/fedro-websites), README and wiki feature lists.
- Screenshots, summary, permissions text and the manifest.
- Opening the FlatPark PR. By the user's decision it waits for 0.9.11.
- The Markdown preview line, and fixing the preview in the Flatpak. The fix
  gets its own ticket.

## Markdown preview line: dropped

The orchestrator first asked for a second line, `Markdown preview in your
browser (Ctrl+M)`, right after "Syntax highlighting for 50+ languages". It was
blocked until the user confirmed that Ctrl+M works inside the Flatpak.

The user tested it on 2026-11-27: **the preview does not work in the FlatPark
Flatpak.** `PreviewController::open_in_browser` writes the HTML under
`std::env::temp_dir()`, which is the sandbox's private `/tmp`, and then calls
`open::that`, which goes through the portal. So the line is dropped here and
from the FlatPark patch.

It comes back once the Flatpak preview fix, T0119, is released.

## FlatPark submission: queued for 0.9.11

The user decided on 2026-11-27: no FlatPark PR now. The FlatPark page is
updated once, at 0.9.11, together with the Markdown preview line once T0119
has landed.

Until then **our mirror intentionally differs from the registry by the themes
`<li>` line**. Everything else is identical to flatpark/flatpark main
`013005c`.

The 0.9.11 release-prep ticket must:
1. add `Markdown preview in your browser (Ctrl+M)` right after "Syntax
   highlighting for 50+ languages", if T0119 is in the release;
2. re-sync the mirror with the registry, since FlatPark's bot will have added
   the releases in between;
3. open the flatpark/flatpark PR, with the user's OK.

The prepared branch and draft below are the starting point.

## How to test

### Sync with the registry

```bash
gh api 'repos/flatpark/flatpark/contents/registry/com.ferrispad.FerrisPad/<file>' \
  -H 'Accept: application/vnd.github.raw'      # for each of the 8 files
diff -r packaging/flatpark/com.ferrispad.FerrisPad <downloaded copy>
```

Expected: no difference except the themes `<li>` line. Before the sync, FlatPark
main (`013005c`) differed from the mirror in two files:
- the metainfo had two extra `<release>` entries, 0.9.10 (2026-11-27) and
  0.9.9 (2026-11-07), each with an empty `<description>`;
- the manifest had both extra-data sources pinned to 0.9.10 instead of 0.9.8:
  - amd64: sha256 `4909ea80…`, 15607432 bytes;
  - arm64: sha256 `f388dbb3…`, 15066824 bytes.

Both came from FlatPark's bot:
- flatpark/flatpark#1064 (0.9.9);
- flatpark/flatpark#1638 (0.9.10).

The mirror now carries them unchanged.

### AppStream validation

```bash
nix shell nixpkgs#appstream -c appstreamcli validate --no-net --pedantic --explain \
  packaging/flatpark/com.ferrispad.FerrisPad/com.ferrispad.FerrisPad.metainfo.xml
```

Expected: "Validation was successful", with one pedantic note,
`cid-contains-uppercase-letter`. The current file gets the same note; the
component ID is fixed and stays as it is. Ran with appstream 1.2.3: passed.

### FlatPark's PR checks (read-only clone, mirror copied in)

Run in a flatpark/flatpark checkout at `013005c` with this directory copied
to `registry/com.ferrispad.FerrisPad/`:
1. `node scripts/read-descriptor.mjs registry/com.ferrispad.FerrisPad/flatpark.yml`:
   OK.
2. `node scripts/audit-descriptor.mjs registry/com.ferrispad.FerrisPad/flatpark.yml`:
   exit 0.
3. `./scripts/check-approvals.sh`: "in sync". The sparse clone holds only this
   app, so the warnings about other apps' rows come from the clone.
4. `check-links-pr.sh` needs the full site build, so it was not run. The diff
   adds no URL.

No build is needed: the manifest and the payload are unchanged.

### Manual (the user)

1. Read the feature list in
   `packaging/flatpark/com.ferrispad.FerrisPad/com.ferrispad.FerrisPad.metainfo.xml`.
   Expected: the themes line reads as approved, and there is no Markdown
   preview line.
2. After the FlatPark PR merges and publishes, the FerrisPad page in the
   FlatPark catalog shows the themes line.

## Acceptance criteria

- [x] Mirror identical to the registry, apart from the themes line.
- [x] The themes line reads exactly as approved; AppStream validation passes.
- [x] The Markdown line is not shipped: Ctrl+M does not work in the Flatpak
      (user test, 2026-11-27).
- [x] The user decided on the FlatPark PR: queued for the 0.9.11 release.

## User verification

2026-11-27, by the user: approved the themes line and merging #237, with the
FlatPark submission queued for 0.9.11 (see above).

## Affected files

- `packaging/flatpark/com.ferrispad.FerrisPad/com.ferrispad.FerrisPad.metainfo.xml`:
  the registry's two `<release>` entries, and the themes `<li>` line.
- `packaging/flatpark/com.ferrispad.FerrisPad/com.ferrispad.FerrisPad.yml`:
  extra-data pinned to 0.9.10, as in the registry.

## Submission draft (queued for 0.9.11, not opened)

- Local commit `f530b86` on branch `docs/ferrispad-description`, on top of
  flatpark/flatpark main `013005c`. The author is the noreply address.
  Nothing is pushed, not even to the fedro86/flatpark fork.
- The diff touches only
  `registry/com.ferrispad.FerrisPad/com.ferrispad.FerrisPad.metainfo.xml`:
  the themes `<li>` line. That is the same change this ticket makes to the
  mirror.
- **Title:** `feat(ferrispad): describe custom editor themes`
- **Body:**

> Lists custom editor themes on FerrisPad's app page: they can be created in
> the app or imported from `.tmTheme` files (new in 0.9.10). Description text
> only; the manifest and payload are unchanged.
>
> **Tested:**
> - `appstreamcli validate --no-net --pedantic` passes;
> - `read-descriptor` and `audit-descriptor` pass;
> - `check-approvals` is in sync.
>
> I'm FerrisPad's developer.
>
> 🤖 Generated with [Claude Code](https://claude.com/claude-code)

## Notes

- Requested by the orchestrator (ferrispad-5a). The user approved the wording.
- When the Flatpak preview fix is released, add the Markdown line back, here
  and on FlatPark, in the place the *dropped* section gives.
- Process and checks follow T0071 (`docs/tickets/3-done/T0071-flatpark-aarch64.md`).
