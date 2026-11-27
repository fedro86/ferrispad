---
id: T0118
title: The FlatPark app page lists Markdown preview and custom editor themes
status: review          # todo | review | done
created: 2026-11-27
severity: ~
area: process
depends-on: []
issue: 236
---

## Goal

FerrisPad's page in the FlatPark catalog misses two features users look for:
- the Markdown preview in the browser (Ctrl+M);
- custom editor themes, created in the app or imported from `.tmTheme`
  files (the theme manager, shipped in 0.9.10).

The page is built from the AppStream metainfo in FlatPark's registry. Add both
to its feature list, with the wording the user approved.

## In scope

- `packaging/flatpark/com.ferrispad.FerrisPad/`, the mirror of
  `registry/com.ferrispad.FerrisPad/` in flatpark/flatpark:
  - first, bring the mirror in line with the registry. FlatPark's update bot
    has changed the registry since our last sync;
  - then, in the metainfo `<description>` `<ul>`:
    - add `Markdown preview in your browser (Ctrl+M)` right after "Syntax
      highlighting for 50+ languages";
    - replace "Light and dark themes" with `Light and dark themes, plus your
      own editor themes: create them in the app or import .tmTheme files`.
- The same metainfo change prepared for flatpark/flatpark (commit and patch).
  The PR there is opened only with the user's explicit OK.

## Out of scope

- The website (fedro86/fedro-websites), README and wiki feature lists.
- Screenshots, summary, permissions text and the manifest.
- Opening the FlatPark PR. That waits for the user's OK and for the blocker
  below.

## Blocker

The Markdown preview line must not ship until the user confirms that
**Ctrl+M works inside the Flatpak**. `PreviewController::open_in_browser`
writes the HTML under `std::env::temp_dir()`, which is the sandbox's private
`/tmp`, then calls `open::that`. In the sandbox that call goes through the
OpenURI portal, so the host browser only sees the file if the portal exports
it.

If it doesn't work, drop that line here and from the FlatPark patch. Fixing
the preview in the Flatpak would be its own ticket.

## How to test

### Sync with the registry

```bash
gh api 'repos/flatpark/flatpark/contents/registry/com.ferrispad.FerrisPad/<file>' \
  -H 'Accept: application/vnd.github.raw'      # for each of the 8 files
diff -r packaging/flatpark/com.ferrispad.FerrisPad <downloaded copy>
```

Expected: no difference except the two `<li>` lines. Before the sync, FlatPark
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

1. In the installed Flatpak (`flatpak run com.ferrispad.FerrisPad`), open a
   `.md` file and press Ctrl+M. Expected: the rendered page opens in the
   browser. This is the blocker above.
2. After the FlatPark PR merges and publishes, the FerrisPad page in the
   FlatPark catalog lists both lines.

## Acceptance criteria

- [x] Mirror identical to the registry, apart from the two lines.
- [x] The two lines read exactly as approved; AppStream validation passes.
- [ ] The user confirmed Ctrl+M works in the Flatpak, or the Markdown line
      was dropped.
- [ ] The user decided on opening the FlatPark PR.

## Affected files

- `packaging/flatpark/com.ferrispad.FerrisPad/com.ferrispad.FerrisPad.metainfo.xml`:
  the registry's two `<release>` entries, and the two `<li>` lines.
- `packaging/flatpark/com.ferrispad.FerrisPad/com.ferrispad.FerrisPad.yml`:
  extra-data pinned to 0.9.10, as in the registry.

## Submission draft (not opened)

- Local commit `0251093` on branch `docs/ferrispad-description`, on top of
  flatpark/flatpark main `013005c`. The author is the noreply address.
  Nothing is pushed, not even to the fedro86/flatpark fork.
- The diff touches only
  `registry/com.ferrispad.FerrisPad/com.ferrispad.FerrisPad.metainfo.xml`:
  the two `<li>` lines, the same diff as this ticket's second commit.
- **Title:** `feat(ferrispad): describe Markdown preview and custom editor themes`
- **Body:**

> Lists two FerrisPad features the app page missed: the Markdown preview in
> the browser (Ctrl+M), and custom editor themes, created in the app or
> imported from `.tmTheme` files (new in 0.9.10). Description text only; the
> manifest and payload are unchanged.
>
> **Tested:**
> - `appstreamcli validate --no-net --pedantic` passes;
> - `read-descriptor` and `audit-descriptor` pass;
> - `check-approvals` is in sync;
> - the Markdown preview was checked in the installed Flatpak.
>
> I'm FerrisPad's developer.
>
> 🤖 Generated with [Claude Code](https://claude.com/claude-code)

The "checked in the installed Flatpak" bullet stays only if the blocker
clears.

## Notes

- Requested by the orchestrator (ferrispad-5a). The user approved the wording.
- Process and checks follow T0071 (`docs/tickets/3-done/T0071-flatpark-aarch64.md`).
