// electron-builder calls this before notarization. Only the outer app gets the
// transition DR; nested helpers keep their individual identifiers/requirements.
const path = require('node:path')
const { execFileSync, spawnSync } = require('node:child_process')
const { policy, teamsFor, requirement, assertReleaseTeam } = require('../../../scripts/dmsg/macos-signing.cjs')

function optionsForTransition(options) {
  const original = options.optionsForFile
  return {
    ...options,
    optionsForFile(file) {
      const base = original ? original(file) : {}
      if (path.resolve(file) !== path.resolve(options.app)) return base
      return { ...base, requirements: `=designated => ${requirement(policy.desktopIdentifier, teamsFor(policy.releaseTeam))}` }
    }
  }
}

async function sign(options) {
  if (options.platform !== 'darwin') throw new Error('The dMsg transition signer is for Developer ID distribution only')
  const { signAsync } = require('@electron/osx-sign')
  await signAsync(optionsForTransition(options))
  const signed = spawnSync('/usr/bin/codesign', ['-d', '--verbose=2', options.app], { encoding: 'utf8' })
  if (signed.status !== 0) throw new Error('Cannot inspect the signed dMsg app')
  assertReleaseTeam(/^TeamIdentifier=([A-Z0-9]{10})$/m.exec(signed.stderr)?.[1])
  // Check the actual signer, not merely the (broader) designated requirement.
  execFileSync('/usr/bin/codesign', ['--verify', '--strict', '-R', `=${requirement(policy.desktopIdentifier, [policy.releaseTeam])}`, options.app])
}

module.exports = sign
module.exports.optionsForTransition = optionsForTransition
