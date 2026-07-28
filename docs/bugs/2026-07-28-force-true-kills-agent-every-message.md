# force=true 语义错配导致每次发消息都杀重建 Agent 实例

## 现象

用户在前端"智能会话"连续发消息时，观察到两个症状：

1. **Tab 标题 instanceId 与后端真值不一致**：前端 Tab 标题里的 instanceId 永远是 CreateSession 第一次拿到的值；但后端真实持有的 instance ID 在 `claude-code` ↔ `claude-code-1` ↔ `claude-code-2` 之间反复跳。
2. **Agent 子进程被反复踢 + 重建**：每次 send 消息，claude/opencode 子进程都被杀掉重启，stdin 长连接打断，进程内 init 事件产生的 active_session_id 全部丢失。

### 影响

- 用户连续对话时 Agent 看不到前几轮的"我"——不记得中间计划、文件路径、变量名，多轮任务需要重述前提
- 每次 send 都 fork 新子进程，冷启动延迟，GPU/CPU 抖动
- 工具调用上下文（文件读写的相对路径）丢失
- 多步骤计划（plan mode / todo）被清空
- 子进程 init 事件里的工作目录、模型、API key 重新协商
- 持久化的 session 历史与新进程脱钩，新进程只看到 input.messages 里那一条最新用户消息

## 根因

两层协同导致：

### 层 1：dh-backend 每次 Run 都强杀

dh-backend 的 `agui_client.go` 每次 Run 都调 `POST /sessions/{id}/agents`，硬编码 `force=true`，且每次生成新 name（`pluginKey + "-" + uuid`）。注释写明设计意图："每个 session 使用独立 instance，避免复用导致的'思考中'卡死问题"——把 `force=true` 作为"防卡死"逃生口。

### 层 2：gatewayd 的 create_agent 按 force 强杀

`apps/gatewayd/src/session.rs` 的 `create_agent` 方法中：

```rust
// 旧代码（已修复）
if !instances.is_empty() && !force {  // force=true -> 跳过复用
    Some(instances.first().unwrap().clone())
} else {
    None  // -> 走新建 -> 杀旧实例
}
```

- `force=true` → 跳过复用 → 走新建分支
- 新建后：`stop_and_remove_instance_with_timeout` 强杀旧实例（10s 宽限）
- instance ID 确定性递增（`opencode` → `opencode-1` → `opencode-2`），每次强杀重建都漂移一次

此外，即使去掉 `!force`，旧复用检查还校验 `existing.name == name`。dh-backend 每次生成不同 name（UUID 后缀），name 不匹配会直接报错，仍无法复用。

### 一句话根因

`force=true` 的语义在调用方（dh-backend）是"允许重建"，在接收方（gatewayd）被实现成"必须重建"。两者错配，导致每次发消息都重复杀+建。

## 解决方案

修改 `apps/gatewayd/src/session.rs` 的 `create_agent` 方法，将 `force=true` 语义从"必须重建"改为"允许重建"：

### 改动 1：去掉 `&& !force`

有实例时总是尝试复用，不论 force 值：

```rust
// 旧
if !instances.is_empty() && !force {
// 新
if !instances.is_empty() {
```

### 改动 2：复用判定去掉 name 比较，不匹配时降级为新建

旧代码在 agent_key/work_directory/name 任一不匹配时直接返回错误。新代码只比 `agent_key` + `work_directory`，不匹配时 fall through 到新建分支（会杀旧建新）：

```rust
// 旧：name 不匹配直接报错
if existing.agent_key == agent_key
    && existing.work_directory == work_directory
    && existing.name == name
{
    return Ok(existing);
}
return Err(PluginError::CreateInstanceFailed(...));

// 新：只比 agent_key + work_directory，不匹配则落新建
if let Some(existing) = agent_service.get_instance(&existing_id).await {
    if existing.agent_key == agent_key
        && existing.work_directory == work_directory
    {
        return Ok(existing);
    }
}
// Instance is dead or config mismatch -> fall through to create new.
```

### 改动 3：清理死代码

`RunError::InstanceAlreadyExists` 变体定义后从未被构造，已删除。

### 不改的部分

- `AgentService::create_instance`（`service.rs`）：session.rs 修复后，`create_instance` 只在真正需要新建时才被调用，`force=true` 语义正确
- dh-backend：不用改，`force=true` 语义从"必须重建"变成"允许重建"
- chat/SSE 路径：已经用 `force=false`，不受影响
- Tauri 桌面端：前端不发 `force`，不受影响

### 风险

dh-backend 注释说 `force=true` 是防"思考中卡死"。如果该 bug 仍存在，复用卡死实例会导致连续对话卡住。需后续确认该 bug 是否已修复；若仍存在，需另加"agent 不响应"超时探测，独立于 force 机制。

## 验证结果

- `cargo check --bin dh-gatewayd` 和 `cargo check --lib -p dh-gatewayd` 编译通过，0 warnings
- 待用户在真实环境中验证：连续发送多条消息，确认 instance ID 不漂移、子进程不被重启
