// Serves a built directory the way the asset canister does: the headers of
// its `.ic-assets.json5` (written as JSON), `/a/` and `/a` aliased to
// `/a/index.html`, dotfiles only where a rule un-ignores them. For local
// runs and the browser tests; production is the canister.
//
//   node scripts/serve.mjs <dir> [port] [host]
//
// Prints `{"origin": "http://<host>:<port>"}` once it listens.
import { createServer } from 'node:http'
import { readFile, stat } from 'node:fs/promises'
import { extname, join, normalize, resolve } from 'node:path'

const [dir = 'dist', port = '0', host = 'localhost'] = process.argv.slice(2)
const root = resolve(dir)
const rules = await readFile(join(root, '.ic-assets.json5'), 'utf8')
  .then(JSON.parse)
  .catch(() => [])

const TYPES = {
  '.html': 'text/html; charset=utf-8',
  '.js': 'text/javascript; charset=utf-8',
  '.css': 'text/css; charset=utf-8',
  '.json': 'application/json',
  '.svg': 'image/svg+xml',
  '.png': 'image/png',
  '.ico': 'image/x-icon'
}

/** `**` any path, `*` one segment; a pattern without a slash also matches that entry's subtree. */
function matches(pattern, path) {
  const source = pattern
    .split('**/')
    .map((part) => part.replace(/[.+?^${}()|[\]\\]/g, '\\$&').replaceAll('*', '[^/]*'))
    .join('(?:.*/)?')
  return new RegExp(`^${source}(?:/.*)?$`).test(path)
}

function headersFor(path) {
  const headers = {}
  let visible = !path.split('/').some((s) => s.startsWith('.'))
  for (const rule of rules) {
    if (!matches(rule.match, path)) continue
    if (rule.ignore === false) visible = true
    Object.assign(headers, rule.headers ?? {})
  }
  return visible ? headers : null
}

async function file(path) {
  const full = normalize(join(root, path))
  if (!full.startsWith(root)) return null
  try {
    return (await stat(full)).isFile() ? full : null
  } catch {
    return null
  }
}

const server = createServer(async (req, res) => {
  const url = new URL(req.url, 'http://local')
  let path = decodeURIComponent(url.pathname).replace(/^\/+/, '')
  const candidates = path === '' || path.endsWith('/') ? [`${path}index.html`] : [path, `${path}/index.html`, `${path}.html`]
  for (const candidate of candidates) {
    const full = await file(candidate)
    const headers = full && headersFor(candidate)
    if (!full || !headers) continue
    res.writeHead(200, { 'content-type': TYPES[extname(full)] ?? 'text/plain; charset=utf-8', ...headers })
    res.end(req.method === 'HEAD' ? undefined : await readFile(full))
    return
  }
  res.writeHead(404, { 'content-type': 'text/plain' })
  res.end('not found')
})

server.listen(Number(port), host, () => {
  console.log(JSON.stringify({ origin: `http://${host}:${server.address().port}` }))
})
for (const signal of ['SIGTERM', 'SIGINT']) process.on(signal, () => server.close(() => process.exit(0)))
