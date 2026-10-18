// Local packages (`pnpm package`): on a Mac whose keychain has a Developer ID
// Application identity, signed with it and given the update feed of the
// GitHub release, so they download and install releases like a release
// package; otherwise ad hoc signed with no feed (electron-builder.yml).
// CSC_IDENTITY_AUTO_DISCOVERY=false keeps them ad hoc. `package:dir` stays on
// electron-builder.yml: a macOS `--dir` package gets no feed.
const { execFileSync } = require('node:child_process')
const fs = require('node:fs')
const path = require('node:path')
const { parse } = require('yaml')

const config = parse(fs.readFileSync(path.join(__dirname, 'electron-builder.yml'), 'utf8'))

/** The keychain's first Developer ID Application identity, without the prefix electron-builder adds itself. */
function developerId() {
  if (process.platform !== 'darwin' || process.env.CSC_IDENTITY_AUTO_DISCOVERY === 'false') return null
  const identities = execFileSync('security', ['find-identity', '-v', '-p', 'codesigning'], { encoding: 'utf8' })
  return identities.match(/"Developer ID Application: (.+)"/)?.[1] ?? null
}

const identity = developerId()
if (identity) {
  // Squirrel.Mac installs only updates signed like the running app, so the
  // feed installs releases only into a package signed by their team. Local
  // packages are not notarized, so they need no secure timestamp either; macOS
  // runs them since they were not downloaded.
  config.mac.identity = identity
  config.mac.entitlements = 'resources/entitlements.mac.plist'
  config.mac.entitlementsInherit = 'resources/entitlements.mac.plist'
  config.mac.notarize = false
  config.mac.timestamp = 'none'
  // The feed of electron-builder.release.cjs (FEED in src/main/updater.ts).
  config.publish = [{ provider: 'github', owner: 'ldclabs', repo: 'ic-panda', releaseType: 'release' }]
}
module.exports = config
