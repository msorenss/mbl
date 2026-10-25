import fs from "node:fs";
import path from "node:path";
import zlib from "node:zlib";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const distDir = path.resolve(__dirname, "../dist");
const indexPath = path.join(distDir, "index.html");

if (!fs.existsSync(indexPath)) {
  console.error("Error: dist/index.html does not exist. Run build first.");
  process.exit(1);
}

const html = fs.readFileSync(indexPath, "utf8");

// Extract the main entry script
const entryMatch = html.match(/<script\s+[^>]*src=["']\/([^"']+)["']/i);
if (!entryMatch) {
  console.error("Error: Could not locate main entry script in dist/index.html");
  process.exit(1);
}

const entryRelativePath = entryMatch[1];
const entryFullPath = path.join(distDir, entryRelativePath);

if (!fs.existsSync(entryFullPath)) {
  console.error(`Error: Entry script file ${entryFullPath} not found.`);
  process.exit(1);
}

function getGzipSize(filePath) {
  const content = fs.readFileSync(filePath);
  const gzipped = zlib.gzipSync(content, { level: 9 });
  return {
    raw: content.length,
    gzip: gzipped.length,
  };
}

const entrySizes = getGzipSize(entryFullPath);
const entryGzipKb = (entrySizes.gzip / 1024).toFixed(2);
const entryRawKb = (entrySizes.raw / 1024).toFixed(2);

// Extract all modulepreload scripts loaded on initial page load
const preloadMatches = [...html.matchAll(/<link\s+[^>]*href=["']\/([^"']+\.js)["']/gi)];
const initialFiles = [entryRelativePath, ...preloadMatches.map((m) => m[1])];

let totalInitialGzip = 0;
let totalInitialRaw = 0;

console.log("\n📦 Bundle Size Budget Check (M3.11):");
console.log("----------------------------------------");
for (const rel of initialFiles) {
  const p = path.join(distDir, rel);
  if (fs.existsSync(p)) {
    const s = getGzipSize(p);
    totalInitialGzip += s.gzip;
    totalInitialRaw += s.raw;
    console.log(
      `  • ${path.basename(rel).padEnd(30)} ${(s.raw / 1024).toFixed(2).padStart(8)} KB (raw) │ ${(s.gzip / 1024).toFixed(2).padStart(8)} KB (gzip)`
    );
  }
}
console.log("----------------------------------------");

const totalGzipKb = (totalInitialGzip / 1024).toFixed(2);
const totalRawKb = (totalInitialRaw / 1024).toFixed(2);

console.log(`Initial Entry Script (${path.basename(entryRelativePath)}): ${entryGzipKb} KB gzip`);
console.log(`Total Initial JS (entry + preloads): ${totalGzipKb} KB gzip (${totalRawKb} KB uncompressed)`);

const BUDGET_KB = 150;
if (Number(totalGzipKb) > BUDGET_KB) {
  console.error(
    `❌ FAILED: Total initial JS gzip size (${totalGzipKb} KB) exceeds performance budget of ${BUDGET_KB} KB!`
  );
  process.exit(1);
}

console.log(`✅ PASSED: Initial JS size (${totalGzipKb} KB gzip) is well within budget (<= ${BUDGET_KB} KB)!\n`);
