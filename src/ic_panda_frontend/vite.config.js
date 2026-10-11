/// <reference types="vitest" />

import { sveltekit } from '@sveltejs/kit/vite'
import tailwindcss from '@tailwindcss/vite'
import { resolve } from 'node:path'
import { defineConfig } from 'vite'

/** @type {import('vite').UserConfig} */
export default defineConfig({
  define: {
    'process.env.NODE_ENV':
      process.env.NODE_ENV === 'production' ? '"production"' : '"development"'
  },
  build: {
    emptyOutDir: true
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
    sveltekit()
  ],
  test: {
    environment: 'jsdom',
    setupFiles: 'src/setupTests.ts'
  },
  resolve: {
    alias: {
      $src: resolve('./src'),
      $declarations: resolve('../declarations')
    }
  }
})
