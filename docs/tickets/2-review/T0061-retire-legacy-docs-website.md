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

## How to test

1. `git ls-files docs | grep -vE '^docs/(tickets|guides|assets)/'` prints nothing
   (or only `docs/index.html` with option b).
2. `./scripts/bump-version.sh` dry run on a scratch branch: it touches no `docs/`
   web files and still updates `Cargo.toml`, `README.md` and `build-releases.sh`.
3. `grep -rn 'docs/js\|docs/index.html\|sync.*website' scripts .claude/CLAUDE.md docs/guides`
   prints nothing stale.
4. After landing:
   - with (a), `curl -s -o /dev/null -w '%{http_code}' https://fedro86.github.io/ferrispad/`
     gives 404;
   - with (b), the page redirects to www.ferrispad.com.
5. The release workflow still finds `docs/assets/logo-transparent.png`. That path
   is unchanged, and it is confirmed at the next tag.

## Acceptance criteria

- [ ] Legacy site files gone; assets, guides and tickets intact.
- [ ] Scripts and docs no longer reference the old site.
- [ ] Pages handled as the user chose.

## Notes

- Origin: user, 2026-10-19 ("forse possiamo toglierci di mezzo questo vecchio sito").
- Do it after T0060 lands, because T0060's version bump still edits `docs/js/main.js`.
