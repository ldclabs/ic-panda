// Preserve the signed ExtensionKit bundle through Electron packaging.
const fs = require('node:fs')
const path = require('node:path')
const { execFileSync, spawnSync } = require('node:child_process')
const { policy, requirement, verifyDesignatedRequirement } = require('../../../scripts/dmsg/macos-signing.cjs')

const moduleIdentifier = 'net.dmsg.desktop.SecretFiles'
const extensionPath = 'Contents/Extensions/SecretsFS.appex'
const daemonPath = 'Contents/Resources/runtime/dmsg'

function plistValue(plist, key, format) {
  return execFileSync('/usr/bin/plutil', ['-extract', key, format, '-o', '-', '--', '-'], { input: plist, encoding: 'utf8' }).trim()
}

function distributionProfileUUID(file) {
  const plist = execFileSync('/usr/bin/security', ['cms', '-D', '-i', file])
  return validateDistributionProfile({
    claims: JSON.parse(plistValue(plist, 'Entitlements', 'json')),
    teams: JSON.parse(plistValue(plist, 'TeamIdentifier', 'json')),
    uuid: plistValue(plist, 'UUID', 'raw'),
    allDevices: plistValue(plist, 'ProvisionsAllDevices', 'raw') === 'true',
    expires: plistValue(plist, 'ExpirationDate', 'raw')
  })
}

function validateDistributionProfile({ claims, teams, uuid, allDevices, expires }) {
  if (!/^[a-f\d]{8}(?:-[a-f\d]{4}){3}-[a-f\d]{12}$/i.test(uuid) ||
      !Array.isArray(teams) || !teams.includes(policy.releaseTeam) ||
      claims['com.apple.application-identifier'] !== `${policy.releaseTeam}.${moduleIdentifier}` ||
      claims['com.apple.developer.fskit.fsmodule'] !== true ||
      !allDevices || !(Date.parse(expires) > Date.now())) {
    throw new Error(`Expected a valid Developer ID FSKit profile for ${policy.releaseTeam}.${moduleIdentifier}`)
  }
  return uuid
}

function entitlements(bundle) {
  const plist = execFileSync('/usr/bin/codesign', ['-d', '--entitlements', '-', '--xml', bundle], { stdio: ['ignore', 'pipe', 'pipe'] })
  return JSON.parse(execFileSync('/usr/bin/plutil', ['-convert', 'json', '-o', '-', '--', '-'], { input: plist, encoding: 'utf8' }))
}

function daemonTeam(daemon, release) {
  if (release) return policy.releaseTeam
  const display = spawnSync('/usr/bin/codesign', ['-d', '--verbose=2', daemon], { encoding: 'utf8' })
  const team = /^TeamIdentifier=([A-Z0-9]{10})$/m.exec(display.stderr)?.[1]
  if (!team) throw new Error('FSKit development builds need a team-signed daemon')
  return team
}

function verifyCode(bundle, identifier, team, release) {
  const expression = release ? requirement(identifier, team)
    : `identifier "${identifier}" and anchor apple generic and certificate leaf[subject.OU] = "${team}"`
  execFileSync('/usr/bin/codesign', ['--verify', '--strict', '-R', `=${expression}`, bundle])
}

function verifyMountEntitlements(moduleClaims, daemonClaims, team) {
  const group = `${team}.net.dmsg.secrets`
  if (moduleClaims['com.apple.developer.fskit.fsmodule'] !== true || moduleClaims['com.apple.security.app-sandbox'] !== true) {
    throw new Error('SecretsFS needs its sandbox and FSKit Module entitlements')
  }
  for (const claims of [moduleClaims, daemonClaims]) {
    const groups = claims['com.apple.security.application-groups']
    if (!Array.isArray(groups) || !groups.includes(group)) throw new Error(`Missing FSKit App Group ${group}`)
  }
}

function verifyModule(extension, daemon, team, release) {
  verifyCode(extension, moduleIdentifier, team, release)
  verifyCode(daemon, policy.daemonIdentifier, team, release)
  verifyMountEntitlements(entitlements(extension), entitlements(daemon), team)
  const profile = path.join(extension, 'Contents', 'embedded.provisionprofile')
  if (release) distributionProfileUUID(profile)
  else if (!fs.existsSync(profile)) throw new Error('Missing embedded FSKit profile')
}

function verifyApp(app, release) {
  const daemon = path.join(app, daemonPath)
  const team = daemonTeam(daemon, release)
  verifyCode(app, policy.desktopIdentifier, team, release)
  if (release) verifyDesignatedRequirement(app, requirement(policy.desktopIdentifier, policy.releaseTeam))
  verifyModule(path.join(app, extensionPath), daemon, team, release)
}

function appPath(context) {
  return path.join(context.appOutDir, context.packager.appInfo.productFilename + '.app')
}

function afterPack(context) {
  const source = process.env.DMSG_FSKIT_EXTENSION
  if (context.electronPlatformName !== 'darwin' || !source) return
  const app = appPath(context)
  const daemon = path.join(app, daemonPath)
  const release = Boolean(process.env.CSC_LINK)
  verifyModule(source, daemon, daemonTeam(daemon, release), release)
  const target = path.join(app, extensionPath)
  if (fs.existsSync(target)) throw new Error(`FSKit destination already exists: ${target}`)
  fs.mkdirSync(path.dirname(target), { recursive: true })
  execFileSync('/usr/bin/ditto', [source, target])
}

function afterSign(context) {
  if (context.electronPlatformName === 'darwin' && process.env.DMSG_FSKIT_EXTENSION) {
    verifyApp(appPath(context), Boolean(process.env.CSC_LINK))
  }
}

module.exports = { afterPack, afterSign, verifyApp, validateDistributionProfile, verifyMountEntitlements }

if (require.main === module) {
  const [operation, file] = process.argv.slice(2)
  if (operation === 'profile-uuid' && file) console.log(distributionProfileUUID(file))
  else if (operation === 'verify-app' && file) verifyApp(file, true)
  else throw new Error('Usage: fskit-package.cjs profile-uuid PROFILE | verify-app APP')
}
