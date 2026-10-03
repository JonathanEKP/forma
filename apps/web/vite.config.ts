import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

export default defineConfig({
  plugins: [react(), tailwindcss()],
  server: {
    port: 5173,
    // In development the SPA runs on Vite and talks to forma-server; in
    // production forma-server serves the built app from the same origin.
    proxy: {
      "/api": {
        target: process.env.FORMA_SERVER_URL ?? "http://127.0.0.1:4765",
        changeOrigin: false,
      },
      // The Pontis UI action channel (Pontis plan section 10(b) D9). Same
      // origin as the editor here for the same reason Caddy does it in the
      // deployment: the sidecar refuses a cross-origin Origin, and this is
      // what makes the dev server and production one shape rather than two.
      "/pons/v1/ui": {
        target: process.env.FORMA_PONTIS_URL ?? "http://127.0.0.1:4767",
        changeOrigin: false,
      },
    },
  },
  build: {
    outDir: "dist",
    emptyOutDir: true,
  },
  test: {
    environment: "jsdom",
    globals: true,
  },
});
