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
    // The editor and its Pontis routes are same-origin; the browser never needs
    // the dev server to grant cross-origin access to loopback sites.
    cors: false,
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
      //
      // Vite tests this against the URL *with its query string* (Caddy's path
      // matcher does not see one), so the optional "?..." at the end is what
      // lets the polled event page, "/pons/v1/events?scope=...&after=0", reach
      // the sidecar. Without it Vite answers that request with index.html and
      // the chat reads "can't reach the agent service" forever.
      // Reject path traversal before the wildcard route allow-list is applied.
      // Encoded dots can appear in mixed or upper-case form, so match both cases.
      "^/(?![^?]*(?:\\.\\.|%2[eE]))(pons/v1/(pairing/codes|devices(/.*)?|events|jobs(/.*)?|ui/.*)|agent/(intent|context))(\\?.*)?$": pontis,
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
