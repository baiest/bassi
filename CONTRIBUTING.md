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
- `main` is protected: no direct pushes, no force-push, linear history only.
- On approval, `auto-merge.yml` squash-merges and deletes the branch.
