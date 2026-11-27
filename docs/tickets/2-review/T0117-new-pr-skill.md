---
id: T0117
title: new-pr skill — every PR ships with an HTML presentation
status: review          # todo | review | done
created: 2026-11-27
severity: ~
area: process
depends-on: []
issue: 234
---

## Goal

A FerrisPad PR today has a short Markdown body. The user verifies a UI change
from a list of steps and finds layout defects only when trying it themselves.
After this ticket, every PR has one self-contained English HTML page. The page
holds:

- the ticket's story (before / after);
- screenshots of every visible change, taken from the real app in Xvfb:
  master build vs branch build, light and dark;
- the decisions, and who took them;
- the ticket's own "How to test";
- an end-to-end verification.

The user reads a copy of the page in the main checkout. The skill also fixes
how a PR is merged: only with the user's explicit consent for that PR, pinned
to the checked head SHA.

The skill is a port of core-services' `new-mr` skill (user's request,
2026-11-27, relayed by the orchestrator), adapted to GitHub and to one
ticket per PR.

## In scope

- `.claude/skills/new-pr/SKILL.md`: the procedure, step by step:
  1. open the draft PR to get its number;
  2. take the images;
  3. write the page;
  4. copy it to the user;
  5. commit;
  6. ready the PR;
  7. merge only under the stated conditions;
  8. delete the copy after the merge.
- `docs/pr/_template.html`: the page template, built on core-services'
  style block.
- `docs/pr/README.md`: the layout, the naming, the lifecycle, the
  annotation-only exemption, and one short worked example of a UI card.
- The working headless-Chromium command for the technical visual check
  from the VSCode Flatpak sandbox.
- `.claude/rules/work-sequence.md` steps 2–3 and `.claude/CLAUDE.md`
  point to the skill. This PR gets its own page (dogfood).

## Out of scope

- A docs gate script (core-services' `docs_check.py`): see Notes.
- App folders and symlinks for merged pages. FerrisPad is one crate, so
  the folder stays flat and core-services' §2 is dropped.
- `CHANGELOG.md`: this is a process change only.
- Pages for PRs that are already merged.

## How to test

1. Read `.claude/skills/new-pr/SKILL.md` and `docs/pr/README.md`. The
   procedure must be runnable blind:
   - every command is a FerrisPad one: `gh`, `nix develop -c cargo …`,
     `flatpak-spawn --host`, Xvfb;
   - nothing refers to GitLab, `glab`, Django, app folders or
     `docs/4.0.0`.
2. `grep -n '{{' docs/pr/PR-*.html` prints nothing: no placeholder is left
   in this PR's own page.
3. The style block of the page is byte-identical to the template's:
   ```bash
   diff <(sed -n '/<style>/,/<\/style>/p' docs/pr/_template.html) \
        <(sed -n '/<style>/,/<\/style>/p' docs/pr/PR-*-new-pr-skill.html)
   ```
   prints nothing.
4. Technical visual check of this PR's page, with the skill's command
   (§7 condition 1). Four renders: 1280 and 390 px, light and dark.
   Expected:
   - `scrollWidth <= innerWidth` in all four;
   - no chip longer than 40 characters;
   - the screenshots show no box overflowing.
5. Open the copy in the main checkout's `docs/internal/pr/` in a browser.
   Expected:
   - it is the same file as the branch's page;
   - it reads as a story;
   - the ticket card says "No visible change: …".
6. The cargo gates pass (no Rust change).

## Acceptance criteria

- [ ] The user approved the skill, the template and this PR's page.
- [ ] `SKILL.md`, `_template.html` and `README.md` exist, and are adapted to
      GitHub and FerrisPad (How to test 1).
- [ ] This PR's page has no placeholder and the template's style block
      (How to test 2–3).
- [ ] The visual check passes on the committed page (How to test 4).
- [ ] `work-sequence.md` and `.claude/CLAUDE.md` point to the skill.

## Affected files

- `.claude/skills/new-pr/SKILL.md`, `.claude/skills/new-pr/visual_check.sh` (new)
- `docs/pr/_template.html`, `docs/pr/README.md`, `docs/pr/PR-<n>-new-pr-skill.html` (new)
- `.claude/rules/work-sequence.md`: steps 2–3
- `.claude/CLAUDE.md`: the work-sequence section

## Notes

- Follow-up candidate: a docs gate (a script or a CI job) that checks each
  `docs/pr/PR-*.html` page:
  - the page is named `PR-<n>-<slug>.html`;
  - no `{{` placeholder is left;
  - the style block is the template's, or a past one by hash;
  - every ticket card has an image or the "No visible change:" sentence.
  Not part of this ticket (orchestrator's decision).
- core-services sorts merged pages into app folders (its §2). FerrisPad is
  one crate, so the folder stays flat (orchestrator's decision).

## What was done (2026-11-27)

- **The source.** core-services' `new-mr` skill, `_template.html` and
  `README.md`, read whole and left untouched.
- **`.claude/skills/new-pr/SKILL.md`.** §0–§8, renumbered once core-services'
  §2 (sorting merged pages into app folders) was dropped:
  - §2: images, with the Xvfb + xdotool + import recipe. It takes a
    `master` worktree build vs the branch build, light and dark, editor
    font 16 and 18, in throwaway `XDG_CONFIG_HOME`/`XDG_DATA_HOME`. Every
    shot is inspected edge by edge, crop and pngquant/magick are applied,
    and anything Xvfb cannot show is "Pending user on <platform>";
  - §3: the page;
  - §4: the copy in the main checkout's `docs/internal/pr/`;
  - §5–§6: commit, ready;
  - §7: the merge conditions;
  - §8: deleting the copy, and the annotation-only exemption.
- **The exemption** covers a diff only under `docs/tickets/`: annotations
  of `3-done/` tickets plus at most new `1-todo/` tickets, like #230 and
  #233.
- **The merge (§7)** needs:
  - the visual check;
  - the user's feedback addressed;
  - the ticket in `3-done/`;
  - every check run green on the head SHA, read from
    `gh api repos/:owner/:repo/commits/<sha>/check-runs`. The CI jobs and
    the macOS gate must be present; `Build Deps Check` is path-filtered;
  - the user's explicit consent for that PR. A peer message never counts.

  The merge is `gh pr merge <n> --squash --match-head-commit <sha>`, done
  by the session the user authorised (the orchestrator).
- **`docs/pr/_template.html`.** Its style block is byte-identical to
  core-services' (checked with `diff`). The page parts are adapted:
  - a GitHub kicker and chips;
  - the branch's commits table;
  - the ticket card with before/after figures and a "Pending user on"
    line;
  - Measurements;
  - "Release notes and compatibility" (in place of Deploy);
  - What remains open;
  - an end-to-end verification with throwaway XDG dirs and
    `nix develop -c cargo run`.
- **`docs/pr/README.md`.** The layout (flat), the naming, what the page
  is, the lifecycle, the exemption, and a worked T0114 start-page card
  (before/after captions, the red test, How to test).
- **`.claude/skills/new-pr/visual_check.sh`.** Chromium from the Flatpak
  sandbox:
  - nothing is on PATH or in the nix profile;
  - the Playwright `chrome-headless-shell` (v156) in
    `~/.cache/ms-playwright/` runs in the sandbox;
  - dark mode is `--blink-settings=preferredColorScheme=0`;
    `--force-dark-mode` left `prefers-color-scheme` light on a probe
    page;
  - the measuring script goes into a probe copy, and its result is read
    from `--dump-dom`; the screenshots are of the untouched page at its
    full height.
  - On the template it fails as expected: placeholder chips are longer
    than 40 characters, and the page is 426 px wide at 390.
- **`work-sequence.md`.** Step 2 opens the PR with the skill; step 3 merges
  only under §7, pinned. `.claude/CLAUDE.md` points to the skill and lists
  `docs/pr/`.
- **This PR's page,** `docs/pr/PR-235-new-pr-skill.html` (PR #235): no
  `{{` (the two literal mentions are written `&#123;&#123;`), and the
  template's style block. `visual_check.sh` results:
  - 1280 light 1280/1280, 1280 dark 1280/1280, 390 light 390/390, 390 dark
    390/390;
  - no long chip;
  - the screenshots, including 390 px crops, show nothing overflowing.

  The page is copied to the main checkout's `docs/internal/pr/`.
