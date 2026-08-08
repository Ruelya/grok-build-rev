#!/usr/bin/env node
/**
 * Install the Grok Build **fork** into the user Grok home (default ~/.grok).
 *
 * This is a distribution helper for the fork — not a git patch applicator.
 *
 *   node bin/cli.mjs status
 *   node bin/cli.mjs install            # primary: backup stock + install fork as `grok`
 *   node bin/cli.mjs install --side-by-side   # also/only install as `grok-rev` (keep stock `grok`)
 *   node bin/cli.mjs restore            # restore stock `grok` from backup
 *   node bin/cli.mjs info
 *
 * Env:
 *   GROK_HOME     override install root (default: ~/.grok)
 *   GROK_FORK_BIN override path to fork executable
 */

import { spawnSync } from "node:child_process";
import {
  copyFileSync,
  existsSync,
  mkdirSync,
  readFileSync,
  renameSync,
  statSync,
  unlinkSync,
  writeFileSync,
  readdirSync,
} from "node:fs";
import { homedir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const PKG_ROOT = resolve(__dirname, "..");
const MARKER_NAME = "fork-install.json";
const MARKER_NAME_LEGACY = "fork-patch.json";
const isWin = process.platform === "win32";
/** Side-by-side executable name (does not replace stock `grok`). */
const SIDE_BY_SIDE_NAME = isWin ? "grok-rev.exe" : "grok-rev";

function grokHome() {
  if (process.env.GROK_HOME) return resolve(process.env.GROK_HOME);
  return join(homedir(), ".grok");
}

function binDir(home) {
  return join(home, "bin");
}

function mainExeName() {
  return isWin ? "grok.exe" : "grok";
}

function mainExePath(home) {
  return join(binDir(home), mainExeName());
}

function markerPath(home) {
  const p = join(home, MARKER_NAME);
  if (existsSync(p)) return p;
  const legacy = join(home, MARKER_NAME_LEGACY);
  if (existsSync(legacy)) return legacy;
  return p;
}

function forkArtifact() {
  if (process.env.GROK_FORK_BIN) return resolve(process.env.GROK_FORK_BIN);
  const a = join(PKG_ROOT, "artifacts", isWin ? "grok-fork.exe" : "grok-fork");
  if (existsSync(a)) return a;
  // fallback: sibling fork build output
  const b = resolve(
    PKG_ROOT,
    "..",
    "grok-build-src",
    "dist-fork",
    isWin ? "grok-fork.exe" : "grok-fork"
  );
  if (existsSync(b)) return b;
  return a;
}

function runExeVersion(exe) {
  if (!existsSync(exe)) return null;
  const r = spawnSync(exe, ["--version"], {
    encoding: "utf8",
    shell: false,
    windowsHide: true,
  });
  const out = ((r.stdout || "") + (r.stderr || "")).trim();
  return out.split(/\r?\n/)[0] || null;
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

function fileSize(p) {
  try {
    return statSync(p).size;
  } catch {
    return null;
  }
}

function cmdInfo() {
  const home = grokHome();
  const fork = forkArtifact();
  console.log("Grok Build fork — install helper");
  console.log(`  package      : ${PKG_ROOT}`);
  console.log(`  GROK_HOME    : ${home}`);
  console.log(`  primary bin  : ${mainExePath(home)}`);
  console.log(`  side-by-side : ${join(binDir(home), SIDE_BY_SIDE_NAME)}`);
  console.log(`  fork binary  : ${fork}${existsSync(fork) ? "" : "  (MISSING)"}`);
  if (existsSync(fork)) {
    console.log(`  fork size    : ${(fileSize(fork) / 1e6).toFixed(1)} MB`);
    console.log(`  fork version : ${runExeVersion(fork) || "?"}`);
  }
  const info = join(PKG_ROOT, "artifacts", "BUILD_INFO.txt");
  if (existsSync(info)) {
    console.log("  build info:");
    for (const line of readFileSync(info, "utf8").trim().split(/\r?\n/)) {
      console.log(`    ${line}`);
    }
  }
  const themesDir = join(PKG_ROOT, "artifacts", "themes");
  if (existsSync(themesDir)) {
    const themes = readdirSync(themesDir).filter((n) => n.endsWith(".toml"));
    console.log(`  themes (${themes.length}): ${themes.map((t) => t.replace(/\.toml$/, "")).join(", ")}`);
  }
  console.log("\nIdentity: fork build (version contains -rev / [rev])");
  console.log("  form: grok X.Y.Z-rev (<sha>) [rev] …");
  console.log("\nInstall modes:");
  console.log("  install              → replace ~/.grok/bin/grok (backup stock first)");
  console.log("  install --side-by-side → write ~/.grok/bin/grok-rev (keep stock grok)");
  console.log("  restore              → put stock grok back from backup");
}

function themesSrcDir() {
  return join(PKG_ROOT, "artifacts", "themes");
}

function configNotesPath() {
  const preferred = join(PKG_ROOT, "artifacts", "config-fork-notes.toml");
  if (existsSync(preferred)) return preferred;
  return join(PKG_ROOT, "artifacts", "config-ruelya-notes.toml");
}

const CONFIG_NOTES_BEGIN = ">>> fork-config-notes";
const CONFIG_NOTES_END = "<<< fork-config-notes";
const CONFIG_NOTES_BEGIN_LEGACY = ">>> ruelya-fork-config-notes";
const CONFIG_NOTES_END_LEGACY = "<<< ruelya-fork-config-notes";

/**
 * Copy package themes → ~/.grok/themes (canonical OpenCode names, no brand prefix).
 * Also removes legacy `ruelya-*.toml` leftovers so /theme never shows branded names.
 */
function installThemes(home, { dryRun = false } = {}) {
  const src = themesSrcDir();
  if (!existsSync(src)) return [];
  const dest = join(home, "themes");
  const files = readdirSync(src).filter((n) => n.endsWith(".toml"));
  if (!files.length) return [];
  if (!dryRun) {
    mkdirSync(dest, { recursive: true });
    // Purge old branded theme files (names must not carry fork identity).
    if (existsSync(dest)) {
      for (const f of readdirSync(dest)) {
        if (/^ruelya[-_]/i.test(f) && f.endsWith(".toml")) {
          try {
            unlinkSync(join(dest, f));
          } catch {
            /* ignore */
          }
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
 * Annotate ~/.grok/config.toml with Ruelya fork notes (comments only).
 * Replaces a previous notes block if present; never changes non-comment settings.
 */
/**
 * Ensure [cli].auto_update = false so official auto-update cannot overwrite the fork.
 * Idempotent: if already false, no rewrite; if true/missing under [cli], set false.
 */
function disableAutoUpdate(home, { dryRun = false } = {}) {
  const cfgPath = join(home, "config.toml");
  if (!existsSync(cfgPath)) {
    if (dryRun) return { ok: true, action: "create-cli", path: cfgPath };
    mkdirSync(home, { recursive: true });
    writeFileSync(
      cfgPath,
      `# Written by Grok Build fork install — keep auto_update off so upstream cannot clobber the fork.\n[cli]\nauto_update = false\n`,
      "utf8"
    );
    return { ok: true, action: "create-cli", path: cfgPath };
  }
  let raw = readFileSync(cfgPath, "utf8").replace(/\r\n/g, "\n");

  // Already disabled somewhere (user may have multiple [cli] tables).
  if (/^\s*auto_update\s*=\s*false\s*$/m.test(raw)) {
    return { ok: true, action: "already-false", path: cfgPath };
  }

  // Flip any true → false
  if (/^\s*auto_update\s*=\s*true\s*$/m.test(raw)) {
    const next = raw.replace(/^\s*auto_update\s*=\s*true\s*$/gm, "auto_update = false");
    if (dryRun) return { ok: true, action: "flip-true", path: cfgPath };
    try {
      copyFileSync(cfgPath, cfgPath + `.bak-auto-update-${stamp()}`);
    } catch {
      /* ignore */
    }
    writeFileSync(cfgPath, next, "utf8");
    return { ok: true, action: "flip-true", path: cfgPath };
  }

  // No auto_update key: append under first [cli] or create [cli]
  const cliIdx = raw.search(/^\[cli\]\s*$/m);
  let next;
  let action;
  if (cliIdx >= 0) {
    const lineEnd = raw.indexOf("\n", cliIdx);
    const insertAt = lineEnd === -1 ? raw.length : lineEnd + 1;
    next =
      raw.slice(0, insertAt) +
      "auto_update = false\n" +
      raw.slice(insertAt);
    action = "insert-cli";
  } else {
    next = raw.replace(/\s*$/, "\n\n") + "[cli]\nauto_update = false\n";
    action = "append-cli";
  }
  if (dryRun) return { ok: true, action, path: cfgPath };
  try {
    copyFileSync(cfgPath, cfgPath + `.bak-auto-update-${stamp()}`);
  } catch {
    /* ignore */
  }
  writeFileSync(cfgPath, next, "utf8");
  return { ok: true, action, path: cfgPath };
}

/**
 * Seed ~/.grok/usage/sync.toml from package example if missing.
 * Never overwrites an existing file (user credentials live there).
 */
function installUsageSyncExample(home, { dryRun = false } = {}) {
  const dest = join(home, "usage", "sync.toml");
  const example = join(PKG_ROOT, "artifacts", "usage-sync.example.toml");
  if (existsSync(dest)) {
    return { ok: true, action: "exists", path: dest };
  }
  if (!existsSync(example)) {
    return { ok: false, reason: "example missing", path: dest };
  }
  if (dryRun) return { ok: true, action: "create", path: dest };
  mkdirSync(join(home, "usage"), { recursive: true });
  copyFileSync(example, dest);
  return { ok: true, action: "create", path: dest };
}

function installConfigNotes(home, { dryRun = false } = {}) {
  const notesFile = configNotesPath();
  if (!existsSync(notesFile)) return { ok: false, reason: "notes file missing" };

  const cfgPath = join(home, "config.toml");
  let notes = readFileSync(notesFile, "utf8").replace(/\r\n/g, "\n").trimEnd() + "\n";
  // Ensure markers present for future updates
  if (!notes.includes(CONFIG_NOTES_BEGIN)) {
    notes = `# ${CONFIG_NOTES_BEGIN}\n` + notes;
  }
  if (!notes.includes(CONFIG_NOTES_END)) {
    notes = notes + `# ${CONFIG_NOTES_END}\n`;
  }

  if (!existsSync(cfgPath)) {
    if (dryRun) return { ok: true, action: "create", path: cfgPath };
    writeFileSync(cfgPath, notes, "utf8");
    return { ok: true, action: "create", path: cfgPath };
  }

  const raw = readFileSync(cfgPath, "utf8").replace(/\r\n/g, "\n");
  const beginRe = new RegExp(
    `^[ \\t]*#.*?${CONFIG_NOTES_BEGIN.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}.*$`,
    "m"
  );
  const endRe = new RegExp(
    `^[ \\t]*#.*?${CONFIG_NOTES_END.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}.*$`,
    "m"
  );
  const beginMatch = raw.match(beginRe);
  const endMatch = raw.match(endRe);

  let next;
  let action;
  if (beginMatch && endMatch && beginMatch.index != null && endMatch.index != null) {
    const start = beginMatch.index;
    // find line end of END marker
    const endLineStart = endMatch.index;
    const endLineEnd = raw.indexOf("\n", endLineStart);
    const end = endLineEnd === -1 ? raw.length : endLineEnd + 1;
    // expand start to beginning of line (already at line start via ^)
    next = raw.slice(0, start) + notes + raw.slice(end);
    // collapse double blank lines at splice
    next = next.replace(/\n{3,}/g, "\n\n");
    action = "update";
  } else if (raw.includes(CONFIG_NOTES_BEGIN)) {
    // broken block — append fresh
    next = raw.replace(/\s*$/, "\n\n") + notes;
    action = "append-repair";
  } else {
    next = raw.replace(/\s*$/, "\n\n") + notes;
    action = "append";
  }

  if (dryRun) return { ok: true, action, path: cfgPath };
  // backup config once per apply
  try {
    copyFileSync(cfgPath, cfgPath + `.bak-before-fork-${stamp()}`);
  } catch {
    /* ignore */
  }
  writeFileSync(cfgPath, next, "utf8");
  return { ok: true, action, path: cfgPath };
}

function cmdStatus() {
  const home = grokHome();
  const exe = mainExePath(home);
  const fork = forkArtifact();
  console.log(`GROK_HOME: ${home}`);
  console.log(`bin exists: ${existsSync(binDir(home))}`);
  console.log(`main exe : ${exe}`);
  console.log(`  present : ${existsSync(exe)}`);
  console.log(`  size    : ${fileSize(exe) ?? "n/a"}`);
  console.log(`  version : ${runExeVersion(exe) || "n/a"}`);
  console.log(`fork art : ${fork}`);
  console.log(`  present : ${existsSync(fork)}`);
  console.log(`  version : ${runExeVersion(fork) || "n/a"}`);
  const marker = markerPath(home);
  if (existsSync(marker)) {
    console.log(`marker   : ${marker}`);
    try {
      console.log(JSON.stringify(JSON.parse(readFileSync(marker, "utf8")), null, 2));
    } catch {
      console.log(readFileSync(marker, "utf8"));
    }
  } else {
    console.log("marker   : (none — fork not recorded as installed)");
  }
  // list backups
  const bin = binDir(home);
  if (existsSync(bin)) {
    const backs = readdirSync(bin).filter(
      (n) =>
        n.startsWith("grok.exe.bak-official") ||
        n.startsWith("grok.bak-official") ||
        n === "grok.exe.old" ||
        /^grok-[\d.]+\.exe$/.test(n)
    );
    if (backs.length) {
      console.log("backups / siblings in bin/:");
      for (const b of backs.sort()) {
        const p = join(bin, b);
        console.log(`  ${b}  (${fileSize(p)} bytes)  ${runExeVersion(p) || ""}`);
      }
    }
  }
}

function ensureInstallLayout(home, { requireExisting = true } = {}) {
  if (!existsSync(home)) {
    if (requireExisting) {
      console.error(`error: GROK_HOME not found: ${home}`);
      console.error("  option A: install stock Grok first → https://x.ai/cli");
      console.error("  option B: mkdir GROK_HOME and use: install --side-by-side --bootstrap");
      process.exit(1);
    }
    mkdirSync(home, { recursive: true });
  }
  mkdirSync(binDir(home), { recursive: true });
  const exe = mainExePath(home);
  if (requireExisting && !existsSync(exe)) {
    console.error(`error: installed binary not found: ${exe}`);
    console.error("  install stock first, or: install --side-by-side --bootstrap");
    process.exit(1);
  }
  return exe;
}

/**
 * Install fork as grok-rev next to stock grok (does not replace primary).
 * With --bootstrap, creates ~/.grok/bin even if stock is missing.
 */
function cmdInstallSideBySide({ dryRun = false, bootstrap = false } = {}) {
  const home = grokHome();
  ensureInstallLayout(home, { requireExisting: !bootstrap });
  const fork = forkArtifact();
  if (!existsSync(fork)) {
    console.error(`error: fork binary missing: ${fork}`);
    console.error("  build first: scripts/build-fork.sh  or set GROK_FORK_BIN=");
    process.exit(1);
  }
  const dest = join(binDir(home), SIDE_BY_SIDE_NAME);
  const forkVer = runExeVersion(fork);
  console.log(`Mode      : side-by-side (keep stock grok if present)`);
  console.log(`Fork      : ${fork}`);
  console.log(`  version : ${forkVer || "?"}`);
  console.log(`Install → : ${dest}`);
  const themeNames = installThemes(home, { dryRun: true });
  if (themeNames.length) {
    console.log(`Themes →  : ${join(home, "themes")} (${themeNames.length})`);
  }
  if (dryRun) {
    console.log("\n[dry-run] would write side-by-side binary + themes. No changes made.");
    return;
  }
  replaceBinary(fork, dest);
  const installedThemes = installThemes(home, { dryRun: false });
  const autoResult = disableAutoUpdate(home, { dryRun: false });
  const syncResult = installUsageSyncExample(home, { dryRun: false });
  console.log("\nInstalled fork side-by-side.");
  console.log(`  run     : ${dest}`);
  console.log(`  version : ${runExeVersion(dest) || "?"}`);
  if (installedThemes.length) {
    console.log(`  themes  : ${installedThemes.length} files under ~/.grok/themes`);
  }
  if (autoResult.ok) {
    console.log(`  auto_update : ${autoResult.action}`);
  }
  if (syncResult.ok) {
    console.log(`  usage sync  : ${syncResult.action}`);
  }
  console.log("\nStock `grok` left unchanged (if present). Use PATH or full path to grok-rev.");
}

/**
 * Atomic-ish replace on Windows: copy to .new then rename over after backup.
 * Running processes may lock the file — we report clearly.
 */
function replaceBinary(from, to) {
  const tmp = to + ".new";
  try {
    copyFileSync(from, tmp);
  } catch (e) {
    console.error(`error: cannot copy fork → ${tmp}: ${e.message}`);
    process.exit(1);
  }
  try {
    // On Windows, rename over existing may fail if locked; try unlink then rename
    if (existsSync(to)) {
      try {
        unlinkSync(to);
      } catch {
        // try direct overwrite via copy
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
    console.error(`error: cannot install over ${to}: ${e.message}`);
    console.error("  tip: quit all grok / agent sessions, then retry");
    try {
      if (existsSync(tmp)) unlinkSync(tmp);
    } catch {
      /* ignore */
    }
    process.exit(1);
  }
}

function cmdApply({ dryRun = false } = {}) {
  const home = grokHome();
  const exe = ensureInstallLayout(home);
  const fork = forkArtifact();
  if (!existsSync(fork)) {
    console.error(`error: fork binary missing: ${fork}`);
    console.error("  build first: scripts/build-fork.sh  or set GROK_FORK_BIN=");
    process.exit(1);
  }

  const before = runExeVersion(exe);
  const forkVer = runExeVersion(fork);
  const ts = stamp();
  const backupName = isWin
    ? `grok.exe.bak-official-${ts}`
    : `grok.bak-official-${ts}`;
  const backupPath = join(binDir(home), backupName);

  const themeNames = installThemes(home, { dryRun: true });
  const cfgPlan = installConfigNotes(home, { dryRun: true });
  const autoPlan = disableAutoUpdate(home, { dryRun: true });
  const syncPlan = installUsageSyncExample(home, { dryRun: true });

  console.log(`Installed : ${exe}`);
  console.log(`  version : ${before || "?"}`);
  console.log(`Fork      : ${fork}`);
  console.log(`  version : ${forkVer || "?"}`);
  console.log(`  brand   : fork (version should contain -rev / [rev])`);
  console.log(`Backup to : ${backupPath}`);
  if (themeNames.length) {
    console.log(`Themes →  : ${join(home, "themes")}`);
    console.log(`  names   : ${themeNames.join(", ")}`);
  }
  if (cfgPlan.ok) {
    console.log(`Config    : ${cfgPlan.path} (${cfgPlan.action} comment block)`);
  }
  if (autoPlan.ok) {
    console.log(`Auto-update: ${autoPlan.action} → [cli].auto_update = false`);
  }
  if (syncPlan.ok) {
    console.log(`Usage sync : ${syncPlan.action} → ${syncPlan.path}`);
  }

  if (dryRun) {
    console.log(
      "\n[dry-run] would backup stock, install fork as primary grok + themes + disable auto_update + seed usage sync.toml. No changes made."
    );
    return;
  }

  if (
    forkVer &&
    !forkVer.toLowerCase().includes("-rev") &&
    !forkVer.toLowerCase().includes("[rev]") &&
    !forkVer.toLowerCase().includes("ruelya")
  ) {
    console.warn(
      "warn: fork binary --version does not look forked (-rev/[rev]); rebuild with fork build.rs"
    );
  }

  try {
    copyFileSync(exe, backupPath);
  } catch (e) {
    console.error(`error: backup failed: ${e.message}`);
    process.exit(1);
  }
  try {
    const old = join(binDir(home), isWin ? "grok.exe.old" : "grok.old");
    copyFileSync(exe, old);
  } catch {
    /* ignore */
  }

  replaceBinary(fork, exe);
  const installedThemes = installThemes(home, { dryRun: false });
  const cfgResult = installConfigNotes(home, { dryRun: false });
  const autoResult = disableAutoUpdate(home, { dryRun: false });
  const syncResult = installUsageSyncExample(home, { dryRun: false });

  const after = runExeVersion(exe);
  const marker = {
    appliedAt: new Date().toISOString(),
    brand: "rev",
    mode: "primary",
    grokHome: home,
    mainExe: exe,
    backup: backupPath,
    forkSource: fork,
    versionBefore: before,
    versionAfter: after,
    themes: installedThemes,
    configNotes: cfgResult,
    autoUpdate: autoResult,
    usageSync: syncResult,
    package: PKG_ROOT,
  };
  writeFileSync(join(home, MARKER_NAME), JSON.stringify(marker, null, 2) + "\n");

  console.log("\nInstalled Grok Build fork as primary `grok`.");
  console.log(`  before : ${before || "?"}`);
  console.log(`  after  : ${after || "?"}`);
  if (installedThemes.length) {
    console.log(`  themes : ${installedThemes.join(", ")}  (/theme <name>)`);
  }
  if (cfgResult.ok) {
    console.log(`  config : ${cfgResult.action} → ${cfgResult.path}`);
  }
  if (autoResult.ok) {
    console.log(`  auto_update : ${autoResult.action} (disabled)`);
  }
  if (syncResult.ok) {
    console.log(`  usage sync  : ${syncResult.action} → ${syncResult.path}`);
  }
  console.log(`  marker : ${join(home, MARKER_NAME)}`);
  console.log("Restore stock: node bin/cli.mjs restore");
}

function cmdRestore({ dryRun = false } = {}) {
  const home = grokHome();
  const exe = ensureInstallLayout(home);
  const markerFile = markerPath(home);
  let backup = null;
  if (existsSync(markerFile)) {
    try {
      const m = JSON.parse(readFileSync(markerFile, "utf8"));
      backup = m.backup;
    } catch {
      /* ignore */
    }
  }
  // fallback: newest bak-official
  if (!backup || !existsSync(backup)) {
    const bin = binDir(home);
    const cands = readdirSync(bin)
      .filter((n) => n.includes("bak-official"))
      .map((n) => join(bin, n))
      .filter((p) => existsSync(p))
      .sort();
    backup = cands[cands.length - 1] || null;
  }
  if (!backup || !existsSync(backup)) {
    // last resort official versioned copy
    const v = join(binDir(home), isWin ? "grok-0.2.120.exe" : "grok-0.2.120");
    if (existsSync(v)) backup = v;
  }
  if (!backup || !existsSync(backup)) {
    console.error("error: no official backup found under bin/");
    console.error("  reinstall: curl -fsSL https://x.ai/cli/install.sh | bash");
    process.exit(1);
  }

  console.log(`Restore from: ${backup}`);
  console.log(`  version   : ${runExeVersion(backup) || "?"}`);
  console.log(`Overwrite   : ${exe}`);
  console.log(`  version   : ${runExeVersion(exe) || "?"}`);

  if (dryRun) {
    console.log("\n[dry-run] would restore backup over main exe. No changes made.");
    return;
  }

  replaceBinary(backup, exe);
  if (existsSync(markerFile)) {
    try {
      unlinkSync(markerFile);
    } catch {
      /* ignore */
    }
  }
  console.log("\nRestored.");
  console.log(`  now: ${runExeVersion(exe) || "?"}`);
}

function usage() {
  console.log(`Grok Build fork installer

Install this fork for end users (prebuilt binary + themes + examples).
Not a git patch — the product is the fork itself.

Usage:
  node bin/cli.mjs status
  node bin/cli.mjs install [--dry-run]              # primary: replace ~/.grok/bin/grok
  node bin/cli.mjs install --side-by-side [--dry-run] [--bootstrap]
                                                    # keep stock grok; write grok-rev
  node bin/cli.mjs restore [--dry-run]              # restore stock primary grok
  node bin/cli.mjs info

Env:
  GROK_HOME      Install root (default: ~/.grok)
  GROK_FORK_BIN  Path to fork executable

Examples:
  # Recommended for most users (side-by-side, no overwrite):
  node bin/cli.mjs install --side-by-side --bootstrap

  # Replace stock grok (backup first; quit all grok sessions):
  node bin/cli.mjs install
`);
}

const args = process.argv.slice(2);
const cmd = args[0];
const dryRun = args.includes("--dry-run") || args.includes("-n");
const sideBySide =
  args.includes("--side-by-side") ||
  args.includes("--sxs") ||
  args.includes("side-by-side");
const bootstrap = args.includes("--bootstrap");

switch (cmd) {
  case "info":
    cmdInfo();
    break;
  case "status":
    cmdStatus();
    break;
  case "apply":
  case "install":
  case "patch": // legacy alias
    if (sideBySide) {
      cmdInstallSideBySide({ dryRun, bootstrap });
    } else {
      cmdApply({ dryRun });
    }
    break;
  case "side-by-side":
  case "sxs":
    cmdInstallSideBySide({ dryRun, bootstrap: bootstrap || true });
    break;
  case "restore":
  case "unpatch":
  case "reverse":
    cmdRestore({ dryRun });
    break;
  case "-h":
  case "--help":
  case "help":
    usage();
    break;
  case undefined:
    usage();
    process.exit(1);
    break;
  default:
    console.error(`unknown command: ${cmd}`);
    usage();
    process.exit(1);
}
