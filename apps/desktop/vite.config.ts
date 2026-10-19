import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";
import path from "path";
import { readFileSync } from "node:fs";

const appConfig = JSON.parse(readFileSync(new URL("../../src-tauri/tauri.conf.json", import.meta.url), "utf8"));

const host = process.env.TAURI_DEV_HOST;
const isTauri = !!host || !!process.env.TAURI_ENV_ARCH;

export default defineConfig({
  root: import.meta.dirname,
  base: "./",
  plugins: [vue()],
  define: { __APP_NAME__: JSON.stringify(appConfig.productName) },
  resolve: {
    alias: {
      "@": path.resolve(import.meta.dirname, "./src"),
    },
  },
  clearScreen: false,
  build: {
    outDir: "../../dist",
    emptyOutDir: true,
    chunkSizeWarningLimit: 800,
  },
  server: {
    port: isTauri ? 1420 : undefined,
    strictPort: isTauri,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },
});
