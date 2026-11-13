// Package only the receiver and its build dependencies, never the MBL admin UI.
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
const dist = fileURLToPath(new URL("../dist/", import.meta.url));
const output = path.resolve(process.argv[2] || fileURLToPath(new URL("../../.dev/cast-receiver-package/", import.meta.url)));
const html = fs.readFileSync(path.join(dist, "cast-receiver/index.html"), "utf8");
const assets = new Set([...html.matchAll(/(?:src|href)="\/(assets\/[^"?]+)"/g)].map((match) => match[1]));
for (const asset of assets) {
  if (!/^assets\/[\w.-]+\.(js|css)$/.test(asset)) throw new Error("Unexpected receiver asset path");
  const source = fs.readFileSync(path.join(dist, asset), "utf8");
  for (const match of source.matchAll(/["'`]\.\/([\w.-]+\.(?:js|css))["'`]/g)) assets.add(`assets/${match[1]}`);
}
fs.mkdirSync(path.join(output,"cast-receiver"), {recursive:true});
fs.mkdirSync(path.join(output,"assets"), {recursive:true});
fs.writeFileSync(path.join(output,"cast-receiver/index.html"),html);
for (const asset of assets) fs.copyFileSync(path.join(dist,asset),path.join(output,asset));
console.log(`Receiver package: ${output} (${assets.size} assets). Register /cast-receiver/index.html on the hosting origin.`);
