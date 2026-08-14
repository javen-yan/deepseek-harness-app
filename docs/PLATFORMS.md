# Platforms

Deepseek Harness targets desktop only in v1: Windows, macOS, and Linux.

## Support matrix

| Platform | Runtime target | Installer target | Signing requirement |
| --- | --- | --- | --- |
| Windows | WebView2 | NSIS or MSI | Authenticode signing recommended before public distribution |
| macOS | WKWebView | `.app` / `.dmg` plus updater archive | Developer ID signing and notarization required for public distribution |
| Linux | WebKitGTK | AppImage or distro package | No universal signing path; publish checksums and updater signatures |

## End-user dependency rule

End users must not need to:

- clone this repository,
- initialize `submodules/deepseek-harness`,
- install `pnpm`,
- run `pnpm install`,
- install upstream npm dependencies manually,
- start a local `dsh` process manually.

The installed app must include everything required to start the pinned upstream
web profile and load it in the Tauri WebView.

## Developer dependency rule

Developers building from source do need the submodule and dependencies:

```bash
git submodule update --init --recursive
pnpm install
pnpm dev
```

The development script may install upstream dependencies inside the submodule.
That behavior is acceptable for source development, but not for installed
release artifacts.

## Release packaging requirement

The release pipeline must package the upstream runtime dependency closure:

- built upstream web assets,
- upstream CLI/runtime entry required to run `dsh --profile web`,
- the launcher script that resolves the runtime root and prints the loopback URL,
- any runtime `node_modules` or compiled sidecar required by upstream,
- platform-specific sidecar metadata if the runtime is shipped as a sidecar.

If upstream cannot be reduced to static assets, the desktop app must bundle a
platform-specific runtime sidecar that starts the local web profile before the
WebView loads `http://127.0.0.1:<port>`.

## Current implementation status

Current development flow starts upstream from the checked-out submodule:

```bash
node ../../scripts/upstream.mjs dev 1420
```

This is correct for local development. Before public distribution, release
packaging still needs the runtime sidecar/resource closure so installed users do
not depend on the repository checkout.
