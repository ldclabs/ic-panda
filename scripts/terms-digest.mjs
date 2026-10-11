// SHA-256 of the dMsg terms of service.
//
// Each version is src/dmsg_frontend/static/_/terms/v<N>.txt, served unchanged at
// https://dmsg.net/_/terms/v<N>.txt. dmsg_commerce records the current digest as the
// product registration's terms_hash and the catalog's terms_digest, with N as each
// plan's terms_version. A version is frozen once it is registered: change the terms
// by adding v<N+1>.txt.
//
//   node scripts/terms-digest.mjs          list every version with its digest
//   node scripts/terms-digest.mjs --hex    print the current digest as hex
//   node scripts/terms-digest.mjs --write  regenerate the manifest read by /_/terms
//   node scripts/terms-digest.mjs --check  fail when the manifest differs from the files
import { createHash } from 'node:crypto'
import { readdirSync, readFileSync, writeFileSync } from 'node:fs'
import { resolve } from 'node:path'

const root = resolve(import.meta.dirname, '..')
const termsDir = `${root}/src/dmsg_frontend/static/_/terms`
const manifestFile = `${root}/src/dmsg_frontend/src/lib/legal/terms.json`

function fail(message) {
  console.error(message)
  process.exit(1)
}

const versions = readdirSync(termsDir)
  .map((file) => /^v([1-9]\d*)\.txt$/.exec(file))
  .filter((match) => match !== null)
  .map(([file, version]) => {
    const bytes = readFileSync(`${termsDir}/${file}`)
    // Pin the bytes that are hashed: printable ASCII, LF line endings and a single
    // trailing newline, so no editor, platform or checkout can change the digest.
    if (bytes.some((byte) => byte > 0x7e || (byte < 0x20 && byte !== 0x0a)))
      fail(`${file}: use printable ASCII and LF line endings only`)
    if (bytes.at(-1) !== 0x0a || bytes.at(-2) === 0x0a)
      fail(`${file}: end with exactly one newline`)
    return {
      version: Number(version),
      path: `/_/terms/${file}`,
      sha256: createHash('sha256').update(bytes).digest('hex')
    }
  })
  .sort((a, b) => a.version - b.version)

if (versions.length === 0 || versions.some((v, i) => v.version !== i + 1))
  fail(`${termsDir}: versions must be v1.txt, v2.txt, ... without gaps`)

const manifest = JSON.stringify(versions, null, 2) + '\n'
switch (process.argv[2]) {
  case undefined:
    for (const v of versions) console.log(`v${v.version}  ${v.sha256}  ${v.path}`)
    break
  case '--hex':
    console.log(versions.at(-1).sha256)
    break
  case '--write':
    writeFileSync(manifestFile, manifest)
    break
  case '--check':
    if (readFileSync(manifestFile, 'utf8') !== manifest)
      fail(
        `${manifestFile} does not match the terms files. A registered version must not change; ` +
          'add a new version, then run node scripts/terms-digest.mjs --write.'
      )
    break
  default:
    fail('usage: node scripts/terms-digest.mjs [--hex | --write | --check]')
}
