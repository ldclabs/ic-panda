import assert from 'node:assert/strict'
import { test } from 'node:test'
import { submission, submit, validateKey } from './indexnow.mjs'

const sitemap =
  '<urlset><url><loc>https://dmsg.net/</loc></url><url><loc>https://dmsg.net/_/about.zh.html</loc></url></urlset>'
const payload = submission(sitemap, 'test-key-1234')

test('only canonical sitemap entries can be submitted', () => {
  assert.equal(payload.urlList.length, 2)
  assert.deepEqual(
    submission(sitemap, payload.key, [payload.urlList[1], payload.urlList[1]])
      .urlList,
    [payload.urlList[1]]
  )
  for (const url of [
    'https://other.net/',
    'https://dmsg.net/_/login',
    'https://dmsg.net/?token=x',
    'https://dmsg.net/#secret'
  ])
    assert.throws(() => submission(sitemap, payload.key, [url]))
  for (const key of ['', '../test-key', 'x'.repeat(129)])
    assert.throws(() => validateKey(key))
  assert.throws(() => submission('<urlset/>', payload.key))
})

test('does not submit when the deployed key or page is a fallback or an older build', async () => {
  for (const badAt of [0, 1, 2]) {
    let calls = 0
    await assert.rejects(
      submit(
        payload,
        () => 'built page',
        async (_url, options) => {
          assert.notEqual(options.method, 'POST')
          const index = calls++
          return new Response(
            index === badAt
              ? 'SPA fallback'
              : index === 0
                ? payload.key
                : 'built page'
          )
        }
      ),
      /does not match/
    )
    assert.equal(calls, badAt + 1)
  }
})

test('submits only after deployment checks and distinguishes API outcomes', async () => {
  for (const status of [200, 202, 403, 429]) {
    const calls = []
    const result = submit(
      payload,
      () => 'built page',
      async (url, options) => {
        calls.push(url)
        if (options.method === 'POST') {
          assert.equal(calls.length, 4)
          assert.deepEqual(JSON.parse(options.body), payload)
          return new Response('', { status })
        }
        return new Response(
          url === payload.keyLocation ? payload.key : 'built page'
        )
      }
    )
    if (status === 200 || status === 202)
      assert.match(
        await result,
        status === 202 ? /validation pending/ : /received \(200\)/
      )
    else await assert.rejects(result, new RegExp(`HTTP ${status}`))
  }
})
