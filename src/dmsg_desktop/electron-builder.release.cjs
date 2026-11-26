// Official desktop packages use Developer ID on macOS and Certum on Windows.
const fs = require('node:fs')
const path = require('node:path')
const { parse } = require('yaml')
const { publishConfig } = require('../../scripts/dmsg/release-config.cjs')
const { version } = require('./package.json')
const { assertReleaseTeam } = require('../../scripts/dmsg/macos-signing.cjs')

const config = parse(fs.readFileSync(path.join(__dirname, 'electron-builder.yml'), 'utf8'))
if (process.platform === 'darwin' && !process.env.CSC_LINK)
  throw new Error('A macOS release needs the Developer ID certificate in CSC_LINK')
if (process.platform === 'darwin' && !process.env.DMSG_FSKIT_EXTENSION)
  throw new Error('A macOS release needs the signed SecretsFS.appex in DMSG_FSKIT_EXTENSION')
config.forceCodeSigning = true
// The main process selects an eligible release and overrides this pinned feed.
config.publish = [publishConfig(version)]
config.generateUpdatesFilesForAllChannels = false
delete config.mac.identity
config.mac.entitlements = 'resources/entitlements.mac.plist'
config.mac.entitlementsInherit = 'resources/entitlements.mac.plist'
config.mac.notarize = true
config.mac.sign = require('./scripts/sign-macos.cjs')
if (
  process.platform === 'darwin' &&
  !(process.env.APPLE_ID && process.env.APPLE_APP_SPECIFIC_PASSWORD && process.env.APPLE_TEAM_ID)
)
  throw new Error('Notarization needs APPLE_ID, APPLE_APP_SPECIFIC_PASSWORD and APPLE_TEAM_ID')
if (process.platform === 'darwin') assertReleaseTeam(process.env.APPLE_TEAM_ID)
if (process.platform === 'win32') {
  const certificateSha1 = process.env.DMSG_WINDOWS_CERT_SHA1
  const publisherName = process.env.DMSG_WINDOWS_PUBLISHER
  if (!/^[a-f0-9]{40}$/i.test(certificateSha1 || '') || !publisherName)
    throw new Error('A Windows release needs the Certum certificate thumbprint and publisher')
  config.win.signtoolOptions = {
    certificateSha1, publisherName, signingHashAlgorithms: ['sha256'],
    rfc3161TimeStampServer: 'http://time.certum.pl'
  }
  config.win.verifyUpdateCodeSignature = true
}
module.exports = config
