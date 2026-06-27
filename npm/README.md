# DeepHarness CLI

本地优先的 AI 编码工作台，支持 Claude Code、OpenCode 等多种编码智能体。

## 安装

```bash
npm install -g deepharness
```

**依赖要求：**

- 已安装 DeepHarness Desktop 应用 或 `dh` 二进制在 PATH 中
- 若从源码编译：需要 Rust 工具链 (`cargo install --path apps/cli`)

## 使用

```bash
# 与 Claude Code 聊天
dh chat claude-code --interactive

# 与 OpenCode 聊天
dh chat opencode --interactive

# 启动网关
dh gwd start

# 查看网关状态
dh gwd status

# 停止网关
dh gwd stop

# 查看会话日志
dh gwd logs

# 查看会话详情
dh gwd session <session-id>

# 查看用量统计
dh gwd stats
```

## 开发

```bash
# 从源码编译
cargo build --release -p deepharness-cli

# 本地链接测试
cd npm
npm link
```

## 安全说明

- 本 npm 包只提供轻量级 JS 包装器，不包含任何二进制或 API 密钥
- 所有 AI 调用和 API 密钥都由本地的 DeepHarness Desktop 或 Claude Code / OpenCode 自行管理
- 数据全部保存在本地，不会发送到第三方服务器
