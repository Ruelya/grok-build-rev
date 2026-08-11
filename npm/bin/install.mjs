#!/usr/bin/env node
/**
 * npm postinstall / `npx grok-build install`
 *
 * Thin installer (official-style): downloads the native binary from GitHub
 * Releases into ~/.grok/bin/grok[.exe]. Themes may ship inside the npm package.
 *
 * Platforms: win32-x64, linux-x64, darwin-arm64 (no Intel macOS).
 *
 * Usage:
 *   node bin/install.mjs              # install / replace official
 *   node bin/install.mjs --dry-run
 *   node bin/install.mjs restore
 *   node bin/install.mjs status
 */

import { spawnSync } from "node:child_process";
import {
  chmodSync,
  copyFileSync,
  createWriteStream,
  existsSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  renameSync,
  statSync,
  unlinkSync,
  writeFileSync,
} from "node:fs";
import { dirname, join } from "node:path";
import { pipeline } from "node:stream/promises";
import {
  PKG_ROOT,
  binDir,
  detectTarget,
  findBundledBinary,
  grokHome,
  mainExePath,
  remoteBinaryUrl,
  supportedTargets,
} from "./platform.mjs";

const MARKER = "fork-install.json";
const isWin = process.platform === "win32";

function loadPkg() {
  try {
    return JSON.parse(readFileSync(new URL("../package.json", import.meta.url), "utf8"));
  } catch {
    return {};
  }
}

function stamp() {
  const d = new Date();
  const p = (n) => String(n).padStart(2, "0");
  return (
    d.getFullYear() +
    p(d.getMonth() + 1) +
    p(d.getDate()) +
    "-" +
    p(d.getHours()) +
    p(d.getMinutes()) +
    p(d.getSeconds())
  );
}

function runVersion(exe) {
  if (!existsSync(exe)) return null;
  const r = spawnSync(exe, ["--version"], {
    encoding: "utf8",
    shell: false,
    windowsHide: true,
  });
  // SIGILL / crash → status null + signal; treat as unusable (e.g. neoverse-v2
  // binary on Neoverse-N1 Ampere).
  if (r.error || r.signal || (typeof r.status === "number" && r.status !== 0)) {
    return null;
  }
  const out = ((r.stdout || "") + (r.stderr || "")).trim();
  return out.split(/\r?\n/)[0] || null;
}

/** True when an on-disk binary exists but cannot even print --version. */
function isBrokenBinary(exe) {
  if (!existsSync(exe) || (fileSize(exe) || 0) < 1_000_000) return false;
  return runVersion(exe) == null;
}

function fileSize(p) {
  try {
    return statSync(p).size;
  } catch {
    return null;
  }
}

async function download(url, dest) {
  const res = await fetch(url, { redirect: "follow" });
  if (!res.ok) {
    throw new Error(`download failed ${res.status} ${url}`);
  }
  mkdirSync(dirname(dest), { recursive: true });
  const tmp = dest + ".download";
  await pipeline(res.body, createWriteStream(tmp));
  try {
    if (existsSync(dest)) unlinkSync(dest);
  } catch {
    /* ignore */
  }
  renameSync(tmp, dest);
  if (!isWin) {
    try {
      chmodSync(dest, 0o755);
    } catch {
      /* ignore */
    }
  }
}



function replaceBinary(from, to) {
  const tmp = to + ".new";
  copyFileSync(from, tmp);
  if (!isWin) {
    try {
      chmodSync(tmp, 0o755);
    } catch {
      /* ignore */
    }
  }
  try {
    if (existsSync(to)) {
      try {
        unlinkSync(to);
      } catch {
        copyFileSync(tmp, to);
        try {
          unlinkSync(tmp);
        } catch {
          /* ignore */
        }
        return;
      }
    }
    renameSync(tmp, to);
  } catch (e) {
    try {
      if (existsSync(tmp)) unlinkSync(tmp);
    } catch {
      /* ignore */
    }
    throw new Error(
      `cannot install over ${to}: ${e.message}\n  tip: quit all grok sessions, then re-run install`
    );
  }
  if (!isWin) {
    try {
      chmodSync(to, 0o755);
    } catch {
      /* ignore */
    }
  }
}

function installThemes(home, { dryRun = false } = {}) {
  const src = join(PKG_ROOT, "artifacts", "themes");
  if (!existsSync(src)) return [];
  const dest = join(home, "themes");
  const files = readdirSync(src).filter((n) => n.endsWith(".toml"));
  if (!files.length) return [];
  if (!dryRun) {
    mkdirSync(dest, { recursive: true });
    for (const f of readdirSync(dest)) {
      if (/^ruelya[-_]/i.test(f) && f.endsWith(".toml")) {
        try {
          unlinkSync(join(dest, f));
        } catch {
          /* ignore */
        }
      }
    }
    for (const f of files) {
      copyFileSync(join(src, f), join(dest, f));
    }
  }
  return files.map((f) => f.replace(/\.toml$/, ""));
}

/**
 * Point in-app auto-update at our npm package (not official x.ai CDN).
 *
 * Writes:
 *   [cli]
 *   auto_update = true
 *   installer = "npm"
 *
 * The fork binary hard-blocks the official internal/CDN installer; this only
 * ensures config matches so `grok update` uses `npm i -g @ruelya/grok-build`.
 */
function configureForkUpdater(home, { dryRun = false } = {}) {
  const cfgPath = join(home, "config.toml");
  const header =
    "# Grok Build fork — auto_update via npm (@ruelya/grok-build), never official CDN.\n";
  const block = `${header}[cli]\nauto_update = true\ninstaller = "npm"\n`;

  if (!existsSync(cfgPath)) {
    if (dryRun) return { ok: true, action: "create-cli" };
    mkdirSync(home, { recursive: true });
    writeFileSync(cfgPath, block, "utf8");
    return { ok: true, action: "create-cli" };
  }

  let raw = readFileSync(cfgPath, "utf8").replace(/\r\n/g, "\n");
  let changed = false;

  if (/^\s*auto_update\s*=\s*false\s*$/m.test(raw)) {
    raw = raw.replace(/^\s*auto_update\s*=\s*false\s*$/gm, "auto_update = true");
    changed = true;
  } else if (!/^\s*auto_update\s*=/m.test(raw)) {
    const cliIdx = raw.search(/^\[cli\]\s*$/m);
    if (cliIdx >= 0) {
      const lineEnd = raw.indexOf("\n", cliIdx);
      const insertAt = lineEnd === -1 ? raw.length : lineEnd + 1;
      raw = raw.slice(0, insertAt) + "auto_update = true\n" + raw.slice(insertAt);
    } else {
      raw = raw.replace(/\s*$/, "\n\n") + "[cli]\nauto_update = true\n";
    }
    changed = true;
  }

  if (/^\s*installer\s*=\s*".*"\s*$/m.test(raw)) {
    const next = raw.replace(/^\s*installer\s*=\s*".*"\s*$/gm, 'installer = "npm"');
    if (next !== raw) {
      raw = next;
      changed = true;
    }
  } else if (!/^\s*installer\s*=/m.test(raw)) {
    const cliIdx = raw.search(/^\[cli\]\s*$/m);
    if (cliIdx >= 0) {
      const lineEnd = raw.indexOf("\n", cliIdx);
      const insertAt = lineEnd === -1 ? raw.length : lineEnd + 1;
      raw = raw.slice(0, insertAt) + 'installer = "npm"\n' + raw.slice(insertAt);
    } else {
      raw = raw.replace(/\s*$/, "\n\n") + '[cli]\ninstaller = "npm"\n';
    }
    changed = true;
  }

  if (!changed) return { ok: true, action: "already-fork-npm" };
  if (dryRun) return { ok: true, action: "update-cli" };
  writeFileSync(cfgPath, raw, "utf8");
  return { ok: true, action: "update-cli" };
}

async function resolveSourceBinary() {
  const t = detectTarget();
  if (!supportedTargets().includes(t.key)) {
    throw new Error(
      [
        `Unsupported platform: ${t.key}`,
        `  shipped builds: ${supportedTargets().join(", ")}`,
        `  (Intel macOS is not published — use win32-x64, linux-x64, linux-arm64, or darwin-arm64)`,
        ``,
        `Override: GROK_FORK_BIN=/path/to/grok  or  GROK_FORK_RELEASE_BASE=https://…/releases/download/vX.Y.Z`,
      ].join("\n")
    );
  }

  const bundled = findBundledBinary();
  if (bundled) return { path: bundled, source: "bundled" };

  const pkg = loadPkg();
  const url = remoteBinaryUrl(pkg);
  if (!url) {
    throw new Error(
      [
        `No binary URL for ${t.key}.`,
        `  package version: ${pkg.version || "?"}`,
        `  set GROK_FORK_RELEASE_BASE or GROK_FORK_BIN`,
      ].join("\n")
    );
  }

  // Versioned cache so upgrading 1.0.2 → 1.0.3 never reuses a bad binary
  // (e.g. neoverse-v2 SIGILL on Ampere Neoverse-N1).
  const ver = String(pkg.version || "unknown").replace(/[^\w.-]+/g, "_");
  const cacheDir = join(PKG_ROOT, "artifacts", "cache", ver, t.key);
  mkdirSync(cacheDir, { recursive: true });
  const dest = join(cacheDir, t.exeName);
  const needFetch =
    !existsSync(dest) ||
    (fileSize(dest) || 0) < 1_000_000 ||
    isBrokenBinary(dest);
  if (needFetch) {
    console.log(`Downloading ${url} …`);
    await download(url, dest);
  }
  // Refuse to install a still-broken download (wrong arch / CPU baseline).
  if (isBrokenBinary(dest)) {
    throw new Error(
      [
        `Downloaded binary cannot run on this CPU (Illegal instruction / crash).`,
        `  url: ${url}`,
        `  tip: need a portable linux-arm64 build (target-cpu=generic, not neoverse-v2).`,
        `  reinstall: npm i -g @ruelya/grok-build@latest && npx grok-build install`,
      ].join("\n")
    );
  }
  return { path: dest, source: "download", url };
}

async function cmdInstall({ dryRun = false } = {}) {
  const home = grokHome();
  const target = detectTarget();
  const exe = mainExePath(home);
  mkdirSync(binDir(home), { recursive: true });

  let src;
  try {
    src = await resolveSourceBinary();
  } catch (e) {
    console.error(`error: ${e.message}`);
    process.exit(1);
  }

  const before = existsSync(exe) ? runVersion(exe) : null;
  const forkVer = runVersion(src.path);
  const ts = stamp();
  const backupName = isWin
    ? `grok.exe.bak-official-${ts}`
    : `grok.bak-official-${ts}`;
  const backupPath = join(binDir(home), backupName);

  console.log(`Platform : ${target.key}`);
  console.log(`Source   : ${src.path} (${src.source})`);
  console.log(`  version: ${forkVer || "?"}`);
  console.log(`Install →: ${exe}`);
  console.log(`  was    : ${before || "(none)"}`);
  if (existsSync(exe)) {
    console.log(`Backup → : ${backupPath}`);
  }

  const themes = installThemes(home, { dryRun: true });
  if (themes.length) console.log(`Themes   : ${themes.length} → ~/.grok/themes`);

  if (dryRun) {
    console.log("\n[dry-run] no files written.");
    return;
  }

  if (existsSync(exe)) {
    try {
      copyFileSync(exe, backupPath);
    } catch (e) {
      console.error(`error: backup failed: ${e.message}`);
      process.exit(1);
    }
  }

  try {
    replaceBinary(src.path, exe);
  } catch (e) {
    console.error(`error: ${e.message}`);
    process.exit(1);
  }

  const installedThemes = installThemes(home, { dryRun: false });
  const auto = configureForkUpdater(home, { dryRun: false });
  const after = runVersion(exe);
  const packageVersion = loadPkg().version || null;

  const marker = {
    appliedAt: new Date().toISOString(),
    brand: "rev",
    mode: "primary-npm",
    platform: target.key,
    grokHome: home,
    mainExe: exe,
    backup: existsSync(backupPath) ? backupPath : null,
    source: src.source,
    sourcePath: src.path,
    versionBefore: before,
    versionAfter: after,
    themes: installedThemes,
    autoUpdate: auto,
    package: PKG_ROOT,
    packageVersion,
    npmPackage: "@ruelya/grok-build",
  };
  writeFileSync(join(home, MARKER), JSON.stringify(marker, null, 2) + "\n");

  console.log("\nInstalled Grok Build fork as primary client.");
  console.log(`  after  : ${after || "?"}`);
  console.log(`  pkg    : @ruelya/grok-build@${packageVersion || "?"}`);
  console.log(`  run    : ${exe}`);
  console.log(`  or     : grok --version   (if ~/.grok/bin is on PATH)`);
  if (installedThemes.length) {
    console.log(
      `  themes : ${installedThemes.length} → ${join(home, "themes")}  (use /theme <name>)`,
    );
  } else {
    console.log("  themes : (none bundled — package missing artifacts/themes/*.toml)");
  }
  if (auto.ok) {
    console.log(`  update : npm installer (${auto.action}) — grok update → @ruelya/grok-build`);
  }
}

function cmdRestore({ dryRun = false } = {}) {
  const home = grokHome();
  const exe = mainExePath(home);
  const markerFile = join(home, MARKER);
  let backup = null;
  if (existsSync(markerFile)) {
    try {
      backup = JSON.parse(readFileSync(markerFile, "utf8")).backup;
    } catch {
      /* ignore */
    }
  }
  if (!backup || !existsSync(backup)) {
    const bin = binDir(home);
    if (existsSync(bin)) {
      const cands = readdirSync(bin)
        .filter((n) => n.includes("bak-official"))
        .map((n) => join(bin, n))
        .sort();
      backup = cands[cands.length - 1] || null;
    }
  }
  if (!backup || !existsSync(backup)) {
    console.error("error: no stock backup found. Reinstall official: https://x.ai/cli");
    process.exit(1);
  }
  console.log(`Restore from: ${backup}`);
  console.log(`  version   : ${runVersion(backup) || "?"}`);
  if (dryRun) {
    console.log("[dry-run] no changes.");
    return;
  }
  replaceBinary(backup, exe);
  try {
    if (existsSync(markerFile)) unlinkSync(markerFile);
  } catch {
    /* ignore */
  }
  console.log(`Restored: ${runVersion(exe) || "?"}`);
}

function cmdStatus() {
  const home = grokHome();
  const t = detectTarget();
  const exe = mainExePath(home);
  const bundled = findBundledBinary();
  console.log(`platform : ${t.key}`);
  console.log(`GROK_HOME: ${home}`);
  console.log(`main exe : ${exe}`);
  console.log(`  present: ${existsSync(exe)}`);
  console.log(`  version: ${runVersion(exe) || "n/a"}`);
  console.log(`  size   : ${fileSize(exe) ?? "n/a"}`);
  console.log(`bundled  : ${bundled || "(none for this platform)"}`);
  if (bundled) console.log(`  version: ${runVersion(bundled) || "?"}`);
  const url = remoteBinaryUrl(loadPkg());
  console.log(`remote   : ${url || "(GROK_FORK_RELEASE_BASE not set)"}`);
  const marker = join(home, MARKER);
  if (existsSync(marker)) {
    console.log(`marker   : ${marker}`);
    console.log(readFileSync(marker, "utf8"));
  } else {
    console.log("marker   : (not installed via this package)");
  }
}

function usage() {
  console.log(`Grok Build fork — npm installer (replaces official client)

  node bin/install.mjs              Install/replace ~/.grok/bin/grok
  node bin/install.mjs --dry-run
  node bin/install.mjs status
  node bin/install.mjs restore

Platform: auto (${detectTarget().key})
Binary layout: artifacts/bin/<platform>-<arch>/grok[.exe]
`);
}

const args = process.argv.slice(2);
const dryRun = args.includes("--dry-run") || args.includes("-n");
const cmd = args.find((a) => !a.startsWith("-")) || "install";

const run = async () => {
  switch (cmd) {
    case "install":
    case "apply":
      await cmdInstall({ dryRun });
      break;
    case "restore":
      cmdRestore({ dryRun });
      break;
    case "status":
    case "info":
      cmdStatus();
      break;
    case "help":
    case "-h":
    case "--help":
      usage();
      break;
    default:
      console.error(`unknown: ${cmd}`);
      usage();
      process.exit(1);
  }
};

run().catch((e) => {
  console.error(e);
  process.exit(1);
});
