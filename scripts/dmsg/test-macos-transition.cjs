#!/usr/bin/env node
// Isolated, locally signed fixtures. No release download, installation, default
// keychain changes, or access to the user's dmsg credentials.
const assert = require('node:assert/strict')
const { execFileSync, spawnSync } = require('node:child_process')
const { mkdtempSync, mkdirSync, writeFileSync, rmSync } = require('node:fs')
const { tmpdir } = require('node:os')
const { join } = require('node:path')
const { policy, requirement, teamsFor } = require('./macos-signing.cjs')
const sign = require('../../src/dmsg_desktop/scripts/sign-macos.cjs')

if (process.platform !== 'darwin') throw new Error('This verification requires macOS')
const identities = execFileSync('/usr/bin/security', ['find-identity', '-v', '-p', 'codesigning'], { encoding: 'utf8' })
function identity(team) {
  return [...identities.matchAll(/\b([A-F0-9]{40}) "Developer ID Application: ([^"\n]+)"/g)]
    .find(match => match[2].endsWith(`(${team})`))?.[1]
}
const source = identity(policy.sourceTeam)
if (!source) throw new Error('The LDC Developer ID signing identity is required for these fixtures')
const recipient = identity(policy.recipientTeam)
const directory = mkdtempSync(join(tmpdir(), 'dmsg-signing-transition-'))
const run = (command, args) => execFileSync(command, args, { encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] })
function matches(path, expression) {
  return spawnSync('/usr/bin/codesign', ['--verify', '--strict', '-R', `=${expression}`, path], { stdio: 'pipe' }).status === 0
}
function designated(app) {
  const result = spawnSync('/usr/bin/codesign', ['-d', '-r-', app], { encoding: 'utf8' })
  assert.equal(result.status, 0)
  return (result.stdout + result.stderr).match(/^designated => (.+)$/m)?.[1]
}
function app(name, identifier = policy.desktopIdentifier) {
  const path = join(directory, `${name}.app`)
  mkdirSync(join(path, 'Contents', 'MacOS'), { recursive: true })
  writeFileSync(join(path, 'Contents', 'Info.plist'), `<?xml version="1.0" encoding="UTF-8"?><plist version="1.0"><dict><key>CFBundleIdentifier</key><string>${identifier}</string><key>CFBundleExecutable</key><string>fixture</string><key>CFBundlePackageType</key><string>APPL</string><key>CFBundleVersion</key><string>1</string></dict></plist>`)
  run('/usr/bin/xcrun', ['clang', join(directory, 'fixture.c'), '-o', join(path, 'Contents', 'MacOS', 'fixture')])
  return path
}
function ordinary(path, certificate) {
  run('/usr/bin/codesign', ['--force', '--sign', certificate, '--options', 'runtime', '--timestamp=none', path])
}
async function releaseApp(path, certificate, team) {
  if (team === policy.releaseTeam) {
    await sign({ app: path, identity: certificate, platform: 'darwin', type: 'distribution',
      preAutoEntitlements: false, optionsForFile: () => ({ timestamp: 'none', hardenedRuntime: true, entitlements: [] }) })
  } else {
    // Reproduce the other phase without changing the current source policy.
    run('/usr/bin/codesign', ['--force', '--sign', certificate, '--options', 'runtime', '--timestamp=none',
      '--requirements', `=designated => ${requirement(policy.desktopIdentifier, teamsFor(team))}`, path])
  }
}

async function main() {
  try {
    writeFileSync(join(directory, 'fixture.c'), 'int main(void) { return 0; }\n')
    const old = app('old'), bridge = app('bridge'), unrelated = app('unrelated', 'net.dmsg.unrelated'), adhoc = app('adhoc')
    ordinary(old, source); ordinary(unrelated, source); ordinary(adhoc, '-')
    await releaseApp(bridge, source, policy.sourceTeam)
    const oldDR = designated(old), bridgeDR = designated(bridge)
    assert.ok(oldDR && bridgeDR)
    assert.ok(matches(bridge, oldDR), 'The transition must remain installable by the old Squirrel requirement')
    assert.ok(matches(old, bridgeDR))
    assert.ok(!matches(unrelated, bridgeDR), 'Another app signed by LDC is not dMsg')
    assert.ok(!matches(adhoc, bridgeDR), 'Ad-hoc signing is never a release signature')
    assert.ok(!matches(old, requirement(policy.desktopIdentifier, [policy.recipientTeam])), 'The recipient cannot trust LDC back')
    writeFileSync(join(bridge, 'Contents', 'tampered'), 'not signed')
    assert.ok(!matches(bridge, bridgeDR), 'A matching identity must not bypass resource integrity')

    // Verify the CLI requirement against an actual Developer ID binary too.
    const daemon = join(directory, 'dmsg')
    run('/usr/bin/xcrun', ['clang', join(directory, 'fixture.c'), '-o', daemon])
    run('/usr/bin/codesign', ['--force', '--sign', source, '--identifier', policy.daemonIdentifier, '--options', 'runtime', '--timestamp=none', daemon])
    assert.ok(matches(daemon, requirement(policy.daemonIdentifier, teamsFor(policy.sourceTeam))))
    assert.ok(!matches(daemon, requirement(policy.desktopIdentifier, teamsFor(policy.sourceTeam))))
    assert.ok(!matches(daemon, requirement(policy.daemonIdentifier, [policy.recipientTeam])))

    if (recipient) {
      const future = app('recipient'); await releaseApp(future, recipient, policy.recipientTeam)
      assert.ok(!matches(future, oldDR))
      assert.ok(matches(future, bridgeDR))
      assert.ok(!matches(old, designated(future)))
      console.log('LDC → bridge → Yiwen Developer ID requirement checks passed')
    } else {
      console.log('LDC → bridge and negative signature checks passed; actual Yiwen signature is NOT tested (identity unavailable)')
    }
    console.log('No application was installed and no production Keychain was accessed')
  } finally { rmSync(directory, { recursive: true, force: true }) }
}
main().catch(error => { console.error(error.message); process.exitCode = 1 })
