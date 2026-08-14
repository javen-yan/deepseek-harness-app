#!/usr/bin/env node
import { copyFile, mkdir, readdir, rm } from 'node:fs/promises'
import { existsSync } from 'node:fs'
import { basename, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const root = fileURLToPath(new URL('..', import.meta.url))
const bundleRoot = process.argv[2] ?? join(root, 'apps', 'desktop', 'src-tauri', 'target', 'release', 'bundle')
const outputRoot = process.argv[3] ?? join(root, 'release-assets')
const notesPath = process.argv[4] ?? join(root, 'docs', 'releases', 'current.md')

const releaseNamePatterns = [
  /\.dmg$/,
  /\.app\.tar\.gz$/,
  /\.app\.tar\.gz\.sig$/,
  /\.msi$/,
  /\.msi\.zip$/,
  /\.msi\.zip\.sig$/,
  /\.exe$/,
  /\.exe\.zip$/,
  /\.exe\.zip\.sig$/,
  /\.AppImage$/,
  /\.AppImage\.tar\.gz$/,
  /\.AppImage\.tar\.gz\.sig$/,
  /\.deb$/,
  /\.deb\.sig$/,
  /\.rpm$/,
  /\.rpm\.sig$/,
  /\.sig$/,
  /^latest.*\.json$/,
]

function isReleaseAsset(name) {
  return releaseNamePatterns.some(pattern => pattern.test(name))
}

async function walk(source, destination, collected) {
  const entries = await readdir(source, { withFileTypes: true })
  for (const entry of entries) {
    const sourcePath = join(source, entry.name)
    if (entry.isDirectory()) {
      await walk(sourcePath, destination, collected)
      continue
    }
    if (!entry.isFile()) continue
    if (!isReleaseAsset(entry.name)) continue
    const targetPath = join(destination, basename(entry.name))
    await copyFile(sourcePath, targetPath)
    collected.push(targetPath)
  }
}

async function main() {
  if (!existsSync(bundleRoot)) {
    throw new Error(`collect-release-assets: bundle root not found: ${bundleRoot}`)
  }

  await rm(outputRoot, { recursive: true, force: true })
  await mkdir(outputRoot, { recursive: true })

  const collected = []
  await walk(bundleRoot, outputRoot, collected)

  if (!existsSync(notesPath)) {
    throw new Error(`collect-release-assets: release notes not found: ${notesPath}`)
  }

  await copyFile(notesPath, join(outputRoot, 'release-notes.md'))
  collected.push(join(outputRoot, 'release-notes.md'))

  if (collected.length === 0) {
    throw new Error(`collect-release-assets: no release assets were collected from ${bundleRoot}`)
  }

  console.log(`collect-release-assets: wrote ${collected.length} file(s) to ${outputRoot}`)
  for (const file of collected) console.log(`collect-release-assets: ${file}`)
}

await main()
