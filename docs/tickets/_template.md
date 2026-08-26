---
id: T0000
title: Imperative one-liner — what this ticket delivers
status: todo            # todo | review | done
created: 2026-08-24
severity: ~             # severe | moderate | minor   (for audit-derived tickets)
area: ~                 # security | robustness | ui | process
depends-on: []          # other ticket IDs that must land first
issue: ~                # GitHub issue number, filled when work starts
---

## Goal

One to three sentences on **why** we're doing this. The user-facing or
system-level outcome a reviewer should keep in mind. For a defect, state the
observed wrong behaviour, not the fix.

## In scope

- The bullet list of changes this ticket makes.
- Keep it small — if you find yourself writing more than ~5 bullets,
  split into multiple tickets.

## Out of scope

- Things a reader might assume are part of this ticket but aren't.
- The explicit boundary protects against scope creep during review.

## How to test

Concrete enough that a human can execute the recipe blind. For a defect,
lead with the **red test** (see `work-sequence.md` → "Bugfix tickets").

### Regression test

```bash
nix develop -c cargo test <test_name>
```

- Before the fix: the new test FAILS for the bug's reason (record the panic /
  wrong output / assertion here).
- After the fix: it passes, and the full suite stays green.

### Manual repro (if the bug is UI/interactive)

1. `nix develop -c cargo run`
2. Do `<specific input>`.
3. Expect `<observable outcome>` (before: `<wrong outcome>`).

## Acceptance criteria

- [ ] Yes/no statement #1 (must be true).
- [ ] Regression test added and green; it failed before the fix.
- [ ] `cargo test`, `cargo clippy --all-targets --all-features`, and
      `cargo fmt --check` all pass with zero warnings.

## Affected files

- `path/to/file.rs:line` — what changes here

## Notes

- Origin: which audit finding this came from (e.g. "plugin/services audit S3").
- Open questions, related tickets, design context.
