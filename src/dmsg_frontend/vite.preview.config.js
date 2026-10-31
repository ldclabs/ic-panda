import { defineConfig } from 'vite'

// Preview the deployed asset directory, including post-build public content.
// SvelteKit's default preview serves .svelte-kit/output instead of dist.
export default defineConfig({
  appType: 'spa',
  build: { outDir: 'dist' }
})
