/// <reference types="vitest" />

import { sveltekit } from '@sveltejs/kit/vite'
import tailwindcss from '@tailwindcss/vite'
import { SvelteKitPWA } from '@vite-pwa/sveltekit'
import { resolve } from 'node:path'
import { defineConfig } from 'vite'

export default defineConfig({
  define: {
    'process.env.NODE_ENV':
      process.env.NODE_ENV === 'production' ? '"production"' : '"development"'
  },
  build: {
    emptyOutDir: true
  },
  optimizeDeps: {
    esbuildOptions: {
      define: {
        global: 'globalThis'
      }
    }
  },
  server: {
    proxy: {
      '/api': {
        target: 'http://127.0.0.1:8000',
        changeOrigin: true
      }
    }
  },
  plugins: [
    tailwindcss(),
    sveltekit(),
    SvelteKitPWA({
      srcDir: 'src',
      mode: 'production',
      strategies: 'injectManifest',
      registerType: 'autoUpdate',
      filename: 'service-worker.ts',
      scope: '/',
      base: '/',
      selfDestroying: process.env.SELF_DESTROYING_SW === 'true',
      manifest: {
        short_name: 'dMsg',
        name: 'dMsg.net',
        description:
          'dMsg: private collaboration for people and agents. Private space, collaboration and an open agent network.',
        icons: [
          {
            src: '/_assets/dmsg/icon-192.png',
            sizes: '192x192',
            type: 'image/png'
          },
          {
            src: '/_assets/dmsg/icon-512.png',
            sizes: '512x512',
            type: 'image/png'
          },
          {
            src: '/_assets/dmsg/icon-maskable-512.png',
            sizes: '512x512',
            type: 'image/png',
            purpose: 'maskable'
          }
        ],
        start_url: '/',
        scope: '/',
        display: 'standalone',
        theme_color: '#F7F8F4',
        background_color: '#F7F8F4'
      },
      injectManifest: {
        globPatterns: [
          'client/**/*.{js,json,css,ico,png,jpg,svg,webp,woff,woff2,xml}',
          'prerendered/**/*.html'
        ],
        injectionPoint: 'self.__WB_MANIFEST'
      },
      devOptions: {
        enabled: false,
        suppressWarnings: process.env.SUPPRESS_WARNING === 'true',
        type: 'module',
        navigateFallback: '/'
      },
      // if you have shared info in svelte config file put in a separate module and use it also here
      kit: {
        includeVersionFile: true
      },
      injectRegister: 'auto'
    })
  ],
  test: {
    environment: 'jsdom',
    server: { deps: { inline: ['@ldclabs/cose-ts'] } },
    setupFiles: 'src/setupTests.ts'
  },
  resolve: {
    // Component tests exercise client runes and lifecycle hooks in jsdom.
    ...(process.env.VITEST ? { conditions: ['browser'] } : {}),
    alias: {
      $src: resolve('./src'),
      $declarations: resolve('../declarations')
    }
  }
})
