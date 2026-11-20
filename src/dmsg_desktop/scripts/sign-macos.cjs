// The packaged app pins its Developer ID signer before notarization.
const path = require('node:path')
const { execFileSync } = require('node:child_process')
const { policy, requirement, verifyDesignatedRequirement } = require('../../../scripts/dmsg/macos-signing.cjs')
const appRequirement = requirement(policy.desktopIdentifier, policy.releaseTeam)

function optionsForRelease(options) {
  const original = options.optionsForFile
  return {
    ...options,
    optionsForFile(file, context) {
      const base = original ? original(file, context) : {}
      if (path.resolve(file) !== path.resolve(options.app)) return base
      return { ...base, requirements: `=designated => ${appRequirement}` }
    }
  }
}

async function sign(options) {
  if (options.platform !== 'darwin') throw new Error('The dMsg signer is for Developer ID distribution only')
  const { sign: signApp } = await import('@electron/osx-sign')
  await signApp(optionsForRelease(options))
  execFileSync('/usr/bin/codesign', ['--verify', '--strict', '-R', `=${appRequirement}`, options.app])
  verifyDesignatedRequirement(options.app, appRequirement)
}

module.exports = sign
module.exports.optionsForRelease = optionsForRelease
