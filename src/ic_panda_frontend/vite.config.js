/// <reference types="vitest" />

import { sveltekit } from '@sveltejs/kit/vite'
import { SvelteKitPWA } from '@vite-pwa/sveltekit'
import dotenv from 'dotenv'
import { resolve } from 'node:path'
import { defineConfig } from 'vite'
import environment from 'vite-plugin-environment'

dotenv.config({ path: '../../.env' })

if (process.env.PUBLIC_DFX_NETWORK === 'ic') {
  process.env.NODE_ENV === 'production'
}

/** @type {import('vite').UserConfig} */
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
        target: 'http://127.0.0.1:4943',
        changeOrigin: true
      }
    }
  },
  plugins: [
    environment('all', { prefix: 'CANISTER_' }),
    environment('all', { prefix: 'DFX_' }),
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
      pwaAssets: {
        config: true
      },
      manifest: {
        short_name: 'ICPanda',
        name: 'ICPanda',
        description:
          'A technical panda fully running on the Internet Computer blockchain, building chain-native infrastructures and practical Web3 apps. About us: https://dmsg.net/panda',
        icons: [
          {
            src: '/_assets/favicons/android-chrome-192x192.png',
            sizes: '192x192',
            type: 'image/png'
          },
          {
            src: '/_assets/favicons/android-chrome-512x512.png',
            sizes: '512x512',
            type: 'image/png'
          }
        ],
        start_url: '/',
        scope: '/',
        display: 'standalone',
        theme_color: '#ffffff',
        background_color: '#ffffff'
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
    setupFiles: 'src/setupTests.js'
  },
  resolve: {
    alias: {
      $src: resolve('./src'),
      $declarations: resolve('./src/declarations')
    }
  }
})
