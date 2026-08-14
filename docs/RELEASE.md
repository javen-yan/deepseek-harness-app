# Release

## Requirements

- Code signing identity for each target platform.
- Tauri updater signing key.
- Update endpoint that serves signed metadata and artifacts.
- A release notes file generated from the current checkout.

## Key generation

```bash
pnpm --dir apps/desktop exec tauri signer generate -w ~/.tauri/deepseek-harness-app.key
```

## Environment

Set these before building release artifacts:

```bash
export TAURI_SIGNING_PRIVATE_KEY="$(cat ~/.tauri/deepseek-harness-app.key)"
export TAURI_SIGNING_PRIVATE_KEY_PASSWORD=""
```

If you prefer a path-based setup, source the repo example:

```bash
source ./release.env.example
```

Tauri also accepts `TAURI_SIGNING_PRIVATE_KEY_PATH` on newer CLI releases, but
this repo keeps the documented `TAURI_SIGNING_PRIVATE_KEY` flow as the portable
default.

## CI release

The repository also includes a GitHub Actions workflow at
`.github/workflows/release.yml`.

- It is triggered manually from the Actions tab.
- It builds Windows, macOS, and Linux release bundles in a matrix.
- It requires the `TAURI_SIGNING_PRIVATE_KEY` secret, and optionally
  `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`.
- It generates `docs/releases/current.md` in CI and publishes it with the
  release assets.
- It creates or updates the GitHub Release from the current checkout instead of
  relying on tag pushes.

## Release steps

1. Run `pnpm release:docs` to snapshot the current app version, upstream version,
   upstream commit, checklist, and macOS install steps into `docs/releases/`.
2. Confirm the updater `pubkey` and `endpoints` in `apps/desktop/src-tauri/tauri.conf.json`.
3. Run `pnpm desktop:stage-runtime` or let `pnpm build` invoke it so the bundled runtime root and Node launcher inputs are prepared for the target platform.
4. Run `pnpm build`.
5. Publish the generated installer and updater artifacts.
6. Publish the generated release note beside the build artifacts.
7. Verify the update endpoint serves the new version.
8. Upload the release notes with the installer assets in the GitHub release or
   your release bucket.

## Notes

- Updater artifacts are only emitted when signing is configured.
- Windows distribution still needs proper code signing to avoid trust warnings.
- The release installer must include the runtime launcher script plus the
  upstream runtime closure it points at; end users should not need a separate
  checkout or `pnpm install`.
- The release build stages the runtime closure into `src-tauri/runtime` and
  copies the host Node binary into `runtime/node/`, so the installed app can
  start without a system Node.
- `DEEPSEEK_HARNESS_RUNTIME_ROOT` and `DEEPSEEK_HARNESS_NODE_BINARY` remain
  override hooks for local debugging only.
- macOS distribution should be notarized, shipped as `.dmg` or `.app`, and
  installed by dragging the app into `Applications` before first launch.
- Prefer a release checklist that is checked into the repo and generated notes
  that are attached to the release, not a tag-only release process.
