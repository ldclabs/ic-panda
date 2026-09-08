import { svelte } from '@sveltejs/vite-plugin-svelte'
import { defineConfig } from 'vite'
import { resolve } from 'node:path'
import config from './dmsg.config.json' with { type: 'json' }

if (config.release !== 'R0' || !['local', 'staging'].includes(config.environment)) {
  throw new Error(
    'This client is an R0 local/staging build; production interoperability is not enabled.'
  )
}

// Permissions are fixed at build time. A relay cannot widen them remotely.
const origins = config.externalOrigins.map((origin) => {
  const url = new URL(origin)
  if (url.protocol !== 'https:' || url.origin !== origin || url.hostname.includes('*')) {
    throw new Error('externalOrigins must contain exact HTTPS origins')
  }
  return `${origin}/*`
})
const hosts = new Set([
  'https://icp-api.io/*',
  'https://id.ai/*',
  'https://panda.fans/*',
  'https://dmsg.net/*'
])
if (config.relayOrigin) {
  const url = new URL(config.relayOrigin)
  if (
    url.origin !== config.relayOrigin ||
    (url.protocol !== 'https:' &&
      !(config.environment === 'local' && ['localhost', '127.0.0.1'].includes(url.hostname)))
  ) {
    throw new Error('Invalid fixed relay origin')
  }
  hosts.add(`${url.origin}/*`)
}
if (config.icHost !== 'https://icp-api.io') {
  const url = new URL(config.icHost)
  if (config.environment !== 'local' || !['localhost', '127.0.0.1'].includes(url.hostname))
    throw new Error('Untrusted IC gateway')
  hosts.add(`${url.origin}/*`)
}

export default defineConfig({
  base: './',
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
              version: '0.1.0',
              minimum_chrome_version: '120',
              permissions: ['storage', 'alarms', 'sidePanel'],
              optional_permissions: ['clipboardWrite', 'unlimitedStorage'],
              host_permissions: [...hosts],
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
              ...(origins.length
                ? { externally_connectable: { matches: origins, ids: [] } }
                : {}),
              content_security_policy: {
                extension_pages: `default-src 'self'; script-src 'self'; worker-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; font-src 'self'; connect-src 'self' ${[...hosts].map((host) => host.replace(/\/\*$/, '')).join(' ')}; object-src 'none'; base-uri 'none'; frame-src 'none'; frame-ancestors 'none'`
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
        ['index', 'popup', 'sidepanel', 'approve', 'recovery']
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
