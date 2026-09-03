import { vitePreprocess } from "@sveltejs/vite-plugin-svelte";

/**
 * Present so `svelte-check` has a config to find when it walks into this
 * package from a consuming app. Without it, it hunts for a `vite.config` here
 * and fails — this package intentionally has no build of its own.
 */
export default {
  preprocess: vitePreprocess(),
};
