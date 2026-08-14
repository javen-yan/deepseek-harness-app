# Design

## Product shape

The first version is a desktop-only host for Windows, Linux, and macOS.
It should feel like an installed product, not a developer wrapper.

## Constraints

- No mobile target in v1.
- No separate agent fork.
- No second UI shell around the upstream app.
- No manual port management for users.
- No dependency on a user-installed `deepseek-harness` repo.

## UX rules

- Launch directly into the upstream web experience.
- Keep the host window quiet and minimal.
- Use the platform native titlebar/window chrome. Do not draw a custom toolbar
  inside the WebView.
- Use a small native app menu for product actions such as About, Check for
  Updates, Quit, and Edit commands.
- Use native update flow in the background.
- Fail with a visible error only when the upstream server cannot start.

## Open-source rules

- Document the repo layout and release process.
- Keep the submodule commit pinned and visible.
- Separate build-time and runtime responsibilities.
- Prefer explicit config and scripts over hidden magic.
