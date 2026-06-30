# DeepHarness CLI

本地优先的 AI 编码工作台，支持 Claude Code、OpenCode 等多种编码智能体。

## 安装

`deepharness` npm 包会在安装时自动下载对应平台的原生 `dh` 二进制文件。

```bash
npm install -g deepharness
```

安装完成后验证：

```bash
dh --version
```

### 权限问题

如果 `npm install -g` 出现 `EACCES` 权限错误，推荐将 npm 全局目录改到用户主目录：

```bash
mkdir -p ~/.npm-global
npm config set prefix '~/.npm-global'
export PATH="$HOME/.npm-global/bin:$PATH"
# 将上面 export 加入 ~/.bashrc 或 ~/.zshrc

npm install -g deepharness
```

或者使用 `npx`（无需全局安装）：

```bash
npx deepharness --version
```

### 手动指定二进制路径

如果自动下载失败，或你想使用自己编译的 `dh`，可通过环境变量指定：

```bash
export DH_BINARY_PATH=/path/to/dh
dh --version
```

## 支持的平台

安装脚本会根据当前系统自动下载对应二进制：

- Linux x64 / arm64
- macOS x64 / arm64
- Windows x64

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
