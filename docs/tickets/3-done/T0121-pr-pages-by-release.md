---
id: T0121
title: PR pages live in a folder per planned release (docs/pr/<release>/)
status: done            # todo | review | done
created: 2026-11-28
severity: ~
area: process
release: 0.9.11         # the release this ticket is planned for
depends-on: []
issue: 245
---

## Goal

The user's rule (2026-11-27): "dentro pr metti una sottocartella per la
release prevista tipo 0.9.11 e metti le pr lì dentro come regola". Today
every PR presentation sits flat in `docs/pr/`, so nothing tells which pages
belong to which release. After this ticket, a PR's page lives in the folder
of the release its ticket is planned for, and preparing a release means
checking one folder.

## In scope

- **Layout.** A page is `docs/pr/<release>/PR-<n>-<slug>.html`, where
  `<release>` is the planned release of the PR's ticket (e.g. `0.9.11`).
  `docs/pr/README.md`, `_template.html`, `examples/` and `scenarios/` stay
  in `docs/pr/`. The user's copy mirrors it:
  `docs/internal/pr/<release>/`.
- **`release:` in the ticket frontmatter** (`docs/tickets/_template.md`):
  the planned release. A new helper,
  `.claude/skills/new-pr/page_dir.sh <ticket.md>`, prints
  `docs/pr/<release>` and **fails loudly** (exit 1, a message naming the
  ticket) when the field is missing or is not a final release `X.Y.Z`
  (see Notes on pre-release tags).
- **Docs and tools.** The new-pr `SKILL.md` (every path, the copy in §4, the
  deletion in §8), `docs/pr/README.md`, `visual_check.sh` if it assumes the
  flat path, and any path in `.claude/rules/work-sequence.md` and
  `.claude/CLAUDE.md`.
- **Move the pages on master** that landed after 0.9.10 (#206, #209, #213,
  #215, #235, #240, and #201, which landed while this ticket was open) into
  `docs/pr/0.9.11/` with `git mv`, and fix the
  references to them (the `3-done/` tickets and the pages themselves).
- **The release-prep rule**, written into the skill and the README: before
  tagging `X.Y.Z`, every page in `docs/pr/X.Y.Z/` must be a PR merged into
  that release; a planned PR that slipped moves (with `git mv`) to the next
  release's folder.

## Out of scope

- Moving the pages of PRs still open: after this lands, each implementer
  merges `master`, adds `release:` to their own ticket, and moves their own
  page into `docs/pr/0.9.11/`.
- Adding `release:` to tickets in `1-todo/` that nobody works yet: the field
  is added when work starts (the helper refuses a page without it).
- Pages of 0.9.10 PRs: there are none in `docs/pr/` (the presentations
  started with T0117, after 0.9.10).

## How to test

No visible change in FerrisPad: a process and documentation change.

```bash
.claude/skills/new-pr/page_dir.sh docs/tickets/3-done/T0121-pr-pages-by-release.md
```

- prints `docs/pr/0.9.11`, exit 0.

```bash
.claude/skills/new-pr/test-page-dir.sh
```

- runs the helper on scratch tickets: `release: 0.9.11` → `docs/pr/0.9.11`;
  no `release:` → exit 1 with "no release: field"; `release: ~` and
  `release: next` → exit 1; a missing file → exit 1. Prints `ok` per case
  and exits 0 only if all pass.

```bash
git ls-files docs/pr | grep -E '^docs/pr/[^/]+\.html$'   # only docs/pr/_template.html
git ls-files 'docs/pr/0.9.11/'             # the eight moved pages, and #247's
git grep -nE 'docs/pr/PR-[0-9]' -- . ':(exclude)docs/pr/0.9.11'   # nothing
```

## Acceptance criteria

- [x] `page_dir.sh` prints the folder for a ticket with `release:`, and
      fails loudly without it; `test-page-dir.sh` passes.
- [x] `docs/tickets/_template.md` has `release:`.
- [x] The eight pages live in `docs/pr/0.9.11/`; no flat page is left, and no
      reference points to a flat path.
- [x] SKILL.md and README describe the layout, the copy path, the §8
      deletion path, and the release-prep rule.
- [x] Gates green (no Rust change); the page's rendering check passes.

## Affected files

- `docs/tickets/_template.md`: the `release:` field.
- `.claude/skills/new-pr/page_dir.sh`, `test-page-dir.sh` (new).
- `.claude/skills/new-pr/SKILL.md`, `docs/pr/README.md`,
  `.claude/skills/new-pr/visual_check.sh` (if needed),
  `.claude/rules/work-sequence.md`, `.claude/CLAUDE.md`.
- `docs/pr/PR-*.html` → `docs/pr/0.9.11/`; the `3-done/` tickets that name
  them.

## Notes

- Origin: the user's rule, relayed by the orchestrator (ferrispad-03) on
  2026-11-27. Every open PR depends on it, so it goes first.
- **Pre-release tags.** The folder is the final release: a ticket planned
  for 0.9.11 that ships in `0.9.11-rc.1` stays in `docs/pr/0.9.11/`.
  `release:` takes `X.Y.Z` only.

## What was done (2026-11-28)

- **`page_dir.sh <ticket>`** reads the `release:` field from the ticket's
  frontmatter only (a `release:` line in the body does not count) and
  prints `docs/pr/<release>`. It exits 1 with the ticket and the reason on
  stderr for: no file, no frontmatter, no field, and a value that is not
  `X.Y.Z` (`~`, a word, `0.9.11-rc.1`).
- **`test-page-dir.sh`**: 10 cases, all `ok` (`release`, `with-comment`,
  `two-digit`, `no-field`, `tilde`, `word`, `pre-release`,
  `missing-file`, `no-frontmatter`, `body-only`).
- **`docs/tickets/_template.md`** has `release: ~` with its comment.
- **Pages moved** with `git mv` to `docs/pr/0.9.11/`: #206, #209, #213,
  #215, #235, #240, #201 (landed during this ticket) and #246 (landed
  before the move to `3-done/`, moved in that commit). The paths that
  name them were rewritten in the eight `3-done/` tickets (T0073, T0095,
  T0104, T0111, T0112, T0116, T0117, T0122) and inside
  #235's page (its recipe and the user's copy). T0117's own recipe keeps
  its `docs/pr/PR-*.html` globs: it records how T0117 was verified, on the
  flat layout of that time.
- **The skill** (`SKILL.md`): the description, the intro, §0 (the
  `release:` precondition and `D=$(page_dir.sh …)`), §1 and §6 (the body's
  path), §3 (`$D/PR-<n>-<slug>.html`, the `{{` grep, the size chip's
  exclude `':(exclude)docs/pr/*PR-*.html'`), §4 (the copy into
  `docs/internal/pr/<release>/`), §7 (`visual_check.sh $D/…`), §8 (the
  deletion `docs/internal/pr/*/PR-<n>-*.html`) and a new "The release
  folder" section at the end of §3, with the pre-tag check.
- **Review (ferrispad-90, APPROVE bd8848c), two nits folded in:** "The
  release folder" moved from inside §8 to the end of §3, so §8's "The one
  PR without a presentation" reads on its own again; `test-page-dir.sh`'s
  closing `&& … ||` became an `if`/`else` (shellcheck SC2015). `docs/pr/README.md` (layout,
  rule, lifecycle), `.claude/rules/work-sequence.md` and
  `.claude/CLAUDE.md` follow. `visual_check.sh` takes the page path as an
  argument, so only its usage comment changed.
- **The pre-tag check, run on `docs/pr/0.9.11/`:** all six pages print
  `MERGED` (run before #201 landed; #201 is merged too).

### For the open PRs (after this lands)

Each implementer merges `master` into their branch, adds `release: 0.9.11`
to their ticket, moves their page with
`git mv docs/pr/PR-<n>-*.html docs/pr/0.9.11/`, fixes the paths in their
ticket and page, and moves the user's copy to `docs/internal/pr/0.9.11/`.
The flat copies already in `docs/internal/pr/` move the same way.

## User verification

The user approved the PR on 2026-11-28, relayed by the orchestrator
(ferrispad-03), after the reviewer's approval (ferrispad-90) at head bd8848c.
The reviewer's two nits are folded into the commit that moves this ticket to
`3-done/` (see *What was done*).
