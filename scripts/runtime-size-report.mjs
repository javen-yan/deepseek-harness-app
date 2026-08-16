#!/usr/bin/env node
import { readdir, stat, writeFile } from 'node:fs/promises'
import { existsSync } from 'node:fs'
import { join, relative } from 'node:path'
import { fileURLToPath } from 'node:url'

const root = fileURLToPath(new URL('..', import.meta.url))
const runtimeRoot = join(root, 'apps', 'desktop', 'src-tauri', 'runtime')
const reportPath = join(root, 'apps', 'desktop', 'src-tauri', 'runtime-size-report.txt')

async function sizeOf(path) {
  if (!existsSync(path)) return 0
  const info = await stat(path)
  if (info.isFile()) return info.size
  if (!info.isDirectory()) return 0
  let total = 0
  for (const entry of await readdir(path)) {
    total += await sizeOf(join(path, entry))
  }
  return total
}

function format(bytes) {
  const units = ['B', 'KB', 'MB', 'GB']
  let value = bytes
  let index = 0
  while (value >= 1024 && index < units.length - 1) {
    value /= 1024
    index += 1
  }
  return `${value.toFixed(index === 0 ? 0 : 1)} ${units[index]}`
}

async function childSizes(path, depth = 1) {
  if (!existsSync(path)) return []
  const entries = await readdir(path, { withFileTypes: true })
  const rows = []
  for (const entry of entries) {
    const child = join(path, entry.name)
    if (depth > 1 && entry.isDirectory() && entry.name.startsWith('@')) {
      rows.push(...await childSizes(child, depth - 1))
      continue
    }
    rows.push({ path: child, size: await sizeOf(child) })
  }
  return rows.sort((a, b) => b.size - a.size)
}

async function main() {
  const nodeModules = join(runtimeRoot, 'deepseek-harness', 'node_modules')
  const sections = []
  sections.push(`# Runtime size report`)
  sections.push('')
  sections.push(`runtime: ${format(await sizeOf(runtimeRoot))}`)
  sections.push(`deepseek-harness: ${format(await sizeOf(join(runtimeRoot, 'deepseek-harness')))}`)
  sections.push(`node: ${format(await sizeOf(join(runtimeRoot, 'node')))}`)
  sections.push(`node_modules: ${format(await sizeOf(nodeModules))}`)
  sections.push('')
  sections.push('## Top node_modules entries')
  for (const row of (await childSizes(nodeModules, 2)).slice(0, 50)) {
    sections.push(`${format(row.size).padStart(9)}  ${relative(runtimeRoot, row.path)}`)
  }
  sections.push('')
  sections.push('## Native prebuilds')
  const prebuilds = (await childSizes(nodeModules, 2)).filter(row => row.path.includes('prebuild') || row.path.includes('@img') || row.path.includes('@vscode'))
  for (const row of prebuilds.slice(0, 50)) {
    sections.push(`${format(row.size).padStart(9)}  ${relative(runtimeRoot, row.path)}`)
  }
  const content = `${sections.join('\n')}\n`
  await writeFile(reportPath, content)
  process.stdout.write(content)
}

await main()
