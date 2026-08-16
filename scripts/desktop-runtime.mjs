#!/usr/bin/env node
import { spawn } from 'node:child_process'
import { copyFile, cp, mkdir, chmod, rm, readdir, readFile, realpath, writeFile } from 'node:fs/promises'
import { existsSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { dirname, join } from 'node:path'

const root = fileURLToPath(new URL('..', import.meta.url))
const upstreamRoot = join(root, 'submodules', 'deepseek-harness')
const upstreamDeployRoot = join(upstreamRoot, 'runtime', 'desktop')
const tauriRuntimeRoot = join(root, 'apps', 'desktop', 'src-tauri', 'runtime')
const desktopDeployRoot = join(tauriRuntimeRoot, 'deepseek-harness')
const nodeDeployRoot = join(tauriRuntimeRoot, 'node')
const pnpmDeployRoot = join(tauriRuntimeRoot, 'pnpm')
const isWindows = process.platform === 'win32'
const pnpmBin = isWindows ? 'pnpm.cmd' : 'pnpm'
const nodeBinaryName = isWindows ? 'node.exe' : 'node'
const nodeBinaryTarget = join(nodeDeployRoot, nodeBinaryName)
const pnpmShimName = isWindows ? 'pnpm.cmd' : 'pnpm'

function run(command, args, cwd, env = process.env) {
  return new Promise((resolve, reject) => {
    const child = spawn(command, args, {
      cwd,
      env,
      stdio: 'inherit',
      shell: isWindows,
    })
    child.on('error', reject)
    child.on('exit', code => {
      if (code === 0) resolve()
      else reject(new Error(`${command} exited with code ${String(code ?? 0)}`))
    })
  })
}

async function mirrorWorkspacePackages() {
  const workspaceRoots = [
    { root: join(upstreamRoot, 'vendor'), depth: 1 },
    { root: join(upstreamRoot, 'apps'), depth: 1 },
    { root: join(upstreamRoot, 'packages'), depth: 2 },
    { root: join(upstreamRoot, 'native', 'landlock-run', 'packages'), depth: 1 },
  ]

  for (const { root, depth } of workspaceRoots) {
    if (!existsSync(root)) continue
    await mirrorWorkspacePackagesAt(root, depth)
  }
}

async function mirrorWorkspacePackagesAt(root, depth) {
  const entries = await readdir(root, { withFileTypes: true })
  for (const entry of entries) {
    if (!entry.isDirectory()) continue
    if (entry.name === 'node_modules') continue
    const source = join(root, entry.name)
    if (depth > 1) {
      await mirrorWorkspacePackagesAt(source, depth - 1)
      continue
    }

    const manifestPath = join(source, 'package.json')
    if (!existsSync(manifestPath)) continue
    const manifest = JSON.parse(await readFile(manifestPath, 'utf8'))
    if (typeof manifest.name !== 'string' || !manifest.name.startsWith('@deepseek-ai/')) continue
    const [scope, packageName] = manifest.name.split('/')
    if (!scope || !packageName) continue
    const destination = join(desktopDeployRoot, 'node_modules', scope, packageName)
    if (existsSync(destination)) continue
    await copyTree(source, destination)
  }
}

async function copyTree(source, destination) {
  const entries = await readdir(source, { withFileTypes: true })
  await mkdir(destination, { recursive: true })
  for (const entry of entries) {
    if (entry.name === 'node_modules') continue
    const sourcePath = join(source, entry.name)
    const destinationPath = join(destination, entry.name)
    if (entry.isDirectory()) {
      await copyTree(sourcePath, destinationPath)
      continue
    }
    if (entry.isSymbolicLink()) {
      continue
    }
    await copyFile(sourcePath, destinationPath)
  }
}

async function pruneRuntimeTree() {
  const removableDirectoryNames = new Set([
    'test',
    'tests',
    'docs',
    'doc',
    'example',
    'examples',
    'coverage',
    '.github',
  ])
  const removableExtensions = [
    '.map',
    '.ts',
    '.tsx',
    '.md',
    '.markdown',
  ]

  async function walk(path) {
    if (!existsSync(path)) return
    const entries = await readdir(path, { withFileTypes: true })
    for (const entry of entries) {
      const child = join(path, entry.name)
      if (entry.isDirectory()) {
        if (removableDirectoryNames.has(entry.name)) {
          await rm(child, { recursive: true, force: true })
          continue
        }
        await walk(child)
        continue
      }
      if (entry.isFile() && removableExtensions.some(extension => entry.name.endsWith(extension))) {
        await rm(child, { force: true })
      }
    }
  }

  await pruneNodePtyPrebuilds()
  await walk(join(desktopDeployRoot, 'node_modules'))
}

async function pruneNodePtyPrebuilds() {
  const prebuilds = join(desktopDeployRoot, 'node_modules', 'node-pty', 'prebuilds')
  if (!existsSync(prebuilds)) return
  const target = `${process.platform}-${process.arch}`
  const entries = await readdir(prebuilds, { withFileTypes: true })
  for (const entry of entries) {
    if (!entry.isDirectory()) continue
    if (entry.name !== target) {
      await rm(join(prebuilds, entry.name), { recursive: true, force: true })
    }
  }
}

async function stagePnpmRuntime() {
  await rm(pnpmDeployRoot, { recursive: true, force: true })
  let pnpmBin
  try {
    pnpmBin = await resolveCommand(pnpmBinCommand())
  } catch {
    console.warn('desktop-runtime: pnpm not found; plugin installation will require system pnpm')
    return
  }
  const realPnpm = await realpath(pnpmBin)
  const packageRoot = join(dirname(realPnpm), '..')
  const manifestPath = join(packageRoot, 'package.json')
  if (!existsSync(manifestPath)) {
    console.warn(`desktop-runtime: pnpm package root not found at ${packageRoot}`)
    return
  }
  await cp(packageRoot, join(pnpmDeployRoot, 'package'), { recursive: true, force: true, dereference: true })
  await mkdir(pnpmDeployRoot, { recursive: true })
  if (isWindows) {
    await writeFile(join(pnpmDeployRoot, pnpmShimName), [
      '@echo off',
      'set DIR=%~dp0',
      '"%DIR%..\\node\\node.exe" "%DIR%package\\bin\\pnpm.mjs" %*',
      '',
    ].join('\r\n'))
  } else {
    await writeFile(join(pnpmDeployRoot, pnpmShimName), [
      '#!/bin/sh',
      'DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)',
      'exec "$DIR/../node/node" "$DIR/package/bin/pnpm.mjs" "$@"',
      '',
    ].join('\n'))
    await chmod(join(pnpmDeployRoot, pnpmShimName), 0o755)
  }
}

function pnpmBinCommand() {
  return isWindows ? 'pnpm.cmd' : 'pnpm'
}

function resolveCommand(command) {
  return new Promise((resolve, reject) => {
    const lookup = isWindows ? 'where' : 'which'
    const child = spawn(lookup, [command], { stdio: ['ignore', 'pipe', 'ignore'], shell: isWindows })
    let stdout = ''
    child.stdout.on('data', chunk => {
      stdout += chunk
    })
    child.on('error', reject)
    child.on('exit', code => {
      if (code !== 0) {
        reject(new Error(`${lookup} ${command} failed`))
        return
      }
      const first = stdout.split(/\r?\n/).find(Boolean)
      if (first) resolve(first)
      else reject(new Error(`${lookup} ${command} returned no path`))
    })
  })
}

async function main() {
  await mkdir(tauriRuntimeRoot, { recursive: true })
  await rm(desktopDeployRoot, { recursive: true, force: true })
  await rm(nodeDeployRoot, { recursive: true, force: true })
  await rm(pnpmDeployRoot, { recursive: true, force: true })
  await rm(upstreamDeployRoot, { recursive: true, force: true })

  await run(pnpmBin, [
    'install',
    '--frozen-lockfile',
    '--ignore-scripts',
  ], upstreamRoot, {
    ...process.env,
    CI: 'true',
    npm_config_ignore_scripts: 'true',
  })

  await run(pnpmBin, [
    'run',
    'build',
  ], upstreamRoot, { ...process.env, CI: 'true' })

  await run(pnpmBin, [
    '--dir',
    upstreamRoot,
    '--filter',
    '@deepseek-ai/dsh',
    'deploy',
    '--legacy',
    '--prod',
    '--config.node-linker=hoisted',
    '--config.auto-install-peers=false',
    '--config.link-workspace-packages=true',
    'runtime/desktop',
  ], root, { ...process.env, CI: 'true' })

  const deployedEntry = join(upstreamDeployRoot, 'lib', 'bin.js')
  if (!existsSync(deployedEntry)) {
    throw new Error(`desktop-runtime: missing deployed launcher at ${deployedEntry}`)
  }

  await cp(upstreamDeployRoot, desktopDeployRoot, { recursive: true, force: true, dereference: true })
  await mirrorWorkspacePackages()
  await pruneRuntimeTree()
  await mkdir(nodeDeployRoot, { recursive: true })
  await copyFile(process.execPath, nodeBinaryTarget)
  if (!isWindows) {
    await chmod(nodeBinaryTarget, 0o755)
  }
  await stagePnpmRuntime()
  await rm(upstreamDeployRoot, { recursive: true, force: true })

  console.log(`desktop-runtime: staged upstream runtime at ${desktopDeployRoot}`)
  console.log(`desktop-runtime: staged node binary at ${nodeBinaryTarget}`)
  console.log(`desktop-runtime: staged pnpm shim at ${join(pnpmDeployRoot, pnpmShimName)}`)
}

await main()
