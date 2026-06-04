# AGENTS.md — DeepHarness Desktop 项目指南

> 本文件面向 AI 编程助手。阅读前请确认你已了解：本项目是一个**DeepHarness 桌面应用**（Web + Tauri 桌面端），使用中文作为界面和文档的主要语言。

---

## 1. 项目概述

- **名称**：`ai-coding-desktop`（DeepHarness Desktop）
- **类型**：React 单页应用（SPA），同时支持 Tauri 打包为桌面端程序
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
| 状态管理 | React Context（AuthContext）+ `localStorage` + SQLite |
| 后端/数据库 | 本地 SQLite（Tauri + rusqlite） |
| 包管理器 | pnpm |
| 代码检查 | Biome 2.4.5 + ast-grep 自定义规则 + `tsgo` + Tailwind CSS 语法检查 |

---

## 3. 目录结构

```
├── src/
│   ├── App.tsx              # 根组件：主题色初始化、路由、AuthProvider 包裹
│   ├── main.tsx             # 应用入口（createRoot）
│   ├── routes.tsx           # 路由配置表（RouteConfig[]）
│   ├── index.css            # Tailwind 指令 + CSS 变量（深色主题默认）
│   ├── pages/               # 页面级组件
│   │   ├── LoginPage.tsx
│   │   ├── SelectAgentPage.tsx
│   │   ├── WorkspacePage.tsx   # 核心工作区（三栏布局）
│   │   ├── SamplePage.tsx
│   │   └── NotFound.tsx
│   ├── components/
│   │   ├── ui/              # shadcn/ui 组件（50+ 个，由 components.json 管理）
│   │   ├── workspace/       # 工作区专属组件
│   │   │   ├── LeftPanel.tsx
│   │   │   ├── ChatPanel.tsx
│   │   │   ├── RightPanel.tsx
│   │   │   ├── SettingsDialog.tsx
│   │   │   ├── AddAgentDialog.tsx
│   │   │   └── AgentIcon.tsx
│   │   ├── common/          # 通用组件
│   │   │   ├── PageMeta.tsx    # HelmetProvider + TooltipProvider 包裹
│   │   │   ├── RouteGuard.tsx  # 未登录跳转（当前未在路由中显式使用）
│   │   │   └── IntersectObserver.tsx
│   │   ├── DirectoryPickerButton.tsx
│   │   └── dropzone.tsx
│   ├── contexts/
│   │   └── AuthContext.tsx  # 认证上下文：useAuth / AuthProvider
│   ├── db/
│   │   └── index.ts         # 数据库适配层入口（Mock / SQLite）
│   ├── hooks/               # 通用自定义 Hooks
│   │   ├── use-mobile.tsx
│   │   ├── use-debounce.ts
│   │   ├── use-go-back.ts
│   │   └── use-file-upload.ts
│   ├── lib/
│   │   └── utils.ts         # cn()、createQueryString()、formatDate()
│   ├── types/
│   │   ├── index.ts         # Option 接口
│   │   └── types.ts         # 业务类型：Profile、Conversation、Message、Task 等
│   ├── services/            # 数据交互层（目前仅 .keep 占位）
│   └── global.d.ts          # 百度地图 GL 全局类型声明
├── src-tauri/               # Tauri 桌面端配置（Rust + SQLite）
│   ├── migrations/
│   │   └── migration.sql    # 完整数据库迁移（含角色、表、函数、触发器、RLS）
│   └── Cargo.toml           # Rust 后端配置（含 rusqlite）
├── src-tauri/               # Tauri 桌面端配置
│   ├── tauri.conf.json
│   └── Cargo.toml
├── .rules/                  # ast-grep 自定义规则（lint 时必须通过）
├── public/
│   └── images/              # 静态图片资源（404 错误页等）
├── package.json
├── pnpm-lock.yaml
├── pnpm-workspace.yaml
├── vite.config.ts           # 生产构建配置（含 Tauri 相关设置）
├── vite.config.dev.ts       # 开发配置（HMR 控制、监控插件、注入脚本）
├── tsconfig.json            # 项目引用配置
├── tsconfig.app.json        # 应用端 TS 配置（strict, noUnusedLocals, bundler mode）
├── biome.json               # Biome linter 配置（禁用 formatter，启用自定义规则）
├── tailwind.config.js       # Tailwind 配置（暗色主题、CSS 变量、自定义动画）
├── components.json          # shadcn/ui 配置（别名、baseColor: slate）
└── .env                     # VITE_APP_ID
```

---

## 4. 构建与运行命令

```bash
# 安装依赖
pnpm i

# 开发服务器（固定端口 5173，127.0.0.1）
pnpm dev
# 或显式指定 host
npm run dev -- --host 127.0.0.1

# 生产构建（先 tsc --noEmit 类型检查，再 vite build）
pnpm build

# 代码检查（复合命令，包含多层检查）
pnpm lint
# 具体执行：
#   tsgo -p tsconfig.check.json
#   npx biome lint
#   .rules/check.sh        （ast-grep 扫描自定义规则）
#   tailwindcss 语法检查
#   .rules/testBuild.sh    （ vite build 测试编译是否通过）

# Tauri 桌面端
pnpm tauri dev      # 开发模式
pnpm tauri build    # 打包桌面应用
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
export LIBGL_ALWAYS_SOFTWARE=1          # 强制使用软件渲染
export WEBKIT_DISABLE_COMPOSITING_MODE=1 # 禁用 WebKit 合成模式
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
| `SelectItem.yml` | `SelectItem` 的 `value` 禁止空字符串 `""`（运行时报错），如需“全选”用 `"all"` |
| `slot-nesting.yml` | 禁止 Radix UI `*Trigger asChild` 内直接包裹 `FormControl`（会导致 ref/事件丢失） |


| `toast-hook.yml` | 禁止使用 `@/hooks/use-toast`，统一使用 `sonner` 的 `toast` |

### 5.4 Tailwind CSS 约定
- 使用 CSS 变量定义主题色（`hsl(var(--primary))` 等）
- 默认主题为**深色 VS Code 风格**（`--background: 222 47% 11%`）
- 支持 5 种主题色切换：blue（默认）、green、orange、purple、pink
- 自定义工具类：`.border-t-solid`、`.border-r-dashed` 等单边边框样式

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
- 用户信息关联 `profiles` 表（通过 `handle_new_user` 触发器自动创建）

### 6.2 核心数据表
| 表名 | 说明 |
|------|------|
| `profiles` | 用户资料（id, username, email, phone, role） |
| `conversations` | 会话（user_id, title, agent, model） |
| `messages` | 消息（conversation_id, role, content, steps, token_in/out, duration_ms） |
| `tasks` | 任务（user_id, conversation_id, title, status） |
| `modified_files` | 文件变更（user_id, conversation_id, file_path, change_type, diff） |
| `user_settings` | 用户设置（agent, model, theme, skills） |

---

## 7. 测试策略

- **当前项目未配置单元测试框架**（无 Vitest/Jest/Playwright/Cypress 配置）
- `pnpm lint` 中的 `.rules/testBuild.sh` 会执行一次**编译测试构建**（`vite build --minify false`），确保 TypeScript 和 Vite 构建不报错
- 建议新增功能时：
  1. 先通过 `pnpm lint` 确保类型检查和 ast-grep 规则通过
  2. 手动在浏览器/Tauri 中验证交互

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

## 10. 关键文件速查

| 文件 | 用途 |
|------|------|
| `src/routes.tsx` | 定义所有路由，新增页面需在此注册 |
| `src/App.tsx` | 根组件，初始化主题色，包裹 Router + AuthProvider |
| `src/contexts/AuthContext.tsx` | 认证状态、登录/注册/登出逻辑 |
| `src/db/index.ts` | 数据库适配层入口，自动选择 Mock 或 SQLite 实现 |
| `src/types/types.ts` | 全部业务类型定义 |
| `src/components/ui/` | shadcn/ui 组件库（不要直接修改内部逻辑，优先在外部封装） |
| `.rules/*.yml` | ast-grep 自定义规则，新增代码必须满足这些规则 |
| `vite.config.dev.ts` | 开发专属插件（HMR 开关、错误监控、编辑器联动） |

---

## 11. 开发提示

- 新增页面：在 `src/pages/` 创建组件 → 在 `src/routes.tsx` 注册 → 如有需要，在 `src/App.tsx` 的 `<Routes>` 中使用
- 新增 shadcn/ui 组件：使用 `npx shadcn add <component>`（需确认 CLI 可用）
- 修改数据库 schema：同步更新 `src-tauri/src/main.rs` 中的 `init_db` 函数
- 主题色切换逻辑在 `App.tsx` 的 `useEffect` 和 `SettingsDialog.tsx` 的 `applyThemeColor` 中，新增颜色需同时修改两处
- `localStorage` 中存储了大量前端状态（智能体实例、主题、提示词、技能等），修改相关逻辑时注意兼容旧数据格式
