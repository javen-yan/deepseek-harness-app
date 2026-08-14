# Contributing

## Local setup

```bash
pnpm install
pnpm dev
```

## Useful commands

```bash
pnpm build
pnpm upstream:dev
pnpm upstream:build
```

## Working on the upstream submodule

- Update the submodule commit explicitly.
- Keep the commit pinned in the repo history.
- Re-run `pnpm build` after any upstream bump.

## Style

- Keep changes scoped.
- Prefer markdown docs for process and release rules.
- Do not add a separate UI shell unless the product direction changes.

## Pull requests

- Describe the user-visible change.
- Note any release or signing impact.
- Include validation steps and results.
