#!/usr/bin/env node
/**
 * npm `bin` entry: forward to the native client under ~/.grok/bin/grok.
 * After `npm i -g`, `grok` on PATH launches the fork installed by postinstall.
 */
import { spawn } from "node:child_process";
import { existsSync } from "node:fs";
import { mainExePath, findBundledBinary } from "./platform.mjs";

const target = mainExePath();
const fallback = findBundledBinary();
const exe = existsSync(target) ? target : fallback;

if (!exe || !existsSync(exe)) {
  console.error(
    "grok binary not found. Run: npm install -g <this-package>  (postinstall installs it)\n" +
      `  expected: ${target}`
  );
  process.exit(127);
}

const child = spawn(exe, process.argv.slice(2), {
  stdio: "inherit",
  windowsHide: false,
});
child.on("exit", (code, signal) => {
  if (signal) process.kill(process.pid, signal);
  process.exit(code ?? 1);
});
