import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

const pontis = {
  target: process.env.FORMA_PONTIS_URL ?? "http://127.0.0.1:4767",
  changeOrigin: false,
};

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
      // What the browser says to the Pontis sidecar (Pontis plan section 10(b)
      // D9). Same origin as the editor here for the same reason Caddy does it
      // in the deployment: the sidecar refuses a cross-origin Origin, and this
      // is what makes the dev server and production one shape rather than two.
      //
      // The same list as deploy/caddy/Caddyfile, and for the same reason it is
      // a list: the Runtime's routes (/pons/v1/pairing/claim, /pons/v1/device/)
      // are not a browser's to call and are deliberately absent. The "devices"
      // pattern ends in a slash or the end of the path so it cannot match the
      // Runtime's "device/".
      "^/(pons/v1/(pairing/codes|devices(/.*)?|events|jobs(/.*)?|ui/.*)|agent/intent)$": pontis,
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
