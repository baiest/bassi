# Contributing

## Branches

`BAS-<ticket-id>-<short-description>`

Examples:
- `BAS-3-configurar-bassi-y-agregar-nala`
- `BAS-12-fix-crlf-line-endings`

Ticket ID matches the Trello card (board "Bassi", list `Epics` tracks
progress per epic). No open ticket, no branch — create the card first.

## Commits

See `.github/COMMIT_TEMPLATE.txt` (auto-loaded via `git config commit.template`)
and `scripts/check_commit.sh` for the enforced format:

```
[Action]: [title]

What: [what changed]
Why: [why it changed]

[Ticket ID]
```

Allowed actions: `Add`, `Fix`, `Cut`, `Optimise`, `Refactor`, `Delete`, `Docs`, `Style`, `Merge`.

## Pull requests

- One branch per ticket, opened against `main`.
- CI (`scripts/check_lf.sh`, `check_commit.sh`, `check_rust.sh`) must pass.
- `main` is protected: no direct pushes, no force-push, linear history only
  (merge commits are disabled repo-wide; only squash or rebase allowed).
- On approval, `auto-merge.yml` **rebase**-merges and deletes the branch —
  the default, since it preserves atomic commits as-is.
- If a PR has messy/WIP commits that shouldn't land individually on `main`,
  don't rely on auto-merge: merge it manually with
  `gh pr merge --squash --delete-branch`.
