import { createHash } from 'node:crypto'
import { existsSync, readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { describe, expect, it } from 'vitest'
import { locales } from '$lib/i18n/locales'
import { currentTerms, renderDocument, termsVersions } from './index'

const dir = resolve('src/lib/legal')
const englishTerms = readFileSync(resolve(`static${currentTerms.path}`), 'utf8')

function source(kind: string, locale: string): string {
  if (kind === 'terms')
    return locale === 'en'
      ? englishTerms
      : readFileSync(
          `${dir}/terms/v${currentTerms.version}/${locale}.md`,
          'utf8'
        )
  return readFileSync(`${dir}/${kind}/${locale}.md`, 'utf8')
}

describe('legal documents', () => {
  it('lifts the title and anchors sections without dropping the text after it', () => {
    const doc = renderDocument(
      '# Title\n\nUpdated today.\n\n## One\n\nBody.\n\n## Two\n\nMore.\n',
      'en'
    )
    expect(doc.title).toBe('Title')
    expect(doc.sections).toEqual([
      { id: 'section-1', title: 'One' },
      { id: 'section-2', title: 'Two' }
    ])
    expect(doc.html).toMatch(/^<p>Updated today\.<\/p>/)
    expect(doc.html).toContain('<h2 id="section-1">One</h2>')
    expect(doc.html).not.toContain('<h1')
  })

  it('opens external links in a new tab and keeps dmsg.net links on the current site', () => {
    const { html } = renderDocument(
      '# T\n\n[a](https://x.com/ICPandaDAO) [b](/_/terms) https://dmsg.net/_/privacy and dmsg.net\n',
      'en'
    )
    expect(html).toContain(
      '<a href="https://x.com/ICPandaDAO" target="_blank" rel="noopener noreferrer">a</a>'
    )
    expect(html).toContain('<a href="/_/terms">b</a>')
    expect(html).toContain(
      '<a href="/_/privacy">https://dmsg.net/_/privacy</a>'
    )
    expect(html).toContain('and dmsg.net')
  })

  it('translates every document with the same sections as the English text', () => {
    for (const kind of ['privacy', 'terms', 'support']) {
      const english = renderDocument(source(kind, 'en'), 'en').sections.length
      expect(english).toBeGreaterThan(0)
      for (const locale of locales) {
        const doc = renderDocument(source(kind, locale), locale)
        expect(doc.title, `${kind}/${locale}`).not.toBe('')
        expect(doc.sections.length, `${kind}/${locale}`).toBe(english)
      }
    }
  })

  it('lists the published terms with the digest of each file', () => {
    expect(termsVersions.map((v) => v.version)).toEqual(
      termsVersions.map((_, i) => i + 1)
    )
    for (const v of termsVersions) {
      const file = resolve(`static${v.path}`)
      expect(existsSync(file)).toBe(true)
      expect(
        createHash('sha256').update(readFileSync(file)).digest('hex')
      ).toBe(v.sha256)
    }
  })
})
