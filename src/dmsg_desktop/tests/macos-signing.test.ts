import assert from 'node:assert/strict'
import { test } from 'node:test'
import { createRequire } from 'node:module'
import { mkdtempSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { execFileSync } from 'node:child_process'

const require = createRequire(import.meta.url)
const { policy, teamsFor, requirement, assertReleaseTeam } = require('../../../scripts/dmsg/macos-signing.cjs')
const { optionsForTransition } = require('../scripts/sign-macos.cjs')

test('reviewed identities are pinned and only the configured team may sign a release', () => {
  assert.deepEqual(teamsFor(policy.sourceTeam), ['S9Z2WAMAV8', '63G8VNQPXB'])
  assert.deepEqual(teamsFor(policy.recipientTeam), ['63G8VNQPXB'])
  assert.throws(() => teamsFor('OTHERTEAM1'))
  assertReleaseTeam(policy.releaseTeam)
  assert.throws(() => assertReleaseTeam(policy.releaseTeam === policy.sourceTeam ? policy.recipientTeam : policy.sourceTeam))
  assert.throws(() => requirement('net.dmsg.dmsg" or true', [policy.sourceTeam]))
  assert.throws(() => requirement(policy.daemonIdentifier, ['S9Z2WAMAV8" or true']))
})

test('the transition requirement applies only to the outer app, preserving nested identity and signing options', () => {
  const app = '/tmp/dMsg signing test/dMsg.app'
  const fileOptions = { entitlements: '/fixture.plist', hardenedRuntime: true, timestamp: 'none' }
  const options = optionsForTransition({ app, identity: 'original-identity', ignore: ['keep-runtime'], optionsForFile: () => fileOptions })
  assert.equal(options.identity, 'original-identity')
  assert.deepEqual(options.ignore, ['keep-runtime'])
  const main = options.optionsForFile(app)
  assert.equal(main.entitlements, fileOptions.entitlements)
  assert.equal(main.timestamp, 'none')
  assert.ok(main.requirements.includes('identifier "net.dmsg.desktop"'))
  assert.ok(main.requirements.includes('63G8VNQPXB'))
  for (const nested of ['/Contents/Frameworks/dMsg Helper.app', '/Contents/Resources/runtime/dmsg', '/Contents/Extensions/SecretsFS.appex']) {
    assert.deepEqual(options.optionsForFile(app + nested), fileOptions)
  }
})

test('Apple accepts the exact Developer ID requirement syntax', { skip: process.platform !== 'darwin' }, () => {
  const directory = mkdtempSync(join(tmpdir(), 'dmsg-requirement-'))
  try {
    for (const [index, teams] of [teamsFor(policy.sourceTeam), teamsFor(policy.recipientTeam)].entries()) {
      execFileSync('/usr/bin/csreq', ['-r', `=${requirement(policy.desktopIdentifier, teams)}`, '-b', join(directory, `${index}.csreq`)])
    }
  } finally { rmSync(directory, { recursive: true, force: true }) }
})
