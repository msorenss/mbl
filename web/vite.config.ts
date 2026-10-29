import { defineConfig } from "vite";
import solid from "vite-plugin-solid";

export default defineConfig({
  plugins: [solid()],
  server: {
    port: 3000,
    proxy: {
      "/api": {
        target: "http://localhost:8420",
        changeOrigin: false,
        ws: true,
      },
      "/mcp": {
        target: "http://localhost:8420",
        changeOrigin: true,
      },
      "/stream": {
        target: "http://localhost:8420",
        changeOrigin: true,
      },
      "/images": {
        target: "http://localhost:8420",
        changeOrigin: true,
      },
    },
  },
});
