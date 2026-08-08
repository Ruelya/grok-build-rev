#!/usr/bin/env node
/**
 * npm lifecycle: install fork as primary Grok client.
 * Skip with: GROK_FORK_SKIP_POSTINSTALL=1
 */
if (process.env.GROK_FORK_SKIP_POSTINSTALL === "1") {
  console.log("[@ruelya/grok-build] skip postinstall (GROK_FORK_SKIP_POSTINSTALL=1)");
  process.exit(0);
}

// Avoid noisy failures in CI that only packs the package without needing install.
if (process.env.CI === "true" && process.env.GROK_FORK_FORCE_POSTINSTALL !== "1") {
  console.log("[@ruelya/grok-build] CI detected — skip postinstall (set GROK_FORK_FORCE_POSTINSTALL=1 to run)");
  process.exit(0);
}

import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const dir = dirname(fileURLToPath(import.meta.url));
const install = join(dir, "install.mjs");
const r = spawnSync(process.execPath, [install, "install"], {
  stdio: "inherit",
  env: process.env,
});
process.exit(r.status ?? 1);
