// @ts-check
import { defineConfig } from "astro/config";
import tailwindcss from "@tailwindcss/vite";

// https://astro.build/config
export default defineConfig({
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
