---
id: T0061
title: Retire the legacy GitHub Pages website in docs/
status: review
created: 2026-10-19
severity: ~
area: process
depends-on: [T0060]
issue: 106
---

## Goal

The project website moved to www.ferrispad.com: a Django app in the private
repo `fedro86/fedro-websites` (`sites/ferrispad/`) that reads the latest version
from the GitHub tags API. The old static site in this repo's `docs/` is still
published by GitHub Pages at https://fedro86.github.io/ferrispad/ (source:
`master` `/docs`, HTTP 200, shows 0.9.5). It duplicates the real site, goes
stale on every release, and the release scripts and docs still spend effort
keeping it updated. Retire it.

## In scope

- Remove the static site files: `docs/index.html`, `docs/js/`, `docs/css/`,
  `docs/robots.txt`, `docs/sitemap.xml`, `docs/google41c8a47a8060834b.html`
  (Search Console verification for the old Pages URL).
- **Keep `docs/assets/`.** It is still used by `.github/workflows/release.yml`
  (macOS icon from `logo-transparent.png`), `scripts/build-releases.sh`, and the
  Flatpak metainfo screenshots (`raw.githubusercontent.com/.../docs/assets/screenshots/…`),
  which FlatPark (T0047) will reuse. Keep `docs/guides/` and `docs/tickets/`.
- `scripts/bump-version.sh`: stop editing `docs/js/main.js` and `docs/index.html`.
- `scripts/release.sh`: drop the "sync website files to master" block.
- Docs: `RELEASE_PROCESS.md` and `.claude/CLAUDE.md` ("sync the website"), and
  `scripts/build-releases.sh`'s "update docs/index.html" hint. Say that the website
  is a separate repo and picks up new tags by itself.
- GitHub Pages, **user decision**:
  - (a) Disable Pages (`gh api -X DELETE repos/fedro86/ferrispad/pages`). Old
    links to fedro86.github.io/ferrispad then 404.
  - (b) Keep Pages, and replace `docs/index.html` with a minimal page that redirects
    to https://www.ferrispad.com (meta refresh + canonical link + a visible link),
    so old links and search results land on the real site.
  **User chose (a), disable Pages, on 2026-10-19.** Run the API call when this
  ticket lands on master, not before, so the site and the file removal go away
  together.

## Out of scope

- Any change to `fedro86/fedro-websites`.
- Moving `docs/assets/` elsewhere: the URLs depend on its path.

## What was done

Branch `ticket/T0061`, stacked on `ticket/T0060` (#105), which must land first.

- Removed `docs/index.html`, `docs/js/main.js`, `docs/css/style.css`,
  `docs/robots.txt`, `docs/sitemap.xml`, `docs/google41c8a47a8060834b.html`.
  `docs/assets/`, `docs/guides/` and `docs/tickets/` are untouched.
- `scripts/bump-version.sh`: dropped the `docs/js/main.js` and `docs/index.html`
  steps and removed them from the `git add` list. It still bumps `Cargo.toml`,
  `README.md` and `scripts/build-releases.sh`, and auto-commits. A comment says
  that the website is the separate `fedro86/fedro-websites` app.
- `scripts/release.sh`: dropped the "Sync website files to master" block. It now
  ends by saying that the website picks up the new tag from the GitHub API.
- `scripts/build-releases.sh`: the closing hint no longer says to update
  `docs/index.html` or deploy the website.
- `docs/guides/RELEASE_PROCESS.md`: removed the website files from the list of
  bumped files, the "sync website" wording, and the "Releasing from a Feature
  Branch" section. It now says the site is a separate repo.
- `.claude/CLAUDE.md` (Releases): same wording fix, plus one line on what
  `docs/` now holds.
- `docs/guides/BUILD_GUIDE.md` (beyond the listed files, because check 3 greps
  `docs/guides`): the stale "Version Management" steps (`docs/index.html`, tags
  with a `v` prefix) now point to RELEASE_PROCESS.md. "Copy to Website" headings
  are now "Collect the Binaries" (the commands still copy into the gitignored
  local `docs/assets/binaries/`, which `build-releases.sh` uses). "Update website
  download links" is now "Check the download links in `README.md`".
- GitHub Pages was **not** touched. Its state when this ticket was written
  (`gh api repos/fedro86/ferrispad/pages`): `status: built`, source `master` `/docs`.

Evidence:
- `git ls-files docs | grep -vE '^docs/(tickets|guides|assets)/'` prints nothing.
- The grep in check 3 prints nothing.
- `bump-version.sh` dry run in a throwaway local clone of this branch
  (`./scripts/bump-version.sh -y 9.9.9-rc.1`, then the clone was deleted, nothing
  pushed): it printed Cargo.toml, README.md and build-releases.sh steps, exited 0,
  and committed `chore: bump version to 9.9.9-rc.1` touching `Cargo.toml` and
  `scripts/build-releases.sh`, with a clean tree after. `README.md` currently has
  no versioned download links, so its seds match nothing, as before this change.
  The script no longer branches on stable vs pre-release, so this one run covers
  both paths.
- `bash -n` passes for all three scripts. No Rust changed; fmt, clippy and test
  gates are green.

## How to test

1. `git ls-files docs | grep -vE '^docs/(tickets|guides|assets)/'` prints nothing.
2. `bump-version.sh` dry run in a throwaway clone (do not push):
   ```bash
   git clone -q --branch ticket/T0061 . /tmp/bumptest && cd /tmp/bumptest
   ./scripts/bump-version.sh -y 9.9.9-rc.1
   git show --stat HEAD     # Cargo.toml + scripts/build-releases.sh, no docs/ files
   git status --porcelain   # empty
   cd - && rm -rf /tmp/bumptest
   ```
3. `grep -rn 'docs/js\|docs/index.html\|sync.*website' scripts .claude/CLAUDE.md docs/guides`
   prints nothing.
4. Gates (no Rust change, run for completeness):
   `nix develop -c cargo test`, `nix develop -c cargo clippy --all-targets --all-features -- -D warnings`,
   `nix develop -c cargo fmt --check`.
5. **When this ticket lands on master** (main session, with the user's go-ahead,
   right after the squash-merge), disable GitHub Pages:
   ```bash
   gh api -X DELETE repos/fedro86/ferrispad/pages
   ```
   Then `gh api repos/fedro86/ferrispad/pages` returns 404, and after a few
   minutes `curl -s -o /dev/null -w '%{http_code}' https://fedro86.github.io/ferrispad/`
   prints `404`.
6. The release workflow still finds `docs/assets/logo-transparent.png`. That path
   is unchanged, and it is confirmed at the next tag.

## Acceptance criteria

- [x] Legacy site files gone; assets, guides and tickets intact.
- [x] Scripts and docs no longer reference the old site.
- [ ] Pages handled as the user chose: disable at landing (How to test, step 5).

## Notes

- Origin: user, 2026-10-19 ("forse possiamo toglierci di mezzo questo vecchio sito").
- Do it after T0060 lands, because T0060's version bump still edits `docs/js/main.js`.
