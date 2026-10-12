/** Builds the browser tests' application page (`DMSG_CONNECT_APP_OUT`). */
import { resolve } from 'node:path'
import { defineConfig } from 'vite'

export default defineConfig({
  root: resolve(import.meta.dirname, 'app'),
  base: './',
  publicDir: false,
  build: {
    outDir: resolve(process.env.DMSG_CONNECT_APP_OUT ?? resolve(import.meta.dirname, 'dist')),
    emptyOutDir: true,
    target: 'es2022'
  }
})
