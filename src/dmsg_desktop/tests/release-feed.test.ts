import assert from 'node:assert/strict'
import { test } from 'node:test'
import { desktopRelease, releaseChannel, ReleaseFeed, selectRelease } from '../src/main/release-feed.ts'

function release(version: string, platform = 'darwin') {
  const beta = version.includes('-beta.')
  const installer = platform === 'darwin' ? 'dMsg-mac-arm64.zip' : 'dMsg-win-x64.exe'
  const runtime = platform === 'darwin' ? 'dmsg-macos-arm64' : 'dmsg-windows-x64.exe'
  const names = [installer, `${installer}.sha256`, runtime, `${runtime}.sha256`, `${beta ? 'beta' : 'latest'}${platform === 'darwin' ? '-mac' : ''}.yml`]
  return { tag_name: version, draft: false, prerelease: beta, assets: names.map(name => ({ name, state: 'uploaded' })) }
}

test('only complete dMsg packages with matching SemVer and GitHub release type are eligible', () => {
  for (const version of ['1.3.0', '1.3.0-beta.1', '1.3.0-beta.10']) {
    assert.equal(desktopRelease(release(version), 'darwin', 'arm64')?.version, version)
    assert.equal(desktopRelease(release(version, 'win32'), 'win32', 'x64')?.version, version)
  }
  for (const version of ['v1.3.0', 'dmsg-v1.3.0', 'canisters-v3.1.6', '1.3.0-alpha.1', '1.3.0-rc.1', '1.3.0-beta.01', '1.3.0+build.2', '../1.3.0']) {
    assert.equal(releaseChannel(version), null, version)
  }
  for (const value of [
    { ...release('1.3.0'), draft: true },
    { ...release('1.3.0'), prerelease: true },
    { ...release('1.3.0-beta.1'), prerelease: false },
    { ...release('1.3.0'), assets: [] },
    { ...release('1.3.0'), assets: release('1.3.0').assets.slice(1) },
    { ...release('1.3.0'), assets: release('1.3.0').assets.map(asset => ({ ...asset, state: 'new' })) }
  ]) assert.equal(desktopRelease(value, 'darwin', 'arm64'), null)
  assert.equal(desktopRelease(release('1.3.0'), 'darwin', 'x64'), null)
  assert.equal(desktopRelease(release('1.3.0'), 'linux', 'x64'), null)
})

test('stable excludes beta, beta uses SemVer rather than publication order, and neither downgrades', () => {
  const versions = ['1.2.4', '1.3.0-beta.2', '1.3.0-beta.10', '1.2.3']
  const candidates = versions.map(version => desktopRelease(release(version), 'darwin', 'arm64')!)
  assert.equal(selectRelease(candidates, 'stable', '1.2.2')?.version, '1.2.4')
  assert.equal(selectRelease(candidates, 'beta', '1.2.2')?.version, '1.3.0-beta.10')
  assert.equal(selectRelease(candidates, 'beta', '1.3.0-beta.10'), null)
  assert.equal(selectRelease(candidates, 'stable', '1.3.0-beta.2'), null)
  candidates.push(desktopRelease(release('1.3.0'), 'darwin', 'arm64')!)
  assert.equal(selectRelease(candidates, 'beta', '1.3.0-beta.10')?.version, '1.3.0')
  assert.equal(selectRelease(candidates, 'stable', '1.3.0-beta.10')?.version, '1.3.0')
})

test('beta discovery paginates past canister releases and caches repeated checks', async () => {
  const requests: string[] = []
  const feed = new ReleaseFeed(async (input) => {
    const url = String(input)
    requests.push(url)
    return Response.json(url.endsWith('page=1') ? Array.from({ length: 100 }, () => release('canisters-v3.1.6')) :
      [release('1.2.4'), release('1.3.0-beta.10'), release('1.3.0-beta.2')], { headers: { etag: 'fixture' } })
  })
  const signal = new AbortController().signal
  assert.equal((await feed.latest('beta', '1.2.2', 'darwin', 'arm64', signal))?.version, '1.3.0-beta.10')
  assert.equal(requests.length, 2)
  await feed.latest('beta', '1.2.2', 'darwin', 'arm64', signal)
  assert.equal(requests.length, 2)
})

test('stable discovery does not trust a beta marked latest; errors and rate limits are not up-to-date', async () => {
  const signal = new AbortController().signal
  const beta = new ReleaseFeed(async () => Response.json(release('1.3.0-beta.1')))
  await assert.rejects(beta.latest('stable', '1.2.2', 'darwin', 'arm64', signal), /complete stable/)
  let calls = 0
  const limited = new ReleaseFeed(async () => {
    calls++
    return new Response('', { status: 429, headers: { 'retry-after': '60' } })
  })
  await assert.rejects(limited.latest('beta', '1.2.2', 'darwin', 'arm64', signal), /HTTP 429/)
  await assert.rejects(limited.latest('beta', '1.2.2', 'darwin', 'arm64', signal), /rate limit/)
  assert.equal(calls, 1)
})
