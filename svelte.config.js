import { vitePreprocess } from "@sveltejs/vite-plugin-svelte";

export default {
  // Consult https://svelte.dev/docs#compile-time-svelte-preprocess
  preprocess: vitePreprocess(),
  compilerOptions: {
    runes: true,
  },
};