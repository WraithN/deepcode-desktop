# 10 分钟空闲后 Agent 丢失上下文（reaper 过早 reap + resume 链路断裂）

## 现象

用户在"智能会话"中间隔超过 10 分钟再次发消息时，Agent 不记得之前对话内容：

- **claude-code**：子进程冷启动，完全没有前几轮历史，等于"新对话"
- **opencode**：POST /session 建的是空 session，子进程看不到之前的工具调用、文件读取记录
- 前端展示的对话历史只来自 gatewayd 侧持久化（消息流），但 Agent 进程侧的"上下文"是真丢

## 根因

三个独立 bug 叠加：

### 根因 A：reaper 用单一 last_input_at 判定过期

`apps/gatewayd/src/session.rs` 的 `is_expired()` 只看 `last_input_at`，不看 `last_run_end_at`。

- 用户发消息后，run 跑了一段时间才结束，`end_run()` 复位 `run_active=false` 瞬间，`last_input_at` 已是 run 开始前的时间
- reaper 下一轮 tick 立刻判过期，instance 被 reap
- 更糟的情况：用户发完消息，5 分钟内收到回复，再过 6 分钟--`last_input_at` 已 11 分钟，直接被 reap
- `touch()` 只在 `start_run` 入口调，run 中途不会刷--长 run 跑得越久，结束后被 reap 的概率越高

### 根因 B：claude 的 --resume 链路断裂

链路全程断裂：

1. `CreateInstanceRequest`（`models.rs`）无 `session_id` 字段--结构上无法传递
2. `InstanceConfig::new()`（`instance.rs`）硬编码 `session_id: None`
3. `create_agent`（`session.rs`）构建 `CreateInstanceRequest` 时无 `session_id` 可填
4. `claude-plugin/instance.rs:132-134` 的 `--resume` 分支存在但 `config.session_id` 永远 None--死代码
5. claude 运行时从 init 事件捕获的 `active_session_id` 纯内存，无持久化，reap 后丢失

### 根因 C：opencode session 续接无机制

- `ConversationSessionMap`（`session_map.rs`）纯内存，instance 被 reap 后映射丢失
- `create_opencode_session()` 调 `POST /session`（空 body）建全新空 session
- opencode 协议无 resume/fork/continue 机制，session 与 `opencode serve` 进程绑定
- 进程死亡 = session 死亡 = 上下文丢失，无法恢复

## 解决方案

### 修复 A：reaper 加 last_run_end_at 守卫（P0，已完成）

文件：`apps/gatewayd/src/session.rs`

- `Session` 结构体加 `last_run_end_at: Arc<Mutex<Instant>>` 字段
- `end_run()` 在复位 `run_active` 时刷新 `last_run_end_at`
- `is_expired()` 改为 `max(last_input_at, last_run_end_at)` 判定
- 新增测试 `long_run_end_prevents_immediate_expiry`：验证长 run 结束后 60s 内不被 reap

### 修复 B：claude --resume 链路补全（P1，已完成）

涉及 5 个文件：

1. **`models.rs`**：`CreateInstanceRequest` 加 `session_id: Option<String>` 字段（`#[serde(default)]`）
2. **`instance.rs`**：`InstanceConfig` 加 `with_session_id()` builder 方法；`AgentInstance` trait 加 `active_session_id()` 方法（默认 None）
3. **`service.rs`**：`create_instance` 用 `config.with_session_id(req.session_id)` 传播；新增 `active_session_id(instance_id)` 访问方法
4. **`claude-plugin/instance.rs`**：实现 `active_session_id()` trait 方法，返回从 init 事件捕获的 session_id
5. **`session.rs`（gatewayd）**：
   - `WorkspaceEntry` 加 `agent_session_id: Option<String>` 字段（持久化到 sessions.json）
   - `create_agent` 从持久化存储读取 `agent_session_id`，填入 `CreateInstanceRequest.session_id`
   - `reap_expired` 在杀实例前调 `agent_service.active_session_id()` 捕获并持久化
6. **`src-tauri/src/gateway/handlers/agent.rs`**：构造 `CreateInstanceRequest` 时补 `session_id: None`

### 修复 C：opencode session 续接（P1，待调研）

调研结论：**opencode 无 resume 机制**。

- `POST /session` 建新 session，无 parent_id / fork 参数
- `POST /session/{id}/message` 只接受消息，无 batch 注入历史
- session 与 `opencode serve` 进程绑定，进程死亡 = session 死亡
- `reset_and_restart()` 清空 `session_map`，新建空 session

**缓解措施**：Fix A（防止过早 reap）是当前最佳缓解--只要 instance 不被 reap，session 就活着。后续可探索：
- opencode 本地存储（`~/.opencode/storage/`）是否可被新进程复用
- 消息历史回放（需 opencode 协议支持 batch 注入）

## 验证结果

- `cargo check --bin dh-gatewayd`、`cargo check --lib -p agent-core`、`cargo check --manifest-path src-tauri/Cargo.toml` 编译通过，0 warnings
- `cargo test -p dh-gatewayd --lib`：29 个测试全部通过（含新增 `long_run_end_prevents_immediate_expiry`）
- `cargo test -p claude-plugin`：6 个测试全部通过
- 待用户在真实环境中验证：间隔 10 分钟后继续对话，claude-code 能通过 --resume 恢复上下文
