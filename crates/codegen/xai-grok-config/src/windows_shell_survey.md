# Windows agent shells: PowerShell family vs Git Bash

Inventory of how this repo detects, builds, and launches the two first-class
Windows shells used for **agent command execution**. `cmd.exe` is a fallback
only (not a third family). Unix bash/zsh is out of scope.

Statuses: **tested** (in-repo test drives the shipped builder), **gap**
(incorrect for agent/hook terminal and fixed or still open), **deferred**
(known, out of this PR).

## Families

| Family | Variants | Detection |
|--------|----------|-----------|
| PowerShell | `pwsh` (7+), `powershell.exe` (5.1) | `GROK_SHELL=pwsh` / `powershell`; else `where pwsh.exe`, then hardcoded 5.1 path |
| POSIX/MSYS | Git `bash.exe` | `GROK_SHELL=bash` / `gitbash` / `git-bash`; else last in cascade after PS |

Cascade (cached `OnceLock`): `GROK_SHELL` → pwsh → powershell.exe → Git Bash → powershell.exe fallback. Default stays **pwsh-first** so native `/flag` toolchains are not mangled.

Builder: `invocation_for` / `shell_command_argv` in `shell.rs`.

## Shared vs per-site spawn

### In agent/hook terminal scope (must use the builder)

| Site | Uses builder? | Notes |
|------|----------------|-------|
| `xai-grok-tools` `computer/local/terminal.rs` `spawn_shell_command` | yes | `run_terminal_command` hot path |
| `xai-grok-shell` `local_terminal.rs` | yes | applies `privileged_env` after request env |
| `xai-grok-shell` `streaming_local_terminal.rs` | yes | `after_env` last |
| `xai-grok-hooks` `runner/command.rs` (Windows) | yes | Unix hooks still `sh -c` + user body (Unix, out of scope) |

### Out of agent-terminal scope (do **not** put user agent scripts on `bash -c` argv for the agent)

| Site | What it does | Classification |
|------|----------------|----------------|
| `pty_session.rs` | Git Bash `-l` / pwsh `-NoLogo` interactive login | **deferred** — PTY UX, not `shell_command_argv` |
| `pty_session.rs` test `CommandBuilder::new("/bin/sh").arg("-c")` | Unix-only test helper | out |
| `workspace/envrc.rs` | hardcoded `/bin/bash -c` to source `.envrc` | **deferred** — direnv helper, Unix path |
| `xai-fast-worktree`, workspace-daemon, pager wrap/tmux/mermaid, voice, marketplace git | internal `arg("-c")` | **deferred** — not agent/hook command execution |
| `tools/.../grep` `arg("-c")` | ripgrep `--count`, not a shell | out |

No leftover **agent/hook** Windows path still does `bash -c <user body>` after this work.

## Per-behavior matrix

| Behavior | PowerShell family | Git Bash | Status |
|----------|-------------------|----------|--------|
| Argv shape | `-NoProfile -NonInteractive -Command <body>` | `-c` + constant eval wrapper; body in `GROK_INTERNAL_SHELL_SCRIPT` | **tested** |
| `\\` in single quotes | Body stays in `-Command` (Win32 quoting, not MSYS fold) | Body not in argv; live spawn `${#x}==2` | **tested** |
| `/flag` (e.g. `/nologo`) | Passed through in `-Command` | `MSYS_NO_PATHCONV=1`, `MSYS2_ARG_CONV_EXCL=*`; live `printf /nologo` | **tested** |
| Chain separator | pwsh: `&&`; powershell.exe 5.1: `;` | `&&` | **tested** via `WindowsShell::supports_chain_operator` |
| Bare `&` | Pwsh: call/job; 5.1: call / parse error | POSIX background | **tested** via `ampersand_semantics` |
| UTF-8 child env | `PYTHONUTF8=1`, `PYTHONIOENCODING=utf-8:surrogateescape` | same + MSYS guards | **tested** |
| Env-staging / `_s` leak | N/A (no wrapper var) | Wrapper uses `__GROK_INTERNAL_EVAL_BODY`, not `_s`; live `$_s` is unset | **tested** (was **gap**) |
| Env-block ~32K vs old cmdline ~32K | Command on argv (CreateProcess limit) | Body in env block (same order of magnitude) | **deferred** — not a quoting bug; no temp-file transport |
| Interactive PTY | `-NoLogo` | `-l` | **deferred** |
| `/tmp` vs native Win32 Python | N/A | MSYS path vs Win32 Python | **deferred** |
| Default cascade | pwsh first | not default | **deferred** (keep) |

## Peer agents (DeepWiki)

Codex / Goose / OpenCode / Cline Git Bash: still `bash -c` + user body on argv. Cline/Gemini move PowerShell bodies off the cmdline (stdin / `__GCLI_POWERSHELL_COMMAND__`). Nobody uses a temp `.sh` as the primary Git Bash transport.

## Gaps closed in this change set

1. MSYS `\\` fold on agent/hook Git Bash (`bash -c` user argv) — env staging.
2. Cursor: wrapper `_s` leaked into the user script — namespaced body var + unset of the env name before `eval`.
3. Tests now cover **both** families (builder + live spawn when the binary exists, explicit skip log otherwise), including `/flag` and chain/`&`/UTF-8, not only the `\\` case.
