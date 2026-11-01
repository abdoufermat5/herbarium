import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import { webPreview } from "./scripts/web-preview.mjs";

const host = process.env.TAURI_DEV_HOST;

// https://vitejs.dev/config/
export default defineConfig(({ mode }) => {
  // `pnpm preview:web`: the frontend in a browser on a seeded demo vault (scripts/web-preview.mjs).
  const preview = mode === "web-preview";
  return {
    plugins: [svelte(), ...(preview ? [webPreview()] : [])],
    // Prevent Vite from obscuring Rust errors
    clearScreen: false,
    server: {
      port: preview ? Number(process.env.PORT ?? 1530) : 1520,
      strictPort: true,
      host: host || false,
      hmr: host
        ? {
            protocol: "ws",
            host,
            port: 1421,
          }
        : undefined,
      watch: {
        // Tell Vite to ignore watching `desktop`
        ignored: ["**/desktop/**"],
      },
    },
  };
});
