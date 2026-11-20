#!/usr/bin/env node
// Inspect existing signed bundles. No signing identity or installation is needed.
const assert = require('node:assert/strict')
const { execFileSync, spawnSync } = require('node:child_process')
const { join } = require('node:path')
const { policy, requirement, designatedRequirement, verifyDesignatedRequirement } = require('./macos-signing.cjs')
const { assertLegacyLatest } = require('./release-config.cjs')
const { verifyApp } = require('../../src/dmsg_desktop/scripts/fskit-package.cjs')

const [bridge, candidate] = process.argv.slice(2)
if (process.platform !== 'darwin' || !bridge || process.argv.length > 4) {
  throw new Error('Usage on macOS: test-macos-transition.cjs LDC-bridge.app [Yiwen-candidate.app]')
}
const version = execFileSync('/usr/bin/plutil', ['-extract', 'CFBundleShortVersionString', 'raw', '-o', '-', join(bridge, 'Contents/Info.plist')], { encoding: 'utf8' }).trim()
assertLegacyLatest(version)
execFileSync('/usr/bin/codesign', ['--verify', '--deep', '--strict', '-R', `=${requirement(policy.desktopIdentifier, policy.sourceTeam)}`, bridge])
execFileSync('/usr/bin/codesign', ['--verify', '--strict', '-R', `=${requirement(policy.daemonIdentifier, policy.sourceTeam)}`, join(bridge, 'Contents/Resources/runtime/dmsg')])
const bridgeDR = verifyDesignatedRequirement(bridge, requirement(policy.desktopIdentifier, policy.sourceTeam, policy.releaseTeam))
console.log(`Verified the signed LDC ${version} bridge and its daemon.`)
if (candidate) {
  verifyApp(candidate, true)
  execFileSync('/usr/bin/codesign', ['--verify', '--deep', '--strict', '-R', `=${bridgeDR}`, candidate])
  const reverse = spawnSync('/usr/bin/codesign', ['--verify', '--strict', '-R', `=${designatedRequirement(candidate)}`, bridge])
  assert.equal(reverse.status, 3, 'The Yiwen requirement must reject the valid LDC app')
  console.log('Verified the bridge accepts the Yiwen update, the reverse is refused, and the distribution FSKit module is embedded.')
} else {
  console.log('Pass a signed Yiwen candidate as the second argument to verify the update.')
}
