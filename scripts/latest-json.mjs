#!/usr/bin/env node
// Writes the updater manifest (latest.json) for every platform whose signed
// artifacts are in a folder:
//   node scripts/latest-json.mjs <dir> <version> <owner/repo>
import { readdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const [dir, version, repo] = process.argv.slice(2);
if (!dir || !version || !repo) {
  console.error("usage: latest-json.mjs <dir> <version> <owner/repo>");
  process.exit(1);
}
const tag = `v${version}`;
const files = readdirSync(dir);
const url = (name) => `https://github.com/${repo}/releases/download/${tag}/${encodeURIComponent(name)}`;
const entry = (pattern) => {
  const name = files.find((f) => pattern.test(f) && files.includes(`${f}.sig`));
  return name && { signature: readFileSync(join(dir, `${name}.sig`), "utf8").trim(), url: url(name) };
};

const platforms = {};
const mac = entry(/^Rec\.app\.tar\.gz$/);
if (mac) platforms["darwin-aarch64"] = platforms["darwin-aarch64-app"] = mac;
const win = entry(/-setup\.exe$/);
if (win) platforms["windows-x86_64"] = platforms["windows-x86_64-nsis"] = win;
const appimage = entry(/\.AppImage$/);
if (appimage) platforms["linux-x86_64"] = platforms["linux-x86_64-appimage"] = appimage;
const deb = entry(/\.deb$/);
if (deb) platforms["linux-x86_64-deb"] = deb;

if (Object.keys(platforms).length === 0) {
  console.error(`no signed artifacts found in ${dir}`);
  process.exit(1);
}
const doc = {
  version,
  notes: `See https://github.com/${repo}/releases/tag/${tag}`,
  pub_date: new Date().toISOString(),
  platforms,
};
writeFileSync(join(dir, "latest.json"), JSON.stringify(doc, null, 2) + "\n");
console.log(`latest.json: ${Object.keys(platforms).join(", ")}`);
