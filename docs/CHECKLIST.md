# Checklist

## Before merge

- [ ] `pnpm dev` launches the desktop app and loads the upstream web profile.
- [ ] `pnpm build` completes without manual intervention.
- [ ] The release launcher script resolves a packaged runtime root and opens the
      upstream web profile on a loopback port.
- [ ] The pinned upstream submodule commit is intentional.
- [ ] The updater pubkey and endpoint are set for the target environment.
- [ ] The release key is available in the build environment.
- [ ] `pnpm release:docs` writes the current release note with app version, upstream version, and upstream commit.
- [ ] No generated artifacts are committed.

## Before release

- [ ] Verify the app opens directly into the harness UI.
- [ ] Verify a clean machine can run the installed app without Git, submodules,
      pnpm, or a local `deepseek-harness` checkout.
- [ ] Verify the packaged app contains the upstream runtime dependency closure
      required to start `dsh --profile web`.
- [ ] Verify the packaged launcher can resolve the bundled runtime root and
      report the loopback URL before redirecting the WebView.
- [ ] Verify update install works against a signed test release.
- [ ] Verify Windows packaging on a Windows machine.
- [ ] Verify macOS packaging on an Apple Silicon and/or Intel Mac as targeted.
- [ ] Verify Linux packaging on the supported distribution baseline.
- [ ] Verify the README matches the current build and release flow.
- [ ] Verify the release notes mention the pinned upstream commit.
- [ ] Verify the macOS install note matches the notarized installer flow.
