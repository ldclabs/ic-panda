import { readdir, readFile, writeFile } from 'node:fs/promises'
import { fileURLToPath } from 'node:url'
import { extname, join } from 'node:path'

const root = fileURLToPath(new URL('../src/declarations/', import.meta.url))
const replacements = new Map([
  ['@dfinity/agent', '@icp-sdk/core/agent'],
  ['@dfinity/candid', '@icp-sdk/core/candid'],
  ['@dfinity/principal', '@icp-sdk/core/principal']
])

async function migrate(directory) {
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    const path = join(directory, entry.name)
    if (entry.isDirectory()) {
      await migrate(path)
      continue
    }
    if (!['.js', '.ts'].includes(extname(entry.name))) continue

    const source = await readFile(path, 'utf8')
    let migrated = source
    for (const [legacy, current] of replacements) {
      migrated = migrated.replaceAll(legacy, current)
    }
    if (migrated !== source) await writeFile(path, migrated)
  }
}

await migrate(root)
