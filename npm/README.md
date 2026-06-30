# DeepHarness CLI

本地优先的 AI 编码工作台，支持 Claude Code、OpenCode 等多种编码智能体。

## 安装

`deepharness` npm 包是一个轻量级 JS 包装器，它需要本地 `dh` 二进制文件才能工作。

### 1. 先安装 `dh` 二进制文件（必须）

选择以下任一方式：

#### 方式 A：安装 DeepHarness Desktop

从发布页下载并安装桌面应用，它会自动将 `dh` 放入 PATH：

```bash
https://github.com/deepharness/deepharness-ent-desktop
```

#### 方式 B：从源码编译（需要 Rust 工具链）

```bash
git clone https://github.com/deepharness/deepharness-ent-desktop.git
cd deepharness-ent-desktop
cargo build --release -p deepharness-cli

# 安装到用户级可执行目录
mkdir -p ~/.local/bin
cp target/release/dh ~/.local/bin/dh

# 确保 ~/.local/bin 在 PATH 中
export PATH="$HOME/.local/bin:$PATH"
```

### 2. 再安装 npm 包

```bash
# 全局安装（推荐安装到用户目录，避免权限问题）
npm install -g deepharness
```

> **权限问题？** 如果看到 `EACCES` 错误，说明 npm 默认全局目录需要 root 权限。推荐以下两种方案：

#### 方案 1：更改 npm 全局目录到用户主目录

```bash
mkdir -p ~/.npm-global
npm config set prefix '~/.npm-global'
export PATH="$HOME/.npm-global/bin:$PATH"
# 将上面这行 export 也加入 ~/.bashrc 或 ~/.zshrc

npm install -g deepharness
```

#### 方案 2：使用 `npx`（无需全局安装）

```bash
npx deepharness --version
```

### 3. 验证安装

```bash
dh --version
```

如果包装器找不到二进制文件，可以通过环境变量显式指定：

```bash
export DH_BINARY_PATH=/path/to/dh
dh --version
```

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
