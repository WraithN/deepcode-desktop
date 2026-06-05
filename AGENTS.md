<!-- AGENTS.md — DeepHarness Desktop 项目指南 -->

> 本文件面向 AI 编程助手。阅读前请确认你已了解：本项目是一个**DeepHarness 桌面应用**（Web + Tauri 桌面端），使用中文作为界面和文档的主要语言。

---

## 1. 项目概述

- **名称**：`ai-coding-desktop`（DeepHarness Desktop）
- **类型**：React 单页应用（SPA），同时支持 Tauri v2 打包为桌面端程序
- **核心功能**：
  - 用户登录/注册（基于本地 SQLite）
  - 选择 AI 编码智能体（OpenCode、Claude Code、Cursor Agent、Codex、自定义）
  - 与 AI 进行多轮编码对话
  - 管理历史会话、任务列表、文件变更列表
  - 智能体配置、技能配置、提示词管理、工程规范编辑、MCP 服务器配置
- **目标用户**：需要 AI 辅助编码的开发者

---

## 2. 技术栈

| 层级 | 技术 |
|------|------|
| 构建工具 | Vite（实际使用 `rolldown-vite`）、Tauri v2 |
| 语言 | TypeScript 5.9（严格模式） |
| 前端框架 | React 18 + react-router-dom v7（HashRouter） |
| 样式 | Tailwind CSS 3.4 + shadcn/ui（New York 风格） |
| 图标 | Lucide React + @lobehub/icons |
| 状态管理 | Zustand（chatStore、agentStore、websocketStore、logStore 等） |
| 后端/数据库 | 本地 SQLite（Tauri + rusqlite） |
| 包管理器 | pnpm |
| 代码检查 | Biome 2.4.5 + ast-grep 自定义规则 + `tsgo` + Tailwind CSS 语法检查 |

---

## 3. 目录结构

```
├── src/
│   ├── App.tsx              # 根组件：主题色初始化、路由、AuthProvider 包裹
│   ├── main.tsx             # 应用入口（createRoot + ErrorBoundary）
│   ├── routes.tsx           # 路由配置表（RouteConfig[]）
│   ├── index.css            # Tailwind 指令 + CSS 变量（深色主题默认）
│   ├── pages/               # 页面级组件
│   │   ├── LoginPage.tsx
│   │   ├── SelectAgentPage.tsx
│   │   ├── WorkspacePage.tsx   # 核心工作区（三栏布局）
│   │   └── NotFound.tsx
│   ├── components/
│   │   ├── ui/              # shadcn/ui 组件（50+ 个，由 components.json 管理）
│   │   ├── workspace/       # 工作区专属组件
│   │   │   ├── LeftPanel.tsx
│   │   │   ├── ChatPanel.tsx
│   │   │   ├── RightPanel.tsx
│   │   │   ├── SettingsDialog.tsx
│   │   │   ├── AddAgentDialog.tsx
│   │   │   ├── AgentIcon.tsx
│   │   │   └── SessionLogDrawer.tsx
│   │   └── common/          # 通用组件
│   │       ├── PageMeta.tsx
│   │       ├── ErrorBoundary.tsx
│   │       ├── RouteGuard.tsx
│   │       ├── WindowTitleBar.tsx
│   │       └── IntersectObserver.tsx
│   ├── contexts/
│   │   └── AuthContext.tsx  # 认证上下文：useAuth / AuthProvider
│   ├── db/                  # 数据库适配层（双模式：Mock / Tauri）
│   │   ├── index.ts         # 自动检测 Tauri 环境并切换实现
│   │   ├── mock.ts          # localStorage 模拟存储
│   │   ├── tauri-client.ts  # Tauri invoke 调用 SQLite
│   │   └── types.ts         # IDataStore 接口定义
│   ├── stores/              # Zustand 状态管理
│   │   ├── chatStore.ts     # 聊天状态与消息发送
│   │   ├── agentStore.ts    # 智能体实例管理
│   │   ├── websocketStore.ts# WebSocket JSON-RPC 连接管理
│   │   ├── logStore.ts      # 会话日志
│   │   └── sessionWsStore.ts# 会话级 WebSocket
│   ├── store/               # 前端 session-log 存储（与 logStore 配合使用）
│   │   └── session-log.ts
│   ├── services/            # 服务层
│   │   ├── opencode.ts      # OpenCode HTTP 客户端（遗留）
│   │   ├── logger.ts        # 前端日志服务
│   │   └── debug-logger.ts  # 调试日志
│   ├── hooks/               # 通用自定义 Hooks
│   ├── lib/                 # 工具函数
│   ├── types/
│   │   └── types.ts         # 业务类型定义
│   └── test/setup.ts        # Vitest 测试初始化（Mock Tauri API）
├── src-tauri/               # Tauri 桌面端（Rust）
│   ├── Cargo.toml           # Rust 工作区（主包 + 3 个 crate）
│   ├── tauri.conf.json      # Tauri 配置（无装饰窗口 1500x900）
│   ├── src/
│   │   ├── main.rs          # 应用入口：DB 初始化、WebSocket 服务、插件注册
│   │   ├── lib.rs           # DbState 定义、模块导出
│   │   ├── agent_db.rs      # Agent 相关数据库操作
│   │   ├── commands/        # Tauri 命令（system、session_log）
│   │   ├── gateway/         # WebSocket JSON-RPC 网关
│   │   │   ├── server.rs    # WebSocket 服务器（tokio-tungstenite）
│   │   │   ├── router.rs    # 请求路由（agent/session/db）
│   │   │   ├── connection.rs# 连接管理
│   │   │   ├── codec.rs     # JSON-RPC 编解码
│   │   │   ├── session_manager.rs
│   │   │   └── handlers/    # 请求处理器
│   │   ├── service/         # 业务服务
│   │   │   ├── agent_service.rs    # 智能体生命周期管理
│   │   │   ├── db_service.rs       # 数据库服务封装
│   │   │   ├── opencode_service.rs # OpenCode CLI 调用服务
│   │   │   ├── plugin_registry.rs  # 插件注册表
│   │   │   └── instance_registry.rs# 实例注册表
│   │   └── models/          # 数据模型
│   └── crates/              # Rust 工作区 crate
│       ├── agent-core/      # 核心抽象：AgentPlugin、AgentInstance、MCP 协议、SessionLogger
│       ├── agent-runtime/   # 进程管理、健康检查
│       └── opencode-plugin/ # OpenCode CLI 适配器、解析器、MCP 适配器
├── .rules/                  # ast-grep 自定义规则（lint 时必须通过）
├── docs/                    # 需求文档（prd.md）、bug 记录、设计文档
├── public/                  # 静态资源（字体、图片、favicon）
└── dist/ / .dist/           # 构建输出
```

---

## 4. 构建与运行命令

```bash
# 安装依赖
pnpm i

# 开发服务器（固定端口 5173，127.0.0.1，使用 vite.config.dev.ts）
pnpm dev

# 生产构建（先 tsc --noEmit 类型检查，再 vite build）
pnpm build

# 代码检查（复合命令，包含多层检查）
pnpm lint
# 具体执行：
#   tsgo -p tsconfig.check.json
#   npx biome lint
#   .rules/check.sh        （ast-grep 扫描自定义规则）
#   tailwindcss 语法检查
#   .rules/testBuild.sh    （vite build --minify false 测试编译）

# 单元测试
pnpm test                   # vitest run
pnpm test:watch             # vitest watch 模式

# Tauri 桌面端
pnpm tauri dev              # 开发模式
pnpm tauri build            # 打包桌面应用
```

### 开发后启动测试
> ⚠️ **硬性规则**：每次完成代码修改并构建完成后，**必须启动 Tauri 桌面应用**，让用户在真实环境中验证功能。

```bash
# 构建桌面端
pnpm tauri build

# 启动已构建的应用（Linux）
# 方法1：使用启动脚本（推荐，兼容无 GPU 环境）
bash run-desktop.sh

# 方法2：直接启动（需要 GPU 支持）
./src-tauri/target/release/ai-coding-desktop
```

启动后等待应用窗口出现，再告知用户进行测试。

#### 启动脚本说明
项目根目录包含 `run-desktop.sh` 启动脚本，用于在 WSL2、虚拟机或无 GPU 环境下启动桌面应用：

```bash
#!/bin/bash
# 启动 DeepHarness Desktop（兼容无 GPU 环境）
export GDK_BACKEND=x11
export LIBGL_ALWAYS_SOFTWARE=1          # 强制使用软件渲染
export WEBKIT_DISABLE_DMABUF_RENDERER=1  # 禁用 DMA-BUF 渲染器

./src-tauri/target/release/ai-coding-desktop "$@"
```

**使用场景**：
- WSL2 环境（WebKit GPU 加速不兼容）
- 远程服务器/无头环境
- 虚拟机环境
- 任何出现 GPU 渲染错误的场景

### 环境要求
- Node.js ≥ 20
- npm ≥ 10
- pnpm（推荐）
- Rust + Tauri 依赖（如需构建桌面端）

---

## 5. 代码规范与开发约定

### 5.1 TypeScript 配置要点
- `strict: true`，`noUnusedLocals: true`，`noUnusedParameters: true`
- `moduleResolution: bundler`，`allowImportingTsExtensions: true`
- `noEmit: true`（由 Vite 负责编译）
- 路径别名：`@/*` → `./src/*`

### 5.2 Biome Linter 规则
- 仅启用 linter，**禁用 formatter**（项目不依赖 Biome 格式化）
- 关键规则：
  - `correctness/noUndeclaredDependencies` — error
  - `suspicious/noRedeclare` — error
  - `style/noCommonJs` — error（`tailwind.config.js` 除外）

### 5.3 ast-grep 自定义规则（`.rules/`）
运行 `pnpm lint` 时会自动执行 `.rules/check.sh`。以下规则**必须全部通过**，否则 CI/本地 lint 失败：

| 规则文件 | 说明 |
|----------|------|
| `contrast.yml` | Button 的 `variant="outline"` 时禁止 `text-foreground`；`variant="default"` 时禁止 `text-primary`；outline 时禁止 `text-white/gray` |
| `require-button-interaction.yml` | `<Button>` 必须有交互：`onClick` / `type="submit"` / `type="reset"` / `asChild`，或包裹在 `*Trigger` / `<Link>` / `<a>` 中 |
| `SelectItem.yml` | `SelectItem` 的 `value` 禁止空字符串 `""`（运行时报错），如需"全选"用 `"all"` |
| `slot-nesting.yml` | 禁止 Radix UI `*Trigger asChild` 内直接包裹 `FormControl`（会导致 ref/事件丢失） |
| `toast-hook.yml` | 禁止使用 `@/hooks/use-toast`，统一使用 `sonner` 的 `toast` |

### 5.4 Tailwind CSS 约定
- 使用 CSS 变量定义主题色（`hsl(var(--primary))` 等）
- 默认主题为**深色 VS Code 风格**（`--background: 222 47% 11%`）
- 支持 5 种主题色切换：blue（默认）、green、orange、purple、pink
- 自定义工具类：`.border-t-solid`、`.border-r-dashed` 等单边边框样式
- 字体：JetBrains Mono + Noto Sans SC/JP/KR（CJK 子集化）

### 5.5 组件与代码风格
- 函数式组件 + Hooks，不使用类组件
- UI 组件统一放在 `src/components/ui/`，由 shadcn/ui 管理
- 页面组件放在 `src/pages/`
- 业务组件按功能域分组（如 `src/components/workspace/`）
- 类型定义放在 `src/types/types.ts`
- `cn()` 工具函数用于合并 Tailwind 类名（来自 `clsx` + `tailwind-merge`）

---

## 6. 认证与数据模型

### 6.1 认证方式
- 基于本地 SQLite 认证
- 用户名登录：前端将用户名拼接为 `${username}@local.dev` 后调用本地数据库认证
- 支持**模拟登录**（`mockSignIn`）：输入任意用户名即可登录，使用本地 SQLite 数据库或纯内存 mock 模式
- 用户信息关联 `profiles` 表

### 6.2 核心数据表
| 表名 | 说明 |
|------|------|
| `profiles` | 用户资料（id, username, email, phone, role） |
| `conversations` | 会话（user_id, title, agent, model） |
| `messages` | 消息（conversation_id, role, content, steps, token_in/out, duration_ms） |
| `tasks` | 任务（user_id, conversation_id, title, status） |
| `modified_files` | 文件变更（user_id, conversation_id, file_path, change_type, diff） |

### 6.3 数据库适配层（双模式）
`src/db/index.ts` 自动检测 Tauri 环境：
- **Tauri 模式**：通过 `tauri-client.ts` 调用 Rust 命令操作 SQLite
- **Mock 模式**：通过 `mock.ts` 使用 localStorage 模拟数据（用于浏览器开发/测试）

---

## 7. 测试策略

- **测试框架**：Vitest + jsdom + `@testing-library/react` + `@testing-library/jest-dom`
- **配置**：`vitest.config.ts` — globals 启用，setup 文件 `src/test/setup.ts`
- **Setup**：Mock `__TAURI_INTERNALS__` 以兼容 Tauri API 测试
- **当前测试**：仅 `src/store/session-log.test.ts`（5 个测试，验证会话日志存储）
- **构建验证**：`.rules/testBuild.sh` 执行 `vite build --minify false` 作为编译冒烟测试
- **无 E2E/Playwright/Cypress** 配置

---

## 8. 部署与发布

### Web 部署
- 运行 `pnpm build` 生成 `dist/` 目录
- `dist/` 为静态文件，可部署到任意静态托管服务

### 桌面端部署
- Tauri 配置文件：`src-tauri/tauri.conf.json`
- 构建命令：`pnpm tauri build`
- 输出目标：Windows（`.msi`/`.exe`）、macOS（`.app`/`.dmg`）、Linux（`.deb`/`.AppImage`）
- CSP 已配置，仅允许 `self`、本地 IPC、`https:` 及 `localhost:*`

---

## 9. 安全注意事项

- **本地数据安全**：所有数据存储在本地 SQLite 数据库中，不涉及云端 API 密钥
- 本地 SQLite 数据库存储在用户应用数据目录，无需网络连接
- 本地 SQLite 数据库由 Rust 后端管理，数据存储在用户应用数据目录
- Tauri 的 CSP 限制了外部脚本来源，开发时若需加载外部资源需同步调整 `tauri.conf.json` 中的 `csp`
- `vite.config.dev.ts` 中注入了来自 CDN 的监控脚本和注入脚本（`resource-static.cdn.bcebos.com`），生产构建不加载

---

## 10. 关键架构模式

### 10.1 WebSocket JSON-RPC 网关
Rust 后端启动本地 WebSocket 服务器（随机端口），前端通过 `websocketStore.ts` 连接。网关路由：
- `agent.*` → 智能体生命周期（createInstance、sendMessage、stopInstance）
- `session.*` → 会话管理
- `db.*` → 数据库操作

### 10.2 Agent 插件系统
Rust 工作区 crate 提供插件架构：
- `agent-core`：抽象定义、MCP 客户端/协议/传输、SessionLogger
- `agent-runtime`：进程管理、健康检查
- `opencode-plugin`：OpenCode CLI 适配器、解析器、MCP 适配器

### 10.3 Zustand 状态管理
- `chatStore`：消息、流式状态、会话管理
- `agentStore`：智能体实例、生命周期、状态
- `websocketStore`：WebSocket 连接管理（自动重连、JSON-RPC 请求/通知）
- `logStore` / `session-log.ts`：会话日志（500 条上限，pub/sub 模式）

### 10.4 工作区文件系统
Tauri 命令提供工作区文件操作：
- `list_workspace_tree`：读取目录树（尊重 `.gitignore`，最大深度 4）
- `read_workspace_file`：文件读取（图片自动 base64 编码，512KB 截断）
- `git_status_workspace` / `git_changed_files`：Git 状态与 diff 统计

### 10.5 主题系统
- 5 种颜色主题存储在 `localStorage.theme_color`
- 应用通过 CSS 自定义属性在 `App.tsx` useEffect 中设置
- `.dark` 和 `.light` 类变体均在 `index.css` 中定义

---

## 11. 关键文件速查

| 文件 | 用途 |
|------|------|
| `src/routes.tsx` | 定义所有路由，新增页面需在此注册 |
| `src/App.tsx` | 根组件，初始化主题色，包裹 Router + AuthProvider |
| `src/contexts/AuthContext.tsx` | 认证状态、登录/注册/登出逻辑 |
| `src/db/index.ts` | 数据库适配层入口，自动选择 Mock 或 SQLite 实现 |
| `src/types/types.ts` | 全部业务类型定义 |
| `src/components/ui/` | shadcn/ui 组件库（不要直接修改内部逻辑，优先在外部封装） |
| `src/stores/` | Zustand 状态管理（chatStore、agentStore、websocketStore 等） |
| `.rules/*.yml` | ast-grep 自定义规则，新增代码必须满足这些规则 |
| `vite.config.dev.ts` | 开发专属插件（HMR 开关、错误监控、编辑器联动） |
| `src-tauri/src/main.rs` | Rust 应用入口：DB、WebSocket、AgentService 初始化 |
| `src-tauri/src/gateway/` | WebSocket JSON-RPC 网关 |
| `src-tauri/crates/` | Agent 插件系统 Rust crate |

---

## 12. 开发提示

- 新增页面：在 `src/pages/` 创建组件 → 在 `src/routes.tsx` 注册
- 新增 shadcn/ui 组件：使用 `npx shadcn add <component>`（需确认 CLI 可用）
- 修改数据库 schema：同步更新 `src-tauri/src/main.rs` 中的 `init_db` 函数
- 主题色切换逻辑在 `App.tsx` 的 `useEffect` 和 `SettingsDialog.tsx` 中，新增颜色需同时修改两处
- `localStorage` 中存储了大量前端状态（智能体实例、主题、提示词、技能等），修改相关逻辑时注意兼容旧数据格式
- 智能体实例数据通过 `localStorage.getItem('agent_instances')` 持久化，旧数据兼容逻辑在 `WorkspacePage.tsx` 的 `getStoredAgents()` 中

---

## 13. 用户自定义规则（不可覆盖）

以下规则为硬性约束，在任何代码变更或 AGENTS.md 更新中**必须保留**，不得删除或修改：

### 规则1：自动化编译与启动
每次需求开发或缺陷修复完成后，必须自动执行编译并启动应用：
1. 运行 `pnpm tauri build` 构建桌面端
2. 使用 `bash run-desktop.sh` 启动应用（兼容无 GPU 环境）
3. 等待应用窗口出现，确认功能正常后再告知用户

### 规则2：缺陷排查流程
如果无法定位缺陷根因：
1. 在相关代码路径中增加详细日志输出（使用 `console.log` 或项目日志服务）
2. 要求用户在真实环境中进行测试操作
3. 通过观察用户测试产生的日志来分析和排查问题
4. 根据日志反馈迭代修复

### 规则3：缺陷文档化
所有缺陷修改必须同步记录到 `docs/bugs/` 目录：
- 文件名格式：`YYYY-MM-DD-<brief-description>.md`
- 必须包含三部分内容：
  1. **现象**：缺陷的具体表现和影响范围
  2. **根因**：导致缺陷的根本原因分析
  3. **解决方案**：修复措施和验证结果

### 规则4：代码嵌套限制
代码中最多不超过 3 层嵌套：
- 当嵌套超过三层时，必须进行以下优化之一：
  - **小函数提取**：将嵌套逻辑提取为独立函数
  - **Guard Clause**：使用提前返回（early return）减少嵌套层级
- 目标：保持主流程清晰可读，避免深层嵌套导致的认知负担

### 规则5：复杂逻辑注释
复杂的业务逻辑必须添加详细的注释：
- **必须注释的场景**：
  - 涉及多步状态转换的流程
  - 非直观的算法或计算逻辑
  - 与外部系统交互的边界处理
  - 存在特殊 case 或容错处理的代码块
- **注释要求**：
  - 对关键变量和条件判断给出上下文解释
  - 如果逻辑有已知限制或 TODO，必须明确标注

### 规则6：重复逻辑封装
同一逻辑在代码中出现超过两处时，必须封装为小函数：
- **判定标准**：相同的代码片段或逻辑模式在项目中出现 ≥2 次
- **封装要求**：
  - 提取为语义清晰的命名函数（动词开头，描述行为）
  - 将函数放置在合适的模块或工具文件中（如 `src/lib/utils.ts` 或相关领域目录）
  - 通过参数化提高复用性，避免为相似逻辑创建多个几乎相同的函数
- **例外**：UI 层极简单的 JSX 重复（如纯样式类名组合）可酌情处理，但业务逻辑必须严格遵循

### 规则7：禁止魔法值
代码中不允许出现魔法值（Magic Values），所有字面量必须提取为常量：
- **必须提取的字面量**：
  - 数字（如超时时间、分页大小、状态码、阈值等）
  - 字符串（如错误消息、路由路径、localStorage key、API 端点等）
  - 布尔值组合或标志位
- **常量组织**：
  - 模块级常量：放在使用文件顶部或同目录 `constants.ts` 中
  - 全局常量：放在 `src/lib/constants.ts` 或 `src/config/` 中
  - 常量命名使用 UPPER_SNAKE_CASE（如 `MAX_RETRY_COUNT`、`DEFAULT_PAGE_SIZE`）
- **例外**：
  - `0`、`1`、`-1` 在明显上下文中的使用（如数组索引 `arr[0]`）
  - `true`/`false` 在简单条件判断中
  - 纯 UI 展示用的临时字符串（如调试日志中的分隔符 `"---"`）

### 规则8：规则持久化
以上七条规则（规则1-7）在变更 AGENTS.md 时必须保留，不允许被覆盖、删除或修改。任何对 AGENTS.md 的更新都应在保留这些规则的前提下进行追加或调整其他内容。
