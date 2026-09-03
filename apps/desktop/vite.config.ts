import { fileURLToPath } from "node:url";
import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

const editorSrc = fileURLToPath(new URL("../../packages/editor/src", import.meta.url));

export default defineConfig({
  plugins: [svelte()],
  // Tauri serves the built files from disk, so asset URLs must be relative.
  base: "./",
  resolve: {
    alias: [
      { find: /^@notedock\/editor$/, replacement: `${editorSrc}/index.ts` },
      { find: /^@notedock\/editor\/(.*)$/, replacement: `${editorSrc}/$1` },
    ],
  },
  server: {
    // Distinct from the web client's 5173 so both can run at once.
    port: 5174,
    strictPort: true,
  },
  build: {
    outDir: "dist",
    emptyOutDir: true,
    // Tauri ships a current WebView2; no need to down-level.
    target: "chrome110",
  },
});
