#!/usr/bin/env node
/**
 * Resolve next npm version for grok-build-rev.
 * - If package never published → 1.0.0
 * - Else bump patch|minor|major from registry latest
 * - force= exact version wins
 *
 * Usage: node scripts/resolve-version.mjs [patch|minor|major|keep] [forceVersion]
 * Prints version to stdout; writes package.json when --write.
 */
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const pkgPath = join(__dirname, "..", "package.json");

const args = process.argv.slice(2).filter((a) => a !== "--write");
const write = process.argv.includes("--write");
const bump = args[0] || "patch";
const force = args[1] || "";

function bumpSemver(v, kind) {
  const m = String(v).match(/^(\d+)\.(\d+)\.(\d+)/);
  if (!m) throw new Error(`bad semver: ${v}`);
  let [maj, min, pat] = m.slice(1).map(Number);
  if (kind === "major") {
    maj += 1;
    min = 0;
    pat = 0;
  } else if (kind === "minor") {
    min += 1;
    pat = 0;
  } else if (kind === "patch") {
    pat += 1;
  }
  return `${maj}.${min}.${pat}`;
}

async function latestOnNpm(name) {
  try {
    const res = await fetch(`https://registry.npmjs.org/${encodeURIComponent(name)}`);
    if (res.status === 404) return null;
    if (!res.ok) throw new Error(`npm registry ${res.status}`);
    const data = await res.json();
    return data["dist-tags"]?.latest || null;
  } catch (e) {
    if (String(e).includes("404")) return null;
    // network failure → treat as unpublished only if force not set
    console.error("[resolve-version]", e.message || e);
    return null;
  }
}

const pkg = JSON.parse(readFileSync(pkgPath, "utf8"));
let ver;
if (force) {
  ver = force;
} else {
  const latest = await latestOnNpm(pkg.name || "grok-build-rev");
  if (!latest) {
    ver = "1.0.0";
  } else if (bump === "keep") {
    ver = latest;
  } else {
    ver = bumpSemver(latest, bump);
  }
}
pkg.version = ver;
if (write) {
  writeFileSync(pkgPath, JSON.stringify(pkg, null, 2) + "\n");
}
process.stdout.write(ver + "\n");
