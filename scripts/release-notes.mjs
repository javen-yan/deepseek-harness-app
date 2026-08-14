#!/usr/bin/env node
import { mkdir, writeFile } from 'node:fs/promises'
import { readFileSync } from 'node:fs'
import { execFileSync } from 'node:child_process'
import { fileURLToPath } from 'node:url'
import { join } from 'node:path'

const root = fileURLToPath(new URL('..', import.meta.url))
const appConfigPath = join(root, 'apps', 'desktop', 'src-tauri', 'tauri.conf.json')
const upstreamManifestPath = join(root, 'submodules', 'deepseek-harness', 'package.json')
const releaseDir = join(root, 'docs', 'releases')

const appConfig = JSON.parse(readFileSync(appConfigPath, 'utf8'))
const upstreamManifest = JSON.parse(readFileSync(upstreamManifestPath, 'utf8'))
const appVersion = String(appConfig.version ?? '0.0.0')
const upstreamVersion = String(upstreamManifest.version ?? 'unknown')
const upstreamCommit = execFileSync('git', [
  '-C',
  join(root, 'submodules', 'deepseek-harness'),
  'rev-parse',
  '--short',
  'HEAD',
], { encoding: 'utf8' }).trim()
const generatedDate = new Date().toISOString().slice(0, 10)

const checklist = [
  'Release build uses the current checkout and the pinned upstream submodule commit.',
  'Updater pubkey and endpoint are set for the target release environment.',
  'Signing key is present in the build environment.',
  'Bundled runtime staged successfully with `pnpm desktop:stage-runtime`.',
  'macOS installer is notarized and opens cleanly from Applications.',
  'Windows installer is signed and launches without the shell depending on Git, pnpm, or a local checkout.',
  'Linux package starts the bundled runtime on a clean machine.',
]

const macosInstall = [
  'Download the signed and notarized `.dmg` from the release page.',
  'Open the disk image and drag `Deepseek Harness.app` into `Applications`.',
  'Eject the disk image after copying finishes.',
  'Launch from `Applications`; if Gatekeeper still prompts on a managed machine, use the normal macOS Open flow once, not a bypass for unsigned builds.',
  'Verify the About dialog shows the expected app version and upstream version.',
]

const content = [
  `# Deepseek Harness ${appVersion}`,
  '',
  `Generated: ${generatedDate}`,
  '',
  '## Snapshot',
  '',
  '| Field | Value |',
  '| --- | --- |',
  `| App version | ${appVersion} |`,
  `| Upstream version | ${upstreamVersion} |`,
  `| Upstream commit | ${upstreamCommit} |`,
  '| Release model | installable desktop app, no tag gate |',
  '',
  '## Release rules',
  '',
  '- Ordinary commits do not publish anything.',
  '- Git tags do not publish anything.',
  '- A release build snapshots the current checkout plus the pinned upstream submodule commit.',
  '- The release docs are generated from this checkout, so the published note always matches the packaged build.',
  '',
  '## Release checklist',
  '',
  ...checklist.map(item => `- [ ] ${item}`),
  '',
  '## macOS safe install',
  '',
  ...macosInstall.map(item => `- ${item}`),
  '',
  '## Platform artifacts',
  '',
  '- Windows: NSIS or MSI, plus updater artifact.',
  '- macOS: signed `.app` / `.dmg`, plus updater archive and notarization.',
  '- Linux: AppImage or distro package, plus updater artifact where supported.',
  '',
  '## Publishing notes',
  '',
  '- Publish the installers and updater metadata from the release build, not from a tag hook.',
  '- Keep the updater endpoint versioned by app version, not by Git branch name.',
  '- Keep the pinned upstream commit in the release note and changelog.',
  '',
].join('\n')

await mkdir(releaseDir, { recursive: true })
await writeFile(join(releaseDir, `${appVersion}.md`), content)
await writeFile(join(releaseDir, 'current.md'), content)

console.log(`release-notes: wrote docs/releases/${appVersion}.md`)
console.log('release-notes: wrote docs/releases/current.md')
