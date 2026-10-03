import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

export default defineConfig({
  plugins: [react()],
  css: {
    postcss: "./postcss.config.js",
  },
  build: {
    target: "esnext",
  },
  server: {
    port: 5173,
  },
  optimizeDeps: {
    exclude: ["penfill-wasm"],
  },
});
