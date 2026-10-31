import { defineConfig } from "vite";

// The dev server binds 0.0.0.0 and accepts the sandbox preview host, so the
// live preview works without extra flags. `public/osdbits_core.wasm` is the
// build artefact produced by scripts/build-wasm.sh.
export default defineConfig({
  base: "/",
  server: {
    host: "0.0.0.0",
    port: 5173,
    strictPort: true,
    allowedHosts: true,
  },
  preview: { host: "0.0.0.0", port: 4173, strictPort: true, allowedHosts: true },
  build: { target: "es2022", sourcemap: true },
});
