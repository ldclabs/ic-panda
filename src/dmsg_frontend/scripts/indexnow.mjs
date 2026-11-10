import { readFileSync, readdirSync } from 'node:fs'
import { resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { JSDOM } from 'jsdom'

const origin = 'https://dmsg.net'
const directory = fileURLToPath(new URL('../dist/', import.meta.url))

export function validateKey(key) {
  if (!/^[a-zA-Z0-9-]{8,128}$/.test(key ?? ''))
    throw new Error(
      'IndexNow key must contain 8–128 letters, digits or hyphens'
    )
  return key
}

export function submission(sitemap, key, requested = []) {
  validateKey(key)
  const dom = new JSDOM(sitemap, { contentType: 'application/xml' })
  const urls = [...dom.window.document.querySelectorAll('url > loc')].map(
    (node) => node.textContent.trim()
  )
  dom.window.close()
  const urlList = [...new Set(requested.length ? requested : urls)]
  if (!urlList.length || urlList.length > 10000)
    throw new Error('Expected 1–10000 public URLs')
  for (const value of urlList) {
    const url = new URL(value)
    if (
      url.origin !== origin ||
      url.search ||
      url.hash ||
      url.username ||
      url.password ||
      !urls.includes(value)
    )
      throw new Error(`URL must be a canonical public sitemap entry: ${value}`)
  }
  return { host: 'dmsg.net', key, keyLocation: `${origin}/${key}.txt`, urlList }
}

export async function submit(payload, read, fetcher = fetch) {
  // The SPA fallback can return 200 for a nonexistent key/page. Compare content,
  // not just status, and refuse to announce a build that is not deployed yet.
  for (const url of [payload.keyLocation, ...payload.urlList]) {
    const path = new URL(url).pathname
    const expected =
      url === payload.keyLocation
        ? payload.key
        : read(path === '/' ? 'index.html' : path.slice(1))
    const response = await fetcher(url, {
      redirect: 'error',
      signal: AbortSignal.timeout(15000)
    })
    if (!response.ok || (await response.text()).trim() !== expected.trim())
      throw new Error(`Deployed content does not match this build: ${url}`)
  }
  const response = await fetcher('https://api.indexnow.org/indexnow', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json; charset=utf-8' },
    body: JSON.stringify(payload),
    redirect: 'error',
    signal: AbortSignal.timeout(15000)
  })
  if (response.status !== 200 && response.status !== 202)
    throw new Error(`IndexNow rejected submission: HTTP ${response.status}`)
  return response.status === 202
    ? 'URLs received; key validation pending (202). Indexing is not guaranteed.'
    : 'URLs received (200). Indexing is not guaranteed.'
}

if (
  process.argv[1] &&
  resolve(process.argv[1]) === fileURLToPath(import.meta.url)
) {
  const args = process.argv.slice(2)
  if (args.some((arg) => arg.startsWith('--') && arg !== '--submit'))
    throw new Error(
      'Usage: indexnow.mjs [--submit] [canonical sitemap URLs...]'
    )
  const read = (path) => readFileSync(resolve(directory, path), 'utf8')
  const keys = readdirSync(directory)
    .filter((name) => /^[a-zA-Z0-9-]{8,128}\.txt$/.test(name))
    .filter((name) => read(name).trim() === name.slice(0, -4))
  if (keys.length !== 1)
    throw new Error(
      'Expected exactly one root IndexNow key file in dist; add <key>.txt to static and rebuild'
    )
  const payload = submission(
    read('sitemap.xml'),
    read(keys[0]).trim(),
    args.filter((arg) => arg !== '--submit')
  )
  if (args.includes('--submit')) console.log(await submit(payload, read))
  else
    console.log(
      JSON.stringify(
        { mode: 'preview; no network requests', ...payload },
        null,
        2
      )
    )
}
