import assert from 'node:assert/strict'
import { createHash } from 'node:crypto'
import { existsSync, readFileSync } from 'node:fs'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { JSDOM } from 'jsdom'

const directory = fileURLToPath(new URL('../dist/', import.meta.url))
const read = (path) => readFileSync(join(directory, path), 'utf8')
const manifest = JSON.parse(read('_/knowledge.json'))

// Exercise the shipped HTML with JavaScript disabled, not Svelte source text.
for (const path of ['index.html', '_/about.html', '_/about.zh.html']) {
  const dom = new JSDOM(read(path), { url: `https://dmsg.net/${path}` })
  const document = dom.window.document
  assert.equal(
    document.querySelectorAll('h1').length,
    1,
    `${path}: one readable title`
  )
  assert.ok(
    document.querySelector('main')?.textContent.includes('DMSG_AGENT_TOKEN'),
    `${path}: agent setup without JS`
  )
  assert.ok(
    document.querySelectorAll('main h2').length >= 7,
    `${path}: complete guide`
  )
  assert.equal(
    document.querySelectorAll('table tbody tr').length,
    4,
    `${path}: shared membership figures`
  )
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
  if (path === 'index.html') {
    assert.ok(
      document.getElementById('dmsg-public-content'),
      'visible overview outside noscript'
    )
    assert.ok(
      !document.getElementById('dmsg-public-content').closest('noscript')
    )
    const data = JSON.parse(
      document.querySelector('script[type="application/ld+json"]').textContent
    )
    assert.equal(data.name, 'dMsg')
    assert.equal(data.url, 'https://dmsg.net/')
    assert.match(
      read(path),
      /_app\/immutable\/entry\/start\./,
      'SPA bootstrap preserved for deep links'
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
  assert.ok(
    content.startsWith('# '),
    `${source.url}: Markdown, not a SPA fallback`
  )
}
assert.equal(manifest.documents.length, 15)
assert.match(manifest.sourceRevision, /^[a-f0-9]{40}$/)
assert.match(read('robots.txt'), /Sitemap: https:\/\/dmsg.net\/sitemap.xml/)
const sitemap = new JSDOM(read('sitemap.xml'), {
  contentType: 'application/xml'
})
for (const location of sitemap.window.document.querySelectorAll('loc')) {
  const path = new URL(location.textContent).pathname
  assert.ok(
    existsSync(join(directory, path === '/' ? 'index.html' : path)),
    path
  )
}
sitemap.window.close()
assert.match(read('llms.txt'), /https:\/\/dmsg.net\/index.zh.md/)
assert.ok(read('llms-full.txt').includes(read('_/docs/agents.md')))

const rules = JSON.parse(read('.ic-assets.json'))
for (const match of [
  'index.html',
  '_/about*.html',
  'index*.md',
  '_/docs/*.md',
  'llms*.txt',
  'robots.txt',
  'sitemap.xml',
  '_/knowledge.json'
]) {
  const rule = rules.find((item) => item.match === match)
  assert.ok(rule, `${match}: asset headers`)
  assert.equal(rule.cache.max_age, 0)
  assert.match(rule.headers['Cache-Control'], /must-revalidate/)
  assert.ok(rule.headers['Content-Type'])
}
console.log(
  'Public content checks passed: readable HTML, discovery, links, provenance and asset headers.'
)
