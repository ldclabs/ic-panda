import { svelte } from '@sveltejs/vite-plugin-svelte'
import { defineConfig } from 'vitest/config'
export default defineConfig({
  plugins: [svelte()],
  test: {
    server: { deps: { inline: ['@ldclabs/cose-ts'] } },
    environment: 'node',
    include: ['tests/**/*.test.ts'],
    setupFiles: ['tests/setup.ts'],
    testTimeout: 60000
  }
})
