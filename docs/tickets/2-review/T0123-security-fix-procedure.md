---
id: T0123
title: A written procedure for fixing security defects privately
status: review
created: 2026-11-28
severity: ~
area: process
release: 0.9.11
depends-on: [T0122]
issue: 252
---

## Goal

Security defects are fixed in private and disclosed with the release that
ships the fix. Until now that rule lived only in the user's instructions to
the sessions. Each session had to be told again what "private" means: no
issue, no push, no PR, no public ticket. A written rule loaded from
`.claude/rules/` makes every session handle the next finding the same way,
including one that starts after a session reset.

## In scope

- New `.claude/rules/security-fixes.md`:
  - **Triage.** What counts as a security defect: input an attacker can shape,
    such as a plugin, a plugin manifest or registry entry, a crafted file, a
    URL, a session or settings file, or a terminal byte stream, that crosses a
    trust boundary or misleads a trust decision. A crash on ordinary local
    input stays a normal public bugfix ticket.
  - **Private workspace.**
    - Findings and their tickets are written as
      `docs/internal/security/S<N>-<slug>.md`. `docs/internal/` is
      gitignored, so they never reach a commit.
    - The ticket format is the same as `_template.md`, with its own `S` IDs.
    - Code goes on local `sec/<slug>` branches with no upstream.
    - Commit messages, branch names and peer messages that could be pasted
      somewhere name the area, not the exploit.
  - **Work loop.**
    - The red test comes first, and its run is recorded in the S ticket.
    - The gates run through `scripts/gates.sh`.
    - The reviewer reviews in the named worktree. Verdicts go by message only.
    - The user verifies locally, as for any ticket.
  - **Disclosure with the release.**
    - Each S ticket becomes a normal T ticket and GitHub issue, and the
      branch is pushed and lands through a PR, in the release that ships it.
    - CHANGELOG gets a `### Security` section and the `SECURITY.md` changelog
      gets a row, with the credit if a reporter asked for it.
    - The order is: all fixes on `master`, then the tag, then the publication.
  - **Never.**
    - No public issue, PR, comment, discussion or blog post about an
      undisclosed finding.
    - Do not push a `sec/` branch.
    - Do not hand the finding to another session that has not been told it is
      private.
- `.claude/CLAUDE.md`: one line pointing to the new rule next to
  `work-sequence.md` and `engineering-standards.md`.
- `.claude/rules/work-sequence.md`: one sentence that the T-ticket and
  GitHub-issue steps apply to security defects only at disclosure (see
  `security-fixes.md`).

## What was done

- `.claude/rules/security-fixes.md` (new), in five parts:
  - triage (what counts, and that an ordinary local crash stays public);
  - the private workspace: `docs/internal/security/S<N>-*.md`, `sec/`
    branches with no upstream, the append-only MERGE-NOTES, wording that
    names the area and not the exploit;
  - the work loop: red test first (`#[ignore]` when the red test must be
    committed before its fix), `scripts/gates.sh`, private compare.sh
    scenarios and montages, review by message in the named worktree, a
    blind recipe with throwaway XDG directories, and an integration dry run
    of all `sec/` branches before the release;
  - disclosure with the release: T ticket and issue at that point, the
    CHANGELOG `### Security` section and the `SECURITY.md` Changelog row,
    all fixes on `master` before the tag and before anything public, and a
    GitHub advisory record as the user's call per release;
  - the "never" list.
- It describes the procedure used so far. It names
  none of them and gives no exploit detail: the examples are generic
  categories, and the sample areas are unrelated ones.
- `S` numbers: plain `S<N>` (S1, S2, …), as used, rather than the four-digit
  form first proposed.
- `.claude/CLAUDE.md`: one line in the list of binding rules.
  `.claude/rules/work-sequence.md`: one bullet under *GitHub issue tracking*.

## Out of scope

- Fixing any particular finding.
- `SECURITY.md` "Reporting Vulnerabilities": the missing contact
  (`[security contact to be added]`) and turning on GitHub private
  vulnerability reporting are the user's decision. They are noted below, not
  changed here.
- GitHub Security Advisories with a temporary private fork (see Notes).

## How to test

Documentation and rules only: no Rust changes, so no new cargo test.

1. `git diff --stat origin/master` lists only `.claude/rules/security-fixes.md`,
   `.claude/rules/work-sequence.md`, `.claude/CLAUDE.md` and this ticket.
2. `git check-ignore -v docs/internal/security/S0001-x.md` prints the
   `/docs/internal/*` rule from `.gitignore`. This proves an S ticket can't be
   committed by accident.
3. Read `security-fixes.md` in a fresh session with no prior context. Expect
   it to answer three questions without other files: where a new finding goes,
   what may be pushed, and when it becomes public.
4. The gates (`scripts/gates.sh`, from T0122) pass.

## Acceptance criteria

- [ ] `.claude/rules/security-fixes.md` covers triage, the private workspace,
      the work loop, disclosure and the "never" list.
- [ ] The rule names no current finding and gives no exploit detail.
- [ ] `CLAUDE.md` and `work-sequence.md` point to it.
- [ ] `docs/internal/security/` is confirmed gitignored.
- [ ] The gates pass.

## Affected files

- `.claude/rules/security-fixes.md`: new.
- `.claude/rules/work-sequence.md`: one sentence.
- `.claude/CLAUDE.md`: one line.

## Notes

- Origin: the user's rule "fix riservato, poi pubblico" (fix privately, then
  disclose), applied by hand so far. The user
  asked on 2026-11-28 to standardise it.
- It depends on T0122 because commits made before T0122 lands run the UI
  tests on the user's display.
- Decision (user, 2026-11-28): **local-only flow**. GitHub Security Advisories
  with a temporary private fork were considered and rejected for now: status
  checks do not run on PRs in a temporary private fork, so they would add no
  CI and only a second place to keep in sync. Publishing an advisory record at
  release time remains possible and is the user's call per release.
