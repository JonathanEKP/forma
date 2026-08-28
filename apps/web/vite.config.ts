import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  server: {
    port: 5173,
    // In development the SPA runs on Vite and talks to forma-server; in
    // production forma-server serves the built app from the same origin.
    proxy: {
      "/api": {
        target: process.env.FORMA_SERVER_URL ?? "http://127.0.0.1:4765",
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
