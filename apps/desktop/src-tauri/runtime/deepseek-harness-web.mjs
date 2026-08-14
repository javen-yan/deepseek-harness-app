#!/usr/bin/env node

import { existsSync } from 'node:fs'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'

const scriptDir = dirname(fileURLToPath(import.meta.url))

function candidateRoots() {
  const roots = []
  if (typeof process.env.DEEPSEEK_HARNESS_RUNTIME_ROOT === 'string' && process.env.DEEPSEEK_HARNESS_RUNTIME_ROOT !== '') {
    roots.push(process.env.DEEPSEEK_HARNESS_RUNTIME_ROOT)
  }
  roots.push(resolve(scriptDir, '../../../../submodules/deepseek-harness'))
  roots.push(resolve(scriptDir, 'deepseek-harness'))
  roots.push(resolve(scriptDir, '../runtime/deepseek-harness'))
  return roots
}

function resolveRuntimeRoot() {
  for (const root of candidateRoots()) {
    if (existsSync(join(root, 'lib/bin.js'))) return root
    if (existsSync(join(root, 'apps/cli/lib/bin.js'))) return root
  }
  throw new Error([
    'Deepseek Harness runtime launcher could not find lib/bin.js.',
    'Set DEEPSEEK_HARNESS_RUNTIME_ROOT to the packaged deepseek-harness checkout.',
  ].join(' '))
}

const runtimeRoot = resolveRuntimeRoot()
process.chdir(runtimeRoot)
const entry = existsSync(join(runtimeRoot, 'lib/bin.js'))
  ? join(runtimeRoot, 'lib/bin.js')
  : join(runtimeRoot, 'apps/cli/lib/bin.js')
await import(pathToFileURL(entry).href)
