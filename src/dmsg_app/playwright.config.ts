import { defineConfig } from '@playwright/test'
export default defineConfig({
  testDir: './e2e',
  timeout: 180000,
  workers: 1,
  reporter: 'list',
  outputDir: './test-results',
  use: { trace: 'retain-on-failure', actionTimeout: 15000 }
})
