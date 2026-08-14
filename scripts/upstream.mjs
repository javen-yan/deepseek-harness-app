#!/usr/bin/env node
import { existsSync } from 'node:fs'
import { spawn } from 'node:child_process'
import { fileURLToPath } from 'node:url'
import { dirname, join } from 'node:path'

const root = fileURLToPath(new URL('..', import.meta.url))
const upstream = join(root, 'submodules', 'deepseek-harness')
const mode = process.argv[2] ?? 'dev'
const port = process.argv[3] ?? '1420'

function spawnCommand(command, args) {
  return new Promise((resolve, reject) => {
    const child = spawn(command, args, {
      cwd: upstream,
      stdio: 'inherit',
      shell: process.platform === 'win32',
    })

    const stop = () => {
      if (!child.killed) child.kill('SIGTERM')
    }

    process.once('SIGINT', stop)
    process.once('SIGTERM', stop)

    child.on('error', reject)
    child.on('exit', (code) => {
      process.off('SIGINT', stop)
      process.off('SIGTERM', stop)
      if (code === 0) resolve()
      else reject(new Error(`${command} exited with code ${String(code ?? 0)}`))
    })
  })
}

async function main() {
  if (!existsSync(join(upstream, 'node_modules'))) {
    await spawnCommand('pnpm', ['install'])
  }

  if (mode === 'dev') {
    await spawnCommand('pnpm', ['dsh', '--profile', 'web', '--port', String(port)])
    return
  }

  if (mode === 'build') {
    await spawnCommand('pnpm', ['build'])
    return
  }

  console.error(`upstream: unknown mode "${mode}"`)
  process.exitCode = 1
}

await main()
