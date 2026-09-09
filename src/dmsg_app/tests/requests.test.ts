import { describe, expect, it } from 'vitest'
import {
  parseRequest,
  sameSource,
  trustedSource,
  type SourceBinding
} from '../src/lib/protocol/requests'
const now = 1788800000000
const sender = {
  origin: 'https://app.example',
  url: 'https://app.example/project',
  frameId: 0,
  documentId: 'document-A',
  documentLifecycle: 'active',
  tab: { id: 4 }
} as chrome.runtime.MessageSender
const source: SourceBinding = {
  origin: 'https://app.example',
  frameId: 0,
  documentId: 'document-A',
  tabId: 4
}
const request = {
  protocol: 'dmsg-extension/3',
  method: 'signature.request',
  accountId: '040g2081040g2081040g',
  requestId: '22'.repeat(32),
  nonce: '33'.repeat(32),
  expiresAt: String(now + 60000),
  statement: {
    issuer: 'https://dmsg.test/u/040g2081040g2081040g',
    content: { kind: 'text', text: 'Confirm this release note.' }
  }
}
describe('external request boundary', () => {
  it('accepts only browser-bound allowlisted top-level documents', () => {
    expect(trustedSource(sender, [source.origin])).toEqual(source)
    expect(
      JSON.stringify(
        trustedSource(
          { ...sender, url: 'https://app.example/project?access_token=private#secret' },
          [source.origin]
        )
      )
    ).not.toContain('access_token')
  })
  it.each([
    { origin: 'https://evil.example' },
    { origin: 'null' },
    { frameId: 1 },
    { documentId: undefined },
    { documentLifecycle: 'prerender' },
    { url: 'https://evil.example' }
  ])('rejects mismatched sender %j', (patch) => {
    expect(() =>
      trustedSource({ ...sender, ...patch } as chrome.runtime.MessageSender, [source.origin])
    ).toThrow()
  })
  it('does not take origin, key purpose or raw bytes from caller claims', () => {
    expect(() => parseRequest({ ...request, origin: source.origin }, source, now)).toThrow()
    expect(() =>
      parseRequest(
        { ...request, body: { kind: 'signHash', hash: '11'.repeat(32) } },
        source,
        now
      )
    ).toThrow()
    expect(() => parseRequest({ ...request, method: 'wallet_sign' }, source, now)).toThrow()
  })
  it('binds every payload byte and document to the review digest', () => {
    const parsed = parseRequest(request, source, now)
    expect(
      parseRequest(
        {
          ...request,
          statement: {
            ...request.statement,
            content: { kind: 'text', text: 'Different statement' }
          }
        },
        source,
        now
      ).digest
    ).not.toBe(parsed.digest)
    expect(
      parseRequest(request, { ...source, documentId: 'document-B' }, now).digest
    ).not.toBe(parsed.digest)
    const spaced = parseRequest(
      {
        ...request,
        statement: {
          ...request.statement,
          content: { kind: 'text', text: ` ${request.statement.content.text} ` }
        }
      },
      source,
      now
    )
    expect(spaced.request.statement.content).toEqual({
      kind: 'text',
      text: ` ${request.statement.content.text} `
    })
    expect(spaced.digest).not.toBe(parsed.digest)
    expect(sameSource(source, { ...source, documentId: 'document-B' })).toBe(false)
  })
  it('keeps approval milliseconds separate from signed CWT seconds and rejects old fields', () => {
    expect(() => parseRequest({ ...request, expiresAt: String(now) }, source, now)).toThrow()
    expect(() =>
      parseRequest({ ...request, expiresAt: String(now + 300001) }, source, now)
    ).toThrow()
    expect(() =>
      parseRequest(
        { ...request, statement: { ...request.statement, issuedAt: '1788800000' } },
        source,
        now
      )
    ).not.toThrow()
    expect(() =>
      parseRequest(
        { ...request, statement: { ...request.statement, issuedAt: '9223372036854775808' } },
        source,
        now
      )
    ).toThrow()
    expect(() =>
      parseRequest(
        {
          ...request,
          statement: {
            ...request.statement,
            content: { kind: 'digest', sha256: '00'.repeat(32) }
          }
        },
        source,
        now
      )
    ).not.toThrow()
    expect(() =>
      parseRequest(
        {
          ...request,
          statement: {
            ...request.statement,
            content: { kind: 'digest', sha256: '11'.repeat(32), size: '12' }
          }
        },
        source,
        now
      )
    ).toThrow()
    expect(() =>
      parseRequest({ ...request, accountId: '11'.repeat(32) }, source, now)
    ).toThrow()
  })
})
