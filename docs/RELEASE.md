# Release

## Requirements

- Code signing identity for each target platform.
- Tauri updater signing key.
- Update endpoint that serves signed metadata and artifacts.

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

## Release steps

1. Update the pinned upstream submodule commit if needed.
2. Confirm the updater `pubkey` and `endpoints` in `apps/desktop/src-tauri/tauri.conf.json`.
3. Make sure the bundled runtime root and Node launcher inputs are prepared for the target platform.
4. Run `pnpm build`.
5. Publish the generated installer and updater artifacts.
6. Verify the update endpoint serves the new version.

## Notes

- Updater artifacts are only emitted when signing is configured.
- Windows distribution still needs proper code signing to avoid trust warnings.
- The release installer must include the runtime launcher script plus the
  upstream runtime closure it points at; end users should not need a separate
  checkout or `pnpm install`.
- The launcher expects `DEEPSEEK_HARNESS_RUNTIME_ROOT` and, when Node is not on
  `PATH`, `DEEPSEEK_HARNESS_NODE_BINARY` to point at the bundled runtime.
