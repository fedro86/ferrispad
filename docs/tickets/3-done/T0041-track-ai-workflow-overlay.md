---
id: T0041
title: Track the AI-assisted ticket system + .claude overlay; standardize the workflow
status: done
created: 2026-08-26
severity: minor
area: process
depends-on: []
issue: 30
---

## Goal

FerrisPad is officially an AI-assisted project, but the ticket system
(`docs/temp/`) and the `.claude/` overlay (rules, skills, the pre-commit hook,
`CLAUDE.md`) were gitignored — a local trial invisible to collaborators. Make the
workflow a tracked, standardized part of the repo, and add GitHub-issue tracking
so each worked ticket is auditable from GitHub.

## In scope

- Rename `docs/temp/` → `docs/tickets/` and repoint every reference
  (`.claude/**`, README, template, the in-flight ticket).
- Un-gitignore `.claude/` and `docs/tickets/` — **after** auditing for local /
  personal / secret references (none found). Keep `.claude/settings.local.json`,
  a root `CLAUDE.md`, and `.mcp.json` ignored for personal overrides; anchor the
  `CLAUDE.md` ignore to `/CLAUDE.md` so `.claude/CLAUDE.md` becomes tracked.
- Add a **GitHub issue per ticket** rule to `.claude/rules/work-sequence.md`
  (open at start of implementation, `Closes #<n>` on landing) + an `issue:`
  frontmatter field in the template/skill.

## Out of scope

- The `ci.yml` CI gates (T0036) — separate ticket/commit.
- Retro-filing GitHub issues for already-`3-done/` tickets.

## How to test

Process/docs change — no `cargo` behaviour changes. Verify:

```bash
# .claude/** and docs/tickets/** are tracked; only overrides stay ignored:
git check-ignore .claude/CLAUDE.md docs/tickets/README.md   # -> no output
git check-ignore .claude/settings.local.json CLAUDE.md .mcp.json  # -> all listed
# No NEW personal/secret leak. The owner handle `fedro86` does appear (signing
# docs) but is already public repo-wide (release.yml, CHANGELOG, issue templates):
grep -rniE "password|BEGIN .*PRIVATE KEY|xoxb-|ghp_|@.*\.(com|be)" .claude/  # -> none
grep -rn "/home/[a-z]" .claude/  # -> none (no local filesystem paths)
# No stale docs/temp references remain:
grep -rn "docs/temp" .claude/ docs/tickets/   # -> none
# Gates still green (pre-commit hook runs them anyway):
nix develop -c cargo test && nix develop -c cargo clippy --all-targets --all-features -- -D warnings && nix develop -c cargo fmt --check
```

## Acceptance criteria

- [ ] `docs/tickets/` replaces `docs/temp/`; all references updated.
- [ ] `.claude/` + `docs/tickets/` tracked; overrides still ignored; no local
      config or secret leaked.
- [ ] GitHub-issue-per-ticket workflow documented; `issue:` field added.
- [ ] Gates clean.

## Affected files

- `.gitignore`
- `docs/temp/` → `docs/tickets/` (whole tree), `README.md`, `_template.md`
- `.claude/CLAUDE.md`, `.claude/rules/work-sequence.md`,
  `.claude/skills/new-ticket/SKILL.md`

## Notes

- Origin: user directive (standardize AI usage). This is the first ticket to use
  the new GitHub-issue workflow — its own issue is the demonstration.
- Audit result: **no local filesystem path, key, token, or email**. The only
  `/home/` hit is a fictional `/home/user/a.rs` test path; `SIGNING_KEY` appears
  only as the secret's *name*; `~/.config/ferrispad/...` are generic XDG paths.
  The owner handle `fedro86` **does** appear in the signing docs — but it is
  already public repo-wide (`release.yml`, `CHANGELOG.md`, issue templates), so
  committing the overlay exposes nothing new. The one `fedro86/ferrispad` I had
  gratuitously added to `work-sequence.md` was removed (`gh` uses `origin`).

## Outcome (2-review)

Done as scoped. Tracking issue **#30** (T0036 also got its own, **#31**).

- **Rename:** `docs/temp/` → `docs/tickets/` (whole tree: 1-todo/2-review/3-done,
  `README.md`, `_template.md`, `INDEX.md`). Every `docs/temp` reference in
  `.claude/**` and the tickets was repointed (`grep -rn "docs/temp"` → none).
- **Un-gitignore:** removed the `.claude` and `/docs/temp/*` ignore lines; added
  `.claude/settings.local.json`; anchored the root ignore `CLAUDE.md` →
  `/CLAUDE.md` so it no longer matches `.claude/CLAUDE.md`. `git check-ignore`
  confirms `.claude/CLAUDE.md` and `docs/tickets/README.md` are tracked while
  `.claude/settings.local.json`, root `CLAUDE.md`, and `.mcp.json` stay ignored.
- **Local-reference audit (before un-ignoring):** no local filesystem path, key,
  token, or email. Fictional `/home/user/a.rs` test path only; `SIGNING_KEY` as a
  name, not a value; generic `~/.config/ferrispad/...` XDG paths. The owner handle
  `fedro86` appears in the signing docs but is already public across the committed
  repo (`release.yml` etc.), so it is not a new exposure — and the gratuitous
  `fedro86/ferrispad` I had added to `work-sequence.md` was removed.
- **Issue workflow:** added a *GitHub issue tracking* section to
  `work-sequence.md` (open at implementation start; `Closes #<n>` on landing),
  an `issue:` frontmatter field to `_template.md` and the `new-ticket` skill, and
  updated the "gitignored trial" notes in `.claude/CLAUDE.md` + `README.md` to
  "tracked".

**Commit note:** this is one `chore` commit (the overlay + rename + gitignore).
`ci.yml` (T0036) stays a separate commit. Committing the tracked ticket system
necessarily includes the other in-flight ticket files (T0036 in `2-review/`,
the `1-todo/` backlog) as tracking artifacts — their *deliverables* remain
uncommitted until each is verified.

## How to verify (reviewer recipe)

```bash
git check-ignore .claude/CLAUDE.md docs/tickets/README.md        # no output (tracked)
git check-ignore .claude/settings.local.json CLAUDE.md .mcp.json # all listed (ignored)
grep -rn "docs/temp" .claude/ docs/tickets/                      # none
grep -rn "/home/[a-z]" .claude/                                  # none (no local paths)
grep -rniE "password|BEGIN .*PRIVATE KEY|xoxb-|ghp_" .claude/    # none (no keys/tokens)
git grep -l "fedro86" -- .github CHANGELOG.md                    # already-public owner handle
gh issue view 30   # this ticket's tracking issue
```
