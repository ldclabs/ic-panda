/** A channel is published only after its manifest, packages and checksums agree. */
const { readFileSync, existsSync } = require('node:fs')
const { resolve, basename } = require('node:path')
const { createHash } = require('node:crypto')
const { parse } = require('yaml')
const { releaseConfig } = require('../../../scripts/dmsg/release-config.cjs')
const { version } = require('../package.json')

function verifyRelease(directory, version, platform) {
  const { channel } = releaseConfig(version)
  const suffix = platform === 'darwin' ? '-mac' : ''
  const manifest = parse(readFileSync(resolve(directory, `${channel}${suffix}.yml`), 'utf8'))
  if (manifest.version !== version || !Array.isArray(manifest.files) || manifest.files.length === 0) throw new Error('Invalid update manifest version or files')
  const installer = platform === 'darwin' ? 'dMsg-mac-arm64.zip' : 'dMsg-win-x64.exe'
  if (!manifest.files.some(file => file.url === installer)) throw new Error(`Missing updater package ${installer}`)
  for (const file of manifest.files) {
    if (basename(file.url) !== file.url) throw new Error(`Expected a release asset filename: ${file.url}`)
    const bytes = readFileSync(resolve(directory, file.url))
    if (createHash('sha512').update(bytes).digest('base64') !== file.sha512 || bytes.length !== file.size) throw new Error(`Invalid updater checksum or size: ${file.url}`)
    const sha256 = readFileSync(resolve(directory, `${file.url}.sha256`), 'utf8').trim().split(/\s+/)[0]
    if (createHash('sha256').update(bytes).digest('hex') !== sha256) throw new Error(`Invalid release checksum: ${file.url}`)
  }
  const other = channel === 'beta' ? 'latest' : 'beta'
  if (existsSync(resolve(directory, `${other}${suffix}.yml`))) throw new Error('Release contains a manifest for the wrong channel')
}

module.exports = { verifyRelease }
if (require.main === module) verifyRelease(resolve(process.argv[2]), version, process.platform)
