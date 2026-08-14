#!/usr/bin/env node
import { spawn } from 'node:child_process'
import { copyFile, cp, mkdir, chmod, rm, readdir, readFile } from 'node:fs/promises'
import { existsSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { join } from 'node:path'

const root = fileURLToPath(new URL('..', import.meta.url))
const upstreamRoot = join(root, 'submodules', 'deepseek-harness')
const upstreamDeployRoot = join(upstreamRoot, 'runtime', 'desktop')
const tauriRuntimeRoot = join(root, 'apps', 'desktop', 'src-tauri', 'runtime')
const desktopDeployRoot = join(tauriRuntimeRoot, 'deepseek-harness')
const nodeDeployRoot = join(tauriRuntimeRoot, 'node')
const isWindows = process.platform === 'win32'
const pnpmBin = isWindows ? 'pnpm.cmd' : 'pnpm'
const nodeBinaryName = isWindows ? 'node.exe' : 'node'
const nodeBinaryTarget = join(nodeDeployRoot, nodeBinaryName)

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

async function main() {
  await mkdir(tauriRuntimeRoot, { recursive: true })
  await rm(desktopDeployRoot, { recursive: true, force: true })
  await rm(nodeDeployRoot, { recursive: true, force: true })
  await rm(upstreamDeployRoot, { recursive: true, force: true })

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
  await mkdir(nodeDeployRoot, { recursive: true })
  await copyFile(process.execPath, nodeBinaryTarget)
  if (!isWindows) {
    await chmod(nodeBinaryTarget, 0o755)
  }
  await rm(upstreamDeployRoot, { recursive: true, force: true })

  console.log(`desktop-runtime: staged upstream runtime at ${desktopDeployRoot}`)
  console.log(`desktop-runtime: staged node binary at ${nodeBinaryTarget}`)
}

await main()
