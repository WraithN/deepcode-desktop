# DeepHarness LLM Gateway Daemon 设计文档

> **日期**: 2026-06-09  
> **主题**: 系统常驻 Daemon 进程设计与多端一体化架构  
> **状态**: 设计中

---

## 1. 需求概述

### 1.1 背景

DeepHarness Desktop 是一个 React + Tauri 的桌面应用，提供 AI 辅助编码的图形界面。但对于习惯使用原生 coding agent（OpenCode、Claude Code、Aider、Codex CLI 等）的开发者，强制使用 GUI 是一种负担。他们希望保持现有的终端工作流，同时享受 DeepHarness 的管控能力。

### 1.2 目标

构建一个**系统常驻的 LLM Gateway Daemon** (`deepharness-gatewayd`)，作为 coding agent 与 LLM 之间的透明中间层，实现：

- **无侵入**：用户继续使用熟悉的 CLI 工具，无需修改工作习惯
- **全管控**：审计、拦截、策略、优化全覆盖
- **多端一体**：Daemon 独立运行，Desktop GUI 和系统托盘程序作为客户端连接
- **企业级**：支持云端管理控制台下发配置，多租户场景

### 1.3 功能需求

| 需求 | 优先级 | 说明 |
|------|--------|------|
| LLM API 拦截 | P0 | 兼容 OpenAI / Anthropic API 格式，拦截所有基于 HTTP 的 LLM 请求 |
| 命令包装器 | P0 | `deepharness exec <agent>` 自动注入环境变量和代理配置 |
| 审计日志 | P0 | 完整记录请求/响应内容，存储本地 SQLite |
| 策略引擎 | P0 | 可配置的规则匹配与动作执行（放行/拦截/转换/审批） |
| 实时审批 | P0 | 高风险操作阻塞等待用户决策，支持系统通知和终端内联交互 |
| RTK Token 优化 | P0 | Rust Token Killer，上下文压缩、去重、摘要 |
| MCP 聚合 | P0 | Daemon 作为聚合 MCP Server，统一暴露工具集，拦截远程请求 |
| CLI Skill 代理拦截 | P1 | HTTP_PROXY 环境变量拦截，覆盖非 MCP 的 CLI 远端请求 |
| 配置中心 | P0 | 云端管理控制台下发配置，定期轮询，本地缓存 |
| 规范注入 | P0 | 自动将 agents.md、design.md 等工程规范注入 LLM 请求 |
| Skill 下发 | P1 | MCP Server 配置和自定义 Skill 模板统一管理 |
| 异步上报 | P1 | 会话数据异步上报云端，支持企业级审计分析 |
| 跨平台 | P0 | macOS / Linux / Windows 全平台支持 |
| 系统托盘 | P1 | 轻量级托盘程序，接收通知和审批交互 |

### 1.4 非功能需求

- **启动时间**: Daemon 冷启动 < 2s
- **请求延迟**: 无策略匹配时，增加延迟 < 10ms
- **RTK 压缩**: 大上下文场景减少 30%+ token
- **可用性**: Daemon 崩溃不影响正在运行的 agent（已建立的 HTTP 连接继续），但新请求失败
- **数据安全**: 本地 SQLite 加密存储敏感配置，审计日志支持脱敏

---

## 2. 架构设计

### 2.1 架构选型

**选型: 一体化 Rust Daemon（方案1）**

理由：
1. 当前需求复杂度适合单体架构：记录审计 + 策略过滤 + RTK + 审批 + MCP 聚合，模块间数据耦合度高
2. 终端用户容忍度：开发者希望 `deepharness exec claude` 即工作，不愿管理多个进程
3. 混合模式需要：Desktop 需能临时嵌入启动 Daemon，单进程最易实现
4. 扩展性：先写清楚模块边界，未来可按需拆分为 gatewayd/policyd/notifyd

### 2.2 进程模型

```
┌──────────────────────────────────────────────────────────────────────┐
│                          用户终端 / Desktop                           │
│                                                                      │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────────────┐ │
│  │ Terminal    │  │  Desktop    │  │  System Tray App            │ │
│  │ (deepharness│  │  (Tauri GUI)│  │  (Mini Tauri / native)      │ │
│  │  exec ...)  │  │             │  │                             │ │
│  └──────┬──────┘  └──────┬──────┘  └──────────────┬──────────────┘ │
│         │                │                        │                │
│         │  包装器注入     │  IPC (Unix Socket /   │  IPC           │
│         │  环境变量       │   Named Pipe / HTTP)   │                │
│         └────────────────┼────────────────────────┘                │
│                          │                                         │
│              ┌───────────▼────────────┐                            │
│              │  deepharness-gatewayd  │                            │
│              │  (系统常驻 Daemon)       │                            │
│              └───────────┬────────────┘                            │
│                          │                                         │
│    ┌─────────────────────┼─────────────────────┐                   │
│    │                     │                     │                   │
│ ┌──▼──┐            ┌─────▼──────┐       ┌────▼─────┐            │
│ │MCP  │◄──────────►│ 策略引擎    │◄─────►│ 审计日志  │            │
│ │聚合层│            │ + RTK      │       │ SQLite   │            │
│ └─────┘            └─────┬──────┘       └──────────┘            │
│                          │                                        │
│    ┌─────────────────────┼─────────────────────┐                  │
│    │                     │                     │                  │
│ ┌──▼────────┐    ┌──────▼──────┐    ┌────────▼──────┐           │
│ │ HTTP 代理  │    │ 兼容 API    │    │ 通知/审批     │           │
│ │ (CLI技能   │    │ 端点        │    │ 协调器       │           │
│ │  拦截)     │    │(OpenAI/    │    │              │           │
│ │             │    │ Anthropic) │    │              │           │
│ └─────────────┘    └─────────────┘    └──────────────┘           │
│                                                                    │
│  外部网络                                                           │
│    ◄──► 真实 LLM API (OpenAI / Anthropic / ...)                    │
│    ◄──► 外部服务 (GitHub / Slack / Docker Hub ...)                  │
│    ◄──► 云端管理控制台 (admin.deepharness.io)                       │
└──────────────────────────────────────────────────────────────────────┘
```

### 2.3 混合模式启动逻辑

```
Desktop 启动时：
  1. 检查 lock file (~/.deepharness/gatewayd.lock) 是否存在
  2. 如果存在 → 读取 PID，发送 health-check ping (HTTP GET /health)
  3. 如果响应成功 → 复用现有 daemon，建立 IPC 连接
  4. 如果无响应 → 清理 lock file，启动新的 daemon 子进程
  5. Desktop 关闭时：检测 daemon 是否由自己启动的，如果是 → 可选地停止它

纯终端用户：
  1. 首次运行 `deepharness exec <agent>` → 自动检测 daemon，未运行则拉起
  2. daemon 在后台持续运行，即使终端关闭也不退出
  3. 用户可显式运行 `deepharness gatewayd start --daemon` 常驻
```

---

## 3. 跨平台兼容性

Daemon 核心使用 Rust + tokio，天然跨平台。平台差异化部分做统一抽象：

| 功能 | macOS | Linux | Windows |
|------|-------|-------|---------|
| **IPC** | Unix Domain Socket (`~/.deepharness/gatewayd.sock`) | Unix Domain Socket | Named Pipe (`\\.\pipe\deepharness-gatewayd`) |
| **系统通知** | `notify-rust` → NSUserNotification | `notify-rust` → dbus | `notify-rust` → WinRT / Toast |
| **进程守护** | `launchd` plist (可选) | `systemd --user` (可选) | Windows Service (可选) |
| **Lock File** | `~/.deepharness/gatewayd.lock` | 同 macOS | `%LOCALAPPDATA%\DeepHarness\gatewayd.lock` |
| **数据目录** | `~/Library/Application Support/DeepHarness/` | `~/.local/share/deepharness/` | `%LOCALAPPDATA%\DeepHarness\` |
| **证书处理** | Keychain Access (可选) | OpenSSL / rustls | Windows Certificate Store (可选) |

```rust
// crates/dh-platform/src/lib.rs
pub trait Platform {
    fn ipc_endpoint() -> IpcEndpoint;  // UnixSocket | NamedPipe
    fn data_dir() -> PathBuf;
    fn notify(title: &str, body: &str) -> Result<(), NotifyError>;
    fn setup_daemon_service() -> Result<(), ServiceError>;  // 可选
}

#[cfg(target_os = "macos")] pub mod macos;
#[cfg(target_os = "linux")] pub mod linux;
#[cfg(target_os = "windows")] pub mod windows;
```

---

## 4. Daemon 内部模块详细设计

### 4.1 HTTP 兼容 API 层（Axum）

暴露两个端口：
- **工作端口**（如 `2345`）：OpenAI/Anthropic 兼容 API，接收 agent 请求
- **管理端口**（如 `2346`）：Admin API 和 health check，供 Desktop/托盘/包装器连接

**工作端口路由：**
```
POST /v1/chat/completions          ← OpenAI 格式
POST /v1/messages                  ← Anthropic 格式
POST /v1/embeddings                ← 可选扩展
```

**管理端口路由：**
```
GET  /health                       ← health check
GET  /admin/sessions               ← 活跃会话列表
GET  /admin/sessions/:id/messages  ← 会话消息历史
GET  /admin/audit-logs             ← 审计日志查询
PUT  /admin/rules                  ← 策略规则 CRUD
GET  /admin/approvals/pending      ← 待审批列表
POST /admin/approvals/:id/resolve  ← 审批决策（approve/reject/modify）
GET  /admin/stats/tokens           ← Token 使用统计
GET  /admin/mcp/tools              ← MCP 工具列表
POST /admin/mcp/tools/:name/call   ← 手动调用 MCP tool
GET  /admin/config/current         ← 当前生效配置
```

### 4.2 策略引擎（Rule Engine）

策略规则结构：

```yaml
rules:
  - id: "block-gpt4-after-hours"
    name: "非工作时间禁止使用 GPT-4"
    enabled: true
    priority: 100
    condition:
      and:
        - model: "gpt-4*"
        - time_range: { start: "18:00", end: "09:00", timezone: "Asia/Shanghai" }
    action: block
    action_config:
      message: "GPT-4 在非工作时间被禁用，请使用 gpt-3.5-turbo"

  - id: "audit-all-requests"
    name: "记录所有请求"
    enabled: true
    priority: 1
    condition:
      always: true
    action: audit

  - id: "rtk-optimize-large-context"
    name: "大上下文自动启用 RTK"
    enabled: true
    priority: 50
    condition:
      request_size_bytes: { gt: 50000 }
    action: transform
    action_config:
      rtk: { strategy: "sliding_window_summary", max_tokens: 16000 }

  - id: "mcp-remote-approval"
    name: "MCP 远程请求需要审批"
    enabled: true
    priority: 200
    condition:
      mcp_tool: "*"
      tool_arg_pattern: { url: "https?://*" }
    action: require_approval
    action_config:
      timeout_sec: 300
      notify_channels: ["system_notification", "terminal_inline"]

  - id: "cli-proxy-remote-approval"
    name: "CLI skill 远程请求需要审批"
    enabled: true
    priority: 200
    condition:
      proxy_request: { destination: "external" }
    action: require_approval
    action_config:
      timeout_sec: 120
```

策略执行流程：
```
Request 进入
  → 匹配所有规则（按 priority 排序）
  → 收集所有匹配的动作（audit, block, transform, require_approval）
  → 如果存在 block → 直接返回错误
  → 如果存在 require_approval → 进入审批流程
  → 依次执行 transform（RTK、参数修改）
  → 执行 audit（异步写入日志）
  → 转发到真实 LLM / MCP server
```

### 4.3 RTK 模块（Rust Token Killer）

```rust
pub trait TokenOptimizer {
    fn optimize(&self, context: &ChatContext) -> Result<ChatContext, RtkError>;
}

pub struct SlidingWindowSummarizer;
pub struct RedundancyEliminator;
pub struct PromptCompressor;

pub fn apply_rtk_hooks(context: ChatContext, rules: &[Rule]) -> ChatContext {
    for hook in rules.iter().filter_map(|r| r.rtk_hook()) {
        context = hook.optimize(&context).unwrap_or(context);
    }
    context
}
```

**具体策略：**
- **Sliding Window + Summary**：当 messages 总 token > threshold，保留最近的 N 条，对更早的进行摘要
- **Redundancy Eliminator**：检测并合并重复的 system prompt、重复的 tool result
- **Prompt Compressor**：移除无意义空白、使用更短同义词、代码注释精简（保留签名，移除实现体）

### 4.4 MCP 聚合层

Daemon 内部维护一个 MCP Client 池，作为聚合 MCP Server 向 agent 暴露统一工具集。

```
┌────────────────────────────────────────┐
│         MCP Aggregator                 │
├────────────────────────────────────────┤
│  MCP Server Config (from SQLite)       │
│  ├─ filesystem: { command: "npx", args: [...] } │
│  ├─ github:     { command: "npx", args: [...] } │
│  └─ fetch:      { command: "uvx", args: [...] } │
├────────────────────────────────────────┤
│  Client Pool (HashMap<String, McpClient>) │
│  ├─ 启动时初始化所有配置的 MCP server    │
│  ├─ 健康检查（每 30s ping）             │
│  ├─ 掉线自动重连                        │
│  └─ 工具列表聚合（去重 + namespace）     │
├────────────────────────────────────────┤
│  Tool Call Interceptor                 │
│  ├─ 解析 tool 参数，检测是否涉及网络请求  │
│  ├─ 如果涉及远程请求 → 触发审批流程       │
│  └─ 审批通过后 → 转发到对应 MCP client   │
└────────────────────────────────────────┘
```

暴露给 agent 的工具命名空间：
```json
{
  "tools": [
    { "name": "filesystem:read_file", "description": "...", "inputSchema": {...} },
    { "name": "filesystem:list_directory", "description": "...", "inputSchema": {...} },
    { "name": "github:create_issue", "description": "...", "inputSchema": {...} },
    { "name": "fetch:fetch_url", "description": "...", "inputSchema": {...} }
  ]
}
```

### 4.5 HTTP 代理层（CLI Skill 拦截）

Daemon 启动轻量级 HTTP 代理（`127.0.0.1:2347`），包装器注入环境变量：

```bash
export HTTP_PROXY=http://127.0.0.1:2347
export HTTPS_PROXY=http://127.0.0.1:2347   # 可选，需用户信任自签名证书
export NO_PROXY=localhost,127.0.0.1,::1
```

代理层行为：
- 只拦截**出站**请求（目标不是 localhost）
- 解析 Host header，判断是否为 LLM API（已知域名列表）
  - 如果是 LLM API → 直接放行（已由兼容 API 层处理）
  - 如果是其他外部服务 → 检查策略（可能需要审批）
- 不强制 HTTPS MITM：如果 agent/CLI 使用 HTTPS 且未配置证书信任，则 CONNECT tunnel 直接放行
- 记录元数据（目标域名、端口、时间、进程名）到审计日志

### 4.6 审批协调器

```rust
pub struct ApprovalCoordinator {
    pending: Arc<Mutex<HashMap<String, ApprovalRequest>>>,
    notifiers: Vec<Box<dyn Notifier>>,
}

pub struct ApprovalRequest {
    id: String,
    request_type: ApprovalType,  // LLMRequest | McpToolCall | ProxyRequest
    details: ApprovalDetails,
    created_at: Instant,
    timeout: Duration,
    resolver: oneshot::Sender<ApprovalResult>,
}

#[async_trait]
pub trait Notifier: Send + Sync {
    async fn notify(&self, req: &ApprovalRequest) -> Result<(), NotifyError>;
}
```

**审批流程：**
1. 策略引擎判断需要审批 → 生成 `approval_id`
2. 将请求放入 `pending` 队列，设置超时（默认 5 分钟）
3. 并行调用所有 notifier：
   - `SystemNotifier`：发送系统通知（点击打开审批详情）
   - `IpcNotifier`：通过 IPC 推送给 Desktop/托盘程序
   - `TerminalNotifier`：如果通过包装器启动，在终端打印审批提示
4. 等待 `resolver` 被唤醒（用户决策）或超时
5. 决策结果：`Approve` / `Reject` / `ModifyAndApprove`

**终端内联审批示例：**
```bash
$ deepharness exec claude
[DeepHarness] Daemon connected. PID: 12345
[DeepHarness] Session: sess_abc123

User: 帮我写个快速排序

[DeepHarness] ⚠️  审批请求 #apr-001
    类型: LLM 请求
    模型: gpt-4
    预估 token: 15,420 (RTK 优化后: 8,200)
    触发规则: "大上下文自动启用 RTK"
    操作: [Y] 同意并发送  [N] 拒绝  [E] 编辑后发送  [S] 跳过审批(本次会话)
> Y
[DeepHarness] 已放行。等待响应...
```

### 4.7 异步上报模块（Async Reporter）

Daemon 可以将会话数据异步上报到远程服务端：

```rust
pub struct AsyncReporter {
    endpoint: Option<String>,      // 远程上报地址
    api_key: Option<String>,
    batch_size: usize,             // 批量上报条数
    flush_interval: Duration,      // 定时 flush
    buffer: Arc<Mutex<Vec<ReportPayload>>>,
}

pub enum ReportPayload {
    SessionStarted { session_id, agent, model, timestamp },
    MessageExchanged { session_id, role, token_count, latency_ms, timestamp },
    ApprovalResolved { approval_id, decision, resolved_by, timestamp },
    TokenOptimized { session_id, original_tokens, optimized_tokens, strategy },
    RuleTriggered { rule_id, rule_name, request_id },
}
```

**上报策略：**
- **默认关闭**：不上报任何数据（纯本地）
- **企业模式**：配置 `endpoint` 后，按批次异步上报
- **本地缓冲**：网络不可用时，写入本地 SQLite 队列，恢复后重传
- **数据脱敏**：可选对 content 字段进行哈希/脱敏后再上报
- **上报时机**：定时 flush（30s）或 buffer 满（100 条）

---

## 5. Config Hub（配置中心）

### 5.1 架构定位

Config Hub 解决**纯终端用户没有 GUI，如何统一接收和管理配置**的问题。

```
┌─────────────────────────────────────────────────────────────┐
│                      Config Hub                             │
├─────────────────────────────────────────────────────────────┤
│  存储层 (SQLite + 文件系统)                                   │
│  ├─ configs 表      (key-value 配置项)                      │
│  ├─ specs 表        (规范文件元数据)                        │
│  ├─ skills 表       (skill/MCP server 配置)                 │
│  └─ files/          (规范文件实际内容)                       │
│      ├─ agents.md                                    │
│      ├─ design.md                                    │
│      ├─ claude.md                                    │
│      └─ <project>/.cursorrules                       │
├─────────────────────────────────────────────────────────────┤
│  下发通道                                                    │
│  ├─ 请求时注入 (system prompt 拼接)                         │
│  ├─ MCP 工具暴露 (get_config, list_specs, read_spec)       │
│  └─ Admin API (/admin/config/*)                            │
├─────────────────────────────────────────────────────────────┤
│  同步机制                                                    │
│  ├─ 本地文件监听 (agents.md 变更自动 reload)                 │
│  ├─ Git 同步 (可选，从远程仓库拉取规范)                       │
│  └─ 云端轮询 (定期拉取管理控制台配置)                         │
└─────────────────────────────────────────────────────────────┘
```

### 5.2 云端配置轮询

```rust
// crates/dh-cloud-client/src/poller.rs
pub struct CloudConfigPoller {
    endpoint: String,
    tenant_id: String,
    api_key: String,
    interval: Duration,
}

impl CloudConfigPoller {
    pub async fn start(&self) {
        loop {
            match self.fetch_config().await {
                Ok(remote_config) => {
                    let local = self.load_local();
                    if remote_config.version > local.version {
                        self.apply_config(remote_config);
                        self.notify_change();
                    }
                }
                Err(e) => {
                    log::warn!("Cloud config fetch failed: {}", e);
                    // 使用本地缓存，继续运行
                }
            }
            tokio::time::sleep(self.interval).await;
        }
    }
}
```

### 5.3 配置层级

```yaml
# 配置优先级（从高到低）

# 1. 会话级覆盖 (临时，本次会话有效)
session_overrides:
  model: "gpt-4o"
  temperature: 0.7
  max_tokens: 4096

# 2. Agent 级配置 (按 agent 类型)
agent_configs:
  opencode:
    model: "claude-sonnet-4"
    api_base: "https://api.anthropic.com"
  claude:
    model: "claude-3-5-sonnet-20241022"
  aider:
    model: "gpt-4o"

# 3. 项目级配置 (按工作目录)
project_configs:
  "/home/user/projects/web-app":
    model: "gpt-4o-mini"
    system_prompt_append: "这是一个 Next.js 项目，使用 App Router。"

# 4. 全局默认
global_defaults:
  model: "gpt-4o"
  temperature: 0.2
  max_tokens: 8192
  timeout_sec: 120
```

### 5.4 规范注入（Specs Injection）

**支持的规范文件：**

| 文件 | 用途 | 注入位置 |
|------|------|---------|
| `agents.md` | 全局 Agent 行为规范 | 所有请求的 system prompt 末尾 |
| `design.md` | 设计系统规范 | 涉及 UI/组件的请求 |
| `claude.md` | Claude 特定规范 | 使用 Claude 模型时 |
| `.cursorrules` | Cursor 特定规范 | 兼容 Cursor 格式 |
| `<project>/CONVENTIONS.md` | 项目级约定 | 该项目的请求 |

**注入策略：**
```rust
pub fn inject_specs(request: &mut UnifiedRequest, ctx: &RequestContext) {
    let mut specs = vec![];
    
    // 1. 全局规范
    specs.push(load_global("agents.md"));
    specs.push(load_global("design.md"));
    
    // 2. Agent 特定规范
    match ctx.agent_type {
        "claude" => specs.push(load_global("claude.md")),
        "cursor" => specs.push(load_global(".cursorrules")),
        _ => {}
    }
    
    // 3. 项目级规范
    if let Some(proj_specs) = get_project_specs(&ctx.workspace) {
        specs.extend(proj_specs);
    }
    
    // 4. 注入到 system prompt
    if !specs.is_empty() {
        let injection = format!("\n\n--- 工程规范 ---\n{}\n---", specs.join("\n\n"));
        request.prepend_system_message(injection);
    }
}
```

**文件监听与热更新：**
- 使用 `notify` crate 监听规范文件目录
- 文件变更后 2 秒内自动 reload
- 通过 IPC 通知所有连接的 Desktop/托盘："规范已更新"

### 5.5 Skill 下发

**MCP Server 配置：**
```yaml
mcp_servers:
  filesystem:
    enabled: true
    command: "npx"
    args: ["-y", "@modelcontextprotocol/server-filesystem", "/home/user"]
    env: {}
    
  github:
    enabled: true
    command: "npx"
    args: ["-y", "@modelcontextprotocol/server-github"]
    env:
      GITHUB_PERSONAL_ACCESS_TOKEN: "${GITHUB_TOKEN}"
      
  fetch:
    enabled: true
    command: "uvx"
    args: ["mcp-server-fetch"]
```

**自定义 Skill（Prompt Templates）：**
```yaml
skills:
  code_review:
    name: "代码审查"
    description: "审查代码变更，检查潜在问题"
    prompt_template: |
      请审查以下代码变更：
      ```diff
      {{diff}}
      ```
      关注：1. 潜在的 bug 和安全问题 2. 代码风格和可读性 3. 性能影响 4. 是否符合项目规范
    trigger:
      type: "mcp_tool"
      tool: "git:diff"
      
  generate_tests:
    name: "生成测试"
    prompt_template: |
      请为以下代码生成完整的单元测试：
      ```{{language}}
      {{code}}
      ```
      要求：使用 {{test_framework}}，覆盖正常路径和边界条件
    trigger:
      type: "manual"
      shortcut: "/test"
```

---

## 6. 数据流（请求生命周期）

以 **OpenCode 通过兼容 API 发送请求** 为例：

```
1. 用户终端
   $ deepharness exec opencode
   (注入 OPENAI_BASE_URL=http://127.0.0.1:2345/v1)

2. OpenCode 发送请求
   POST http://127.0.0.1:2345/v1/chat/completions
   Headers: Authorization: Bearer <key>
   Body: { messages: [...], model: "gpt-4", ... }

3. Daemon HTTP API 层 (Axum)
   接收请求 → 解析为标准内部格式 (UnifiedRequest)
   生成 request_id, session_id

4. Config Hub 注入
   加载规范文件 → 拼接 system prompt
   应用层级配置（模型、参数）

5. 策略引擎
   加载匹配规则 → 检查 block / approval / transform / audit
   发现 token 数 > 50000 → 触发 RTK 优化
   发现无 block 规则 → 继续
   发现需要审批 → 进入审批流程

6. RTK 模块（如果触发）
   SlidingWindowSummarizer: messages[0..-5] → summary
   RedundancyEliminator: 移除重复 system prompt
   生成优化后的 UnifiedRequest

7. 审批协调器（如果需要）
   生成 approval_id
   发送系统通知 + IPC push + 终端提示
   等待用户决策 (Y/N/E)
   用户选择 Y → 继续
   超时 → 按默认策略处理（默认放行或拒绝）

8. 审计日志（异步）
   将原始请求 + 优化后请求 + 规则触发记录 → SQLite audit_logs
   同时加入上报 buffer

9. 转发到真实 LLM
   根据 model 选择目标 API (OpenAI / Anthropic / ...)
   转换回目标 API 格式
   发送 HTTP 请求
   流式接收 SSE 响应

10. 响应处理
    将 SSE chunk 实时转发给 agent（保持流式体验）
    同时记录响应内容到审计日志
    统计 token 使用量

11. 完成
    发送 agent.done 通知（如果有 IPC 监听）
    更新 session 统计
    flush 审计日志和上报 buffer
```

---

## 7. 工程目录（Monorepo 架构）

当前单一 Desktop 应用需重构为 **Rust + Node.js 混合 Monorepo**。

```
depharness/                              # 项目根（Monorepo）
│
├── Cargo.toml                            # Rust workspace 定义
├── pnpm-workspace.yaml                   # Node.js workspace 定义
├── package.json                          # 根 package（scripts、husky 等）
├── rust-toolchain.toml
│
├── crates/                               # 🔧 共享 Rust 库（被多个 apps 依赖）
│   │
│   ├── dh-core/                          # 核心领域模型与协议
│   │   ├── src/
│   │   │   ├── models/                   # 共享数据模型
│   │   │   │   ├── request.rs            # UnifiedRequest
│   │   │   │   ├── response.rs           # UnifiedResponse
│   │   │   │   ├── session.rs            # Session 定义
│   │   │   │   ├── config.rs             # Config 模型
│   │   │   │   └── audit.rs              # 审计日志模型
│   │   │   ├── policy/                   # 策略引擎 trait 与定义
│   │   │   │   ├── engine.rs             # PolicyEngine trait
│   │   │   │   ├── rule.rs               # Rule 结构
│   │   │   │   └── action.rs             # Action 枚举
│   │   │   ├── mcp/                      # MCP 协议（从现有 agent-core/mcp 迁移）
│   │   │   │   ├── client.rs
│   │   │   │   ├── transport.rs
│   │   │   │   ├── codec.rs
│   │   │   │   └── types.rs
│   │   │   ├── events/                   # 事件类型（跨进程通信）
│   │   │   │   ├── gateway_event.rs
│   │   │   │   └── approval_event.rs
│   │   │   └── lib.rs
│   │   └── Cargo.toml
│   │
│   ├── dh-platform/                      # 跨平台基础设施
│   │   ├── src/
│   │   │   ├── ipc/                      # IPC 抽象
│   │   │   │   ├── unix_socket.rs        # macOS/Linux
│   │   │   │   ├── named_pipe.rs         # Windows
│   │   │   │   └── mod.rs
│   │   │   ├── notify/                   # 系统通知
│   │   │   │   ├── macos.rs
│   │   │   │   ├── linux.rs
│   │   │   │   ├── windows.rs
│   │   │   │   └── mod.rs
│   │   │   ├── fs/                       # 数据目录、lock file
│   │   │   ├── process/                  # 进程管理
│   │   │   └── lib.rs
│   │   └── Cargo.toml
│   │
│   ├── dh-rtk/                           # RTK Token Killer 引擎
│   │   ├── src/
│   │   │   ├── compressor.rs
│   │   │   ├── summarizer.rs
│   │   │   ├── dedup.rs
│   │   │   └── lib.rs
│   │   └── Cargo.toml
│   │
│   ├── dh-db/                            # 数据库共享层（SQLite schema + 迁移）
│   │   ├── src/
│   │   │   ├── schema.rs                 # 所有表定义
│   │   │   ├── migration.rs
│   │   │   ├── connection.rs
│   │   │   └── lib.rs
│   │   └── Cargo.toml
│   │
│   └── dh-cloud-client/                  # 云端管理控制台客户端
│       ├── src/
│       │   ├── client.rs                 # HTTP client + 轮询逻辑
│       │   ├── auth.rs                   # 租户认证（API Key / JWT）
│       │   ├── sync.rs                   # 配置同步
│       │   └── lib.rs
│       └── Cargo.toml
│
├── apps/                                 # 🚀 可独立运行的应用
│   │
│   ├── gatewayd/                         # LLM Gateway Daemon（核心新应用）
│   │   ├── src/
│   │   │   ├── main.rs
│   │   │   ├── server/                   # Axum HTTP 服务器
│   │   │   │   ├── mod.rs
│   │   │   │   ├── api.rs                # OpenAI/Anthropic 兼容端点
│   │   │   │   └── admin.rs              # Admin API (/admin/*)
│   │   │   ├── gateway/                  # 请求生命周期管理
│   │   │   │   ├── router.rs             # 协议转换路由
│   │   │   │   ├── transformer.rs        # 请求/响应转换
│   │   │   │   └── stream.rs             # SSE 流处理
│   │   │   ├── policy/                   # 策略引擎实现
│   │   │   │   ├── engine.rs
│   │   │   │   ├── matcher.rs            # 规则匹配器
│   │   │   │   └── executor.rs           # 动作执行器
│   │   │   ├── rtk/                      # RTK 模块
│   │   │   │   ├── pipeline.rs           # Hook 链
│   │   │   │   └── ...
│   │   │   ├── mcp/                      # MCP 聚合器
│   │   │   │   ├── aggregator.rs         # 工具聚合
│   │   │   │   ├── registry.rs           # MCP server 注册表
│   │   │   │   └── interceptor.rs        # Tool call 拦截
│   │   │   ├── proxy/                    # HTTP 代理层（CLI skill 拦截）
│   │   │   │   ├── server.rs             # 小型 HTTP proxy
│   │   │   │   └── handler.rs
│   │   │   ├── approval/                 # 审批协调器
│   │   │   │   ├── coordinator.rs
│   │   │   │   ├── queue.rs
│   │   │   │   └── notifiers/            # 各种通知实现
│   │   │   │       ├── system.rs
│   │   │   │       ├── terminal.rs
│   │   │   │       └── ipc.rs
│   │   │   ├── config_hub/               # Config Hub（配置中心）
│   │   │   │   ├── resolver.rs           # 配置解析
│   │   │   │   ├── injector.rs           # 规范注入
│   │   │   │   ├── spec_loader.rs        # 规范文件加载
│   │   │   │   └── skill_registry.rs     # Skill 注册表
│   │   │   ├── audit/                    # 审计日志
│   │   │   │   ├── logger.rs
│   │   │   │   └── storage.rs
│   │   │   ├── reporter/                 # 异步上报
│   │   │   │   ├── batch.rs
│   │   │   │   └── queue.rs
│   │   │   ├── cloud/                    # 云端控制台同步
│   │   │   │   ├── poller.rs             # 定期轮询
│   │   │   │   └── applier.rs            # 配置应用
│   │   │   ├── platform/                 # 平台适配（thin wrapper around dh-platform）
│   │   │   └── db/                       # 本地 SQLite 操作
│   │   └── Cargo.toml
│   │
│   ├── cli/                              # deepharness CLI（包装器 + 管理）
│   │   ├── src/
│   │   │   ├── main.rs
│   │   │   ├── commands/
│   │   │   │   ├── mod.rs
│   │   │   │   ├── exec.rs               # deepharness exec <agent>
│   │   │   │   ├── gatewayd.rs           # gatewayd start/stop/status/logs
│   │   │   │   ├── config.rs             # config get/set
│   │   │   │   ├── session.rs            # session list/attach/kill
│   │   │   │   ├── approval.rs           # approval list/approve/reject
│   │   │   │   └── logs.rs               # logs tail
│   │   │   ├── wrapper/                  # 包装器核心
│   │   │   │   ├── mod.rs
│   │   │   │   ├── env_injector.rs       # 环境变量注入
│   │   │   │   ├── process_manager.rs    # 子进程管理
│   │   │   │   ├── daemon_connector.rs   # 与 gatewayd 保持连接
│   │   │   │   └── terminal_ui.rs        # 终端内联审批 UI
│   │   │   └── client/                   # gatewayd HTTP API client
│   │   └── Cargo.toml
│   │
│   ├── desktop/                          # DeepHarness Desktop（现有应用迁移）
│   │   ├── src-tauri/                    # Rust 后端
│   │   │   ├── src/
│   │   │   │   ├── main.rs               # Tauri 入口
│   │   │   │   ├── commands/             # Tauri commands（精简版）
│   │   │   │   │   ├── mod.rs
│   │   │   │   │   ├── db.rs             # 本地 DB CRUD（保留）
│   │   │   │   │   ├── workspace.rs      # 文件系统（保留）
│   │   │   │   │   ├── git.rs            # Git 操作（保留）
│   │   │   │   │   └── gateway_proxy.rs  # 代理到 gatewayd 的命令
│   │   │   │   ├── ipc_client/           # gatewayd IPC client
│   │   │   │   │   ├── mod.rs
│   │   │   │   │   ├── connection.rs
│   │   │   │   │   └── api.rs            # 调用 gatewayd Admin API
│   │   │   │   └── lib.rs
│   │   │   └── Cargo.toml
│   │   ├── src/                          # React 前端（基本不变）
│   │   │   ├── App.tsx
│   │   │   ├── pages/
│   │   │   ├── components/
│   │   │   ├── stores/
│   │   │   └── ...
│   │   ├── package.json
│   │   └── ...
│   │
│   └── tray/                             # 系统托盘程序（轻量 GUI）
│       ├── src-tauri/                    # Tauri 后端
│       │   ├── src/
│       │   │   ├── main.rs
│       │   │   ├── approval_ui.rs        # 审批弹窗/面板
│       │   │   ├── notify_handler.rs     # 接收 gatewayd 通知
│       │   │   └── ipc_client/           # 同 desktop
│       │   └── Cargo.toml
│       ├── src/                          # 极简 React 前端
│       │   ├── App.tsx                   # 只包含托盘菜单 + 审批面板
│       │   └── ...
│       └── package.json
│
├── packages/                             # 📦 共享前端资源
│   ├── ui/                               # shadcn/ui 组件（desktop + tray 共享）
│   │   ├── src/
│   │   └── package.json
│   └── shared-types/                     # TypeScript 类型定义（前后端共享）
│       ├── src/
│       └── package.json
│
├── specs/                                # 📋 规范文件模板
│   ├── agents.md.template
│   ├── design.md.template
│   ├── claude.md.template
│   └── .cursorrules.template
│
├── scripts/                              # 🔨 构建脚本
│   ├── build-all.sh
│   ├── package-daemon.sh
│   └── setup-dev.sh
│
└── docs/                                 # 📚 文档
    ├── architecture/
    │   ├── 01-overview.md
    │   ├── 02-gatewayd.md
    │   ├── 03-config-hub.md
    │   └── 04-mcp-aggregation.md
    ├── api/
    │   ├── admin-api.md
    │   └── mcp-tools.md
    └── deployment/
        ├── systemd.md
        ├── launchd.md
        └── windows-service.md
```

### 7.1 关键迁移说明

| 现有路径 | 新路径 | 说明 |
|---------|--------|------|
| `src-tauri/crates/agent-core/` | `crates/dh-core/` | 迁移并扩展 |
| `src-tauri/crates/agent-runtime/` | `crates/dh-platform/src/process/` | 迁移 |
| `src-tauri/crates/opencode-plugin/` | `apps/gatewayd/src/plugins/opencode/` | 变为 gatewayd 插件 |
| `src-tauri/src/gateway/` | `apps/gatewayd/src/gateway/` | 迁移并重构 |
| `src-tauri/src/service/` | `apps/gatewayd/src/{policy,rtk,mcp,...}/` | 拆分 |
| `src/` (React) | `apps/desktop/src/` | 迁移 |
| `src-tauri/src/` (Tauri Rust) | `apps/desktop/src-tauri/src/` | 迁移并精简 |

### 7.2 依赖关系

```
apps/cli ───────┐
apps/desktop ───┼──► crates/dh-core + dh-platform + dh-db + dh-rtk + dh-cloud-client
apps/tray ──────┘
                │
apps/gatewayd ──┘（也依赖上述 crates）
```

### 7.3 Workspace 配置

**Cargo.toml（根）：**
```toml
[workspace]
members = [
    "crates/dh-core",
    "crates/dh-platform",
    "crates/dh-rtk",
    "crates/dh-db",
    "crates/dh-cloud-client",
    "apps/gatewayd",
    "apps/cli",
    "apps/desktop/src-tauri",
    "apps/tray/src-tauri",
]
resolver = "2"
```

**pnpm-workspace.yaml：**
```yaml
packages:
  - "apps/desktop"
  - "apps/tray"
  - "packages/*"
```

---

## 8. IPC 协议设计

Desktop / Tray / CLI 与 gatewayd 的通信协议：

```
┌──────────┐      Unix Socket / Named Pipe      ┌──────────┐
│ Desktop  │◄──────────────────────────────────►│          │
│   /Tray  │                                    │          │
└──────────┘                                    │          │
                                                │ gatewayd │
┌──────────┐      HTTP Admin API (localhost)    │          │
│   CLI    │◄──────────────────────────────────►│          │
└──────────┘                                    └──────────┘
```

**IPC 消息类型**（通过 Unix Socket 的 JSON 协议）：

```rust
#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
enum IpcMessage {
    // Desktop/Tray → gatewayd
    SubscribeApprovals,
    GetPendingApprovals,
    ResolveApproval { id: String, decision: Decision },
    GetStats,
    
    // gatewayd → Desktop/Tray
    ApprovalRequested { approval: ApprovalRequest },
    ConfigUpdated { changed_keys: Vec<String> },
    SessionStarted { session_id: String },
    Notification { title: String, body: String },
}
```

---

## 9. CLI 设计

```bash
# 启动 daemon
depharness gatewayd start --daemon --config ~/.deepharness/config.yaml

# 停止 daemon
depharness gatewayd stop

# 查看状态
depharness gatewayd status
# 输出: PID: 12345 | Uptime: 2h30m | Sessions: 3 | Pending approvals: 1

# 查看日志
depharness gatewayd logs --tail 100

# 执行 agent（包装器模式）
depharness exec claude
depharness exec opencode -- --model claude-sonnet
depharness exec aider

# 配置管理
depharness config get model.default
depharness config set model.default gpt-4o
depharness config reload

# 会话管理
depharness session list
depharness session logs <session_id>
depharness session kill <session_id>

# 审批管理
depharness approval list --pending
depharness approval approve <approval_id>
depharness approval reject <approval_id>
```

---

## 10. 错误处理与容错

| 故障场景 | 处理策略 |
|---------|---------|
| gatewayd 崩溃 | CLI 包装器检测到连接断开 → 自动重启 gatewayd（如果由包装器启动） |
| 云端控制台不可达 | 使用本地缓存配置，指数退避重试，不影响本地功能 |
| MCP server 掉线 | 标记为 unhealthy，从工具列表移除，30s 后自动重连 |
| 审批超时 | 按默认策略处理（可配置：放行/拒绝/通知管理员） |
| RTK 失败 | 记录错误，使用原始请求继续（降级策略） |
| SQLite 锁定 | 使用 `rusqlite` 的 busy timeout，失败时写入内存队列 |
| Agent 子进程崩溃 | 包装器自动清理，通知 daemon 会话结束 |
| 网络分区 | 本地模式继续运行，云端配置不更新，本地审计队列堆积 |

---

## 11. 扩展性设计

**未来可自然扩展的方向：**

1. **插件系统**：`apps/gatewayd/src/plugins/` 目录，支持动态加载适配器（OpenCode、Claude、Aider 等）
2. **多节点部署**：将 `dh-db` 扩展为支持 PostgreSQL，`dh-cloud-client` 扩展为支持集群发现
3. **Web 控制台**：在 `apps/` 下新增 `admin-web/`，作为云端管理控制台的前端
4. **移动端**：`apps/mobile/`，通过 HTTP Admin API 远程管理

---

## 12. 安全考量

- **API Key 存储**：使用系统密钥链（macOS Keychain / Windows Credential Store / Linux Secret Service）存储 LLM API Key，不写入配置文件
- **SQLite 加密**：审计日志数据库使用 SQLCipher 加密，密钥派生自用户密码或系统密钥链
- **HTTPS 代理**：不强制 HTTPS MITM，除非用户显式安装自签名证书并信任
- **配置隔离**：多租户场景下，不同租户的规范文件和配置存储在隔离目录
- **审计不可篡改**：审计日志使用追加写模式，关键记录计算哈希链，防止本地篡改

---

## 13. 里程碑规划

| 阶段 | 目标 | 预计周期 |
|------|------|---------|
| **MVP** | gatewayd 核心：HTTP API 兼容层 + 审计日志 + 基础 CLI 包装器 | 2 周 |
| **V0.2** | 策略引擎 + RTK + 审批协调器（终端内联） | 2 周 |
| **V0.3** | MCP 聚合 + CLI 代理拦截 + 系统通知 | 2 周 |
| **V0.4** | Config Hub + 规范注入 + Skill 下发 | 2 周 |
| **V0.5** | 系统托盘程序 + Desktop 集成（混合模式） | 2 周 |
| **V1.0** | 云端控制台集成 + 异步上报 + 企业功能 | 2 周 |

---

*本文档基于 2026-06-09 的设计讨论整理，后续迭代请更新此文件。*
