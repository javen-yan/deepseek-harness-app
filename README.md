这个项目只是我自己用的，现在也有一些开源比较好的，后期我后转到plugin开发中这个维护， 你们想研究的可以试试

# Deepseek Harness App

Desktop host for [DeepSeek Harness](https://github.com/deepseek-ai/deepseek-harness).

This repository packages the upstream harness as a native Tauri app. It does
not fork the agent logic or the web UI. Instead, it boots the pinned upstream
`deepseek-harness` source from `submodules/deepseek-harness` during
development, stages a bundled runtime closure for release builds, and renders
the local web profile inside a desktop WebView.

## What this repo owns

- Desktop packaging and native shell behavior
- Runtime bootstrap for dev and release builds
- Update flow, signing, and release docs
- Repository-level documentation and build scripts

## Upstream relationship

- App source of truth: [deepseek-harness-app](https://github.com/javen-yan/deepseek-harness-app)
- Upstream source of truth: [deepseek-harness](https://github.com/deepseek-ai/deepseek-harness)
- Pinned checkout in this repo: `submodules/deepseek-harness`
- The upstream submodule provides the agent, web app, plugins, and runtime
  behavior
- This repo should stay thin and avoid duplicating upstream product logic

## Architecture

```mermaid
flowchart TD
  U[User launches Deepseek Harness App] --> T[Tauri desktop shell]
  T --> M[Native app menu and update flow]
  T --> B[Bootstrap runtime]
  B --> D[Dev: scripts/upstream.mjs]
  B --> R[Release: bundled runtime launcher]
  D --> S[Start upstream dsh web profile]
  R --> S
  S --> P[Local loopback web server]
  P --> W[Tauri WebView loads the upstream UI]
  W --> I[User works in DeepSeek Harness]
```

See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for the full boundary and
release model.

## Repository layout

- `apps/desktop`: Tauri desktop host
- `submodules/deepseek-harness`: pinned upstream harness source
- `scripts/upstream.mjs`: upstream build/dev bridge
- `scripts/desktop-runtime.mjs`: release staging for the upstream runtime and
  packaged Node binary
- `docs/`: architecture, design, platform, release, and checklist notes

## Quick start

```bash
git submodule update --init --recursive
pnpm install
pnpm dev
```

## Common commands

```bash
pnpm upstream:dev
pnpm upstream:build
pnpm desktop:stage-runtime
pnpm release:docs
pnpm release:build
pnpm build
```

Release signing example: [release.env.example](release.env.example)

## Development notes

- The app starts the upstream web profile locally and redirects the desktop
  window to the emitted loopback URL.
- Native app actions live in the Tauri shell, while the upstream harness keeps
  ownership of the agent experience.
- Release builds bundle the runtime closure and a packaged Node binary so end
  users do not need to clone the repo or install upstream dependencies.

## Docs

- [Architecture](docs/ARCHITECTURE.md)
- [Design](docs/DESIGN.md)
- [Platforms](docs/PLATFORMS.md)
- [Release](docs/RELEASE.md)
- [Checklist](docs/CHECKLIST.md)
- [Contributing](CONTRIBUTING.md)

## License

MIT. See [LICENSE](LICENSE).
