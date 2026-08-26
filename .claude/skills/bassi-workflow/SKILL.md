---
name: bassi-workflow
description: Use for any change to this repo (bassi) — bug fix, feature, refactor, chore. Ensures a Trello ticket exists, work follows TDD, and a PR is opened per CONTRIBUTING.md. Trigger on requests like "arregla X", "agrega Y", "implementa Z", "crea un ticket para...", or any code change to apps/nala.
---

# Bassi workflow

This repo (`bassi`) requires every code change to follow this loop:
**ticket → TDD → PR**. Do not skip a step, even for small changes.

## 1. Ticket (Trello board "Bassi", id `6a8ee0b9004a7396412719cf`)

- Check if the user already gave a ticket ID (`BAS-N`). If not, create one:
  - Use `mcp__trello__add_card_to_list` in the list matching the current
    stage (usually `To Do`, list id `6a8ee70e6f18d03d0234953d`).
  - The card's `idShort` from the response IS the ticket number. Rename the
    card immediately to `BAS-<idShort>: <title>` via `update_card_details`.
  - Apply labels: type (`Bug`/`Feature`/`Chore`/`Docs`) + `Epic: Nala`
    (label id `6a8ee0b9004a7396412719f7`).
  - Add an `Acceptance Criteria` checklist to the card.
  - Add a line item `BAS-<id>: <title>` to the `Tickets` checklist on the
    `EPIC: Nala` card (id `6a8ee744d84490d119e6874d`) so epic progress
    tracks it.
- If a card for this work already exists, move it to `In Progress`
  (list id `6a8ee710a2a533f3616c9d35`) instead of creating a new one.

## 2. Branch

`git checkout main && git pull && git checkout -b BAS-<id>-<short-desc>`

Never branch off another feature branch — always off up-to-date `main`.

## 3. TDD

For every behavior change:
1. Write the test first. Run it — it MUST fail (red). If it doesn't fail,
   the test isn't testing anything new; fix the test before continuing.
2. Write the minimum code to make it pass (green).
3. Refactor if needed, keeping tests green.
4. Repeat per behavior, not per file — small red/green cycles, not one
   big implementation followed by one big test file.

Do not write implementation code before its test exists and fails.

## 4. Verify

`bash scripts/check_rust.sh` must pass (fmt, clippy, check, test) before
committing. Run `bash scripts/check_coverage.sh` if the change touches
testable logic (excludes `main.rs` and `adapters/process/windows.rs` —
see that script for why).

## 5. Commit

Follow `.github/COMMIT_TEMPLATE.txt` / `scripts/check_commit.sh`:

```
[Action]: [title]

What: [what changed]
Why: [why it changed]

BAS-<id>
```

## 6. PR

`gh pr create` targeting `main`, following `.github/pull_request_template.md`.
Reference `BAS-<id>` in the body. Add a comment with the PR link on the
Trello card (`mcp__trello__add_comment`) and move the card to `In Review`
(list id `6a8ee7120bc47eeac2e54c14`).

## 7. Merge

Default merge is **rebase** via the `auto-merge.yml` workflow on approval
(preserves atomic commits). If the branch has messy/WIP commits that
shouldn't land individually on `main`, merge manually instead:
`gh pr merge --squash --delete-branch`. See `CONTRIBUTING.md`.

After merge: mark the Trello card's acceptance criteria complete, move it
to `Done` (list id `6a8ee7145a956af82fc800f1`), and check off its line
item in the `EPIC: Nala` `Tickets` checklist.
