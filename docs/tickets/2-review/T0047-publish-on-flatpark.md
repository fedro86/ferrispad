---
id: T0047
title: Package FerrisPad for FlatPark and prepare the submission
status: review
created: 2026-10-19
severity: ~
area: process
depends-on: []
issue: 109
---

## Goal

Flathub closed FerrisPad's submission (flathub/flathub#9888) under its
generative-AI policy, so FerrisPad has no Flatpak distribution channel.
[FlatPark](https://flatpark.org/) is a Flatpak hub that judges apps by what
they do, not by whether AI wrote them, and is built to be driven by AI agents.
Linux users should be able to install FerrisPad from FlatPark.

## In scope

- Read FlatPark's packager playbook and listing policy, and record the
  requirements here.
- Write a FlatPark package that wraps the official 0.9.6 Linux release asset as
  `extra-data`.
- Reuse the desktop file, icon and metainfo; pick the runtime.
- Re-evaluate the permissions and justify them.
- Build, install and run it locally with FlatPark's own scripts.
- Prepare the submission PR. It is opened only after the user says so.

## Out of scope

- Flathub: blocked. Claude does not open, comment on, or draft anything for a
  Flathub submission.
- Building from source (the existing `packaging/flatpak/` manifest keeps
  that role), Fedora COPR/RPM, a self-hosted ostree repo.
- **Opening the PR on `flatpark/flatpark`**: this is an outward-facing action
  on a third-party repo. It needs the user's explicit OK.
- Wiring FerrisPad's `release.yml` to FlatPark's release hook (push updates).
  The twice-daily pull check is enough to start; see Notes.

## FlatPark requirements (read 2026-10-20, flatpark/flatpark@43f67e6)

Sources:
- [`docs/packagers/playbook.md`](https://github.com/flatpark/flatpark/blob/main/docs/packagers/playbook.md)
- [`can-it-be-packaged.md`](https://github.com/flatpark/flatpark/blob/main/docs/packagers/can-it-be-packaged.md)
- [`basics.md`](https://github.com/flatpark/flatpark/blob/main/docs/packagers/basics.md)
- [`formats/`](https://github.com/flatpark/flatpark/blob/main/docs/packagers/formats/README.md)
- [`permissions.md`](https://github.com/flatpark/flatpark/blob/main/docs/packagers/permissions.md)
- [`submitting.md`](https://github.com/flatpark/flatpark/blob/main/docs/packagers/submitting.md)
- [`reviewers/testing-playbook.md`](https://github.com/flatpark/flatpark/blob/main/docs/reviewers/testing-playbook.md)

- **Eligibility.** Upstream publishes an official Linux build at a stable
  public URL. The main feature works in the sandbox, and the app runs on the
  catalog's current runtime. It is not on Flathub from its own developer. How
  the app was written does not matter. FerrisPad meets all of these.
- **Extra-data only.** The package holds no copy of the app. The manifest pins
  the vendor download (URL, `sha256`, `size`), and `apply_extra.sh` puts it in
  place offline at install time. On a system install it runs as root with no
  capabilities. Nothing exported (desktop file, icon, metainfo) may come from
  the payload.
- **Package layout.** The files go in `registry/<app-id>/`:
  - `flatpark.yml` (the descriptor);
  - `<id>.yml` (the manifest, with a `# BEGIN/END MANAGED EXTRA-DATA` block);
  - `apply_extra.sh`, installed to `/app/bin/apply_extra`;
  - a wrapper;
  - `<id>.desktop`, `<id>.metainfo.xml`, and an `<id>.png` of at most 512×512;
  - `resolve-update.sh`, which prints `{version, releaseDate, sources[]}` as JSON.
- **The version lives in the metainfo** (the newest `<release>`). The update job
  compares against it, re-pins the managed block and prepends a `<release>`.
  No version is bumped by hand.
- **Runtime.** The catalog's current major, chosen by what the binary links:
  `org.freedesktop.Platform` 26.08 for self-contained builds.
- **Permissions.** Grant the smallest set that makes the main feature work, with
  a comment on each. Optional extras are documented as `flatpak override` lines
  in the metainfo. High-risk grants (`home`, `host`, …) are listed in
  `policy.dangerous_permissions` and argued in the PR. Open-source payloads run
  unmodified.
- **Testing.** Run the testing playbook before any PR: unpack as a system
  install, starts and draws, data survives a restart, nothing new in the home
  folder, window class, bus names, main feature. The PR says what was tested
  and what wasn't.
- **Process.** Branch `add/<app-id>`. The commit and PR title are
  `feat(<short-name>): add <App>, <what it is> (<app-id>)`. Opening the PR needs
  a human OK. A developer who submits their own app says so in the PR; that
  PR is the evidence for the developer-approved shield, which needs a row in
  `docs/reference/upstream-approvals.md`.

## What was done

Package in `packaging/flatpark/com.ferrispad.FerrisPad/`, which becomes
`registry/com.ferrispad.FerrisPad/` in the FlatPark PR.

- **Download.** The raw `FerrisPad-v0.9.6-linux-amd64` release asset. It is a
  single self-contained executable, so there is nothing to unpack, and
  `apply_extra.sh` only moves it to `/app/extra/FerrisPad`. The `.deb` would
  need a second unpack for the same binary.
  - sha256 `4fb545542d9a84852a17e91ba23d02feca35f47996aac1ed39719fc9dd88e6d8`
  - size 13995984
- **Runtime: `org.freedesktop.Platform` 26.08.** `readelf -d` on the binary lists
  wayland-client/cursor, xkbcommon, dbus-1, X11, Xinerama, Xcursor, Xfixes,
  pango, pangocairo, cairo, gobject, libgcc_s, libm, libc. All of them are in
  26.08. The highest symbol version is GLIBC_2.39. 26.08 also ships
  `libgtk-3.so.0`, which FLTK dlopens for its native file chooser, so the
  GNOME runtime is not needed.
- **App id: `com.ferrispad.FerrisPad`.** It is upstream's own id, the same as
  `packaging/flatpak/`.
- **Permissions.**
  - `ipc`, `wayland`, `fallback-x11`: the window. There is no `--device=dri`,
    because FLTK draws with cairo in software.
  - `--filesystem=home`: declared as dangerous. FLTK's file chooser shows the
    sandbox's own view of the filesystem and does not use the file chooser
    portal, so without a grant the user's files are invisible to an editor
    whose main job is opening and saving them.
  - `--share=network`: plugin downloads from the registry, and the MCP
    server's localhost TCP port.
  - `host` is narrowed to `home`. Files outside home (`/etc`, mounted drives)
    are an opt-in `flatpak override` documented in the metainfo.
- **Updater.** FerrisPad detects `/.flatpak-info` and turns its updater off,
  so no seeded config is needed and the package level stays "Unmodified".
- `resolve-update.sh` builds the asset name from the latest release tag
  (`0.9.6` → `FerrisPad-v0.9.6-linux-amd64`) and matches it exactly. The
  release also has the `.sig`, the `.deb`, and the macOS/Windows assets.
- **Metainfo.**
  - It says the package is maintained by FerrisPad's developer and installs
    the official binary unmodified.
  - It notes that the built-in terminal runs inside the sandbox, and that MCP
    agents connect through `flatpak run com.ferrispad.FerrisPad --mcp-server`.
  - It has one `<release version="0.9.6">`.
- `docs/guides/BUILD_GUIDE.md`:
  - new "Option 4: FlatPark package" (install, layout, how updates work, how to
    check a packaging change);
  - the stale Flathub notes under Option 3 are removed.

### Verification run by Claude (2026-10-20)

All in a clone of flatpark/flatpark@43f67e6, with the package copied to
`registry/`.

| Check | Result |
|---|---|
| `node scripts/read-descriptor.mjs` / `audit-descriptor.mjs` | OK / exit 0 |
| `./scripts/check-apply-extra.sh com.ferrispad.FerrisPad` | "apply_extra OK as root with no capabilities" |
| `./scripts/publish.sh --verify com.ferrispad.FerrisPad` | built, signed, "installed … from local signed repo OK" |
| `./resolve-update.sh` | `0.9.6`, `2026-10-20`, the exact asset URL |
| Starts and draws (Xvfb `:99`, X11) | window drawn, CLI file opened with Rust highlighting |
| Window class | `WM_CLASS` = `FerrisPad`, equal to `StartupWMClass` |
| Main feature | Ctrl+O shows the GTK 3 file chooser on `$HOME`; typing + Ctrl+S saved a file in `$HOME` |
| Data survives a restart | session written to `~/.var/app/com.ferrispad.FerrisPad/data/ferrispad/session/default/session.json` and restored on the next start |
| Nothing new in home | `ls -A ~` diff: only the test file |
| Bus names | `--log-session-bus`: no `Filtering` lines |
| Updater off | `/.flatpak-info` present in the sandbox (what `is_flatpak()` checks) |

Run notes:
- `publish.sh` needed `FLATPARK_ALLOW_SHALLOW=1` (shallow clone), and a host
  `flatpak-builder` plus `appstream` from nixpkgs (Fedora's `appstreamcli` has
  no compose addon).
- **Not tested:** a fresh first run with no `~/.var/app/com.ferrispad.FerrisPad`
  (the user's own data was there, and was backed up and restored); the Wayland
  socket; aarch64 (no upstream arm build).
- **Found while testing, but not caused by the package:** Escape in the editor
  hides FerrisPad's main window. The native 0.9.6 binary does the same, so it
  is a separate ticket.

## How to test

### 1. Install the package from a local FlatPark repo (manual)

```bash
git clone https://github.com/flatpark/flatpark.git ~/flatpark-test && cd ~/flatpark-test
cp -r ~/Code/personal/ferrispad/packaging/flatpark/com.ferrispad.FerrisPad registry/
./scripts/check-apply-extra.sh com.ferrispad.FerrisPad
FLATPARK_ALLOW_SHALLOW=1 FLATPARK_VERIFY_KEEP=1 ./scripts/publish.sh --verify com.ferrispad.FerrisPad
flatpak run com.ferrispad.FerrisPad//stable
```

Expect:
- the editor opens;
- File → Open shows the GTK file chooser on your home, and you can open and
  save a file there;
- Help → About shows 0.9.6;
- no update banner at startup.

### 2. Clean up

```bash
flatpak kill com.ferrispad.FerrisPad
flatpak --user uninstall -y com.ferrispad.FerrisPad//stable
flatpak --user remote-delete --force flatpark-local
```

Your local `master` build stays installed. Both share
`~/.var/app/com.ferrispad.FerrisPad`.

### 3. Checksum failure

Change one hex digit of `sha256` in the manifest and rerun `check-apply-extra.sh`.
Expect it to fail on the checksum.

## Acceptance criteria

- [x] FlatPark's requirements are summarised in this ticket with links.
- [x] Manifest builds, installs and runs locally from the published release asset.
- [x] Permissions are justified in the manifest and the ticket.
- [ ] The submission content is ready, and the user has decided whether to open it.
- [x] Repo gates stay green (no Rust changes).

## Submission draft (not opened, awaiting the user's OK)

The user chooses the GitHub account and the fork. The steps:
1. Fork flatpark/flatpark.
2. Create branch `add/com.ferrispad.FerrisPad`.
3. Copy this directory to `registry/com.ferrispad.FerrisPad/`.
4. Optionally add a row to `docs/reference/upstream-approvals.md` and set
   `catalog.upstream_approved: true`. The PR is the evidence, because the
   developer submits it.

**Title / commit subject:**
`feat(ferrispad): add FerrisPad, a fast minimalist text editor (com.ferrispad.FerrisPad)`

**Body:**

> **What:** FerrisPad (`com.ferrispad.FerrisPad`), a single-binary text editor
> written in Rust with FLTK: syntax highlighting, tabs and sessions, a sandboxed
> Lua plugin system, a built-in terminal. I'm FerrisPad's developer and submit
> this package myself.
>
> **Packaging:**
> - The download is the official `FerrisPad-v<ver>-linux-amd64` executable from
>   GitHub releases, pinned as extra-data. `apply_extra.sh` only moves it into
>   place, and the wrapper only `exec`s it, so the package is Unmodified.
> - The runtime is `org.freedesktop.Platform` 26.08. Everything the binary links
>   is in it, including the GTK 3 that FLTK loads for its file chooser.
> - The app detects Flatpak and turns its own updater off. `resolve-update.sh`
>   follows GitHub's latest release.
>
> **Sandbox:**
> - `ipc`, `wayland`, `fallback-x11` for the window. There is no `dri`, because
>   FLTK renders with cairo.
> - `--filesystem=home`, declared as dangerous: opening and saving the user's
>   files is the app's main job, and FLTK's file chooser shows the sandbox's own
>   view of the filesystem rather than using the file chooser portal.
> - `--share=network` for plugin downloads and the MCP server's localhost port.
> - Files outside home are documented as a `flatpak override --filesystem=host`
>   line in the metainfo, not granted.
>
> **Tested** (Fedora 44, X11 on a private Xvfb display):
> - `check-apply-extra.sh` and `publish.sh --verify` pass.
> - The window draws, and `WM_CLASS` matches `StartupWMClass`.
> - I opened a file from the CLI and from the GTK file chooser, edited it and
>   saved it to `$HOME`.
> - The session is restored after a restart, under `~/.var/app`.
> - Nothing new appeared in the home folder, and `--log-session-bus` shows no
>   filtered names.
> - Not tested: Wayland inside the sandbox, a first run with no
>   `~/.var/app/com.ferrispad.FerrisPad`.
>
> **Flathub:** not on Flathub.
>
> 🤖 Generated with [Claude Code](https://claude.com/claude-code)

## Affected files

- `packaging/flatpark/com.ferrispad.FerrisPad/`: new; `flatpark.yml`, manifest,
  `apply_extra.sh`, `ferrispad-wrapper`, `resolve-update.sh`, desktop file,
  metainfo, icon
- `docs/guides/BUILD_GUIDE.md`: FlatPark section

## Notes

- Origin: user request, 2026-10-19, after the Flathub rejection.
- FlatPark is not affiliated with Flathub.
- The package follows releases with no step on our side. A later option is to
  have `release.yml` call FlatPark's release hook (`release-dispatch.yml`), so
  updates land within minutes instead of at the next twice-daily check.
- The user already has the `flatpark` remote on their `--user` installation.
