---
id: T0043
title: Fix the commit rule — branch commits are allowed, master is the human gate
status: review
created: 2026-09-16
severity: minor
area: process
depends-on: []
issue: 34
---

## Goal

The work-sequence rule currently reads **"No commit while any ticket is in
`1-todo/` or `2-review/`"** — an unconditional ban on commits. That is wrong in
practice: the work has to be committed and pushed to a `ticket/T<NNNN>` branch
to move between machines at all, and since T0036 the CI gates only run on
`push`/`pull_request`, so an uncommitted ticket can never be checked by CI.

What human verification actually gates is the change **landing on `master`**,
not the existence of a commit. The rule must say that, in every place it is
restated — right now four files repeat the wrong version, and T0032 was flagged
as a rule violation during review purely because of it.

## In scope

- `.claude/rules/work-sequence.md` — the hard rule, plus the `1-todo/` →
  `2-review/` and `2-review/` → `3-done/` transition descriptions.
- `docs/tickets/README.md` — short-form rule 4, the directory tree comment, and
  the state-machine table.
- `.claude/CLAUDE.md` — the "no commit happens until the user has verified"
  sentence in the *Work sequence* section.
- `.claude/skills/new-ticket/SKILL.md` — the closing invariant that repeats the
  ban.
- The landing mechanism: a draft PR while in review, then a squash-merge at
  `3-done/` — no amend or force-push of a pushed ticket branch.

## Out of scope

- The pre-commit hook and the three gates — unchanged, they still run on every
  commit.
- Branch protection or any GitHub-side enforcement of the rule — it stays a
  documented process, backed by the pre-commit hook and CI.
- Re-auditing already-landed tickets against the corrected rule.

## How to test

Documentation only — no code changes, so the suite is a non-regression check.

### Recipe

```bash
# 1. No file may still carry the unconditional ban.
grep -rn "No commit while" .claude/ docs/tickets/README.md
#    Expected AFTER: no matches (before: 3 matches).

# 2. Every restatement names master as the gate.
grep -rn "master" .claude/rules/work-sequence.md docs/tickets/README.md \
  .claude/CLAUDE.md .claude/skills/new-ticket/SKILL.md
#    Expected: each of the four files mentions the master gate.

# 3. No step asks to amend or rebase a pushed ticket branch; landing is a squash-merge.
grep -rniE "amend|rebase" .claude/rules/work-sequence.md docs/tickets/README.md \
  .claude/CLAUDE.md .claude/skills/new-ticket/SKILL.md
#    Expected: only the "never amend or rebase a pushed branch" line.
grep -rn "squash" .claude/rules/work-sequence.md docs/tickets/README.md
#    Expected: transition 3, the one-commit hard rule, README rule 5.

# 4. Nothing else moved.
nix develop -c cargo test
#    Expected: green, unchanged from before the ticket.
```

### Manual read-through

Read `.claude/rules/work-sequence.md` top to bottom: the state machine, the
transition list and the hard rules must agree with each other — commit + push
on the ticket branch at `2-review/`, verification, then landing on `master` at
`3-done/`.

## What was done

The single new rule, worded the same way in all four places: *nothing reaches
`master` while a ticket is in `1-todo/` or `2-review/`; committing and pushing
on the ticket's own branch is expected; the ticket reaching `3-done/` is what
authorises `master`.*

- `.claude/rules/work-sequence.md` — hard rule rewritten; state table now says
  `2-review/` is "committed and pushed on the `ticket/T<NNNN>` branch — **not on
  `master`**"; transition 2 now ends in commit + push + **draft** PR (and says
  why: T0036 wired CI to `push`/`pull_request`, so an uncommitted ticket cannot
  be checked at all); transition 3 amends the commit to carry `Closes #<issue>`
  and then lands it.
- `docs/tickets/README.md` — tree comment, short-form rule 4, state table.
- `.claude/CLAUDE.md` — *Work sequence* intro; the `work-sequence.md` bullet now
  calls it "the `master` gate", not "the commit gate".
- `.claude/skills/new-ticket/SKILL.md` — closing invariant.

Verification run (see the recipe above):

```
grep -rn "No commit while" .claude/ docs/tickets/README.md   → no matches (was 3)
grep -rni "not committed|do not commit"                      → no matches
master gate named in all four files                          → 9 / 6 / 2 / 1 hits
```

Review round 1 (2026-09-23) — fixes applied on the branch:

- Transition 3 no longer amends the pushed branch commit (that needs a
  force-push, which is what blocked the T0032 landing). The `3-done/` move is a
  new branch commit; the PR is **squash-merged**, and the squash commit body
  carries `Closes #<issue>` + `Co-Authored-By`.
- "One ticket, one commit" is now "one commit **on `master`**": the ticket
  branch may hold WIP, review-fix and merge-from-`master` commits; the squash
  collapses them (`work-sequence.md` hard rules, `README.md` rule 5).
- *GitHub issue tracking* → "Close when the ticket lands" rewritten to match
  (it still said "`3-done/` + commit" and offered a manual `gh issue close`).
- Out of scope no longer contradicts the diff on draft PRs; the landing
  mechanism is now explicitly in scope.
- The branch was brought up to date by merging `master` (not rebasing — no
  force-push; the squash-merge keeps `master` linear).

## Acceptance criteria

- [x] No file in `.claude/` or `docs/tickets/` states an unconditional commit ban.
- [x] All four restatements agree: committing/pushing on the ticket branch is
      expected; the user's verification is what authorises `master`.
- [x] The `2-review/` → `3-done/` step still requires user approval, and still
      carries `Closes #<issue>`.
- [x] No step requires amending, rebasing or force-pushing a pushed ticket
      branch; `master` gets exactly one (squash) commit per ticket.
- [x] `cargo test`, `cargo clippy --all-targets --all-features`, and
      `cargo fmt --check` all pass with zero warnings.

## Affected files

- `.claude/rules/work-sequence.md` — hard rule + transitions 2 and 3
- `docs/tickets/README.md:16-17,29-31,46-47` — tree comment, rule 4, state table
- `.claude/CLAUDE.md` — *Work sequence* intro sentence
- `.claude/skills/new-ticket/SKILL.md` — closing invariant

## Notes

- Origin: fell out of the T0032 review. The commit on `ticket/T0032` was
  reported as a rule violation; the user's call is that the rule is the defect
  ("work has to be committed if I am to move between computers").
- Interacts with T0036 (CI on push/PR): without a pushed branch commit those
  gates cannot run at all, which is independent evidence the old wording was
  unworkable.
