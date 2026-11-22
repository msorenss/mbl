import { execSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { defineConfig } from "vite";
import solid from "vite-plugin-solid";

let commit = "release";
try {
  commit = execSync("git rev-parse --short HEAD", { stdio: ["ignore", "pipe", "ignore"] })
    .toString()
    .trim();
} catch {}

let version = "1.2.1";
try {
  const cargoToml = fs.readFileSync(path.resolve(__dirname, "../Cargo.toml"), "utf-8");
  const match = cargoToml.match(/version\s*=\s*"([^"]+)"/);
  if (match) version = match[1];
} catch {}

const buildDate = `${new Date().toISOString().slice(0, 19).replace("T", " ")} UTC`;

/** The MBL server the dev server forwards API calls to (`MBL_API`, default the local install). */
const apiTarget = process.env.MBL_API ?? "http://localhost:8420";

export default defineConfig({
  plugins: [solid()],
  build: {
    target: "chrome80",
    rolldownOptions: {
      input: {
        app: path.resolve(__dirname, "index.html"),
        receiver: path.resolve(__dirname, "cast-receiver/index.html"),
      },
    },
  },
  define: {
    __MBL_VERSION__: JSON.stringify(version),
    __MBL_GIT_COMMIT__: JSON.stringify(commit),
    __MBL_BUILD_DATE__: JSON.stringify(buildDate),
  },
  server: {
    port: Number(process.env.MBL_DEV_PORT ?? 3000),
    proxy: {
      "/api": {
        target: apiTarget,
        changeOrigin: false,
        ws: true,
      },
      "/mcp": {
        target: apiTarget,
        changeOrigin: true,
      },
      "/stream": {
        target: apiTarget,
        changeOrigin: true,
      },
      "/images": {
        target: apiTarget,
        changeOrigin: true,
      },
    },
  },
});
