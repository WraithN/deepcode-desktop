# Claude Code REPL 卡在 starting

## 现象

通过 `dh chat claude-code --interactive` 发送消息后，终端只显示 `[status]>>>> starting`，没有返回 Claude Code 的思考或回复内容。

## 根因

Claude 插件存在两处协议适配问题：

1. 启动 Claude CLI 时把项目绝对路径传给 `--worktree`，但 Claude Code 的 `--worktree` 参数要求 worktree 名称，不接受包含 `/` 的路径。
2. 插件在创建实例时立即等待 `system/init` 事件；但 Claude Code 的 `--input-format=stream-json` 模式会在收到第一条用户消息后才输出 init，导致启动流程和发送流程互相等待。
3. 当前 Claude Code 输出结构为 `assistant.message.content`，原解析器只支持旧的顶层 `assistant.content` 结构，导致即使收到输出也无法映射为 token 事件。

## 解决方案

- 移除 Claude 插件启动参数中的 `--worktree=<workspace>`，仅通过进程 `cwd` 指定工作目录。
- 将 Claude 实例改为懒启动：创建实例不启动进程，首次发送消息时启动进程、标记 running，然后写入 stream-json 用户消息。
- 将发送 payload 改为 Claude Code 要求的 `{ "type": "user", "message": ... }` 结构。
- 扩展解析器兼容当前 Claude Code 的 `assistant.message.content`、`system/init`、`result` 输出。
- 启动失败时不再长期停留在 starting。

验证结果：

```bash
printf "你好，只回复 OK\n/exit\n" | ./target/debug/dh chat claude-code --interactive
```

输出包含：

```text
[status]>>>> starting
[status]>>>> running
[thinking]>>>> ...
[ai]>>>> OK
```
