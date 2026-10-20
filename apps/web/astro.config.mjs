// @ts-check
import { defineConfig } from "astro/config";
import tailwindcss from "@tailwindcss/vite";

import node from "@astrojs/node";
import sitemap from "@astrojs/sitemap";

// https://astro.build/config
export default defineConfig({
  site: "https://explorer.litepod.sh",
  output: "server",
  integrations: [
    sitemap({
      changefreq: "monthly",
      priority: 0.5,
    }),
  ],
  adapter: node({
    mode: "standalone",
  }),
  vite: {
    plugins: [tailwindcss()],
    // The web app reads the desktop release version from ../desktop.
    server: {
      fs: {
        allow: ["..", "../.."],
      },
    },
  },
});
