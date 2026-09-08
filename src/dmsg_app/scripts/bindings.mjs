import { execFileSync } from 'node:child_process'
import { mkdirSync, writeFileSync } from 'node:fs'
import { resolve } from 'node:path'
const root = resolve(import.meta.dirname, '../../..')
for (const name of ['user', 'handle', 'cose', 'payment']) {
  const dir = resolve(import.meta.dirname, `../src/lib/canisters/generated/${name}`)
  mkdirSync(dir, { recursive: true })
  for (const [target, extension] of [
    ['js', 'js'],
    ['ts', 'd.ts']
  ]) {
    const source = execFileSync(
      'didc',
      ['bind', '-t', target, `${root}/src/dmsg_${name}/dmsg_${name}.did`],
      { encoding: 'utf8' }
    )
      .replaceAll('@dfinity/agent', '@icp-sdk/core/agent')
      .replaceAll('@dfinity/candid', '@icp-sdk/core/candid')
      .replaceAll('@dfinity/principal', '@icp-sdk/core/principal')
    writeFileSync(
      `${dir}/index.${extension}`,
      `// Generated from the public dmsg_${name}.did. Run npm run bindings.\n${source}`
    )
  }
}
