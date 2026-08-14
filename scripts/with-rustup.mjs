#!/usr/bin/env node
import { existsSync } from 'node:fs'
import { spawn } from 'node:child_process'
import { delimiter, join } from 'node:path'

const command = process.argv[2]
const args = process.argv.slice(3)

if (!command) {
  console.error('with-rustup: missing command')
  process.exit(1)
}

const pathCandidates = [
  '/opt/homebrew/opt/rustup/bin',
  '/usr/local/opt/rustup/bin',
  join(process.env.HOME ?? '', '.cargo', 'bin'),
].filter((candidate) => candidate && existsSync(candidate))

const env = {
  ...process.env,
  PATH: [...pathCandidates, process.env.PATH ?? ''].join(delimiter),
}

const child = spawn(command, args, {
  env,
  stdio: 'inherit',
  shell: process.platform === 'win32',
})

child.on('exit', (code) => {
  process.exitCode = code ?? 0
})

child.on('error', (error) => {
  console.error(error)
  process.exit(1)
})
