// @vitest-environment node
// WebCrypto needs buffers of its own realm; jsdom's would be refused.
//
// The upload page's work against a live relay, as the page runs it: the desktop
// end-to-end test (`tests/dmsg_integration`, desktop.rs) names a JSON file in
// DMSG_REQUEST_LIVE with the link and the files to send, and reads back what
// this writes to `output`. Without it there is nothing to send and the test is
// skipped.
import { readFileSync, writeFileSync } from 'node:fs'
import { describe, expect, it } from 'vitest'
import { keyFromFragment, unbase64url } from '$lib/share/decrypt'
import { RequestApi, RequestApiError } from './api'
import { readRecords } from './records'
import { openDescriptor, receiptKey, uploadToken } from './seal'
import { uploadFile, type UploadLink } from './upload'

type Input = {
  url: string
  relay: string
  site: string
  output: string
  sender: string
  files: { path: string; name: string; mime: string }[]
}

const input = process.env['DMSG_REQUEST_LIVE']

describe.skipIf(!input)('a live request link', () => {
  it('takes files as the page sends them', { timeout: 240_000 }, async () => {
    const live = JSON.parse(readFileSync(input!, 'utf8')) as Input
    const url = new URL(live.url)
    const [, , account, requestId] = url.pathname.split('/') as [
      string,
      string,
      string,
      string
    ]
    const pageKey = keyFromFragment(url.hash)
    const base = `${live.relay}/public/requests/${account}/${requestId}`
    // A browser, as far as the relay can tell: its origin, its cookie.
    let cookie = ''
    const send: typeof fetch = async (target, init) => {
      const headers = new Headers(init?.headers)
      headers.set('Origin', live.site)
      if (cookie) headers.set('Cookie', cookie)
      const response = await fetch(target, { ...init, headers })
      const set = response.headers.get('set-cookie')
      if (set) cookie = set.split(';')[0]!
      return response
    }
    const api = new RequestApi(
      base,
      uploadToken(pageKey, account, requestId),
      send
    )
    const status = await api.status()
    expect(status.status).toBe('open')
    const descriptor = await openDescriptor(
      pageKey,
      account,
      requestId,
      status.descriptor!
    )
    const link: UploadLink = {
      account,
      requestId,
      hpkePub: unbase64url(descriptor.hpke_pub),
      receiptKey: receiptKey(pageKey, account, requestId)
    }
    const uploads: string[] = []
    for (const { path, name, mime } of live.files) {
      const bytes = readFileSync(path)
      const file = Object.assign(new Blob([bytes], { type: mime }), { name })
      const sent = await uploadFile(
        api,
        link,
        {
          file,
          sender: { name: live.sender, note: 'As agreed.' },
          replaces: null
        },
        { progress: () => {} }
      )
      uploads.push(sent.uploadId)
    }
    const mine = await api.mine()
    expect(mine.persistent).toBe(true)
    const records = await readRecords(
      mine.entries,
      link.receiptKey,
      link.requestId
    )
    expect(records.map((r) => r.uploadId).sort()).toEqual([...uploads].sort())
    expect(records.every((r) => r.state === 'delivered')).toBe(true)
    // Another browser sees nothing and can withdraw nothing.
    const other = new RequestApi(
      base,
      uploadToken(pageKey, account, requestId),
      (target, init) => {
        const headers = new Headers(init?.headers)
        headers.set('Origin', live.site)
        return fetch(target, { ...init, headers })
      }
    )
    expect((await other.mine()).entries).toEqual([])
    await expect(other.withdraw(uploads[0]!)).rejects.toBeInstanceOf(
      RequestApiError
    )
    writeFileSync(live.output, JSON.stringify({ uploads, cookie }))
  })
})
