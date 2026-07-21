# DeepHarness Gatewayd — 工程整改 TODO

> 基于 2026-07-21 工程审查结果，当前优先聚焦 **agent 生命周期**与 **session 管理**两大子系统的稳定性整改。其他问题（安全、构建、CORS、MCP 等）暂记为后续阶段。

---

## 当前最高优先级：P0

### 1. Agent 生命周期稳定性

- [x] 修复 `AgentService::create_instance` 实例复用逻辑：复用时不应忽略 `work_directory` 与 `name`
  - 文件：`crates/agent-core/src/service.rs:143-170`
- [x] 修复 `AgentService::send_message` 的 fire-and-forget 问题：捕获子进程发送失败并返回给调用方
  - 文件：`crates/agent-core/src/service.rs:197-217`
- [x] 修复 `AgentService::send_message` 内部的 `tokio::spawn` 结果丢弃问题
  - 文件：`crates/agent-core/src/service.rs:197-217`
- [x] 增加 agent 进程优雅退出与超时控制
  - `crates/agent-core/src/instance.rs` 新增 `graceful_shutdown` 默认方法
  - `crates/agent-core/src/service.rs` 新增 `stop_and_remove_instance_with_timeout` 与 `stop_all_instances_with_timeout`
  - `apps/gatewayd/src/session.rs::reap_expired` 已改用带超时停止
- [x] 修复 OpenCode 端口分配 TOCTOU 竞态
  - 文件：`crates/opencode-plugin/src/transport.rs:148-158`、`crates/opencode-plugin/src/instance.rs:95-117`
- [x] 增加 `work_directory` 路径校验，防止路径遍历
  - 文件：`crates/opencode-plugin/src/transport.rs:133-145`、`crates/claude-plugin/src/instance.rs:131`、`crates/codex-plugin/src/instance.rs:128`
- [x] 修复 MCP client 子进程不清理、reader task 泄漏
  - 文件：`crates/dh-core/src/mcp/client.rs:18-215`

### 2. Session 管理稳定性

- [x] 将 `SessionManager` 内部 `std::sync::Mutex` 替换为 `tokio::sync::RwLock` 或 `dashmap`
  - 文件：`apps/gatewayd/src/session.rs:143-144`
- [x] 将 `Session` 的 `broadcast::channel(1024)` 改为带反压或慢消费者保护策略
  - 文件：`apps/gatewayd/src/session.rs:49`
- [x] 修复 `SessionManager` 在 `reap_expired` 中 clone 整个 session 集合导致的阻塞
  - 文件：`apps/gatewayd/src/session.rs:447-455`
- [x] 修复 `session.json` 持久化静默失败问题：明确错误处理、日志、降级策略
  - 文件：`apps/gatewayd/src/session.rs:113-122`、`session.rs:124-138`
- [x] 统一 `create_agent` 与 `AgentService` 的复用/幂等语义
  - 文件：`apps/gatewayd/src/session.rs:219-223`、`crates/agent-core/src/service.rs:143-170`
- [x] 增加 session 创建/挂载 agent 的事务性或回滚机制
  - 文件：`apps/gatewayd/src/session.rs:206-253`
- [x] 修复 `session.json` 无 schema 版本问题，避免结构变化后无法兼容
  - 文件：`apps/gatewayd/src/session.rs:18`
- [x] 限制预热任务并发数，防止启动时同时创建大量子进程
  - 文件：`apps/gatewayd/src/main.rs:979-1000`

> **P0 验证结果**（2026-07-21）：
> - `cargo check -p dh-gatewayd` 通过
> - `cargo clippy --all-targets -p dh-gatewayd` 通过，无新增 warning（4 个为已有 warning）
> - `cargo test -p agent-core -p dh-core -p opencode-plugin -p claude-plugin -p codex-plugin -p dh-gatewayd` 全部通过（共 65 个测试）

---

## 后续优先级：P1（完成 P0 后处理）

### 3. Gateway 安全

- [x] 为 `/v1/chat/completions` 与 `/v1/messages` 增加鉴权中间件
  - 文件：`apps/gatewayd/src/auth.rs`、`apps/gatewayd/src/server.rs`
- [x] 收紧 `CorsLayer::permissive()` 为显式 origin 白名单
  - 文件：`apps/gatewayd/src/server.rs`
- [x] 校验 `work_directory` 防止路径遍历与 symlink 逃逸
  - 文件：`apps/gatewayd/src/workspace.rs`
- [x] MCP 命令白名单与 args 校验
  - 文件：`apps/gatewayd/src/mcp_aggregator.rs`
- [x] API key 安全存储与轮换机制
  - 文件：`apps/gatewayd/src/auth.rs`
  - 实现：优先读取 `GATEWAYD_API_KEY`，否则从 DB `configs` 表读取，首次启动自动生成并持久化；新增 `POST /admin/auth/rotate` 用于轮换

### 4. Gateway 稳定性

- [x] 审计日志从 `mpsc::unbounded_channel` 改为有界队列
  - 文件：`apps/gatewayd/src/audit.rs`
- [x] `/v1/chat/completions` 支持真正的流式转发（不 `to_bytes(usize::MAX)`）
  - 文件：`apps/gatewayd/src/gateway.rs`、`apps/gatewayd/src/handlers/chat.rs`
- [x] 请求/响应体大小限制
  - 文件：`apps/gatewayd/src/server.rs`、`apps/gatewayd/src/handlers/chat.rs`
- [x] WebSocket session events 增加心跳、超时、连接数限制
  - 文件：`apps/gatewayd/src/handlers/websocket.rs`
- [x] graceful shutdown（SIGTERM、停止接受新连接、等待 in-flight 请求）
  - 文件：`apps/gatewayd/src/server.rs`、`apps/gatewayd/src/main.rs`
- [x] 替换 `std::sync::Mutex<DbManager>` 为 async 友好连接池或 spawn_blocking
  - 文件：`apps/gatewayd/src/reporter/poller.rs`、`apps/gatewayd/src/audit.rs`

### 5. 代码质量

- [x] 拆分 `apps/gatewayd/src/main.rs`（979 有效行，超 600 行限制）
  - 新增：`apps/gatewayd/src/lib.rs`、`apps/gatewayd/src/server.rs`、`apps/gatewayd/src/gateway.rs`、`apps/gatewayd/src/handlers/chat.rs`、`apps/gatewayd/src/handlers/context.rs`、`apps/gatewayd/src/handlers/health.rs`
- [x] 统一错误处理，移除关键路径 `unwrap()`
  - 已 review 关键路径（session 持久化、workspace 校验、MCP 校验）并改用显式错误处理
- [x] 清理 `cargo clippy` gatewayd 相关 warning
- [x] 增加 gatewayd 集成测试
  - 文件：`apps/gatewayd/tests/integration_test.rs`
  - 覆盖：/health、鉴权 401、有效 key 通过、session 创建、非法 workspace 400

---

## 验收标准

- [x] `cargo clippy --all-targets -p dh-gatewayd` 无 gatewayd 新增 warning
- [x] `cargo test -p dh-gatewayd` 通过（16 个测试：12 单元 + 4 集成）
- [x] 不存在新的 `std::sync::Mutex` 阻塞 async 关键路径
- [x] agent 创建/复用/停止有明确错误返回与日志
- [x] session 创建、agent 挂载、run 启动、过期回收全链路可观测

---

## 备注

- 当前桌面端问题不在本次处理范围内（用户已明确排除）。
- rustc 1.95 目录模块 ICE 问题仍通过扁平文件结构规避。
- 其他 crate（dh-core、dh-db、agent-core）存在既有 clippy warning，不在本次 gatewayd 整改范围内。
