import { fileURLToPath } from "node:url";
import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

const editorSrc = fileURLToPath(new URL("../../packages/editor/src", import.meta.url));

export default defineConfig({
  plugins: [svelte()],
  resolve: {
    // Regex entries rather than a bare string so `@notedock/editor` resolves to
    // the entry module explicitly and `@notedock/editor/styles/*` still works.
    // The shared package is consumed as source, running through this app's own
    // Svelte/TS pipeline instead of needing a build step of its own.
    alias: [
      { find: /^@notedock\/editor$/, replacement: `${editorSrc}/index.ts` },
      { find: /^@notedock\/editor\/(.*)$/, replacement: `${editorSrc}/$1` },
    ],
  },
  server: {
    port: 5173,
    strictPort: true,
    proxy: {
      // Dev-only: makes the API same-origin so the browser is in the same
      // situation as production, where the server serves this app itself.
      "/api": {
        target: process.env.NOTEDOCK_API ?? "http://127.0.0.1:8080",
        changeOrigin: true,
      },
    },
  },
  build: {
    outDir: "dist",
    emptyOutDir: true,
  },
});
