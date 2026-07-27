# 2026-07-27 ensure_agent_for_run 使用错误工作目录

## 现象

智能会话执行 `/prd-write`、`/proto-make` 等命令时，Agent 在错误的目录下执行操作，导致：

- `mkdir: Permission denied`
- 文件写入错误位置
- 任务失败或卡死后报 network error

服务器实证：

| 项 | 值 |
|---|---|
| Gatewayd cwd | `/root` |
| Session workspace（正确） | `/home/nan/test/9763b158.../a7390f0b...` |
| Agent 实际工作目录（错误） | `/home/nan/test/746e5d5b.../9113acf4...` |
| sessions.json 记录 | ✅ 有正确路径 |

## 根因

文件：`apps/gatewayd/src/session.rs:550-560`（`ensure_agent_for_run` 函数）

`ensure_agent_for_run` 在 session 没有预挂载 Agent 时自动挂载，但使用 `std::env::current_dir()` 获取工作目录，而不是从 session 的持久化记录中获取正确的 workspace 路径：

```rust
// 修改前（BUG）
let work_directory = std::env::current_dir()
    .unwrap_or_default()
    .to_string_lossy()
    .to_string();
```

影响链路：

1. 后端传入 `workspace=9763b158...`（正确），`create_agent` 调用 `persist_workspace` 记录到 `sessions.json`
2. Agent 被 reaper 回收或 crash 后，session 无 instance
3. 下次 `start_run` 时触发 `ensure_agent_for_run` 自动挂载
4. `ensure_agent_for_run` 用 `current_dir()` = gatewayd 进程的 cwd（错误）
5. 创建 Agent 进程，工作目录设为错误路径
6. Agent 执行 `mkdir` → Permission denied → 任务失败

复现条件：
1. Gatewayd 进程启动时 cwd 与当前用户的 workspace 不一致
2. Session 没有预挂载 Agent（被回收或 crash），触发 `ensure_agent_for_run` 自动挂载
3. Agent 以 gatewayd 的 cwd 启动，而非正确的 workspace

## 解决方案

使用已有的 `workspace_for_session` 方法从 `sessions.json` 获取正确的 workspace 路径，回退顺序：

1. `workspace_for_session` -- sessions.json 中记录的正确路径
2. `platform_workspace_path` -- 平台同步分配的沙箱路径
3. `current_dir` -- 兜底，仅在前两者均不可用时使用

```rust
// 修改后
let work_directory = self
    .workspace_for_session(session_id)
    .await
    .or_else(|| self.platform_workspace_path())
    .unwrap_or_else(|| {
        std::env::current_dir()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| ".".to_string())
    });
```

`workspace_for_session` 读取 `sessions.json` 中的持久化记录，该记录在 `create_agent` 时由 `persist_workspace` 写入（`session.rs:443`）。因此只要 session 曾经成功挂载过 Agent，`workspace_for_session` 就能返回正确路径。

## 验证结果

- `cargo build -p dh-gatewayd --release`：0 warnings。
- 重启开发环境后 gatewayd 正常启动，版本 0.1.16。
- 连续发送两条消息，instance `opencode-1` 被复用（未重启），workspace 路径正确。
