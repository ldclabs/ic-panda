import { defineConfig } from 'vitest/config'
// scripts/*.test.mjs are node:test suites (test:scripts).
export default defineConfig({
  test: { include: ['tests/**/*.test.ts'], server: { deps: { inline: ['@ldclabs/cose-ts'] } } }
})
