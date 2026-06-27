# DeepHarness Desktop

> 本地优先的 AI 编码工作台 —— 在桌面端无缝接入 OpenCode、Claude Code、Cursor Agent、Codex 等多种编码智能体，统一管理会话、任务与文件变更。

DeepHarness Desktop（以下简称 **dh-desktop**）是一款基于 React + Tauri 的跨平台桌面应用。它把多个 AI 编码 CLI 包装成统一的会话式工作流，所有数据默认保存在本地 SQLite，无需把代码或对话记录上传到云端。

---

## 目录

- [功能总览](#功能总览)
- [系统要求](#系统要求)
- [安装](#安装)
- [快速上手](#快速上手)
- [使用指南](#使用指南)
  - [登录与注册](#登录与注册)
  - [选择 / 创建智能体](#选择--创建智能体)
  - [工作区布局](#工作区布局)
  - [对话与消息](#对话与消息)
  - [会话与任务管理](#会话与任务管理)
  - [文件变更与 Git](#文件变更与-git)
  - [会话日志](#会话日志)
  - [设置面板](#设置面板)
- [命令行工具：dh CLI 与 dh-gatewayd](#命令行工具dh-cli-与-dh-gatewayd)
  - [组件关系](#组件关系)
  - [dh-gatewayd 守护进程](#dh-gatewayd-守护进程)
    - [AG-UI 协议接口详情](#ag-ui-协议接口详情)
  - [`dh exec` —— 透明拦截编码 CLI](#dh-exec--透明拦截编码-cli)
  - [`dh chat` —— 交互式 REPL](#dh-chat--交互式-repl)
  - [`dh gwd` —— 网关运维与审计](#dh-gwd--网关运维与审计)
  - [`dh mcp` —— MCP 服务器与工具管理](#dh-mcp--mcp-服务器与工具管理)
  - [`dh config` —— 配置与云端同步](#dh-config--配置与云端同步)
  - [典型工作流](#典型工作流)
- [智能体配置](#智能体配置)
- [数据与隐私](#数据与隐私)
- [故障排查](#故障排查)
- [键盘与鼠标](#键盘与鼠标)
- [开发者指南](#开发者指南)
- [架构概览](#架构概览)
- [相关文档](#相关文档)
- [许可证](#许可证)

---

## 功能总览

| 功能 | 说明 |
|------|------|
| 多智能体接入 | 通过插件机制支持 OpenCode、Claude Code，可扩展 Cursor Agent、Codex 与自定义 CLI |
| 多轮对话 | 流式 token 输出、思考过程（thinking）、工具调用、权限请求、待办列表可视化 |
| 会话管理 | 会话历史持久化、按用户隔离、随时切换/重命名/删除 |
| 任务追踪 | 智能体写入的 todo 列表自动同步到右侧任务面板 |
| 文件变更 | 实时显示当前会话产生的文件修改、删除与新增，并提供 diff 摘要 |
| 工作区浏览 | 内置文件树（尊重 `.gitignore`）、Git 状态徽章、文件预览 |
| 会话日志 | 500 条前端缓存 + SQLite 持久化的结构化日志，可在抽屉中检索 |
| 主题切换 | 深色 VS Code 风格界面，5 种主题色可选 |
| 本地数据 | SQLite 单文件存储，所有用户/会话/消息/任务都在本地 |
| 跨平台 | Windows（`.msi`/`.exe`）、macOS（`.dmg`/`.app`）、Linux（`.deb`/`.AppImage`） |

---

## 系统要求

### 运行 dh-desktop

- **操作系统**：Windows 10+、macOS 12+、主流 Linux（Ubuntu 20.04+ / Fedora 36+ 等）
- **WebView**：Windows 自带 WebView2；macOS 使用 WKWebView；Linux 需安装 `webkit2gtk-4.1` 与 `libsoup-3`
- **磁盘**：约 200 MB（含安装包、数据库与日志）
- **内存**：建议 ≥ 4 GB

### 想接入哪个智能体，就需要哪个 CLI

> dh-desktop **不会**自动下载这些工具；它会调用本地 `PATH` 中的命令并管理子进程。

| 智能体 | 需要预装的 CLI | 验证命令 |
|--------|----------------|----------|
| OpenCode | `opencode` | `opencode --version` |
| Claude Code | `claude` | `claude --version` |
| Cursor Agent | `cursor-agent`（实验性） | `cursor-agent --version` |
| Codex | `codex`（实验性） | `codex --version` |
| 自定义 | 任意可执行文件 | 用户自行配置 |

请按各 CLI 官方文档完成安装与登录后再启动 dh-desktop。

---

## 安装

### 方式 1：下载已构建的安装包（推荐）

前往 Release 页面下载对应平台的安装包：

- Windows：`dh-desktop_<version>_x64-setup.exe` 或 `.msi`
- macOS：`dh-desktop_<version>_x64.dmg` / `dh-desktop_<version>_aarch64.dmg`
- Linux：`dh-desktop_<version>_amd64.deb` 或 `dh-desktop_<version>_amd64.AppImage`

> macOS 首次打开如出现"无法验证开发者"，请在「系统设置 → 隐私与安全性」中点击「仍要打开」。

### 方式 2：从源码构建

需要 Node.js ≥ 20、pnpm 与 Rust ≥ 1.70：

```bash
git clone <repo-url>
cd deepharness-ent-desktop
pnpm install
pnpm tauri-build
```

构建产物位于 `target/release/`：

- Linux 可执行文件：`target/release/dh-desktop`
- 安装包：`target/release/bundle/<deb|appimage|...>/`

### Linux / WSL2 启动脚本

在无 GPU 或 WebKit 渲染异常的环境（如 WSL2、远程虚拟机）下，请使用项目根目录的 `run-desktop.sh`：

```bash
bash run-desktop.sh
```

脚本设置了软件渲染相关环境变量，可显著提升兼容性。

---

## 快速上手

1. **启动应用**：双击安装后的图标，或在源码目录执行 `bash run-desktop.sh`。
2. **登录**：首次进入显示登录页。可任意填写用户名/密码完成模拟登录，或点击 **访客登录**。
3. **选择智能体**：选择 OpenCode 或 Claude Code 等已安装好的 CLI，点击 **进入工作区**。
4. **绑定工作目录**：在弹出的对话框中选择想让智能体访问的本地代码目录。
5. **开始对话**：在中间面板输入需求，例如"帮我把 `Button` 组件抽出 `variant` 属性"。
6. **查看产出**：右侧面板展示新增的任务与文件变更；点击文件可查看 diff。

> 第一次启动某个智能体时，dh-desktop 会拉起对应的子进程，可能需要数秒。

---

## 使用指南

### 登录与注册

- 登录页支持用户名 + 密码（用户名会被自动拼接为 `<username>@local.dev`）。
- **首次注册**：点击切换到注册标签输入即可创建本地账号；账号信息存储在本地 SQLite。
- **访客登录**：使用 `guest` 账号一键进入工作区，适合临时试用。
- 所有账号、会话与日志都按 `user_id` 隔离，多用户互不可见。

### 选择 / 创建智能体

进入"选择智能体"页面后，可以执行以下操作：

- **快速选择**：点击预置卡片（OpenCode、Claude Code、Cursor Agent、Codex）。
- **新建实例**：点击 **+ 添加智能体** 自定义名称、CLI 程序、工作目录与默认模型。
- **删除实例**：在卡片右上角的菜单中选择"删除"；删除仅移除前端记录，不会清理 CLI 自带的会话数据。

智能体实例信息保存在浏览器 `localStorage.agent_instances` 与 SQLite，下次打开会自动恢复。

### 工作区布局

工作区采用三栏式布局：

```
┌────────────┬───────────────────────────┬──────────────┐
│  LeftPanel │        ChatPanel          │  RightPanel  │
│            │                           │              │
│ • 会话列表 │   • 消息流（流式渲染）    │ • 任务列表   │
│ • 智能体   │   • 输入框 + 模型切换     │ • 文件变更   │
│ • 文件树   │   • 思考/工具/权限气泡    │ • 工作区 Git │
└────────────┴───────────────────────────┴──────────────┘
```

- **LeftPanel**：上方为智能体实例与会话列表，下方为工作区文件树。点击文件可在中间面板预览。
- **ChatPanel**：核心对话区域。顶部显示当前会话标题、模型与连接状态；底部为输入框与发送按钮。
- **RightPanel**：右侧的任务、文件变更与 Git 状态会随对话实时更新。

每个面板的宽度可拖拽调整，状态会被记忆。

### 对话与消息

- **发送消息**：在输入框中键入内容后按 `Ctrl+Enter`（macOS 为 `Cmd+Enter`）或点击发送按钮。
- **多行输入**：直接 `Enter` 换行，无需特殊操作。
- **消息类型**：
  - 普通文本：流式 token 输出。
  - 思考（thinking）：折叠展示，可点击展开。
  - 工具调用：显示工具名、参数与执行结果。
  - 权限请求：智能体请求访问外部命令/文件时，会弹出确认气泡，需手动批准或拒绝。
  - Todo 列表：智能体写入的待办会同步到右侧任务面板，并在消息中以勾选项显示。
- **中断生成**：点击发送按钮位置出现的 **■ 停止** 可中断当前回复。
- **重发 / 编辑**：长按或右键消息可看到上下文菜单（视实现而定）。

### 会话与任务管理

- **新建会话**：在 LeftPanel 顶部点击 **+ 新会话**，会创建一条空白对话。
- **切换会话**：单击会话条目即切换；切换时会重新加载对应的消息历史。
- **重命名 / 删除**：通过会话右侧的菜单按钮操作。
- **任务面板**：智能体每次写入 `TodoWrite` 时，右侧任务列表会更新；勾选状态会同步到对话中。

### 文件变更与 Git

- **变更列表**：右侧面板的 **变更** 标签会列出当前会话产生的所有文件改动，按 `Modified / Added / Deleted` 分组。
- **查看 diff**：点击文件项即可在中间面板加载 diff 视图（支持 unified diff 摘要）。
- **Git 状态**：文件树中的图标会标识 `M`（已修改）、`A`（新增）、`?`（未跟踪），与本机 `git status` 对齐。
- **大文件保护**：超过 512 KB 的文件会被自动截断显示；图片会以 base64 形式预览。

### 会话日志

- 顶部工具栏的 **日志** 按钮会打开 SessionLogDrawer。
- 日志包含 INFO / WARN / ERROR 级别，每条会带时间戳与来源（前端、Tauri 命令、Agent 进程）。
- 前端缓存最近 500 条；持久化日志存储在 SQLite，可在排查问题时翻阅历史。

### 设置面板

点击右上角齿轮图标打开 **设置**，包含以下选项：

- **主题色**：5 种内置颜色（blue / green / orange / purple / pink）。
- **窗口**：是否开启紧凑模式、字体大小（部分版本支持）。
- **智能体**：默认模型、每个会话的最大上下文长度等。
- **MCP 服务器**：配置 Model Context Protocol 服务器列表（高级）。
- **提示词 / 技能 / 工程规范**：可编辑全局或项目级提示词与技能定义。
- **关于**：版本号、数据目录路径、检查更新（如启用）。

设置变更会立即生效，并写入 `localStorage` 或对应的本地配置文件。

---

## 命令行工具：dh CLI 与 dh-gatewayd

除了图形化的桌面端，本仓库还提供两个独立的命令行二进制：

- **`dh`** —— 多功能 CLI（`apps/cli`），子命令包括 `chat`、`exec`、`gwd`、`mcp`、`config`。
- **`dh-gatewayd`** —— 本地 LLM 网关守护进程（`apps/gatewayd`），负责模型代理、审计日志、Agent 生命周期、MCP 聚合。

两者可独立于桌面端使用，特别适合 CI、远程服务器、终端工作流等无 GUI 场景。

### 组件关系

```
┌─────────────────┐    spawn / signal    ┌───────────────────────┐
│   dh (CLI)      │ ───────────────────► │   dh-gatewayd         │
│                 │                       │                       │
│ chat / exec     │   HTTP (admin port)   │  /agents              │
│ gwd / mcp       │ ◄───────────────────► │  /mcp/*               │
│ config          │                       │  /context  /health    │
└─────────────────┘                       │                       │
        │                                  │  /v1/chat/completions │
        │ 透明拦截 (env + 配置改写)        │  /v1/messages         │
        ▼                                  │   ↑                   │
┌─────────────────┐                       │   │ OpenAI/Anthropic  │
│ 编码 CLI        │ ─── HTTP base_url ──► │   │ 兼容              │
│ claude/opencode │                       └───┴───────────────────┘
└─────────────────┘                              │
                                                  ▼
                                       ┌───────────────────────┐
                                       │   gatewayd.db (SQLite)│
                                       │  audit_logs / sessions│
                                       │  configs / mcp_servers│
                                       └───────────────────────┘
```

- **API 端口**（默认 `2345`）：暴露 OpenAI / Anthropic 兼容接口给被拦截的 CLI。
- **Admin 端口**（默认 `2346`，即 API 端口 + 1）：暴露管理接口（健康检查、Agent CRUD、MCP 聚合、上下文注入、审计 SSE）。
- **数据目录**：`gatewayd.db`、PID 锁文件均位于 `dh_platform::fs::ensure_data_dir()` 返回的目录（即上文「数据存储位置」表格中的路径）。

### dh-gatewayd 守护进程

`dh-gatewayd` 既能由 `dh exec` / `dh gwd start` 自动拉起，也可以手动运行：

```bash
# 前台运行（默认端口 2345 / 2346）
dh-gatewayd

# 自定义端口
dh-gatewayd --port 3345 --admin-port 3346

# 守护模式（后台运行，写入 PID 锁文件）
dh-gatewayd --daemon

# 启动时自动挂载若干 Agent 插件
dh-gatewayd --daemon --attach opencode --attach claude
# 或使用 --agent-type 简写
dh-gatewayd --daemon --agent-type opencode
```

启动后会暴露下列端点：

| 路径 | 端口 | 说明 |
|------|------|------|
| `POST /v1/chat/completions` | API | OpenAI 兼容入口（被透明拦截的请求会走这里） |
| `POST /v1/messages` | API | Anthropic 兼容入口 |
| `GET /health` | Admin | 健康检查（返回 `version` 等元信息） |
| `POST /context` | Admin | 由 `dh exec` 注入 `agent_type / session_id / workspace / model` |
| `POST /sessions` | Admin | 创建 AG-UI session，返回 `sessionId`（同时作为 AG-UI `threadId`） |
| `POST /sessions/{sessionId}/agents` | Admin | 在指定 session 下创建 opencode / claude-code 实例 |
| `GET /sessions/{sessionId}/events` | Admin | WebSocket 实时事件流，双向收发 AG-UI 事件 |
| `POST /sessions/{sessionId}/runs` | Admin | HTTP POST + SSE，启动一次 run 并流式返回 AG-UI 事件 |
| `GET /mcp/servers`、`GET /mcp/tools`、`POST /mcp/tools/{name}/call` | Admin | MCP 聚合器（仅在配置了 MCP server 时启用） |
| `GET /admin/reporter/status` | Admin | 远程上报器状态 |

> **注意**：旧版 Agent 接口（`/agents`、`/agents/{id}/message`、`/agents/events`）已废弃，请使用以 `/sessions` 为入口的 AG-UI 接口。

#### AG-UI 协议接口详情

dh-gatewayd 的 Agent 对外交互已统一为 [AG-UI](https://docs.ag-ui.com/introduction) 事件协议。所有事件均为 JSON，通过 `type` 字段区分，采用 `SCREAMING_SNAKE_CASE` 命名。

**1. 创建 Session**

```bash
curl -X POST http://127.0.0.1:2346/sessions
```

返回：

```json
{
  "sessionId": "550e8400-e29b-41d4-a716-446655440000"
}
```

`sessionId` 同时作为 AG-UI 的 `threadId`。

**2. 挂载 Agent 实例**

```bash
curl -X POST http://127.0.0.1:2346/sessions/{sessionId}/agents \
  -H "Content-Type: application/json" \
  -d '{
    "plugin_key": "opencode",
    "name": "my-opencode",
    "workspace": "/path/to/project"
  }'
```

支持 `plugin_key`: `opencode`、`claude-code`。一个 session 当前仅支持挂载一个实例。

**3. WebUI 信道：双向 WebSocket**

```bash
websocat ws://127.0.0.1:2346/sessions/{sessionId}/events
```

连接建立后，客户端发送 AG-UI `RunAgentInput`：

```json
{
  "threadId": "550e8400-e29b-41d4-a716-446655440000",
  "messages": [
    { "role": "user", "content": "帮我重构 Button 组件" }
  ],
  "state": {},
  "tools": [],
  "context": [],
  "forwardedProps": {}
}
```

服务端持续推送 AG-UI 事件，例如：

```json
{ "type": "RUN_STARTED", "threadId": "...", "runId": "..." }
{ "type": "TEXT_MESSAGE_START", "messageId": "msg-1", "role": "assistant" }
{ "type": "TEXT_MESSAGE_CONTENT", "messageId": "msg-1", "delta": "好的" }
{ "type": "TEXT_MESSAGE_END", "messageId": "msg-1" }
{ "type": "RUN_FINISHED", "threadId": "...", "runId": "..." }
```

**4. HTTP POST + SSE 信道**

```bash
curl -X POST http://127.0.0.1:2346/sessions/{sessionId}/runs \
  -H "Content-Type: application/json" \
  -H "Accept: text/event-stream" \
  -d '{
    "threadId": "550e8400-e29b-41d4-a716-446655440000",
    "messages": [
      { "role": "user", "content": "帮我重构 Button 组件" }
    ]
  }'
```

响应为 SSE 流，每行一个 AG-UI 事件：

```
data: {"type":"RUN_STARTED","threadId":"...","runId":"..."}

data: {"type":"TEXT_MESSAGE_START","messageId":"...","role":"assistant"}

data: {"type":"TEXT_MESSAGE_CONTENT","messageId":"...","delta":"好的"}

data: {"type":"TEXT_MESSAGE_END","messageId":"..."}

data: {"type":"RUN_FINISHED","threadId":"...","runId":"..."}
```

**5. 主要事件类型**

| 事件类型 | 说明 |
|----------|------|
| `RUN_STARTED` / `RUN_FINISHED` / `RUN_ERROR` | run 生命周期 |
| `TEXT_MESSAGE_START` / `TEXT_MESSAGE_CONTENT` / `TEXT_MESSAGE_END` |  assistant 文本消息流 |
| `THINKING_TEXT_MESSAGE_*` | 思考过程 |
| `TOOL_CALL_START` / `TOOL_CALL_ARGS` / `TOOL_CALL_END` / `TOOL_CALL_RESULT` | 工具调用 |
| `STATE_SNAPSHOT` | 共享状态快照（MVP 回显输入 state） |
| `CUSTOM` | permission / question / todowrite 等扩展事件 |
| `RAW` | session.log 等原始事件 |

> ⚠️ 守护进程使用 PID 锁文件防止重复启动。如果 `dh gwd status` 报告"lock 文件存在但进程已死"，请先执行 `dh gwd stop` 清理。

### `dh exec` —— 透明拦截编码 CLI

最常见的入口，用于把任意编码 CLI 的 LLM 请求路由到 `dh-gatewayd` 进行审计与统计：

```bash
# 透明启动 claude，所有请求会走本地网关
dh exec claude

# 给被拦截的 CLI 传额外参数（用 -- 分隔以避免歧义）
dh exec opencode -- --model deepseek-coder

# 同时支持任意自定义 agent
dh exec aider -- --4-turbo
```

`dh exec` 会自动完成以下步骤：

1. 生成持久化的 `DEEPHARNESS_SESSION_ID`，用于审计串联。
2. 检查/重启 `dh-gatewayd`（确保拿到最新 API key）。
3. 通过 `POST /context` 把 `agent_type / session_id / workspace / model` 注入网关。
4. 注入环境变量（`OPENAI_BASE_URL`、`ANTHROPIC_BASE_URL` 等）使下游 CLI 走本地网关。
5. 改写下游 CLI 的配置（如 `~/.config/opencode/opencode.json`）的 `base_url`，进程退出时自动还原。
6. 拉起目标进程并等待退出。

### `dh chat` —— 交互式 REPL

直接与挂载在 `dh-gatewayd` 上的 Agent 对话，无需打开桌面端：

```bash
# 启动 REPL（必须使用 --interactive）
dh chat opencode --interactive

# 也可以是 claude 或任何已注册的插件
dh chat claude --interactive
```

REPL 行为：

- 自动查找 `dh-gatewayd` 的 admin 端口（默认扫描 `2346-2350`）。
- 通过 `POST /sessions` 创建 AG-UI session，获取 `sessionId`。
- 通过 `POST /sessions/{sessionId}/agents` 在 session 下创建指定 plugin 的 agent 实例。
- 通过 `ws://.../sessions/{sessionId}/events` 订阅 AG-UI 事件。
- 输入消息后按 `Enter` 发送，消息会被包装为 AG-UI `RunAgentInput` 推送到 WebSocket。
- 渲染 AG-UI 事件：`RUN_STARTED/FINISHED`、`TEXT_MESSAGE_*`、`THINKING_TEXT_MESSAGE_*`、`TOOL_CALL_*`、`CUSTOM`（permission/question/todowrite）等。
- 输入 `/quit` 或 `/exit` 退出。

### `dh gwd` —— 网关运维与审计

```bash
# 启动 / 停止 / 状态
dh gwd start                           # 前台
dh gwd start --daemon                  # 后台
dh gwd start --daemon opencode claude  # 后台并预挂载 agent
dh gwd stop
dh gwd status                          # 查看 PID、admin 端口、已挂载 agent

# 在已运行的网关上挂载新 agent
dh gwd --attach opencode --attach claude

# 审计日志（按时间倒序）
dh gwd logs                            # 最近 50 条
dh gwd logs --limit 200
dh gwd logs --session-id <uuid>

# 单条会话/请求详情
dh gwd session <session_id|prefix|-1>  # -1 = 最近一次会话
dh gwd request <request_id|prefix|-1>  # 含 payload、token、metadata

# Token 用量统计（默认表格输出）
dh gwd stats
dh gwd stats --provider openai --since 2026-06-01
dh gwd stats --session-id <uuid> --json
```

`stats` 支持的过滤器：`--session-id`、`--since`（ISO 时间）、`--provider`、`--model`、`--json`。

### `dh mcp` —— MCP 服务器与工具管理

```bash
# 列出所有 MCP 服务器（优先调用 gatewayd /mcp/servers，否则回落到 DB）
dh mcp list

# 添加一个本地 MCP 服务器
dh mcp add filesystem \
  --cmd npx \
  --args "@modelcontextprotocol/server-filesystem,--,/path/to/dir" \
  --env "FOO=bar,BAZ=qux"

# 移除
dh mcp remove filesystem

# 调用具体工具（namespace:tool_name）
dh mcp call filesystem:read_file --args '{"path":"/etc/hosts"}'
```

> 修改 MCP 配置后需要重启 `dh-gatewayd` 才能生效（`dh gwd stop && dh gwd start --daemon`）。

### `dh config` —— 配置与云端同步

`dh config` 写入的键值对存储在 `gatewayd.db.configs` 表中，可在多个终端共享。

```bash
# 设置远端基地址（用于规则与技能同步）
dh config set remote-url https://example.com

# 自定义刷新周期（秒）
dh config set refresh-time 300

# 查询 / 列出
dh config get remote-url
dh config list

# 从云端拉取规则与技能（写回本地 DB 的 rules_data / skills_data）
dh config refresh rules
dh config refresh skills
```

### 典型工作流

#### 场景 1：把现有 Claude Code / OpenCode 请求纳入审计

```bash
# 一次性命令：dh exec 自带 gatewayd 启停与配置回滚
dh exec claude

# 用量复盘
dh gwd stats --session-id "$DEEPHARNESS_SESSION_ID"
dh gwd session -1
```

#### 场景 2：纯终端 REPL（无桌面端）

```bash
dh gwd start --daemon --attach opencode
dh chat opencode --interactive
# 退出后查看历史
dh gwd logs --limit 100
```

#### 场景 3：MCP 工具集成

```bash
dh mcp add fs --cmd npx --args "@modelcontextprotocol/server-filesystem,--,$HOME/code"
dh gwd stop && dh gwd start --daemon
dh mcp list
dh mcp call fs:read_file --args '{"path":"README.md"}'
```

---

## 智能体配置

dh-desktop 不内置 CLI，请按以下步骤准备好对应的智能体：

### OpenCode

```bash
# 参考 https://opencode.ai/docs
npm i -g @opencode/cli   # 或对应安装方式
opencode auth login
opencode --version
```

dh-desktop 会自动以 `opencode serve` 模式拉起后端，端口在 `3001-3050` 之间随机选择。

### Claude Code

```bash
# 参考 Anthropic 官方文档
npm i -g @anthropic-ai/claude-code
claude login
claude --version
```

dh-desktop 通过 stdio 与 `claude` 进程通信，支持 stream-json 输出协议。

### Cursor Agent / Codex（实验性）

请关注对应官方文档进行安装。当前实现提供基础的进程拉起能力，事件解析仍在迭代中。

### 自定义智能体

在 **添加智能体** 对话框中：

1. 选择"自定义"类型。
2. 填写可执行命令、参数、工作目录与环境变量。
3. 选择 stdio 或 HTTP 传输。
4. 保存后即可在工作区中选用。

---

## 数据与隐私

### 数据存储位置

| 平台 | 路径 |
|------|------|
| Windows | `%APPDATA%\com.deepharness.dh-desktop\` |
| macOS | `~/Library/Application Support/com.deepharness.dh-desktop/` |
| Linux | `~/.local/share/com.deepharness.dh-desktop/` |

主要文件：

- `app.db` —— 用户、会话、消息、任务、文件变更、日志
- `agents/<instance_id>/data.db` —— 每个智能体实例的独立数据库
- `session.log` —— 应用级日志

### 隐私保证

- 所有对话内容、代码片段、Git diff 都仅在本地 SQLite 中保存。
- dh-desktop 自身不会向任何远端服务器上传数据。
- 对接的智能体 CLI（OpenCode、Claude Code 等）会按其各自策略向其后端 API 发送请求；请阅读对应服务条款。
- Tauri CSP 仅允许 `self`、本地 IPC、`https:` 与 `localhost:*` 资源加载。

### 备份与迁移

直接拷贝上文数据目录即可完成完整备份；恢复时把目录放回新设备的相同路径下重启应用。

---

## 故障排查

| 现象 | 排查建议 |
|------|----------|
| 应用启动黑屏（Linux/WSL2） | 改用 `bash run-desktop.sh` 启动；确认已安装 `webkit2gtk-4.1`。 |
| 登录后立即退出 | 删除数据目录下的 `app.db` 并重启；可能因旧 schema 不兼容。 |
| 智能体一直显示 "未运行" | 在终端运行 `<cli> --version` 确认 PATH 中可执行；查看会话日志中的错误堆栈。 |
| 端口冲突 | OpenCode 默认尝试 `3001-3050`，请关闭占用端口的进程；Claude Code 走 stdio 不占端口。 |
| 消息卡住没有流式输出 | 顶部状态条若显示 "WebSocket 未连接"，重启应用即可；若仍异常，请查看日志中的 `gateway` 相关错误。 |
| 文件树为空 | 确认在 **添加智能体** 时绑定了正确的工作目录；隐藏文件默认遵循 `.gitignore`。 |
| 数据想完全清空 | 退出应用后删除上文数据目录，再次启动会重新初始化。 |

定位不到原因时，请打开会话日志抽屉并把相关条目反馈到 `docs/bugs/`，命名格式 `YYYY-MM-DD-<brief-description>.md`，包含「现象 / 根因 / 解决方案」三部分。

---

## 键盘与鼠标

| 操作 | 快捷键 |
|------|--------|
| 发送消息 | `Ctrl+Enter` / `Cmd+Enter` |
| 换行 | `Enter` |
| 新建会话 | LeftPanel **+ 新会话** 按钮 |
| 切换面板宽度 | 拖拽分隔线 |
| 打开会话日志 | 顶部工具栏 **日志** 按钮 |
| 打开设置 | 右上角齿轮图标 |
| 退出当前账号 | 设置 → 关于 → 退出登录 |

---

## 开发者指南

### 项目脚本

```bash
pnpm install               # 安装前端依赖

pnpm dev                   # 启动 Vite 开发服务器（127.0.0.1:5173）
pnpm tauri-dev             # 启动 Tauri 开发模式

pnpm build                 # tsc --noEmit + vite build
pnpm tauri-build           # 打包桌面端

pnpm lint                  # tsgo + biome lint + ast-grep + tailwind 语法检查 + 构建冒烟
pnpm test                  # vitest run
pnpm test:watch            # vitest watch
```

### 目录速览

```
deepharness-ent-desktop/
├── src/                  # React 前端
│   ├── pages/            # 路由级页面：Login / SelectAgent / Workspace
│   ├── components/       # ui/ + workspace/ + common/
│   ├── stores/           # Zustand：chat / agent / websocket / log
│   ├── db/               # IDataStore 适配层（Mock / Tauri）
│   ├── services/         # 前端服务（logger、debug-logger）
│   └── types/            # 业务类型
├── src-tauri/            # Tauri 应用（Rust）
│   ├── src/
│   │   ├── main.rs       # 入口：DB 初始化、网关、插件注册
│   │   ├── commands/     # Tauri Invoke 命令
│   │   ├── gateway/      # WebSocket JSON-RPC 网关
│   │   ├── service/      # AgentService 等业务服务
│   │   └── setup/        # DB / 窗口初始化
│   └── tauri.conf.json
├── crates/               # 共享 Rust crate（被 src-tauri 与 apps 共用）
│   ├── agent-core/       # AgentPlugin / AgentInstance / SessionLogger / MCP
│   ├── opencode-plugin/  # OpenCode CLI 适配器（HTTP + SSE）
│   ├── claude-plugin/    # Claude Code 适配器（stdio + stream-json）
│   ├── dh-core/          # 共享领域模型
│   ├── dh-db/            # 数据库连接、迁移、Repository
│   └── dh-platform/      # 平台抽象（通知、IPC、文件系统）
├── apps/
│   ├── cli/              # 命令行工具 dh
│   └── gatewayd/         # 独立网关服务 dh-gatewayd
├── .rules/               # ast-grep 自定义 lint 规则
├── docs/                 # PRD、设计文档、bug 记录
├── DESIGN.md             # UI/UX 设计规范
├── AGENTS.md             # AI 编程助手指南（包含硬性规则）
└── run-desktop.sh        # 兼容无 GPU 环境的启动脚本
```

### 常见开发任务

- **新增页面**：在 `src/pages/` 创建组件 → 在 `src/routes.tsx` 注册路由。
- **新增 UI 组件**：优先 `npx shadcn add <component>`；业务组件放在 `src/components/<domain>/`。
- **修改数据库 schema**：同步更新 `src-tauri/src/setup/db.rs` 与 `crates/dh-db/src/schema.rs`，并在 `dh-db` 中编写迁移。
- **新增 Agent 插件**：
  1. 在 `crates/` 下创建新 crate。
  2. 实现 `agent_core::plugin::AgentPlugin` 与 `agent_core::instance::AgentInstance`。
  3. 在 `src-tauri/src/main.rs` 通过 `agent_service.register_plugin(...)` 注册。
  4. 在前端 `SelectAgentPage` / `AddAgentDialog` 暴露选项。
- **新增主题色**：同时修改 `src/App.tsx` 的 `themeColorMap` 与 `SettingsDialog.tsx` 中的选项。

### 编码规范要点

- TypeScript 严格模式（`strict`、`noUnusedLocals`、`noUnusedParameters`）。
- Biome 仅用作 linter（项目不使用 Biome formatter）。
- `.rules/*.yml` 中的 ast-grep 规则必须通过：
  - 按钮必须有 `onClick` / `type` / `asChild` / 或被 Trigger 包裹。
  - `Button variant="outline"` 禁止 `text-foreground`，`SelectItem.value` 禁止空字符串。
  - 禁止 `@/hooks/use-toast`，统一使用 `sonner`。
- 单文件有效代码 ≤ 600 行；嵌套不超过 3 层；禁止魔法值（必须抽常量）。
- 编译警告必须清零：`cargo check --workspace` 与 `npx tsc --noEmit -p tsconfig.check.json` 均要保持 0 warning / error。
- 详细约束见 [AGENTS.md](./AGENTS.md)。

### 测试

- 前端：Vitest + jsdom + @testing-library/react，配置见 `vitest.config.ts`，初始化在 `src/test/setup.ts`（Mock `__TAURI_INTERNALS__`）。
- 后端：使用 `cargo test --workspace`；核心 crate（`agent-core`、`opencode-plugin`）已包含解析与会话映射的单元测试。

---

## 架构概览

```
┌────────────────────────────────────────────────────────────┐
│                    React SPA (前端)                         │
│  Pages ──► Components ──► Zustand Stores                   │
│                              │                              │
│                              ▼                              │
│                    src/db (IDataStore 适配)                 │
│                ┌──────────────┬──────────────┐              │
│                │   Mock       │   Tauri      │              │
│                │ localStorage │   invoke     │              │
└────────────────┴──────────────┴──────┬───────┴──────────────┘
                                       │
                       Tauri Invoke  │  WebSocket JSON-RPC
                                       ▼
┌────────────────────────────────────────────────────────────┐
│                    Tauri (Rust)                             │
│  Commands ──► Service Layer ──► Shared crates               │
│   • db        • AgentService     • agent-core               │
│   • workspace • PluginRegistry   • opencode-plugin          │
│   • git       • InstanceRegistry • claude-plugin            │
│   • system    • DbService        • dh-db                    │
│                                                              │
│  Gateway: tokio-tungstenite WebSocket + JSON-RPC router     │
│   • agent.*   生命周期、消息、流式事件                       │
│   • session.* 会话管理                                      │
│   • db.*      数据库操作                                    │
└──────────────────────────────┬──────────────────────────────┘
                               ▼
                  ┌──────────────────────┐
                  │       SQLite         │
                  │ app.db / agents/*.db │
                  └──────────────────────┘
```

关键设计：

- **双通道通信**：Tauri Invoke 处理请求-响应类操作（DB / FS / Git），WebSocket JSON-RPC 处理流式异步事件。
- **Agent 插件抽象**：`AgentPlugin::create_instance` 返回 `Box<dyn AgentInstance>`，统一 `start`/`send_message`/`respond`/`stop` 接口，`DynEventSink` 把事件回传到前端。
- **共享 service 层**：`agent_core::service` 提供 `AgentService`、`PluginRegistry`、`InstanceRegistry`，`src-tauri` 与 `apps/gatewayd` 共用同一份实现。
- **会话映射**：`agent_core::session_map::ConversationSessionMap` 维护 `conversation_id ↔ agent session_id` 双向映射，确保事件能路由回正确的对话。
- **本地数据所有权**：主库按用户隔离会话；每个智能体实例可拥有独立 DB 用于工具状态、向量缓存等。

---

## 相关文档

- [DESIGN.md](./DESIGN.md) —— UI/UX 设计规范（色彩、字体、间距、组件）
- [AGENTS.md](./AGENTS.md) —— AI 编程助手开发指南与硬性规则
- [docs/prd.md](./docs/prd.md) —— 产品需求文档
- [docs/bugs/](./docs/bugs/) —— 缺陷记录（按日期归档）
- [TODO.md](./TODO.md) —— 当前待办事项
- [REVIEW.md](./REVIEW.md) —— 代码审查记录

---

## 许可证

详见 [LICENSE](./LICENSE)。
