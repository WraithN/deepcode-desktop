# opencode serve 子进程继承 gatewayd 的 CWD

## 现象

通过 OpenCode 智能体执行编码任务时，`opencode serve` 子进程的工作目录并非用户选择的目标工作区，而是 gatewayd（桌面端 Rust 后端）进程自身的当前工作目录（CWD）。这导致：

1. opencode 的所有文件操作（读写、git 状态、工具调用）都落在错误的目录上，而非用户在 UI 中选择的工作区路径。
2. 相对路径解析与用户预期不一致，可能出现"文件不存在"或"写到了非预期位置"。
3. 与 Claude Code 插件的行为不一致——Claude 插件正确地将子进程 CWD 设置为工作区目录。

## 根因

`crates/opencode-plugin/src/transport.rs` 中的 `start_opencode_process()` 在 `tokio::process::Command` 上只设置了参数与 stdio，**没有调用 `.current_dir()`**：

```rust
pub fn start_opencode_process(port: u16) -> Result<Child, InstanceError> {
    let mut cmd = tokio::process::Command::new(OPCODE_BINARY);
    cmd.arg(ARG_SERVE)
        .arg(ARG_PORT)
        .arg(port.to_string())
        .arg(ARG_PURE)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    // 缺少 .current_dir() → 子进程继承父进程（gatewayd）的 CWD
    cmd.spawn()...
}
```

### 全插件适配状态审查

"子进程 CWD = 工作区" 是通用能力，需所有插件适配。审查结果：

| 插件 | spawn 方式 | CWD 设置位置 | 适配状态 |
|------|-----------|-------------|---------|
| claude-plugin | `StdioTransport::new(PROGRAM_CLAUDE, args, work_directory)` | `agent-core/src/process/stdio.rs:43` `.current_dir(&self.cwd)` | ✅ 正确 |
| codex-plugin | `StdioTransport::new(PROGRAM_CODEX, args, work_directory)` | 同上 `stdio.rs:43` + 协议级 `cwd` 参数(`instance.rs:200`，双保险) | ✅ 正确 |
| opencode-plugin | 直接 `tokio::process::Command`（HTTP+SSE 架构，绕过 StdioTransport） | 原缺失 → 本次修复于 `transport.rs:139` | ✅ 已修复 |

claude 与 codex 共用 `agent_core::process::stdio::StdioTransport`，该 transport 在 `start()` 中统一调用 `.current_dir(&self.cwd)`（`stdio.rs:43`），因此二者本就正确。opencode 因采用 HTTP serve 架构、手写 spawn 绕过了 StdioTransport，才漏掉 current_dir——这是唯一缺陷点，已修复。

`InstanceConfig` 已持有 `work_directory: String`（`crates/agent-core/src/instance.rs:22`），但 opencode 的 `start_opencode_process` 旧签名只接收 `port`，工作目录信息根本没被传入。

## 解决方案

### 1. 扩展 start_opencode_process 签名并设置 current_dir

在 `crates/opencode-plugin/src/transport.rs` 中给 `start_opencode_process` 增加 `work_directory: &str` 参数，并在命令构造链中调用 `.current_dir(work_directory)`，使子进程 CWD 显式指向目标工作区，与 claude-plugin 对齐：

```rust
pub fn start_opencode_process(port: u16, work_directory: &str) -> Result<Child, InstanceError> {
    let mut cmd = tokio::process::Command::new(OPCODE_BINARY);
    cmd.arg(ARG_SERVE)
        .arg(ARG_PORT)
        .arg(port.to_string())
        .arg(ARG_PURE)
        .current_dir(work_directory)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    cmd.spawn()...
}
```

### 2. 更新调用方

在 `crates/opencode-plugin/src/instance.rs` 的 `ensure_started()` 中，把 `self.config.work_directory` 透传给新签名：

```rust
let mut child = start_opencode_process(port, &self.config.work_directory)?;
```

### 验证

- `cargo check --bin dh-desktop` 与 `cargo check --lib -p opencode-plugin` 均通过，0 warning。
- 全仓库仅有 `instance.rs:98` 一处调用，已同步更新，无遗漏。
- 三插件 CWD 适配状态审计完成（见根因表格）：claude/codex 经 StdioTransport 统一正确，opencode 已补齐。
