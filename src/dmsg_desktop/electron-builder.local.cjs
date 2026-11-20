// Local packages (`pnpm package`): on a Mac whose keychain has a Developer ID
// Application identity for the release team, signed with it and given the update feed of the
// GitHub release, so they download and install releases like a release
// package; otherwise ad hoc signed with no feed (electron-builder.yml).
// CSC_IDENTITY_AUTO_DISCOVERY=false keeps them ad hoc. `package:dir` stays on
// electron-builder.yml: a macOS `--dir` package gets no feed.
const { execFileSync } = require('node:child_process')
const fs = require('node:fs')
const path = require('node:path')
const { parse } = require('yaml')
const { publishConfig } = require('../../scripts/dmsg/release-config.cjs')
const { version } = require('./package.json')
const { policy } = require('../../scripts/dmsg/macos-signing.cjs')

const config = parse(fs.readFileSync(path.join(__dirname, 'electron-builder.yml'), 'utf8'))

/** The release team's Developer ID identity, without the prefix electron-builder adds itself. */
function developerId() {
  if (process.platform !== 'darwin' || process.env.CSC_IDENTITY_AUTO_DISCOVERY === 'false') return null
  const identities = execFileSync('security', ['find-identity', '-v', '-p', 'codesigning'], { encoding: 'utf8' })
  return [...identities.matchAll(/"Developer ID Application: ([^"\n]+)"/g)]
    .map(match => match[1]).find(identity => identity.endsWith(`(${policy.releaseTeam})`)) ?? null
}

const identity = developerId()
if (identity) {
  // Squirrel.Mac checks the running app's designated requirement. Local
  // packages are not notarized, so they need no secure timestamp either; macOS
  // runs them since they were not downloaded.
  config.mac.identity = identity
  config.mac.entitlements = 'resources/entitlements.mac.plist'
  config.mac.entitlementsInherit = 'resources/entitlements.mac.plist'
  config.mac.notarize = false
  config.mac.timestamp = 'none'
  config.mac.sign = require('./scripts/sign-macos.cjs')
  config.publish = [publishConfig(version)]
  config.generateUpdatesFilesForAllChannels = false
}
module.exports = config
