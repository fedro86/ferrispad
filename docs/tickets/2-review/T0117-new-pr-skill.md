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
