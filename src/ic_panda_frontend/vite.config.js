/// <reference types="vitest" />

import { sveltekit } from "@sveltejs/kit/vite"
import dotenv from "dotenv"
import { resolve } from "node:path"
import { defineConfig } from "vite"
import environment from "vite-plugin-environment"

dotenv.config({ path: "../../.env" });

export default defineConfig({
  build: {
    emptyOutDir: true,
  },
  optimizeDeps: {
    esbuildOptions: {
      define: {
        global: "globalThis",
      },
    },
  },
  server: {
    proxy: {
      "/api": {
        target: "http://127.0.0.1:4943",
        changeOrigin: true,
      },
    },
  },
  plugins: [
    sveltekit(),
    environment("all", { prefix: "CANISTER_" }),
    environment("all", { prefix: "DFX_" }),
  ],
  test: {
    environment: "jsdom",
    setupFiles: "src/setupTests.js",
  },
  resolve: {
    alias: {
      $declarations: resolve("../declarations"),
    },
  },
})
