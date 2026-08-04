# reaper/看门狗重建后上下文丢失（persist_workspace 覆盖 + 看门狗新建 session）

## 现象

用户在同一个 threadId（gatewayd sessionId）内连续对话，任何一次实例重建后下次发消息都失忆：

### 场景 A：reaper 回收后失忆

- 16:21 跑 /proto-make，AI 正常生成原型
- 16:33 reaper 回收实例（10 分钟空闲）
- 17:56 发"你觉得做的如何审查一下"，AI 像失忆一样，完全不知道之前的真机列表状态文案优化任务

### 场景 B：看门狗重建时丢失 opencode 端历史

- 22:22 / 22:25 / 22:32 三次 run，每次 opencode 进程被看门狗杀后都用新 session id
- 22:22 -> ses_051be9940...
- 22:25 看门狗重建 -> ses_051b9554...（新 session id，22:22 那个的 opencode 端历史不再被使用）
- 22:32 又重建 -> ses_051b6da40...（新 session id，22:25 那个的 opencode 端历史不再被使用）

### 数据印证

`/home/dh/.dh-gatewayd/sessions.json` 里 2f5cec22 的 `agent_session_id` 是 17:56 的
`ses_04d8b7166...`，16:21 的 `ses_04de31248...` 已被覆盖丢失。

### 日志印证

`grep -c "resuming persisted" /home/dh/gatewayd.log = 0`，整个 27MB 日志里
instance.rs 的 resume 分支从来没被走过。

## 根因

上一轮修复（见 `2026-07-28-reaper-and-resume-context-loss.md`）添加了
`agent_session_id` 持久化与 `initial_session_id` 恢复的基础设施，但三处实现缺陷
导致 resume 链路全程断裂：

### 根因 1：persist_workspace 清空 agent_session_id 字段

`apps/gatewayd/src/session.rs` 的 `persist_workspace` 用 `HashMap::insert` 整体覆盖
entry，硬编码 `agent_session_id: None`，把 reaper 在 `reap_expired` 中写入的值擦掉。

`create_agent` 的调用顺序也错：先 `persist_workspace`（写 None），后
`load_agent_session_id`（读到 None）。即使 persist_workspace 不覆盖，顺序也应
"先读后写"。

### 根因 2：看门狗重建时调 create_opencode_session() 创建新 session

`crates/opencode-plugin/src/instance.rs` 的 `send_with_watchdog_retry` 在重试时
调用 `create_opencode_session()`，该函数调 `POST /session` 永远返回新 id。
opencode 把 session 持久化到磁盘，同一 ses_xxx 在新进程里通过
`POST /session/{old_id}/message` 还能续上历史，但 gatewayd 不再发消息到旧 session，
历史等于丢失。

### 根因 3：resume 分支从未被走到

由于根因 1 导致 `load_agent_session_id` 始终返回 None，`initial_session_id` 为
None，`send_message` 走 `create_opencode_session()` 新建分支，instance.rs 中
"resuming persisted session" 的日志从未输出。

## 解决方案

### 修复 1：persist_workspace 保留 agent_session_id 字段

文件：`apps/gatewayd/src/session.rs`

将 `HashMap::insert` 改为 `entry().or_insert_with()`，仅更新 `workspace_path` 和
`last_used`，不触碰 `agent_session_id`。

### 修复 2：调换 create_agent 里 load 和 persist 的顺序

文件：`apps/gatewayd/src/session.rs`

先 `load_agent_session_id` 读取 reaper 持久化的值，后 `persist_workspace` 写入
workspace 映射。

### 修复 3：看门狗重试时复用旧 session id

文件：`crates/opencode-plugin/src/instance.rs`

`send_with_watchdog_retry` 重试时不再调 `create_opencode_session()`，改为从
`last_session_id` 读取旧 session id，重新建立 session_map 映射后重试。新 opencode
进程通过 `POST /session/{old_id}/message` 续上磁盘上持久化的历史。

### 增强：看门狗超时阈值可配置

文件：`crates/agent-core/src/models.rs`、`crates/agent-core/src/instance.rs`、
`crates/agent-core/src/service.rs`、`crates/opencode-plugin/src/instance.rs`、
`apps/gatewayd/src/handlers/agent.rs`、`src-tauri/src/gateway/handlers/agent.rs`

- `ModelConfig` / `UpdateModelConfigRequest` 新增 `watchdog_timeout_secs: Option<u64>`
- `AgentInstance` trait 新增 `set_watchdog_timeout(&self, secs: u64)` 方法（默认空实现）
- `OpencodeInstance` 用 `Arc<Mutex<u64>>` 存储可配置阈值，默认 120s
- `AgentService::update_model_config` 在收到更新时调用 `instance.set_watchdog_timeout()`
- `http_with_watchdog` / `watchdog_until_stalled` 读取可配置阈值
- gatewayd HTTP 接口（`PUT /sessions/{id}/agents/{id}/config`）和 Tauri JSON-RPC
  接口（`agent.updateModelConfig`）均支持 `watchdogTimeoutSecs` 参数

## 验证结果

- `cargo check --bin dh-gatewayd`、`cargo check --bin dh`、`cargo check --bin dh-desktop`
  编译通过，0 warnings
- `cargo clippy --lib -p agent-core -p opencode-plugin` 无新增 warning
- `cargo test --lib -p agent-core -p opencode-plugin -p dh-gatewayd`：55 个测试全部通过
- `cargo test --lib`（src-tauri）：17 个测试全部通过
- 待用户在真实环境中验证：
  - 场景 A（reaper 路径）：间隔 11 分钟后继续对话，检查日志出现
    `resuming persisted session=ses_xxx`
  - 场景 B（看门狗路径）：长任务触发看门狗重建后，`oc_sid` 保持不变
  - 追问"做的如何"时 AI 能引用之前的对话内容
