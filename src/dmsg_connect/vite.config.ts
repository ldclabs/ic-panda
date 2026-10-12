/**
 * Builds connect.dmsg.net: `/v1/` (the bridge window) and `/v1/pair` (the
 * pairing page). The environment comes from a JSON file
 * (`DMSG_CONNECT_CONFIG`, default `connect.config.json`) and is inlined at
 * build time; a Staging or Production build refuses missing or non-HTTPS
 * values. The response headers of connect-v1 §2.2 go to
 * `dist/.ic-assets.json5`, which the asset canister applies.
 */
import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { defineConfig, type Plugin } from 'vite'

interface ConnectConfig {
  environment: 'Local' | 'Staging' | 'Production'
  icHost: string
  apiOrigin: string
  canisters: { handle: string; commerce: string }
  /** DER of the IC root key, hex; Local only (mainnet uses the IC root key). */
  rootKey?: string
  /** Custom domains for `.well-known/ic-domains`. */
  domains?: string[]
}

function load(): ConnectConfig {
  const file = resolve(process.env.DMSG_CONNECT_CONFIG ?? 'connect.config.json')
  let config: ConnectConfig
  try {
    config = JSON.parse(readFileSync(file, 'utf8'))
  } catch (error) {
    throw new Error(`dmsg_connect needs its environment config (${file}): ${error}`)
  }
  const fail = (what: string) => {
    throw new Error(`${file}: ${what}`)
  }
  if (!['Local', 'Staging', 'Production'].includes(config.environment)) fail('environment must be Local, Staging or Production')
  const local = config.environment === 'Local'
  for (const [name, value] of [
    ['icHost', config.icHost],
    ['apiOrigin', config.apiOrigin]
  ] as const) {
    if (!value || !URL.canParse(value)) fail(`${name} is required`)
    const url = new URL(value)
    if (!local && url.protocol !== 'https:') fail(`${name} must be HTTPS`)
    if (url.origin !== value.replace(/\/$/, '')) fail(`${name} must be an origin`)
  }
  for (const name of ['handle', 'commerce'] as const)
    if (!/^[a-z2-7]{5}(?:-[a-z2-7]{5})*(?:-[a-z2-7]{1,5})?$/.test(config.canisters?.[name] ?? '')) fail(`canisters.${name} is required`)
  if (local !== !!config.rootKey) fail('rootKey is required for Local and refused elsewhere')
  if (config.rootKey && !/^(?:[0-9a-f]{2})+$/.test(config.rootKey)) fail('rootKey must be hex')
  return config
}

/** connect-v1 §2.2: no COOP same-origin (it would cut `window.opener`), no COEP, no framing. */
function assets(config: ConnectConfig): Plugin {
  const connect = [config.icHost, config.apiOrigin].map((u) => new URL(u).origin).join(' ')
  const csp = [
    "default-src 'none'",
    "script-src 'self'",
    "style-src 'self'",
    "img-src 'self'",
    `connect-src ${connect}`,
    "frame-ancestors 'none'",
    "base-uri 'none'",
    "form-action 'none'"
  ].join('; ')
  const rules = [
    {
      match: '**/*',
      security_policy: 'disabled',
      allow_raw_access: false,
      headers: {
        'Content-Security-Policy': csp,
        'Cross-Origin-Opener-Policy': 'unsafe-none',
        'X-Frame-Options': 'DENY',
        'Referrer-Policy': 'no-referrer',
        'X-Content-Type-Options': 'nosniff',
        'Permissions-Policy': 'camera=(), microphone=(), geolocation=(), payment=()',
        'Cache-Control': 'no-cache'
      }
    },
    { match: 'assets/**/*', headers: { 'Cache-Control': 'public, max-age=31536000, immutable' } },
    { match: '.well-known', ignore: false }
  ]
  return {
    name: 'dmsg-connect-assets',
    generateBundle() {
      this.emitFile({ type: 'asset', fileName: '.ic-assets.json5', source: JSON.stringify(rules, null, 2) + '\n' })
      this.emitFile({ type: 'asset', fileName: '.well-known/ic-domains', source: (config.domains ?? []).join('\n') + '\n' })
    }
  }
}

const config = load()

export default defineConfig({
  root: resolve('web'),
  base: '/',
  publicDir: false,
  define: {
    __DMSG_CONNECT__: JSON.stringify({
      environment: config.environment,
      icHost: new URL(config.icHost).origin,
      apiOrigin: new URL(config.apiOrigin).origin,
      handle: config.canisters.handle,
      commerce: config.canisters.commerce,
      rootKey: config.rootKey ?? null
    })
  },
  plugins: [assets(config)],
  build: {
    outDir: resolve(process.env.DMSG_CONNECT_OUT ?? 'dist'),
    emptyOutDir: true,
    target: 'es2022',
    modulePreload: { polyfill: false },
    rollupOptions: {
      input: {
        bridge: resolve('web/v1/index.html'),
        pair: resolve('web/v1/pair/index.html')
      }
    }
  }
})
