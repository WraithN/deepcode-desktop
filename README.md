# DeepHarness Desktop

AI 编码助手桌面应用 —— 基于 React + Tauri 的本地优先 AI 编码工作台，支持多智能体管理、多轮对话、会话日志、任务追踪与文件变更管理。

---

## 目录

- [项目简介](#项目简介)
- [核心特性](#核心特性)
- [技术栈](#技术栈)
- [系统架构](#系统架构)
- [目录结构](#目录结构)
- [快速开始](#快速开始)
- [开发工作流](#开发工作流)
- [架构决策与改进方向](#架构决策与改进方向)
- [相关文档](#相关文档)

---

## 项目简介

DeepHarness Desktop 是一款面向开发者的本地 AI 编码辅助工具。应用采用 Web 前端 + Tauri 后端的混合架构：前端提供类似 VS Code 的沉浸式界面，后端通过 Rust 管理本地 SQLite 数据库、智能体进程与 WebSocket JSON-RPC 网关。

### 设计目标

- **本地优先**：所有数据默认存储在本地 SQLite，无需联网即可使用。
- **多智能体**：支持 OpenCode、Claude Code、Cursor Agent、Codex 与自定义智能体。
- **可扩展**：通过 `agent-core` 抽象与插件机制接入新的编码智能体。
- **开发者体验**：深色主题、高密度信息、流式输出、步骤可视化。

---

## 核心特性

- 用户登录/注册（基于本地 SQLite）
- 智能体选择、创建、切换与持久化
- 多轮编码对话与流式消息渲染
- 会话级日志（500 条前端缓存 + SQLite 持久化）
- 任务列表与文件变更追踪
- 工作区文件浏览、Git 状态与 diff 查看
- 智能体配置、主题色切换与设置面板

---

## 技术栈

| 层级 | 技术 |
|------|------|
| 构建工具 | Vite（rolldown-vite）、Tauri v2 |
| 前端语言 | TypeScript 5.9（严格模式） |
| 前端框架 | React 18 + react-router-dom v7（HashRouter） |
| 状态管理 | Zustand |
| UI 组件 | shadcn/ui（New York 风格）+ Tailwind CSS 3.4 |
| 图标 | Lucide React + @lobehub/icons |
| 后端语言 | Rust |
| 桌面框架 | Tauri v2 |
| 数据库 | SQLite（rusqlite） |
| 通信 | Tauri Invoke + WebSocket JSON-RPC |
| 测试 | Vitest + jsdom + @testing-library/react |
| 代码检查 | Biome 2.4.5 + ast-grep 自定义规则 + tsgo |

---

## 系统架构

### 整体分层

```
┌─────────────────────────────────────────────────────────────┐
│                     前端 (React SPA)                         │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐  │
│  │   Pages     │  │  Components │  │      Stores         │  │
│  │  - Login    │  │  - Workspace│  │  - chatStore        │  │
│  │  - Select   │  │  - ChatPanel│  │  - agentStore       │  │
│  │  - Workspace│  │  - LeftPanel│  │  - websocketStore   │  │
│  └─────────────┘  └─────────────┘  └─────────────────────┘  │
│                              │                               │
│                         Zustand / Hooks                      │
│                              │                               │
│  ┌────────────────────────────────────────────────────────┐  │
│  │              数据库适配层 (src/db)                       │  │
│  │         Mock (localStorage)  /  Tauri SQLite             │  │
│  └────────────────────────────────────────────────────────┘  │
└──────────────────────────────┬──────────────────────────────┘
                               │ Tauri Invoke / WebSocket
┌──────────────────────────────┼──────────────────────────────┐
│                         Tauri (Rust)                         │
│  ┌───────────────────────────┼───────────────────────────┐  │
│  │      Commands (src/commands)                            │  │
│  │   - db.rs       : 数据库 CRUD 命令                      │  │
│  │   - workspace.rs: 工作区文件操作                        │  │
│  │   - git.rs      : Git 状态与 diff                       │  │
│  │   - system.rs   : 系统信息                              │  │
│  └───────────────────────────┼───────────────────────────┘  │
│  ┌───────────────────────────┼───────────────────────────┐  │
│  │      WebSocket JSON-RPC Gateway (src/gateway)           │  │
│  │   - server.rs     : tokio-tungstenite WebSocket 服务    │  │
│  │   - router.rs     : 请求路由                            │  │
│  │   - handlers/     : agent / session / db 处理器         │  │
│  │   - session_manager.rs : 连接与会话管理                 │  │
│  └───────────────────────────┼───────────────────────────┘  │
│  ┌───────────────────────────┼───────────────────────────┐  │
│  │      Service Layer (src/service)                        │  │
│  │   - agent_service.rs    : 智能体生命周期管理            │  │
│  │   - db_service.rs       : 数据库服务薄封装              │  │
│  │   - plugin_registry.rs  : 插件注册表                    │  │
│  │   - instance_registry.rs: 实例注册表                    │  │
│  └───────────────────────────┼───────────────────────────┘  │
│                              │                               │
│  ┌───────────────────────────┼───────────────────────────┐  │
│  │           Shared Rust Crates (crates/)                  │  │
│  │   - agent-core      : Agent 插件抽象、MCP、SessionLogger│  │
│  │   - opencode-plugin : OpenCode CLI 适配器               │  │
│  │   - claude-plugin   : Claude Code CLI 适配器            │  │
│  │   - codex-plugin    : OpenAI Codex CLI 适配器           │  │
│  │   - dh-core         : 共享领域模型                      │  │
│  │   - dh-db           : 数据库连接、迁移、Repository      │  │
│  │   - dh-platform     : 平台能力（通知、IPC、FS）         │  │
│  └───────────────────────────┼───────────────────────────┘  │
└──────────────────────────────┼──────────────────────────────┘
                               │
                        ┌──────┴──────┐
                        │   SQLite    │
                        │   app.db    │
                        └─────────────┘
```

### 关键架构模式

1. **双通道通信**
   - **Tauri Invoke**：用于同步/阻塞的数据库查询、文件系统与系统命令。
   - **WebSocket JSON-RPC**：用于智能体生命周期、流式事件、会话日志等异步推送场景。

2. **Agent 插件系统**
   - `agent-core` 定义 `AgentPlugin` 与 `AgentInstance` trait。
   - `opencode-plugin` 实现 OpenCode CLI 的启动、SSE 解析与 MCP 适配。
   - 新增智能体类型只需实现 trait 并注册到 `PluginRegistry`。

3. **事件驱动**
   - 智能体通过 `DynEventSink` 向后端推送事件（token、thinking、question、permission、done、error）。
   - 后端通过 WebSocket 广播或 Tauri event 转发到前端。

4. **本地数据持久化**
   - 主数据库 `app.db` 存储用户、会话、消息、任务、文件变更、会话日志。
   - 每个智能体实例可拥有独立数据库（`agents/<instance_id>/data.db`）。
   - 前端部分状态通过 `localStorage` 持久化（智能体列表、主题色等）。

---

## 目录结构

```
├── src/                          # 前端源码
│   ├── App.tsx                   # 根组件：主题色、路由、AuthProvider
│   ├── routes.tsx                # 路由配置
│   ├── main.tsx                  # 应用入口
│   ├── index.css                 # Tailwind 指令 + CSS 变量
│   ├── pages/                    # 页面组件
│   │   ├── LoginPage.tsx
│   │   ├── SelectAgentPage.tsx
│   │   └── WorkspacePage.tsx
│   ├── components/               # 组件
│   │   ├── ui/                   # shadcn/ui 基础组件
│   │   ├── common/               # 通用组件
│   │   └── workspace/            # 工作区专属组件
│   ├── contexts/                 # React Context
│   │   └── AuthContext.tsx
│   ├── db/                       # 数据库适配层
│   │   ├── index.ts              # 自动切换 Mock / Tauri 实现
│   │   ├── mock.ts               # localStorage 模拟存储
│   │   ├── tauri-client.ts       # Tauri invoke 调用
│   │   └── types.ts              # IDataStore 接口
│   ├── stores/                   # Zustand 状态管理
│   │   ├── chatStore.ts
│   │   ├── agentStore.ts
│   │   ├── websocketStore.ts
│   │   └── logStore.ts
│   ├── store/                    # 前端 session-log 存储
│   │   └── session-log.ts
│   ├── services/                 # 服务层
│   ├── hooks/                    # 自定义 Hooks
│   ├── lib/                      # 工具函数
│   └── types/                    # 业务类型定义
│
├── src-tauri/                    # Tauri 桌面端（Rust）
│   ├── Cargo.toml
│   ├── tauri.conf.json
│   ├── src/
│   │   ├── main.rs               # 应用入口
│   │   ├── lib.rs                # DbState、模块导出
│   │   ├── agent_db.rs           # 智能体独立数据库操作
│   │   ├── setup/                # DB 初始化、窗口管理
│   │   ├── commands/             # Tauri 命令
│   │   ├── gateway/              # WebSocket JSON-RPC 网关
│   │   ├── service/              # 业务服务
│   │   └── models/               # 数据模型
│   └── crates/                   # 当前为空，保留用于未来 Tauri 专属 crate
│
├── crates/                       # Rust 工作区 crate（被 src-tauri、apps/cli、apps/gatewayd 共享）
│   ├── agent-core/               # Agent 插件抽象、MCP 协议、SessionLogger
│   ├── opencode-plugin/          # OpenCode CLI 适配器
│   ├── claude-plugin/            # Claude Code CLI 适配器
│   ├── codex-plugin/             # OpenAI Codex CLI 适配器
│   ├── dh-core/                  # 共享领域模型
│   ├── dh-db/                    # 数据库连接与 schema
│   └── dh-platform/              # 平台能力（通知、IPC、文件系统）
│
├── apps/                         # 可独立运行的 Rust 应用
│   ├── cli/                      # 命令行工具 dh
│   └── gatewayd/                 # 独立网关服务 dh-gatewayd
│
├── .rules/                       # ast-grep 自定义 lint 规则
├── docs/                         # 需求与 bug 文档
├── public/                       # 静态资源
├── dist/                         # Web 构建输出
└── package.json
```

---

## 快速开始

### 环境要求

- Node.js ≥ 20
- npm ≥ 10
- pnpm（推荐）
- Rust ≥ 1.70

### 安装依赖

```bash
pnpm install
```

### 开发模式

```bash
# Web 开发服务器（端口 5173）
pnpm dev

# Tauri 桌面端开发
pnpm tauri-dev
```

### 构建

```bash
# Web 生产构建
pnpm build

# 桌面端打包
pnpm tauri-build
```

### 代码检查与测试

```bash
# 完整 lint（类型检查、Biome、ast-grep、Tailwind、构建冒烟）
pnpm lint

# 单元测试
pnpm test
```

### 启动已构建的桌面应用

```bash
# 推荐：使用兼容脚本（支持 WSL2 / 无 GPU 环境）
bash run-desktop.sh

# 或直接启动
./src-tauri/target/release/ai-coding-desktop
```

---

## 开发工作流

1. **新增页面**：在 `src/pages/` 创建组件 → 在 `src/routes.tsx` 注册。
2. **新增 UI 组件**：优先使用 `npx shadcn add <component>`，业务组件放在 `src/components/<domain>/`。
3. **修改数据库 schema**：同步更新 `src-tauri/src/setup/db.rs` 与 `crates/dh-db/src/schema.rs`。
4. **新增主题色**：修改 `src/App.tsx` 的 `themeColorMap` 与 `SettingsDialog.tsx`。
5. **新增 Agent 插件**：实现 `agent-core::plugin::AgentPlugin` 与 `AgentInstance`，在 `main.rs` 注册。

---

## 架构决策与改进方向

### 当前架构的优势

- **清晰的前后端分层**：React 负责 UI，Rust 负责数据与智能体生命周期，职责边界明确。
- **Agent 插件抽象**：`agent-core` 的 trait 设计使接入新智能体只需关注实例实现。
- **双模式数据库适配**：`src/db/index.ts` 在浏览器环境自动降级到 Mock，便于前端独立开发。
- **事件驱动流式输出**：WebSocket JSON-RPC 支持 token、thinking、interaction 等异步事件推送。
- **设计系统完整**：《DESIGN.md》定义了色彩、字体、间距、组件规范，保证 UI 一致性。

### 需要关注的架构问题

1. **数据模型分裂**
   - 主数据库 `app.db` 的 `conversations`/`messages` 带 `user_id`；智能体独立数据库的同名表不带 `user_id`。
   - 同一领域对象存在两套 schema，增加迁移与维护成本，建议统一数据 ownership。

2. **状态管理存在重复**
   - `chatStore` 与 `agentStore` 都维护 `activeInstanceId`。
   - 智能体持久化逻辑分散在 `SelectAgentPage`、`useAgentManagement` 与 `WorkspacePage` 中。
   - 建议由 `agentStore` 作为单一数据源，统一管理激活状态与持久化。

3. **通信通道冗余**
   - 数据库走 Tauri Invoke，智能体走 WebSocket，部分功能（如 `chatStore.sendMessage`）还存在 fallback invoke。
   - 建议明确通道职责：Tauri Invoke 用于请求-响应型操作；WebSocket 用于流式/异步事件。

4. **Cargo workspace 管理不一致**
   - 根目录 `Cargo.toml` 声明了 workspace members，但 `src-tauri/Cargo.toml` 又独立声明空 workspace。
   - `dh-desktop` 不在根 workspace 中，依赖版本存在重复声明风险，建议统一 workspace 管理。

5. **测试覆盖不足**
   - 前端测试集中在 store；Rust 后端测试较少；SSE 解析、Agent 生命周期等核心路径缺乏自动化测试。
   - 建议为核心 crate（`agent-core`、`opencode-plugin`、`dh-db`）补充单元测试。

6. **魔法值与硬编码**
   - 主题色、智能体列表、默认模型等存在多处硬编码。
   - 建议抽取到 `src/lib/constants.ts` 或配置文件，并通过类型约束使用。

7. **文件体积增长风险**
   - `src/components/workspace/LeftPanel.tsx` 等文件有效代码已接近 500 行，需持续拆分避免触及 600 行限制。

### 已完成的改进

- **数据库层统一**：`src-tauri/src/commands/db.rs` 与 `src-tauri/src/service/db_service.rs` 已委托给 `dh-db::desktop::AppRepository`；`src-tauri/src/agent_db.rs` 已委托给 `dh-db::desktop::AgentRepository`。`dh-db` crate 现在管理 desktop 与 agent 数据库的 schema 迁移和 CRUD。

### 剩余改进优先级建议

| 优先级 | 方向 | 目标 |
|--------|------|------|
| P1 | 收敛状态管理 | `agentStore` 统一负责智能体状态与持久化 |
| P1 | 统一 workspace | 将 `dh-desktop` 纳入根 Cargo workspace |
| P2 | 补充核心测试 | `opencode-plugin` 解析器、`dh-db` schema 迁移 |
| P2 | 抽取常量配置 | 主题色、默认模型、智能体元数据 |
| P3 | 文档同步 | 保持 README、DESIGN.md、AGENTS.md 与代码一致 |

---

## 相关文档

- [DESIGN.md](./DESIGN.md) — UI/UX 设计规范
- [AGENTS.md](./AGENTS.md) — AI 编程助手开发指南与约束
- [docs/prd.md](./docs/prd.md) — 产品需求文档
- [docs/bugs/](./docs/bugs/) — 缺陷记录
