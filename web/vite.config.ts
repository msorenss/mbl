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

export default defineConfig({
  plugins: [solid()],
  define: {
    __MBL_VERSION__: JSON.stringify(version),
    __MBL_GIT_COMMIT__: JSON.stringify(commit),
    __MBL_BUILD_DATE__: JSON.stringify(buildDate),
  },
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
