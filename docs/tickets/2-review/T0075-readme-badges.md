---
id: T0075
title: Show release, CI and license badges at the top of the README
status: review          # todo | review | done
created: 2026-11-07
severity: ~
area: process
depends-on: []
issue: 141
---

## Goal

A visitor landing on the GitHub repo should see at a glance which version is
current, whether `master` builds and passes its checks, and the license —
without scrolling to the releases page or the Actions tab.

## In scope

- `README.md`: one centred row of three badges under the tagline, inside the
  existing header `<div align="center">`:
  - **Release** — `img.shields.io/github/v/release/fedro86/ferrispad?sort=semver`,
    links to `releases/latest`;
  - **CI** — GitHub's native badge for `.github/workflows/ci.yml` on `master`,
    links to that workflow's runs;
  - **License** — `img.shields.io/github/license/fedro86/ferrispad`, links to
    `LICENSE`.

## Out of scope

- A downloads badge (it would count the `.sig` files the updater fetches).
- Decorative or slogan badges ("made with…", speed claims) — see T0076.
- Moving or replacing the FlatPark badge in the Install section.

## How to test

1. Each badge URL answers `200` with an SVG:
   ```bash
   for u in "https://img.shields.io/github/v/release/fedro86/ferrispad?sort=semver" \
            "https://github.com/fedro86/ferrispad/actions/workflows/ci.yml/badge.svg?branch=master" \
            "https://img.shields.io/github/license/fedro86/ferrispad"; do
     curl -sL -o /dev/null -w '%{http_code} %{content_type}\n' "$u"; done
   ```
2. Open the README on the `ticket/T0075` branch on GitHub: three badges sit
   centred under "A minimalist text editor written in Rust", reading
   `release v0.9.9`, `CI passing`, `license MIT`; each one links to the
   releases page, the CI workflow and `LICENSE` respectively.

## Acceptance criteria

- [x] The three badge URLs render (checked 2026-11-07: `release: v0.9.9`,
      `CI - passing`, `license: MIT`).
- [ ] The user checked the rendered README on the branch.
- [ ] The badges update on their own: no version number is hard-coded.

## Affected files

- `README.md` — header block

## Notes

- shields.io prints the version as `v0.9.9` although FerrisPad tags have no
  `v` prefix; that is its display convention, the link still targets the
  right release.
