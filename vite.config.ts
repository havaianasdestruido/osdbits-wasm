import { defineConfig } from "vite";

export default defineConfig({
  root: ".",
  publicDir: "web/public",
  base: "./",
  build: {
    outDir: "dist",
    emptyOutDir: true,
    target: "es2022",
    sourcemap: true,
  },
  server: {
    host: "0.0.0.0",
    port: 5173,
    strictPort: true,
    allowedHosts: true,
  },
  preview: {
    host: "0.0.0.0",
    port: 4173,
    strictPort: true,
    allowedHosts: true,
  },
  assetsInclude: ["**/*.wasm", "**/*.wgsl"],
});
