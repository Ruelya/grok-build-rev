/**
 * Platform / arch resolution for shipping native `grok` binaries.
 *
 * Thin npm installer: prefer download from GitHub Releases, then optional
 * local bundled/dev paths.
 *
 * Release assets (v${version}):
 *   grok-win32-x64.exe
 *   grok-linux-x64
 *   grok-darwin-arm64
 *
 * Override: GROK_FORK_RELEASE_BASE, package.json forkReleaseBase, GROK_FORK_BIN
 */

import { existsSync, readFileSync } from "node:fs";
import { homedir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = dirname(fileURLToPath(import.meta.url));
export const PKG_ROOT = resolve(__dirname, "..");

const DEFAULT_REPO = "Ruelya/grok-build-rev";

/** Platforms we ship (no Intel macOS). */
export const SUPPORTED_TARGETS = ["win32-x64", "linux-x64", "darwin-arm64"];

/** @returns {{ platform: string, arch: string, key: string, exeName: string }} */
export function detectTarget() {
  const p = process.platform;
  const a = process.arch;

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

export function isTargetSupported(key = detectTarget().key) {
  return SUPPORTED_TARGETS.includes(key);
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

function loadPackageJson() {
  try {
    return JSON.parse(readFileSync(join(PKG_ROOT, "package.json"), "utf8"));
  } catch {
    return null;
  }
}

/**
 * Locate a platform binary inside the npm package or monorepo (dev).
 * Thin packages normally have none — installer falls back to Releases.
 * @returns {string|null}
 */
export function findBundledBinary() {
  if (process.env.GROK_FORK_BIN) {
    const p = resolve(process.env.GROK_FORK_BIN);
    return existsSync(p) ? p : null;
  }

  const { platform, arch, key, exeName } = detectTarget();
  const candidates = [
    join(PKG_ROOT, "artifacts", "bin", key, exeName),
    join(PKG_ROOT, "artifacts", "bin", `grok-${key}${platform === "win32" ? ".exe" : ""}`),
    join(PKG_ROOT, "artifacts", platform === "win32" ? "grok-fork.exe" : "grok-fork"),
    join(PKG_ROOT, "artifacts", exeName),
    resolve(
      PKG_ROOT,
      "..",
      "target",
      "release",
      platform === "win32" ? "xai-grok-pager.exe" : "xai-grok-pager"
    ),
  ];

  if (arch === "arm64") {
    candidates.push(join(PKG_ROOT, "artifacts", "bin", `${platform}-aarch64`, exeName));
  }

  for (const c of candidates) {
    if (existsSync(c)) return c;
  }
  return null;
}

/** Asset filename for a platform key on GitHub Releases. */
export function releaseAssetName(key = detectTarget().key) {
  return key.startsWith("win32") ? `grok-${key}.exe` : `grok-${key}`;
}

/**
 * Base URL for release assets, e.g.
 * https://github.com/Ruelya/grok-build-rev/releases/download/v1.0.1
 */
export function releaseBaseUrl(pkgJson = null) {
  if (process.env.GROK_FORK_RELEASE_BASE) {
    return String(process.env.GROK_FORK_RELEASE_BASE).replace(/\/$/, "");
  }
  const pkg = pkgJson || loadPackageJson() || {};
  if (pkg.forkReleaseBase) {
    return String(pkg.forkReleaseBase).replace(/\/$/, "");
  }
  const ver = pkg.version;
  if (!ver) return null;
  return `https://github.com/${DEFAULT_REPO}/releases/download/v${ver}`;
}

/**
 * URL to download binary for current platform.
 */
export function remoteBinaryUrl(pkgJson = null) {
  const base = releaseBaseUrl(pkgJson);
  if (!base) return null;
  return `${base}/${releaseAssetName()}`;
}

export function supportedTargets() {
  return [...SUPPORTED_TARGETS];
}
