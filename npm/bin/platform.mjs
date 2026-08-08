/**
 * Platform / arch resolution for shipping native `grok` binaries.
 *
 * Layout (either works):
 *   artifacts/bin/<platform>-<arch>/grok[.exe]
 *   artifacts/bin/grok-<platform>-<arch>[.exe]
 *   artifacts/grok-fork[.exe]          (legacy Windows-only)
 *
 * Optional remote fallback:
 *   GROK_FORK_RELEASE_BASE  e.g. https://github.com/org/repo/releases/download/v1.0.0
 *   →  {base}/grok-{platform}-{arch}[.exe]
 */

import { existsSync } from "node:fs";
import { homedir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = dirname(fileURLToPath(import.meta.url));
export const PKG_ROOT = resolve(__dirname, "..");

/** @returns {{ platform: string, arch: string, key: string, exeName: string }} */
export function detectTarget() {
  const p = process.platform; // win32 | darwin | linux | ...
  const a = process.arch; // x64 | arm64 | ...

  let platform;
  if (p === "win32") platform = "win32";
  else if (p === "darwin") platform = "darwin";
  else if (p === "linux") platform = "linux";
  else platform = p;

  let arch;
  if (a === "x64" || a === "x86_64") arch = "x64";
  else if (a === "arm64" || a === "aarch64") arch = "arm64";
  else arch = a;

  const key = `${platform}-${arch}`;
  const exeName = platform === "win32" ? "grok.exe" : "grok";
  return { platform, arch, key, exeName };
}

export function grokHome() {
  if (process.env.GROK_HOME) return resolve(process.env.GROK_HOME);
  return join(homedir(), ".grok");
}

export function binDir(home = grokHome()) {
  return join(home, "bin");
}

export function mainExePath(home = grokHome()) {
  const { exeName } = detectTarget();
  return join(binDir(home), exeName);
}

/**
 * Locate a platform binary inside the npm package (no network).
 * @returns {string|null} absolute path
 */
export function findBundledBinary() {
  if (process.env.GROK_FORK_BIN) {
    const p = resolve(process.env.GROK_FORK_BIN);
    return existsSync(p) ? p : null;
  }

  const { platform, arch, key, exeName } = detectTarget();
  const candidates = [
    // Preferred multi-platform layout
    join(PKG_ROOT, "artifacts", "bin", key, exeName),
    join(PKG_ROOT, "artifacts", "bin", `grok-${key}${platform === "win32" ? ".exe" : ""}`),
    join(PKG_ROOT, "artifacts", "bin", key, "grok-fork" + (platform === "win32" ? ".exe" : "")),
    // Legacy single-file names
    join(PKG_ROOT, "artifacts", platform === "win32" ? "grok-fork.exe" : "grok-fork"),
    join(PKG_ROOT, "artifacts", exeName),
    // Sibling monorepo build (dev only)
    resolve(
      PKG_ROOT,
      "..",
      "grok-build-src",
      "dist-fork",
      platform === "win32" ? "grok-fork.exe" : "grok-fork"
    ),
    resolve(
      PKG_ROOT,
      "..",
      "grok-build-src",
      "target",
      "release",
      platform === "win32" ? "xai-grok-pager.exe" : "xai-grok-pager"
    ),
  ];

  // Also try arch aliases (e.g. aarch64 folder names)
  if (arch === "arm64") {
    candidates.push(
      join(PKG_ROOT, "artifacts", "bin", `${platform}-aarch64`, exeName)
    );
  }
  if (arch === "x64") {
    candidates.push(
      join(PKG_ROOT, "artifacts", "bin", `${platform}-amd64`, exeName)
    );
  }

  for (const c of candidates) {
    if (existsSync(c)) return c;
  }
  return null;
}

/**
 * URL to download binary for current platform (optional).
 * Set GROK_FORK_RELEASE_BASE or package.json "forkReleaseBase".
 */
export function remoteBinaryUrl(pkgJson = null) {
  const { platform, arch, key } = detectTarget();
  const base =
    process.env.GROK_FORK_RELEASE_BASE ||
    (pkgJson && pkgJson.forkReleaseBase) ||
    null;
  if (!base) return null;
  const file =
    platform === "win32" ? `grok-${key}.exe` : `grok-${key}`;
  return `${String(base).replace(/\/$/, "")}/${file}`;
}

export function supportedTargets() {
  return [
    "win32-x64",
    "darwin-x64",
    "darwin-arm64",
    "linux-x64",
    "linux-arm64",
  ];
}
