import assert from 'node:assert/strict'
import { test } from 'node:test'
import { createRequire } from 'node:module'
import { createHash } from 'node:crypto'
import { existsSync, mkdirSync, mkdtempSync, readFileSync, writeFileSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { pathToFileURL } from 'node:url'
import { execFileSync } from 'node:child_process'
import { stringify } from 'yaml'

const require = createRequire(import.meta.url)
const { releaseConfig, publishConfig, assertLegacyLatest } = require('../../../scripts/dmsg/release-config.cjs')
const { verifyRelease } = require('../scripts/verify-release.cjs')

test('release tags, feed channels and publication flags agree without leaking beta into stable', () => {
  for (const bridge of ['1.5.8', '1.5.10']) {
    assertLegacyLatest(bridge)
    assert.throws(() => releaseConfig(bridge), /bridges must retain their LDC signatures/)
  }
  for (const tag of ['1.5.7', '1.5.10-beta.1', '1.6.0-beta.1', '1.6.0']) assert.throws(() => assertLegacyLatest(tag))
  assert.deepEqual(releaseConfig('1.6.0-beta.1'), { version: '1.6.0-beta.1', channel: 'beta', prerelease: true, make_latest: false })
  assert.equal(releaseConfig('1.5.9').make_latest, false, 'Keep the old-team bridge discoverable by legacy clients')
  assert.equal(releaseConfig('2.0.0').make_latest, false)
  assert.deepEqual(releaseConfig('1.3.0-beta.10'), { version: '1.3.0-beta.10', channel: 'beta', prerelease: true, make_latest: false })
  assert.equal(publishConfig('1.3.0-beta.10').url, 'https://github.com/ldclabs/ic-panda/releases/download/1.3.0-beta.10/')
  for (const tag of ['v3.0.0', 'dmsg-v1.3.0', 'canisters-v3.1.6', '1.3.0-alpha.1', '1.3.0-beta.01', '1.3.0-beta.1/extra', '1.03.0']) {
    assert.throws(() => releaseConfig(tag), /Expected/)
  }
})

test('Windows packaging requires the pinned store certificate, SHA-256 and updater verification', () => {
  const config = require.resolve('../electron-builder.release.cjs')
  const read = (fingerprint: string, publisher: string) => JSON.parse(execFileSync(process.execPath, ['-e', `
    Object.defineProperty(process, 'platform', { value: 'win32' })
    const config = require(process.argv[1])
    console.log(JSON.stringify([config.forceCodeSigning, config.win]))
  `, config], {
    env: { ...process.env, DMSG_WINDOWS_CERT_SHA1: fingerprint, DMSG_WINDOWS_PUBLISHER: publisher },
    encoding: 'utf8', stdio: 'pipe'
  }))
  const fingerprint = 'AB'.repeat(20)
  assert.throws(() => read('', 'CN=Fixture'), /thumbprint and publisher/)
  assert.throws(() => read(fingerprint, ''), /thumbprint and publisher/)
  const [required, win] = read(fingerprint, 'CN=Fixture')
  assert.equal(required, true)
  assert.equal(win.verifyUpdateCodeSignature, true)
  assert.deepEqual(win.signtoolOptions, {
    certificateSha1: fingerprint, publisherName: 'CN=Fixture', signingHashAlgorithms: ['sha256'],
    rfc3161TimeStampServer: 'http://time.certum.pl'
  })
})

test('release verification rejects incomplete packages, wrong hashes, and cross-channel manifests', () => {
  const directory = mkdtempSync(join(tmpdir(), 'dmsg-release-test-'))
  try {
    const version = '1.3.0-beta.2'
    const filename = 'dMsg-mac-arm64.zip'
    const bytes = Buffer.from('synthetic signed package fixture')
    const manifest = { version, files: [{ url: filename, size: bytes.length, sha512: createHash('sha512').update(bytes).digest('base64') }] }
    writeFileSync(join(directory, 'beta-mac.yml'), stringify(manifest))
    assert.throws(() => verifyRelease(directory, version, 'darwin'), /ENOENT/)
    writeFileSync(join(directory, filename), bytes)
    writeFileSync(join(directory, `${filename}.sha256`), `${createHash('sha256').update(bytes).digest('hex')}  ${filename}\n`)
    verifyRelease(directory, version, 'darwin')
    assert.throws(() => verifyRelease(directory, '1.3.0-beta.3', 'darwin'), /Invalid update manifest/)
    writeFileSync(join(directory, 'latest-mac.yml'), stringify(manifest))
    assert.throws(() => verifyRelease(directory, version, 'darwin'), /wrong channel/)
    rmSync(join(directory, 'latest-mac.yml'))
    writeFileSync(join(directory, filename), 'tampered')
    assert.throws(() => verifyRelease(directory, version, 'darwin'), /checksum or size/)
  } finally { rmSync(directory, { recursive: true, force: true }) }
})

test('Homebrew rejects beta tags and its stable dry run uses the new tag URLs', { skip: process.platform === 'win32' }, () => {
  const directory = mkdtempSync(join(tmpdir(), 'dmsg-homebrew-test-'))
  const script = new URL('../../../scripts/dmsg/publish-homebrew.sh', import.meta.url).pathname
  try {
    const options = { env: { ...process.env, CHECKSUM_DIR: directory, DRY_RUN: '1' }, encoding: 'utf8' as const }
    assert.throws(() => execFileSync('sh', [script, '1.3.0-beta.1'], { ...options, stdio: 'pipe' }), /stable X.Y.Z/)
    for (const asset of ['dmsg-macos-arm64', 'dmsg-linux-x86_64', 'dmsg-linux-arm64', 'dMsg-mac-arm64.dmg']) {
      writeFileSync(join(directory, `${asset}.sha256`), `${'a'.repeat(64)}  ${asset}\n`)
    }
    const output = execFileSync('sh', [script, '1.3.0'], options)
    assert.ok(output.includes('/releases/download/1.3.0/dmsg-macos-arm64'))
    assert.ok(output.includes('/releases/download/#{version}/dMsg-mac-arm64.dmg'))
    assert.ok(!output.includes('dmsg-v'))
    assert.ok(!output.includes('auto_updates true'))
  } finally { rmSync(directory, { recursive: true, force: true }) }
})

test('the shell installer defaults to stable and an explicit beta uses its bare tag', { skip: process.platform === 'win32' }, () => {
  const directory = mkdtempSync(join(tmpdir(), 'dmsg-install-test-'))
  const script = new URL('../../../scripts/dmsg/install.sh', import.meta.url).pathname
  const bin = join(directory, 'bin')
  const called = join(directory, 'curl-call')
  try {
    mkdirSync(bin)
    writeFileSync(join(bin, 'curl'), '#!/bin/sh\ncase "$*" in\n  */latest) printf \'{"tag_name":"%s"}\\n\' "$DMSG_FIXTURE_TAG";;\n  *) printf \'%s\\n\' "$@" > "$DMSG_FIXTURE_CALL"; exit 22;;\nesac\n', { mode: 0o755 })
    const env = { ...process.env, PATH: `${bin}:${process.env.PATH}`, DMSG_VERSION: '', DMSG_FIXTURE_TAG: '1.3.0-beta.1', DMSG_FIXTURE_CALL: called }
    const options = { env, encoding: 'utf8' as const, stdio: 'pipe' as const }
    assert.throws(() => execFileSync('sh', [script], options), /not a stable dMsg/)
    assert.equal(existsSync(called), false)
    for (const [explicit, tag] of [['', '1.3.0'], ['1.4.0-beta.10', '1.4.0-beta.10']]) {
      env.DMSG_VERSION = explicit
      env.DMSG_FIXTURE_TAG = tag
      assert.throws(() => execFileSync('sh', [script], options), error => (error as { status?: number }).status === 22)
      assert.ok(readFileSync(called, 'utf8').includes(`/download/${tag}/dmsg-`))
      rmSync(called)
    }
    env.DMSG_VERSION = '1.4.0-alpha.1'
    assert.throws(() => execFileSync('sh', [script], options), /DMSG_VERSION must/)
    assert.equal(existsSync(called), false)
  } finally { rmSync(directory, { recursive: true, force: true }) }
})

test('both Linux CLIs install from a binary and its SHA-256 and reject changed bytes', { skip: process.platform === 'win32' }, () => {
  const directory = mkdtempSync(join(tmpdir(), 'dmsg-linux-install-'))
  const script = new URL('../../../scripts/dmsg/install.sh', import.meta.url).pathname
  const version = '1.6.1'
  const release = join(directory, 'download', version)
  const marker = join(directory, 'installed')
  try {
    mkdirSync(release, { recursive: true })
    writeFileSync(join(directory, 'uname'), '#!/bin/sh\ncase "$1" in -s) echo Linux;; -m) echo "$TEST_ARCH";; esac\n', { mode: 0o755 })
    for (const arch of ['arm64', 'x86_64']) {
      const asset = `dmsg-linux-${arch}`
      const bytes = `#!/bin/sh\necho '${arch}' > "$DMSG_INSTALL_DIR/installed"\n`
      writeFileSync(join(release, asset), bytes)
      writeFileSync(join(release, asset + '.sha256'), createHash('sha256').update(bytes).digest('hex') + '  ' + asset + '\n')
      const env = { ...process.env, PATH: `${directory}:${process.env.PATH}`, TEST_ARCH: arch,
        DMSG_RELEASE_URL: pathToFileURL(directory).href, DMSG_VERSION: version, DMSG_INSTALL_DIR: directory }
      const install = () => execFileSync('sh', [script], { env, encoding: 'utf8', stdio: 'pipe' })
      install()
      assert.equal(readFileSync(marker, 'utf8').trim(), arch)
      rmSync(marker)
      writeFileSync(join(release, asset), bytes + '# changed\n')
      assert.throws(install, /does not match its published SHA-256/)
      assert.equal(existsSync(marker), false)
    }
  } finally { rmSync(directory, { recursive: true, force: true }) }
})
