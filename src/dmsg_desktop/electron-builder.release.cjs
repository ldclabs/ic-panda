// Release packages: Developer ID signed and notarized on macOS, Authenticode
// signed on Windows once a certificate is configured, with the update feed of
// the GitHub release that also carries the dmsg binaries.
const fs = require('node:fs')
const path = require('node:path')
const { parse } = require('yaml')

const config = parse(fs.readFileSync(path.join(__dirname, 'electron-builder.yml'), 'utf8'))
const signed = Boolean(process.env.CSC_LINK)
// Squirrel.Mac installs only updates signed like the running app, so a macOS
// feed needs the Developer ID certificate. Windows packages may ship unsigned
// until a certificate is configured.
if (process.platform === 'darwin' && !signed)
  throw new Error('A macOS release needs the Developer ID certificate in CSC_LINK')
config.forceCodeSigning = signed
// dmsg-v* releases are the repository's latest release; canister releases are not.
// Packages without this feed check the same one (FEED in src/main/updater.ts).
config.publish =[{ provider: 'github', owner: 'ldclabs', repo: 'ic-panda', releaseType: 'release' }]
delete config.mac.identity
config.mac.entitlements = 'resources/entitlements.mac.plist'
config.mac.entitlementsInherit = 'resources/entitlements.mac.plist'
config.mac.notarize = true
if (
  process.platform === 'darwin' &&
  !(process.env.APPLE_ID && process.env.APPLE_APP_SPECIFIC_PASSWORD && process.env.APPLE_TEAM_ID)
)
  throw new Error('Notarization needs APPLE_ID, APPLE_APP_SPECIFIC_PASSWORD and APPLE_TEAM_ID')
if (process.platform === 'win32') {
  if (signed) {
    if (!process.env.DMSG_WINDOWS_PUBLISHER) throw new Error('Set DMSG_WINDOWS_PUBLISHER to the certificate subject')
    config.win.signtoolOptions = {
      publisherName: process.env.DMSG_WINDOWS_PUBLISHER,
      signingHashAlgorithms: ['sha256']
    }
  }
  // Unsigned, electron-updater trusts the sha512 that the release's latest.yml lists.
  config.win.verifyUpdateCodeSignature = signed
}
module.exports = config
