#!/usr/bin/env node

/**
 * Bumps the version number in all project manifests:
 *   - package.json
 *   - src-tauri/Cargo.toml
 *   - src-tauri/tauri.conf.json
 *
 * Usage:
 *   node scripts/version-bump.js 0.2.0
 *   npm run version:bump 0.2.0
 */

import { readFileSync, writeFileSync } from "fs";
import { resolve, dirname } from "path";
import { fileURLToPath } from "url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const root = resolve(__dirname, "..");

const newVersion = process.argv[2];

if (!newVersion) {
  console.error("Usage: node scripts/version-bump.js <version>");
  console.error("Example: node scripts/version-bump.js 0.2.0");
  process.exit(1);
}

// Validate semver format
if (!/^\d+\.\d+\.\d+(-[\w.]+)?$/.test(newVersion)) {
  console.error(`Invalid version format: "${newVersion}"`);
  console.error("Expected: MAJOR.MINOR.PATCH (e.g. 0.2.0, 1.0.0-beta.1)");
  process.exit(1);
}

const files = [
  {
    path: resolve(root, "package.json"),
    update(content) {
      const json = JSON.parse(content);
      const old = json.version;
      json.version = newVersion;
      console.log(`  package.json: ${old} → ${newVersion}`);
      return JSON.stringify(json, null, 2) + "\n";
    },
  },
  {
    path: resolve(root, "src-tauri", "tauri.conf.json"),
    update(content) {
      const json = JSON.parse(content);
      const old = json.version;
      json.version = newVersion;
      console.log(`  tauri.conf.json: ${old} → ${newVersion}`);
      return JSON.stringify(json, null, 2) + "\n";
    },
  },
  {
    path: resolve(root, "src-tauri", "Cargo.toml"),
    update(content) {
      const updated = content.replace(
        /^version\s*=\s*"[^"]*"/m,
        `version = "${newVersion}"`
      );
      const oldMatch = content.match(/^version\s*=\s*"([^"]*)"/m);
      console.log(`  Cargo.toml: ${oldMatch?.[1] ?? "?"} → ${newVersion}`);
      return updated;
    },
  },
];

console.log(`\nBumping version to ${newVersion}:\n`);

for (const file of files) {
  const content = readFileSync(file.path, "utf-8");
  const updated = file.update(content);
  writeFileSync(file.path, updated, "utf-8");
}

console.log("\n✅ Version bumped successfully!");
console.log(`\nNext steps:`);
console.log(`  git add -A`);
console.log(`  git commit -m "chore: release v${newVersion}"`);
console.log(`  git tag v${newVersion}`);
console.log(`  git push origin main --tags`);
