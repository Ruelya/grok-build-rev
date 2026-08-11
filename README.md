# grok-build-rev

Fork of [Grok Build](https://github.com/xai-org/grok-build) (`grok` CLI/TUI).

Version stamp uses a **`-rev`** suffix, e.g. `1.0.0-rev (abc1234) [stable]`. The channel label is fixed to **`[stable]`** (aligned with the official stable train).

- English: [English](#english)
- 中文：[中文](#中文)

---

## English

### Install

Native binaries (GitHub Releases): Windows x64, Linux x64, Linux arm64, macOS arm64. Intel macOS not shipped.
Updates: `grok update` / auto-update use npm package `@ruelya/grok-build` (official x.ai CDN blocked).


Replaces the official client at `~/.grok/bin/grok`.

```bash
npm install -g @ruelya/grok-build
grok --version                           # expect …-rev (…) [stable]
npx grok-build restore                   # restore previous binary from backup
```

| Platform key | Release asset |
|--------------|---------------|
| `win32-x64` | `grok-win32-x64.exe` |
| `linux-x64` | `grok-linux-x64` |
| `linux-arm64` | `grok-linux-arm64` |
| `darwin-arm64` | `grok-darwin-arm64` |

```bash
bash scripts/stage-binaries.sh win32-x64 ./target/release/xai-grok-pager.exe
cargo build -p xai-grok-pager-bin --release   # needs a real protoc on PATH
```

---

### Loose API backends

Gateways often omit fields that official SDKs treat as required. This fork keeps stock backends and adds **loose** siblings (type-level `#[serde(default)]`, no synthetic JSON fill-in).

| Config value | Protocol | Use when |
|--------------|----------|----------|
| `chat_completions` | Chat Completions | Stock default |
| **`openai_chat_completions`** | Chat Completions | **Loose** — third-party OpenAI-compatible chat |
| `responses` | Responses | Stock / first-party oriented |
| **`openai_responses`** | Responses | **Loose** — AxonHub / proxy Responses |
| `messages` | Anthropic Messages | Stock |
| **`anthropic_messages`** | Anthropic Messages | **Loose** — Anthropic-compatible proxies |

```toml
[model.sol]
name = "sol"
model = "upstream-id"
base_url = "https://gateway.example/v1"
api_backend = "openai_responses"
env_key = "OPENAI_API_KEY"
context_window = 200000
```

See `examples/config.api-backends.toml`.

---

### Themes (UI + markdown + syntax)

Themes are a first-class surface in this fork — not a color swap.

**What you get**

1. **Builtin kinds** (via `/theme` or `[ui] theme`): `groknight`, `grokday`, `tokyonight`, `rosepine-moon`, `oscura-midnight`, plus `auto` (follow system light/dark).
2. **User TOML themes** under `~/.grok/themes/<name>.toml` — OpenCode-style full palettes:
   - UI chrome: backgrounds, accents, selection, mode colors (plan / always-approve / auto)
   - Markdown roles: headings, code, links, lists, …
   - Optional `[syntax]` roles for code highlighting (keyword, string, comment, …)
3. **Shipped palette pack** (npm package `artifacts/themes/`, 61 dark/light TOMLs): Catppuccin, Gruvbox, Nord, Dracula, One Dark, GitHub, Matrix, OpenCode, …  and a original theme, Lonetrail. ;-)
4. **Session mode colors** on the prompt line so plan / always-approve / auto stay readable under any palette.

**Custom theme file shape**

```toml
# ~/.grok/themes/my-theme.toml
base = "groknight"          # required: builtin base
accent_user = "#7aa2f7"
accent_success = "#9ece6a"
# …any Theme color field as #RGB / #RRGGBB

[syntax]
keyword = "#bb9af7"
string = "#9ece6a"
comment = "#565f89"
```

**Switch**

```text
/theme <name>     # or /t
/theme auto       # system appearance
```

Settings UI also exposes theme; truecolor terminals get full RGB, limited terminals fall back cleanly.

---

### Built-in agents & toolsets

#### What agent type starts by default?

On a fresh config (no `[agent] name`, no per-model override), the main session harness is:

**`grok-build-plan`**

That is `DEFAULT_AGENT_TYPE` in the shell: full Grok Build tools **plus plan mode** (`enter_plan_mode` / `exit_plan_mode` / `ask_user_question`).

Override order (later wins only where configured):

1. Model catalog field `agent_type` on the active model (default string is also `grok-build-plan` if omitted)
2. Config `[agent] name = "…"` (set from `/agents` as default)
3. Env `GROK_AGENT` when used by the shell selection path

This is **independent** of `[subagents.toggle]`: the main session type is not the Task subagent enable list.

#### Default visibility (Task / `/agents` toggles)

The **full catalog is registered**, but only the **stock five** are **enabled by default** for **Task spawn** and related lists. Extended types appear in `/agents` as **off** until you enable them (key **`t`**, or config). Task spawn only offers **enabled** agents.

| Name | Default | Role |
|------|---------|------|
| `grok-build` | **on** | Main software-engineering agent (full Grok Build toolset) |
| `general-purpose` | **on** | Multi-step general work (execute/read/edit/search/web/plan) |
| `explore` | **on** | Fast **read-only** exploration (`read_file`, `list_dir`, `grep` only) |
| `plan` | **on** | **Read-only** architecture / implementation plans (+ todo) |
| `browser-use` | **on** | Web browsing / interaction agent |
| `grok-build-concise` | off | Same family, **concise** tool I/O (SFT/RL-oriented, no AGENTS.md inject) |
| `grok-build-plan` | off | Full tools **+ plan mode** (`enter_plan_mode` / `exit_plan_mode` / `ask_user_question`) |
| `grok-build-plan-no-subagents` | off | Plan mode **without** Task spawn (bash bg helpers kept) |
| `grok-build-ask-user` | off | Full tools + **`ask_user_question`** without full plan mode |
| `codex` | off | **Codex** tool naming + apply-patch style edit + Codex system prompt |
| `opencode` | off | **OpenCode** tool conventions (read/edit/write/grep/glob/skill) |
| `grok-build-orchestrator` | off | Orchestrator: research + Task stack; **no** local search_replace — delegates coding |

#### Toolset summary (what each can actually do)

| Agent | Shell | Edit files | Task/subagents | Plan mode | Ask user | Notes |
|-------|-------|------------|----------------|-----------|----------|--------|
| `grok-build` | yes | yes | yes | — | — | Default implementer |
| `general-purpose` | yes | yes | via kinds | plan tools | — | Broad multi-step |
| `explore` | **no** | **no** | no | — | — | Strict read-only toolset |
| `plan` | **no** | **no** | no | todo only | — | Architect, read-only |
| `browser-use` | browsing tools | — | — | — | — | Web automation |
| `grok-build-concise` | concise bash/read/edit | yes | limited | — | — | Compact tool payloads |
| `grok-build-plan` | yes | yes | yes | **yes** | **yes** | Plan-then-code |
| `grok-build-plan-no-subagents` | yes | yes | **no Task** | **yes** | **yes** | Plan without spawn |
| `grok-build-ask-user` | yes | yes | yes | — | **yes** | Structured questions |
| `codex` | yes | apply_patch | no Task | — | — | Codex prompt + tools |
| `opencode` | OpenCode bash | OpenCode edit/write | no Task | — | — | OpenCode param style |
| `grok-build-orchestrator` | research bash | **no edit** | **yes** | **yes** | **yes** | Delegate to implementers |

#### Enable / disable

`/agents` → select row → **`t`** toggles. Persists as:

```toml
# ~/.grok/config.toml
[subagents.toggle]
codex = true
opencode = true
grok-build-orchestrator = true
# stock five can be forced off:
# explore = false
```

---

### Prompt cache key & recap model

#### `auto_prompt_cache_key` (per model)

**Effect:** On **Responses** backends (`responses` / `openai_responses`), each main turn automatically sends a **session-stable** `prompt_cache_key`. Providers that support prompt caching can reuse KV for the stable prefix of the conversation → lower latency and cost on long sessions. **Off by default.** Chat Completions / Messages ignore this flag.

**Config** (on a `[model.*]` entry):

```toml
[model.sol]
api_backend = "openai_responses"
auto_prompt_cache_key = true
```

Keys are derived from the session (main vs recap use different derivations so caches do not collide).

#### Recap model (`[models] recap`)

**Effect:** Session **recap** (`/recap` and automatic return-from-away recap) can run on a **cheaper / smaller** model than the main chat model. When unset, recap follows the OAuth-preferred official built-in. When set, your override wins.

**Config:**

```toml
[models]
default = "sol"
recap = "cheap-summary"     # model id or catalog key

[model.cheap-summary]
name = "Cheap recap"
model = "fast-small-id"
base_url = "https://gateway.example/v1"
api_backend = "openai_chat_completions"
env_key = "OPENAI_API_KEY"
```

Also controlled by feature gate `[features] session_recap` / env `GROK_SESSION_RECAP` (upstream defaults apply).

---

### Usage activity, pricing, live cost

`/usage` is the **official tabbed modal**; the fork adds a fourth tab **Activity**.

| Tab | Content |
|-----|---------|
| Context usage | Context window breakdown |
| Usage limit | Subscription / credits |
| Session info | Session metadata |
| **Activity** | Local heatmap, WebDAV, cost mode, live `$` |

**Cost modes** (`~/.grok/usage/pricing.toml`):

```toml
mode = "all"                 # off | all | official_only
live_display = true          # prompt + subagent frame $
auto_sync_catalog = true     # refresh models.dev on sync
```

| Mode | Behavior |
|------|----------|
| `off` | No USD attribution |
| `all` | All models (ticks → custom → models.dev → seed → placeholder) |
| `official_only` | Grok models only (subscription login) |

- **`p`**: cycle cost mode (re-scan local) · **`d`**: live `$` · **`s`**: force WebDAV · **`w`**: day/week heatmap  
- Live `$` on prompt (`model · $0.12 · flags`) and **subagent title bar** when enabled  
- Ticks: `1 USD = 10_000_000_000`  
- Custom rates: `~/.grok/usage/prices/custom.toml` (add-only)

**WebDAV** (`~/.grok/usage/sync.toml`): per-device snapshots, **additive** day×model merge — never whole-DB overwrite. See `examples/usage-sync.toml`.

---

### Examples

| File | Purpose |
|------|---------|
| `examples/config.api-backends.toml` | Loose backends + `auto_prompt_cache_key` |
| `examples/config.usage-pricing.toml` | Cost modes / live display |
| `examples/prices.custom.toml` | Custom $/1M rates |
| `examples/usage-sync.toml` | WebDAV sync |
| `config_example.toml` | Broader config catalog (package) |

---

## 中文

[Grok Build](https://github.com/xai-org/grok-build) 的 fork（`grok` CLI/TUI）。

版本号带 **`-rev`** 后缀，通道标签固定为 **`[stable]`**，与官方 stable 产品线对齐。

原生二进制（GitHub Releases）：Windows x64、Linux x64、Linux arm64、macOS arm64。不提供 Intel macOS。
更新：`grok update` / 自动更新走 npm 包 `@ruelya/grok-build`（官方 x.ai CDN 已屏蔽）。

### 安装

安装后会替换官方客户端路径 `~/.grok/bin/grok`。

```bash
npm install -g @ruelya/grok-build
grok --version                           # 期望 …-rev (…) [stable]
npx grok-build restore                   # 从备份恢复上一份二进制
```

| 平台键 | Release 资源 |
|--------|--------------|
| `win32-x64` | `grok-win32-x64.exe` |
| `linux-x64` | `grok-linux-x64` |
| `linux-arm64` | `grok-linux-arm64` |
| `darwin-arm64` | `grok-darwin-arm64` |

```bash
bash scripts/stage-binaries.sh win32-x64 ./target/release/xai-grok-pager.exe
cargo build -p xai-grok-pager-bin --release   # PATH 上需要真正的 protoc
```

---

### 宽松 API backend

第三方网关常会省略官方 SDK 视为必填的字段。本 fork 保留官方/严格 backend，并增加 **宽松** 变体（类型层 `#[serde(default)]`，**不会**注入假 JSON 字段）。

| 配置值 | 协议 | 适用场景 |
|--------|------|----------|
| `chat_completions` | Chat Completions | 官方默认 |
| **`openai_chat_completions`** | Chat Completions | **宽松** — 第三方 OpenAI 兼容 chat |
| `responses` | Responses | 官方 / 第一方取向 |
| **`openai_responses`** | Responses | **宽松** —第三方代理 Responses |
| `messages` | Anthropic Messages | 官方 |
| **`anthropic_messages`** | Anthropic Messages | **宽松** — Anthropic 兼容代理 |

```toml
[model.sol]
name = "sol"
model = "upstream-id"
base_url = "https://gateway.example/v1"
api_backend = "openai_responses"
env_key = "OPENAI_API_KEY"
context_window = 200000
```

见 `examples/config.api-backends.toml`。

---

### 主题系统（UI + Markdown + 语法高亮）

本 fork 的主题是一等能力，不是简单换色。

**你能得到什么**

1. **内置种类**（`/theme` 或 `[ui] theme`）：`groknight`、`grokday`、`tokyonight`、`rosepine-moon`、`oscura-midnight`，以及 `auto`（跟随系统亮/暗）。
2. **用户 TOML 主题**，路径 `~/.grok/themes/<name>.toml` — OpenCode 风格完整色板：
   - UI 外壳：背景、强调色、选区、会话模式色（plan / always-approve / auto）
   - Markdown 角色：标题、代码、链接、列表等
   - 可选 `[syntax]`：代码高亮角色（keyword、string、comment 等）
3. **随包主题包**（npm 包 `artifacts/themes/`，61个暗/亮 TOML）：Catppuccin、Gruvbox、Nord、Dracula、One Dark、GitHub、Matrix、OpenCode 等。以及一个原创theme, Lonetrail.  ;-)
4. **会话模式色**：提示行上 plan / always-approve / auto 在任意主题下仍可区分。

**自定义主题文件形态**

```toml
# ~/.grok/themes/my-theme.toml
base = "groknight"          # 必填：内置基底
accent_user = "#7aa2f7"
accent_success = "#9ece6a"
# …任意 Theme 颜色字段，#RGB 或 #RRGGBB

[syntax]
keyword = "#bb9af7"
string = "#9ece6a"
comment = "#565f89"
```

**切换**

```text
/theme <name>     # 或 /t
/theme auto       # 跟随系统外观
```

设置界面也可选主题；truecolor 终端用完整 RGB，能力有限的终端会干净降级。

---

### 开放内置 agent 与 toolset 选择

#### 启动默认是哪个 agent type？

在全新配置（无 `[agent] name`、无模型级覆盖）下，主会话 harness 为：

**`grok-build-plan`**

即 shell 里的 `DEFAULT_AGENT_TYPE`：完整 Grok Build 工具集 **+ plan mode**（`enter_plan_mode` / `exit_plan_mode` / `ask_user_question`）。

覆盖优先级（有配置才覆盖）：

1. 当前模型的 `agent_type` 字段（缺省字符串同样是 `grok-build-plan`）
2. 配置 `[agent] name = "…"`（可在 `/agents` 设为默认）
3. 环境变量 `GROK_AGENT`（选择链路用到时）

这与 **`[subagents.toggle]` 无关**：主会话 agent type ≠ Task 子代理启用列表。

#### 默认可见 / 启用（Task / `/agents` 开关）

**注册所有agent**，但默认 **只开启官方那 5 个** 供 **Task spawn** 等使用。扩展类型在 `/agents` 里默认 **关闭**，需用 **`t`** 或配置打开后，Task 才会提供给模型 spawn。以下为**官方内置**的所有agent。

| 名称 | 默认 | 角色 / 实际效用 |
|------|------|-----------------|
| `grok-build` | **开** | 主软件工程 agent（完整 Grok Build 工具集） |
| `general-purpose` | **开** | 多步通用任务（执行/读/改/搜/网页/计划类能力） |
| `explore` | **开** | 快速 **只读** 探代码（仅 `read_file`、`list_dir`、`grep`） |
| `plan` | **开** | **只读** 架构/实现方案（+ todo） |
| `browser-use` | **开** | 网页浏览与交互 |
| `grok-build-concise` | 关 | 同族 **简洁** 工具 I/O（偏 SFT/RL，不注入 AGENTS.md，内部工具） |
| `grok-build-plan` | 关 | 完整工具 **+ plan mode**（`enter_plan_mode` / `exit_plan_mode` / `ask_user_question`） |
| `grok-build-plan-no-subagents` | 关 | plan mode **无** Task spawn（保留 bash 后台辅助） |
| `grok-build-ask-user` | 关 | 完整工具 + **`ask_user_question`**，不进完整 plan mode |
| `codex` | 关 | **Codex** 工具命名 + apply-patch 风格编辑 + Codex 系统提示词 |
| `opencode` | 关 | **OpenCode** 工具约定（read/edit/write/grep/glob/skill） |
| `grok-build-orchestrator` | 关 | 编排型：研究 + Task 栈；**不**本地 search_replace — 委派编码 |

Tips: 通过 grok --agent [已注册agent] 可以切换主对话agent type

eg. grok --agent grok-build-orchestrator

#### Toolset 能力对照

| Agent | Shell | 改文件 | Task/子代理 | Plan mode | 问用户 | 说明 |
|-------|-------|--------|-------------|-----------|--------|------|
| `grok-build` | 有 | 有 | 有 | — | — | 默认实现者 |
| `general-purpose` | 有 | 有 | 按 kind | plan 类 | — | 广谱多步 |
| `explore` | **无** | **无** | 无 | — | — | 工具集强制只读 |
| `plan` | **无** | **无** | 无 | 仅 todo | — | 架构师，只读 |
| `browser-use` | 浏览工具 | — | — | — | — | 网页自动化 |
| `grok-build-concise` | 简洁 bash/读/改 | 有 | 有限 | — | — | 压缩工具载荷 |
| `grok-build-plan` | 有 | 有 | 有 | **有** | **有** | 先计划再写码 |
| `grok-build-plan-no-subagents` | 有 | 有 | **无 Task** | **有** | **有** | 计划但不 spawn |
| `grok-build-ask-user` | 有 | 有 | 有 | — | **有** | 结构化提问 |
| `codex` | 有 | apply_patch | 无 Task | — | — | Codex prompt + 工具 |
| `opencode` | OpenCode bash | OpenCode 编辑/写 | 无 Task | — | — | OpenCode 参数风格 |
| `grok-build-orchestrator` | 调研 bash | **不编辑** | **有** | **有** | **有** | 委派给实现子 agent |

#### 开关方式

`/agents` → 选中行 → **`t`** 切换。写入：

```toml
# ~/.grok/config.toml
[subagents.toggle]
codex = true
opencode = true
grok-build-orchestrator = true
# 也可强制关闭默认开启的五个：
# explore = false
```

---

### Prompt cache key 与 recap 模型

#### `auto_prompt_cache_key`（按模型）

**效用：** 在 **Responses** 后端（`responses` / `openai_responses`）上，每一主轮自动发送 **会话稳定** 的 `prompt_cache_key`。支持 prompt 缓存的提供商可复用对话稳定前缀的 KV → 长会话更低延迟与费用。**默认关闭。** Chat Completions / Messages 会忽略该标志。

**配置**（写在某个 `[model.*]` 上）：

```toml
[model.sol]
api_backend = "openai_responses"
auto_prompt_cache_key = true
```

key 由会话派生（主轮与 recap 使用不同派生，避免缓存互相污染）。

#### Recap 模型（`[models] recap`）

**效用：** 会话 **recap**（`/recap` 与离开再回时的自动 recap）可以跑在比主对话 **更便宜/更小** 的模型上。未设置时跟随 OAuth 偏好的官方内置；设置后以你的覆盖为准。

**配置：**

```toml
[models]
default = "sol"
recap = "cheap-summary"     # 模型 id 或目录键

[model.cheap-summary]
name = "Cheap recap"
model = "fast-small-id"
base_url = "https://gateway.example/v1"
api_backend = "openai_chat_completions"
env_key = "OPENAI_API_KEY"
```

还受功能开关 `[features] session_recap` / 环境变量 `GROK_SESSION_RECAP` 控制（遵循上游默认）。

---

### 用量 Activity、计价、实时 `$`

`/usage` 使用 **官方标签面板**；本 fork 增加第四页 **Activity**。

| 标签 | 内容 |
|------|------|
| Context usage | 上下文窗口分解 |
| Usage limit | 订阅 / 额度 |
| Session info | 会话元数据 |
| **Activity** | 本地热力图、WebDAV、计费模式、实时 `$` |

**计费模式**（`~/.grok/usage/pricing.toml`）：

```toml
mode = "all"                 # off | all | official_only
live_display = true          # 提示行 + 子代理边框上的 $
auto_sync_catalog = true     # 同步时刷新 models.dev
```

| 模式 | 行为 |
|------|------|
| `off` | 不做 USD 归因 |
| `all` | 全部模型（ticks → 自定义 → models.dev → 内置 seed → 占位） |
| `official_only` | 仅 Grok 模型（订阅登录） |

- **`p`**：循环计费模式（会重扫本地）· **`d`**：实时 `$` · **`s`**：强制 WebDAV · **`w`**：日/周热力图  
- 实时 `$` 出现在提示行（`model · $0.12 · flags`）以及 **子代理标题栏**（开启时）  
- Ticks：`1 USD = 10_000_000_000`  
- 自定义单价：`~/.grok/usage/prices/custom.toml`（只增不改写）

**WebDAV**（`~/.grok/usage/sync.toml`）：每设备一份 snapshot，按 **日×模型加总合并**。见 `examples/usage-sync.toml`。

---

### 示例文件

| 文件 | 用途 |
|------|------|
| `examples/config.api-backends.toml` | 宽松 backend + `auto_prompt_cache_key` |
| `examples/config.usage-pricing.toml` | 计费模式 / 实时显示 |
| `examples/prices.custom.toml` | 自定义 $/1M |
| `examples/usage-sync.toml` | WebDAV 同步 |
| `config_example.toml` | 更全的配置目录（npm 包） |

---

Upstream license and notices: `LICENSE`, `THIRD-PARTY-NOTICES`, `SECURITY.md`.
