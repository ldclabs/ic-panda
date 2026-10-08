import { svelte } from '@sveltejs/vite-plugin-svelte'
import { defineConfig } from 'vite'
import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { Principal } from '@icp-sdk/core/principal'
import pkg from './package.json' with { type: 'json' }

// DMSG_CONFIG selects an environment's public build configuration; the app
// imports it in place of dmsg.config.json.
const configFile = resolve(import.meta.dirname, process.env.DMSG_CONFIG ?? 'dmsg.config.json')
const config: typeof import('./dmsg.config.json') = JSON.parse(
  readFileSync(configFile, 'utf8')
)

if (!['local', 'staging', 'production'].includes(config.environment))
  throw new Error('environment must be local, staging or production')
const local = config.environment === 'local'
/** An exact origin: HTTPS, or HTTP loopback in a local build. */
function exactOrigin(value: string, field: string, localhostSubdomains = false) {
  const url = new URL(value)
  const loopback =
    ['localhost', '127.0.0.1'].includes(url.hostname) ||
    (localhostSubdomains && url.hostname.endsWith('.localhost'))
  if (
    url.origin !== value ||
    url.hostname.includes('*') ||
    (url.protocol !== 'https:' && !(local && url.protocol === 'http:' && loopback))
  )
    throw new Error(`${field} must be an exact HTTPS origin`)
  return url
}
if (
  config.icHost !== 'https://icp-api.io' &&
  !(
    local && ['localhost', '127.0.0.1'].includes(exactOrigin(config.icHost, 'icHost').hostname)
  )
)
  throw new Error('Untrusted IC gateway')
const canisterIds = [
  config.canisters.handle,
  config.canisters.cose,
  config.canisters.payment,
  config.canisters.commerce,
  config.canisters.membership,
  ...config.canisters.userHomes,
  ...config.legacy.channels,
  config.legacy.identity,
  ...config.legacy.buckets
]
for (const id of canisterIds.filter(Boolean)) Principal.fromText(id)
if (
  config.coseRootPublicKey &&
  !(
    /^[A-Za-z0-9_-]+$/.test(config.coseRootPublicKey) &&
    Buffer.from(config.coseRootPublicKey, 'base64url').length === 96
  )
)
  throw new Error('coseRootPublicKey must be the 96-byte key in unpadded base64url')
if (config.environment === 'production') {
  const missing = Object.entries({
    'canisters.handle': config.canisters.handle,
    'canisters.userHomes': config.canisters.userHomes.length,
    'canisters.cose': config.canisters.cose,
    coseRootPublicKey: config.coseRootPublicKey,
    'canisters.payment': config.canisters.payment,
    'canisters.commerce': config.canisters.commerce,
    'canisters.membership': config.canisters.membership,
    relayOrigin: config.relayOrigin
  })
    .filter(([, value]) => !value)
    .map(([field]) => field)
  if (missing.length) throw new Error(`A production build needs ${missing.join(', ')}`)
}

// Permissions are fixed at build time. A relay cannot widen them remotely.
const externalOrigins = config.externalOrigins.map(
  (origin) => `${exactOrigin(origin, 'externalOrigins').origin}/*`
)
const hosts = new Set([new URL(config.icHost).origin])
const connect = new Set(hosts)
if (config.relayOrigin) {
  const url = exactOrigin(config.relayOrigin, 'relayOrigin')
  hosts.add(url.origin)
  // Channel activity hints use a WebSocket on the relay; https: does not cover wss:.
  connect.add(url.origin).add(`${url.protocol === 'https:' ? 'wss' : 'ws'}://${url.host}`)
}
// Agent Protocols: principal documents (ICP directory) and the delegation service.
for (const value of [config.principalOrigin, config.agentOrigin].filter(Boolean)) {
  const url = exactOrigin(value, 'principalOrigin/agentOrigin', true)
  hosts.add(url.origin)
  connect.add(url.origin)
}

export default defineConfig({
  base: './',
  resolve: { alias: [{ find: /^.*\/dmsg\.config\.json$/, replacement: configFile }] },
  plugins: [
    svelte(),
    {
      name: 'dmsg-manifest',
      generateBundle() {
        this.emitFile({
          type: 'asset',
          fileName: 'manifest.json',
          source: JSON.stringify(
            {
              manifest_version: 3,
              name: 'dMsg — Your space. Your say.',
              description:
                'A private workspace for encrypted notes, files and explicit signature requests.',
              version: pkg.version,
              minimum_chrome_version: '120',
              permissions: ['storage', 'alarms', 'sidePanel'],
              optional_permissions: ['clipboardWrite', 'unlimitedStorage'],
              host_permissions: [...hosts].map((origin) => `${origin}/*`),
              background: { service_worker: 'service_worker.js', type: 'module' },
              action: { default_popup: 'popup.html', default_title: 'dMsg' },
              side_panel: { default_path: 'sidepanel.html' },
              options_ui: { page: 'index.html#settings', open_in_tab: true },
              icons: {
                16: 'icons/16.png',
                32: 'icons/32.png',
                48: 'icons/48.png',
                128: 'icons/128.png'
              },
              ...(externalOrigins.length
                ? { externally_connectable: { matches: externalOrigins, ids: [] } }
                : {}),
              content_security_policy: {
                extension_pages: `default-src 'self'; script-src 'self'; worker-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; font-src 'self'; connect-src 'self' ${[...connect].join(' ')}; object-src 'none'; base-uri 'none'; frame-src 'none'; frame-ancestors 'none'`
              }
            },
            null,
            2
          )
        })
      }
    }
  ],
  build: {
    target: 'chrome120',
    rollupOptions: {
      input: Object.fromEntries(
        ['index', 'popup', 'sidepanel', 'approve']
          .map((p) => [p, resolve(import.meta.dirname, `${p}.html`)])
          .concat([['service_worker', resolve(import.meta.dirname, 'src/service-worker.ts')]])
      ),
      output: {
        entryFileNames: (chunk) =>
          chunk.name === 'service_worker' ? 'service_worker.js' : 'assets/[name]-[hash].js'
      }
    }
  },
  worker: { format: 'es' }
})
