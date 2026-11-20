import assert from 'node:assert/strict'
import { test } from 'node:test'
import { createRequire } from 'node:module'
import { copyFileSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { execFileSync, spawnSync } from 'node:child_process'

const require = createRequire(import.meta.url)
const { policy, requirement, assertReleaseTeam } = require('../../../scripts/dmsg/macos-signing.cjs')
const sign = require('../scripts/sign-macos.cjs')
const { validateDistributionProfile, verifyApp, verifyMountEntitlements } = require('../scripts/fskit-package.cjs')

test('reviewed identities are pinned and only the configured team may sign a release', () => {
  assertReleaseTeam('63G8VNQPXB')
  for (const other of ['S9Z2WAMAV8', 'OTHERTEAM1']) assert.throws(() => assertReleaseTeam(other))
  assert.throws(() => requirement('net.dmsg.dmsg" or true', policy.releaseTeam))
  assert.throws(() => requirement(policy.daemonIdentifier, 'S9Z2WAMAV8" or true'))
})

test('the Yiwen requirement applies only to the outer app and preserves osx-sign 2 file options', () => {
  const app = '/tmp/dMsg signing test/dMsg.app'
  const fileOptions = { entitlements: '/fixture.plist', hardenedRuntime: true, timestamp: 'none' }
  const context = { platform: 'darwin' }
  const options = sign.optionsForRelease({ app, identity: 'original-identity', ignore: ['keep-runtime'], optionsForFile: (_file: string, ctx: unknown) => {
    assert.equal(ctx, context)
    return fileOptions
  } })
  assert.equal(options.identity, 'original-identity')
  assert.deepEqual(options.ignore, ['keep-runtime'])
  const main = options.optionsForFile(app, context)
  assert.equal(main.entitlements, fileOptions.entitlements)
  assert.equal(main.timestamp, 'none')
  assert.ok(main.requirements.includes('identifier "net.dmsg.desktop"'))
  assert.equal(main.requirements, `=designated => ${requirement('net.dmsg.desktop', '63G8VNQPXB')}`)
  assert.ok(!main.requirements.includes(policy.sourceTeam))
  for (const nested of ['/Contents/Frameworks/dMsg Helper.app', '/Contents/Resources/runtime/dmsg', '/Contents/Extensions/SecretsFS.appex']) {
    assert.deepEqual(options.optionsForFile(app + nested, context), fileOptions)
  }
})

test('Apple accepts the exact Developer ID requirement syntax', { skip: process.platform !== 'darwin' }, () => {
  const directory = mkdtempSync(join(tmpdir(), 'dmsg-requirement-'))
  try {
    for (const team of [policy.sourceTeam, policy.releaseTeam]) {
      execFileSync('/usr/bin/csreq', ['-r', `=${requirement(policy.desktopIdentifier, team)}`, '-b', join(directory, `${team}.csreq`)])
    }
  } finally { rmSync(directory, { recursive: true, force: true }) }
})

test('the release signing hook reaches the installed osx-sign API', async () => {
  const directory = mkdtempSync(join(tmpdir(), 'dmsg-sign-entry-'))
  try {
    await assert.rejects(sign({ platform: 'darwin', app: join(directory, 'missing.app') }), /Application at path .* could not be found/)
  } finally { rmSync(directory, { recursive: true, force: true }) }
})

test('a macOS release cannot package without the FSKit extension', { skip: process.platform !== 'darwin' }, () => {
  const config = require.resolve('../electron-builder.release.cjs')
  const result = spawnSync(process.execPath, ['-e', 'require(process.argv[1])', config], {
    encoding: 'utf8',
    env: { ...process.env, CSC_LINK: 'test-certificate', DMSG_FSKIT_EXTENSION: '' }
  })
  assert.equal(result.status, 1)
  assert.match(result.stderr, /signed SecretsFS\.appex in DMSG_FSKIT_EXTENSION/)
})

test('macOS update verification rejects modified resources in an Electron helper', { skip: process.platform !== 'darwin' }, () => {
  const directory = mkdtempSync(join(tmpdir(), 'dmsg-nested-signature-'))
  try {
    const app = join(directory, 'dMsg.app')
    const helper = join(app, 'Contents', 'Frameworks', 'Helper.app')
    for (const [bundle, identifier] of [[app, policy.desktopIdentifier], [helper, policy.desktopIdentifier + '.helper']]) {
      const contents = join(bundle, 'Contents')
      mkdirSync(join(contents, 'MacOS'), { recursive: true })
      copyFileSync('/usr/bin/true', join(contents, 'MacOS', 'fixture'))
      writeFileSync(join(contents, 'Info.plist'), `<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict>
  <key>CFBundleIdentifier</key><string>${identifier}</string>
  <key>CFBundleExecutable</key><string>fixture</string>
  <key>CFBundlePackageType</key><string>APPL</string>
</dict></plist>`)
    }
    const resource = join(helper, 'Contents', 'Resources', 'fixture.txt')
    mkdirSync(dirname(resource), { recursive: true })
    writeFileSync(resource, 'sealed content')
    for (const bundle of [helper, app]) {
      execFileSync('/usr/bin/codesign', ['--force', '--sign', '-', bundle], { stdio: 'pipe' })
    }
    // The intact ad hoc signature fails only the Developer ID requirement.
    assert.throws(() => verifyApp(app, true), { status: 3 })
    writeFileSync(resource, 'modified content')
    assert.throws(() => verifyApp(app, true), { status: 1 })
  } finally { rmSync(directory, { recursive: true, force: true }) }
})

test('FSKit distribution requires the Yiwen module profile and the IPC group on both peers', () => {
  const team = '63G8VNQPXB', identifier = team + '.net.dmsg.desktop.SecretFiles'
  const claims = { 'com.apple.application-identifier': identifier, 'com.apple.developer.fskit.fsmodule': true }
  const profile = { teams: [team], claims, uuid: 'AA75F0CB-4E75-4465-9EAF-E66DF2716D5B', expires: new Date(Date.now() + 86_400_000).toISOString(), allDevices: true }
  assert.equal(validateDistributionProfile(profile), profile.uuid)
  for (const change of [{ teams: [policy.sourceTeam] }, { allDevices: false }, { expires: '2000-01-01' },
    { claims: { ...claims, 'com.apple.application-identifier': team + '.net.dmsg.desktop.SecretsFS' } },
    { claims: { ...claims, 'com.apple.application-identifier': team + '.*' } }, { claims: { ...claims, 'com.apple.developer.fskit.fsmodule': false } }]) {
    assert.throws(() => validateDistributionProfile({ ...profile, ...change }))
  }
  const daemon = { 'com.apple.security.application-groups': [team + '.net.dmsg.secrets'] }
  const extension = { ...daemon, 'com.apple.developer.fskit.fsmodule': true, 'com.apple.security.app-sandbox': true }
  verifyMountEntitlements(extension, daemon, team)
  assert.throws(() => verifyMountEntitlements(daemon, daemon, team))
  assert.throws(() => verifyMountEntitlements({ ...extension, 'com.apple.security.app-sandbox': false }, daemon, team))
  assert.throws(() => verifyMountEntitlements({ ...extension, 'com.apple.security.application-groups': [] }, daemon, team))
  assert.throws(() => verifyMountEntitlements(extension, { 'com.apple.security.application-groups': [policy.sourceTeam + '.net.dmsg.secrets'] }, team))
})
