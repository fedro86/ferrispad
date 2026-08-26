---
name: new-ticket
description: Open a new FerrisPad work ticket the collision-safe way — pick the next free T-ID across ALL ticket state dirs, scaffold from the template into docs/tickets/1-todo/. Use whenever a change needs a ticket (i.e. before ANY implementation work).
---

# Open a new ticket (collision-safe)

Every implementation change lands through a ticket (`docs/tickets/README.md`;
binding rules in `.claude/rules/work-sequence.md`). This skill covers the
∅ → `1-todo/` step only.

## 1. Pick the ID — check ALL three dirs, at the last moment

Parallel sessions can take IDs concurrently, so compute the next ID
**immediately before writing the file**, never earlier in the conversation:

```bash
command ls docs/tickets/1-todo/ docs/tickets/2-review/ docs/tickets/3-done/ 2>/dev/null \
  | grep -oE '^T[0-9]+' | sort -V | tail -1
```

Next ID = that + 1, zero-padded to 4 digits. IDs are monotonic, never reused,
never renumbered — even for abandoned tickets.

## 2. Scaffold from the template

Create `docs/tickets/1-todo/T<NNNN>-<short-slug>.md` with the structure of
`docs/tickets/_template.md` (frontmatter: `id`, `title`, `status: todo`,
`created`, `severity`, `area`, `depends-on`, `issue`). Leave `issue: ~` — it is
filled with the GitHub issue number when implementation starts (see
`.claude/rules/work-sequence.md` → *GitHub issue tracking*). The file name never
changes across state moves — only its directory does.

## 3. Authoring bars (what review will bounce)

- **How to test** is a concrete recipe a human can run blind — a `cargo test`
  invocation with the expected pass/fail, and/or a `cargo run` manual repro
  with the observable outcome. "Run the test suite" is NOT a recipe.
- **Red test first** for defects: the ticket must name the regression test and
  the failure it produces on the *current* code.
- **Small**: past ~5 in-scope bullets, split into dependent tickets.
- **Self-contained**: readable without prior ticket context.

## 4. Invariants from here

- **No implementation before the ticket file exists** — plan approval does not
  bypass the work sequence.
- **No commit while the ticket sits in `1-todo/` or `2-review/`** — user
  verification moves it to `3-done/`, then ONE commit (then push).
