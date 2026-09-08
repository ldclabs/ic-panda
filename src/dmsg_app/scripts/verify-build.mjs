import { readFileSync, existsSync, readdirSync } from 'node:fs'
import { resolve } from 'node:path'
const dir = resolve(import.meta.dirname, '../dist')
const manifest = JSON.parse(readFileSync(`${dir}/manifest.json`, 'utf8'))
for (const page of [
  'index.html',
  'popup.html',
  'sidepanel.html',
  'approve.html',
  'recovery.html',
  manifest.background.service_worker,
  ...Object.values(manifest.icons)
]) {
  if (!existsSync(`${dir}/${page}`)) throw new Error(`Missing extension entry: ${page}`)
}
if (
  manifest.permissions.some((p) =>
    ['debugger', 'cookies', 'history', 'scripting', 'nativeMessaging'].includes(p)
  ) ||
  manifest.host_permissions.includes('<all_urls>')
)
  throw new Error('Unexpected broad permission')
for (const name of readdirSync(`${dir}/assets`).filter((f) => /\.js$/.test(f))) {
  const source = readFileSync(`${dir}/assets/${name}`, 'utf8')
  if (/\beval\s*\(/.test(source) || /\bnew Function\s*\(/.test(source))
    throw new Error(`CSP-incompatible code: ${name}`)
}
console.log('MV3 entries, local assets and permission/CSP checks passed.')
