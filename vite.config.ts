import { defineConfig } from "vite";
import solid from "@solidjs/vite-plugin";
import tailwindcss from "@tailwindcss/vite";
import { resolve } from "path";

const host = process.env.TAURI_DEV_HOST;

// Phase 3 of docs/PLAN_SOLIDJS_2.md: the main window is now Solid, so
// `@vitejs/plugin-react` is gone and the Solid plugin covers all of `src/`.
// `tsconfig.json` still uses `jsx: "preserve"` so esbuild passes untransformed
// JSX to the Solid plugin — both entry points go through the same compiler.
// The overlay's per-file `/** @jsxImportSource @solidjs/web */` pragma is now
// redundant (the default matches), but harmless; it stays in for clarity until
// Phase 4 cleans up.

// https://vitejs.dev/config/
export default defineConfig(async () => ({
  plugins: [...solid(), tailwindcss()],

  // Path aliases
  resolve: {
    alias: {
      "@": resolve(import.meta.dirname, "./src"),
      "@/bindings": resolve(import.meta.dirname, "./src/bindings.ts"),
    },
  },

  // Multiple entry points for main app and overlay
  build: {
    rollupOptions: {
      input: {
        main: resolve(import.meta.dirname, "index.html"),
        overlay: resolve(import.meta.dirname, "src/overlay/index.html"),
      },
    },
    // Assets load from the local asset protocol, not the network, so the
    // settings window ships as one ~700 kB chunk (no route splitting). The
    // limit stays to catch accidental bloat: the eagerly bundled locales
    // (2 MB, now one lazy chunk per language) tripped the 500 kB default.
    chunkSizeWarningLimit: 1000,
  },

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 1420,
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
      // 3. tell vite to ignore watching `src-tauri`
      ignored: ["**/src-tauri/**"],
    },
  },
}));
