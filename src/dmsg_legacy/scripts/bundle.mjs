// Loads a TypeScript entry of this package in Node: Vite bundles it, with its
// dependencies, into a temporary module that is imported and then removed.
import { build } from 'vite'
import { writeFile, mkdtemp, rm } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { resolve, join } from 'node:path'
import { pathToFileURL, fileURLToPath } from 'node:url'

export async function loadEntry(entry) {
  const app = resolve(fileURLToPath(new URL('..', import.meta.url)))
  const temporary = await mkdtemp(join(tmpdir(), 'dmsg-legacy-'))
  try {
    const result = await build({
      configFile: false,
      root: app,
      logLevel: 'error',
      ssr: { noExternal: true },
      build: {
        ssr: resolve(app, entry),
        target: 'node22',
        write: false,
        minify: false,
        rollupOptions: {
          external: [/^node:/],
          output: { format: 'es', codeSplitting: false }
        }
      }
    })
    const code = result.output.find((value) => value.type === 'chunk' && value.isEntry).code
    const path = join(temporary, 'entry.mjs')
    await writeFile(path, code)
    return await import(pathToFileURL(path).href)
  } finally {
    await rm(temporary, { recursive: true, force: true })
  }
}
