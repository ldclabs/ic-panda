// Generates the Candid declarations the frontends import as $declarations/<name>/<name>.did.js
// from src/<name>/<name>.did and the external interfaces in candid/.
import { existsSync } from 'node:fs'
import { readdir, rm } from 'node:fs/promises'
import { basename, resolve } from 'node:path'
import { generate } from '@icp-sdk/bindgen/core'

const root = resolve(import.meta.dirname, '..')
const outDir = `${root}/src/declarations`
const didFiles = [
  ...(await readdir(`${root}/candid`))
    .filter((file) => file.endsWith('.did'))
    .map((file) => `${root}/candid/${file}`),
  ...(await readdir(`${root}/src`))
    .map((name) => `${root}/src/${name}/${name}.did`)
    .filter((file) => existsSync(file))
]

await rm(outDir, { recursive: true, force: true })
for (const didFile of didFiles) {
  await generate({
    didFile,
    outDir: `${outDir}/${basename(didFile, '.did')}`,
    output: { actor: { disabled: true }, declarations: { flat: true } }
  })
}
