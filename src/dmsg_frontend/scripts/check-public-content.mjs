import assert from 'node:assert/strict'
import { createHash } from 'node:crypto'
import { existsSync, readFileSync, statSync } from 'node:fs'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { JSDOM } from 'jsdom'

const directory = fileURLToPath(new URL('../dist/', import.meta.url))
const read = (path) => readFileSync(join(directory, path), 'utf8')
const hasStaticTarget = (path) => {
  const exact = join(directory, path)
  if (existsSync(exact) && !statSync(exact).isDirectory()) return true
  return existsSync(`${exact}.html`)
}
const manifest = JSON.parse(read('_/knowledge.json'))
const appStore = 'https://apps.apple.com/app/id6827050680'
const normalizeQuotes = (text) => text.replace(/[‘’]/g, "'").trim()

// Ensure temporary adapter-static outputs were promoted cleanly to IC paths.
assert.equal(existsSync(join(directory, '200.html')), false, 'dist/200.html should be removed')
for (const route of [
  '_/privacy',
  '_/support',
  '_/delete-account',
  '_/verify',
  '_/redeem'
]) {
  assert.equal(
    existsSync(join(directory, `${route}.html`)),
    false,
    `stray ${route}.html duplicate should be removed`
  )
}

// 1. Verify the prerendered homepage (dist/index.html) without JavaScript.
{
  const raw = read('index.html')
  assert.ok(!/(?:href=|import\()["']\.\.?\//.test(raw), 'index.html: root-absolute asset URLs')
  assert.match(raw, /\/_app\/immutable\/entry\/start\./, 'index.html: SPA bootstrap preserved')
  assert.match(
    raw,
    /kit\.start\(app, element, __dmsgDeepLink \? undefined : \{/,
    'index.html: conditional hydration guard for deep links'
  )

  const dom = new JSDOM(raw, { url: 'https://dmsg.net/' })
  const document = dom.window.document
  assert.equal(document.querySelectorAll('title').length, 1, 'index.html: single title')
  assert.equal(document.querySelectorAll('link[rel="canonical"]').length, 1)
  assert.equal(document.querySelector('link[rel="canonical"]').href, 'https://dmsg.net/')
  assert.equal(document.querySelector('meta[property="og:url"]').content, 'https://dmsg.net/')
  assert.equal(document.getElementById('dmsg-public-content'), null, 'index.html: no overlaid markdown overview')
  assert.equal(document.getElementById('dmsg-reading-style'), null, 'index.html: no reading.css flash')
  assert.ok(document.querySelector('.dmsg-site.home'), 'index.html: real prerendered Landing page')
  assert.equal(document.querySelectorAll('h1').length, 1, 'index.html: one readable title')
  assert.ok(
    document.querySelector('main')?.textContent.includes('DMSG_AGENT_TOKEN'),
    'index.html: agent setup without JS'
  )
  assert.ok(
    document.querySelector(`main a[href="${appStore}"]`),
    'index.html: public iPhone/iPad download without JS'
  )
  assert.ok(document.querySelectorAll('main h2').length >= 7, 'index.html: complete sections')
  assert.equal(document.querySelectorAll('.plans .plan').length, 4, 'index.html: four plan cards')
  const rows = [...document.querySelectorAll('.compare-table tbody tr')].map((row) => [
    row.querySelector('th')?.textContent.trim(),
    ...[...row.querySelectorAll('td')].map((cell) => cell.textContent.trim())
  ])
  const hostedAgents = rows.find((row) => row[0] === 'Hosted Agent IDs')
  assert.deepEqual(hostedAgents?.slice(1), ['1', '3', '5', '10'], 'index.html: hosted Agent ID limits')
  const backupLimits = rows.find((row) => row[0] === 'Cloud backup file limit')
  assert.deepEqual(
    backupLimits?.slice(1),
    ['100 MiB', '100 MiB', '100 MiB', '1,000 MiB'],
    'index.html: cloud backup limits'
  )
  assert.equal(
    document.querySelector('link[rel="describedby"]')?.getAttribute('href'),
    '/llms.txt'
  )

  const linked = [
    ...document.querySelectorAll('script[type="application/ld+json"]')
  ].map((script) => JSON.parse(script.textContent))
  const data = linked.find((item) => item['@type'] === 'SoftwareApplication')
  assert.equal(data.name, 'dMsg')
  assert.equal(data.offers.length, 4, 'one offer per plan')
  assert.ok(data.offers.every((offer) => offer.priceCurrency === 'USD'))
  assert.equal(data.url, 'https://dmsg.net/')
  assert.ok(data.operatingSystem.includes('iOS'))
  assert.ok(data.operatingSystem.includes('iPadOS'))
  assert.ok(Array.isArray(data.downloadUrl), 'index.html: downloadUrl is an array')
  assert.ok(
    data.downloadUrl.some((url) => url === appStore),
    'index.html: downloadUrl contains the exact App Store URL'
  )

  const faq = linked.find((item) => item['@type'] === 'FAQPage')
  const visibleFaq = [...document.querySelectorAll('.faq details > summary')].map((el) =>
    normalizeQuotes(el.textContent)
  )
  assert.ok(faq.mainEntity.length >= 8, 'labelled questions')
  for (const entry of faq.mainEntity) {
    assert.ok(
      visibleFaq.includes(normalizeQuotes(entry.name)),
      `FAQ heading in Landing.svelte: ${entry.name}`
    )
    assert.ok(entry.acceptedAnswer.text.length > 40, entry.name)
  }

  // Exercise the <head> deep-link guard script on both `/` and a dynamic deep link.
  const guardScript = document.getElementById('dmsg-deep-link-guard')?.textContent
  assert.ok(guardScript, 'index.html: deep-link guard script present')
  assert.ok(document.getElementById('dmsg-deep-link-style'), 'index.html: deep-link guard style present')
  for (const [testUrl, shouldHide] of [
    ['https://dmsg.net/', false],
    ['https://dmsg.net/index.html', false],
    [`https://dmsg.net/s/${'a'.repeat(64)}`, true],
    [`https://dmsg.net/r/alice/${'b'.repeat(64)}`, true],
    ['https://dmsg.net/alice', true]
  ]) {
    const probe = new JSDOM('<!doctype html><html><head></head><body></body></html>', {
      url: testUrl
    })
    new Function('location', 'document', guardScript)(
      probe.window.location,
      probe.window.document
    )
    assert.equal(
      probe.window.document.documentElement.classList.contains('dmsg-deep-link'),
      shouldHide,
      `deep-link guard for ${testUrl}`
    )
    probe.window.close()
  }
  dom.window.close()
}

// 2. Verify prerendered static legal and utility pages at their IC paths.
for (const page of [
  {
    path: '_/privacy',
    file: '_/privacy',
    title: 'Privacy Policy — dMsg',
    h1: 'dMsg Privacy Policy',
    legal: true
  },
  {
    path: '_/terms',
    file: '_/terms.html',
    title: 'Terms of Service — dMsg',
    h1: 'dMsg Terms of Service',
    legal: true
  },
  {
    path: '_/support',
    file: '_/support',
    title: 'Support — dMsg',
    h1: 'Get help with dMsg',
    legal: true
  },
  {
    path: '_/delete-account',
    file: '_/delete-account',
    title: 'Delete account — dMsg',
    h1: 'Delete your dMsg account',
    legal: true
  },
  {
    path: '_/verify',
    file: '_/verify',
    title: 'Verify a dMsg signature | dMsg',
    h1: 'Verify a dMsg signature',
    legal: false
  },
  {
    path: '_/redeem',
    file: '_/redeem',
    title: 'Redeem code',
    h1: 'Redeem code',
    legal: false
  }
]) {
  const raw = read(page.file)
  assert.ok(!/(?:href=|import\()["']\.\.?\//.test(raw), `${page.path}: root-absolute asset URLs`)
  const url = `https://dmsg.net/${page.path}`
  const dom = new JSDOM(raw, { url })
  const document = dom.window.document
  assert.equal(document.querySelectorAll('title').length, 1, `${page.path}: single title`)
  assert.equal(document.title, page.title, `${page.path}: page title`)
  assert.equal(
    document.querySelector('meta[property="og:title"]')?.content,
    page.title,
    `${page.path}: social title`
  )
  assert.equal(document.querySelectorAll('link[rel="canonical"]').length, 1)
  assert.equal(document.querySelector('link[rel="canonical"]').href, url)
  assert.equal(document.querySelector('meta[property="og:url"]').content, url)
  assert.equal(document.getElementById('dmsg-deep-link-guard'), null, `${page.path}: no deep-link guard`)
  assert.equal(document.querySelectorAll('h1').length, 1, `${page.path}: single h1`)
  assert.equal(document.querySelector('main h1')?.textContent.trim(), page.h1, `${page.path}: h1 text`)
  if (page.legal) {
    const article = document.querySelector('article.legal-doc')
    assert.ok(article, `${page.path}: rendered legal article`)
    assert.ok(article.querySelectorAll('h2').length >= 3, `${page.path}: rendered sections`)
    assert.ok(!article.textContent.includes('Loading…'), `${page.path}: no loading placeholder`)
  }
  if (page.path === '_/delete-account') {
    assert.ok(document.querySelector('main a[href^="mailto:hi@yiwen.ai?subject="]'))
  }
  if (page.path === '_/terms') {
    assert.ok(document.querySelector('.terms-digest code')?.textContent.trim().length === 64)
  }
  dom.window.close()
}

// 3. Verify clean SPA shells for fixed dynamic/auth routes (no homepage flash).
for (const shellRoute of ['_/login', 'legacy']) {
  const raw = read(shellRoute)
  assert.match(raw, /\/_app\/immutable\/entry\/start\./, `${shellRoute}: SPA bootstrap`)
  assert.ok(!raw.includes('dmsg-site home'), `${shellRoute}: must not embed Landing page`)
  assert.ok(!raw.includes('dmsg-public-content'), `${shellRoute}: must not embed overview doc`)
  const dom = new JSDOM(raw, { url: `https://dmsg.net/${shellRoute}` })
  const document = dom.window.document
  assert.equal(document.querySelectorAll('title').length, 1, `${shellRoute}: single title`)
  assert.equal(document.querySelector('meta[name="robots"]')?.content, 'noindex', `${shellRoute}: noindex`)
  assert.equal(document.getElementById('dmsg-deep-link-guard'), null, `${shellRoute}: no deep-link guard`)
  dom.window.close()
}

// 4. Verify standalone bilingual reading guides (GEO / Markdown-free reading pages).
for (const path of ['_/about.html', '_/about.zh.html']) {
  const dom = new JSDOM(read(path), { url: `https://dmsg.net/${path}` })
  const document = dom.window.document
  const canonical = `https://dmsg.net/${path}`
  assert.equal(document.querySelectorAll('link[rel="canonical"]').length, 1)
  assert.equal(document.querySelector('link[rel="canonical"]').href, canonical)
  assert.equal(document.querySelector('meta[property="og:url"]').content, canonical)
  const headings = [...document.querySelectorAll('main h1, main h2, main h3')]
  assert.ok(headings.every((heading) => heading.id), `${path}: citable headings`)
  assert.equal(new Set(headings.map((heading) => heading.id)).size, headings.length)
  for (const lang of ['en', 'zh-Hans', 'x-default'])
    assert.ok(document.querySelector(`link[hreflang="${lang}"]`))
  assert.ok(document.querySelector('meta[property="og:description"]').content)
  assert.ok(!document.querySelector('meta[name="description"]').content.includes('`'))
  assert.equal(document.querySelectorAll('h1').length, 1, `${path}: one readable title`)
  assert.ok(
    document.querySelector('main')?.textContent.includes('DMSG_AGENT_TOKEN'),
    `${path}: agent setup without JS`
  )
  assert.ok(
    document.querySelector(`main a[href="${appStore}"]`),
    `${path}: public iPhone/iPad download without JS`
  )
  assert.ok(document.querySelectorAll('main h2').length >= 7, `${path}: complete guide`)
  assert.equal(document.querySelectorAll('table tbody tr').length, 4, `${path}: shared membership figures`)
  for (const row of document.querySelectorAll('table tbody tr')) {
    const cells = [...row.querySelectorAll('td')].map((cell) => cell.textContent.trim())
    assert.equal(cells.length, 9, `${path}: complete plan comparison`)
    assert.equal(
      cells[5],
      { Free: '1', Plus: '3', Pro: '5', Max: '10' }[cells[0]],
      `${path}: ${cells[0]} hosted Agent ID limit`
    )
    assert.equal(
      cells[3],
      cells[0] === 'Max' ? '1000 MiB' : '100 MiB',
      `${path}: ${cells[0]} backup limit`
    )
  }
  assert.equal(
    document.querySelector('link[rel="describedby"]')?.getAttribute('href'),
    '/llms.txt'
  )
  for (const link of document.querySelectorAll('a[href], link[href]')) {
    const url = new URL(link.getAttribute('href'), 'https://dmsg.net')
    if (
      url.origin !== 'https://dmsg.net' ||
      !/\.(html|md|json|txt|css|svg)$/.test(url.pathname)
    )
      continue
    assert.ok(
      existsSync(join(directory, url.pathname)),
      `${path}: missing static target ${url.pathname}`
    )
  }
  dom.window.close()
}

for (const source of manifest.documents) {
  const content = read(new URL(source.url).pathname)
  assert.equal(
    createHash('sha256').update(content).digest('hex'),
    source.sha256,
    source.url
  )
  assert.ok(content.startsWith('# '), `${source.url}: Markdown, not a SPA fallback`)
}
assert.equal(manifest.documents.length, 15)
assert.match(manifest.sourceRevision, /^[a-f0-9]{40}$/)
assert.match(read('robots.txt'), /Sitemap: https:\/\/dmsg.net\/sitemap.xml/)
const sitemap = new JSDOM(read('sitemap.xml'), {
  contentType: 'application/xml'
})
for (const location of sitemap.window.document.querySelectorAll('loc')) {
  const path = new URL(location.textContent).pathname
  assert.ok(hasStaticTarget(path === '/' ? 'index.html' : path.slice(1)), path)
}
sitemap.window.close()
assert.match(read('llms.txt'), /https:\/\/dmsg.net\/index.zh.md/)
assert.match(read('llms.txt'), /## Key facts/)
assert.match(read('sitemap.xml'), /<lastmod>\d{4}-\d{2}-\d{2}<\/lastmod>/)
assert.ok(read('llms-full.txt').includes(read('_/docs/agents.md')))

assert.equal(
  read('6eb632e3a1a444e9a68ec57d3d650e21.txt').trim(),
  '6eb632e3a1a444e9a68ec57d3d650e21'
)

const rules = JSON.parse(read('.ic-assets.json'))
for (const match of [
  'index.html',
  '_/privacy',
  '_/terms.html',
  '_/support',
  '_/delete-account',
  '_/verify',
  '_/redeem',
  '_/login',
  'legacy',
  '_/about*.html',
  'index*.md',
  '_/docs/*.md',
  'llms*.txt',
  'robots.txt',
  'sitemap.xml',
  '_/knowledge.json',
  '6eb632e3a1a444e9a68ec57d3d650e21.txt'
]) {
  const rule = rules.find((item) => item.match === match)
  assert.ok(rule, `${match}: asset headers`)
  assert.equal(rule.cache.max_age, 0)
  assert.match(rule.headers['Cache-Control'], /must-revalidate/)
  assert.ok(rule.headers['Content-Type'])
  if (match === '_/terms.html') assert.equal(rule.enable_aliasing, true)
}
console.log(
  'Public content checks passed: prerendered pages, deep-link guard, static shells, discovery, provenance and asset headers.'
)
