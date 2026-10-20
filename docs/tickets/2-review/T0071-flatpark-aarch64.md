---
id: T0071
title: ARM64 (aarch64) in the FlatPark package
status: review
created: 2026-10-20
severity: ~
area: process
depends-on: [T0068]
issue: 129
---

## Goal

FlatPark users on ARM64 Linux (Raspberry Pi, Pinebook, ARM laptops) cannot
install FerrisPad: the package pins only the x86_64 executable. Since 0.9.8
every release carries a signed `FerrisPad-v<ver>-linux-arm64` (T0068), and
FlatPark builds packages for aarch64 too. Add it.

## In scope

- `packaging/flatpark/com.ferrispad.FerrisPad/`, the mirror of
  `registry/com.ferrispad.FerrisPad/` in flatpark/flatpark:
  - `flatpark.yml`: `build.arches: [x86_64, aarch64]`;
  - manifest: two extra-data sources in the managed block, one per arch
    (`only-arches`). Both use the filename `FerrisPad-linux`, so one
    `apply_extra.sh` serves both. Both are pinned to 0.9.8:
    - x86_64: `…/0.9.8/FerrisPad-v0.9.8-linux-amd64`, sha256 `89b5bd4b…`,
      13972232 bytes;
    - aarch64: `…/0.9.8/FerrisPad-v0.9.8-linux-arm64`, sha256 `5f68bf09…`,
      13575432 bytes;
  - `resolve-update.sh` emits both sources with `arch`, as FlatPark's
    `basics.md` describes and the `figcraft` package does;
  - `apply_extra.sh` uses the new filename;
  - metainfo: `<release version="0.9.8">`;
  - `catalog.upstream_approved: true`, which was only in FlatPark's copy, so
    the mirror stays identical to the registry.
- The PR on flatpark/flatpark, opened only with the user's OK.

## Out of scope

- The website and README text saying the Flatpak is x86_64 only. That changes
  once the FlatPark PR is merged.

## How to test

Run in a flatpark/flatpark checkout with this directory copied to
`registry/com.ferrispad.FerrisPad/`. All of these passed on 2026-10-20:

1. `node scripts/audit-descriptor.mjs registry/com.ferrispad.FerrisPad/flatpark.yml`
   passes.
2. `node scripts/read-extra-data.mjs registry/com.ferrispad.FerrisPad/com.ferrispad.FerrisPad.yml aarch64`
   lists the arm64 URL, and with `x86_64` it lists the amd64 one.
3. `./scripts/check-apply-extra.sh com.ferrispad.FerrisPad`: "apply_extra OK
   as root with no capabilities" (host arch x86_64).
4. `./scripts/publish.sh com.ferrispad.FerrisPad` builds and signs (x86_64).
5. `bash scripts/check-approvals.sh`: in sync.
6. `cd registry/com.ferrispad.FerrisPad && ./resolve-update.sh` gives
   version 0.9.8 with both sources.

**Not tested here:** building and running the aarch64 Flatpak. There is no ARM
machine, and the x86_64 host has no aarch64 runtime. FlatPark's PR CI builds
aarch64 (`build-changed (aarch64)` passed on #773), and the binary itself is
tested natively on ARM by FerrisPad's `test (linux arm64)` CI job.

## Acceptance criteria

- [ ] Mirror and registry copy identical.
- [ ] The user decided on opening the FlatPark PR.
- [ ] After merge: `flatpak remote-ls flatpark` lists FerrisPad for aarch64.

## Submission draft

- Branch `fix/ferrispad-aarch64` on the fedro86/flatpark fork, commit
  `bc64117`, author set to the noreply address.
- **Title:** `feat(ferrispad): aarch64 build, update to 0.9.8`
- **Body:**

> FerrisPad 0.9.8 publishes a signed ARM64 Linux executable
> (`FerrisPad-v0.9.8-linux-arm64`) next to the x86_64 one. This pins both as
> extra-data under one filename with `only-arches`, adds `aarch64` to
> `build.arches`, and makes `resolve-update.sh` resolve both assets (same shape
> as `ai.storyteller.figcraft`). It also bumps the package to 0.9.8 and ends
> the desktop file with a newline, as suggested in #773.
>
> **Tested:**
> - `audit-descriptor`, `check-approvals` and `check-apply-extra` pass (x86_64
>   host);
> - `read-extra-data` resolves the right asset per arch;
> - `publish.sh` builds (x86_64);
> - the resolver prints both sources for 0.9.8;
> - the ARM64 binary itself is built and tested on a native ARM runner in
>   FerrisPad's CI, and its signature verifies.
>
> **Not tested:** the aarch64 Flatpak on real hardware (no ARM machine here),
> so I'm relying on this repo's aarch64 build job.
>
> I'm FerrisPad's developer.
>
> 🤖 Generated with [Claude Code](https://claude.com/claude-code)
