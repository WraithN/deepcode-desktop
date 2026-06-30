# dh 命令与 npm 安装失败

## 现象

用户执行 `dh --version` 时报错：

```
Error: `dh` binary not found.

Please install DeepHarness Desktop from: https://github.com/deepharness/deepharness-ent-desktop
Or build from source: cargo install --path apps/cli
```

同时 `npm install -g deepharness` 因默认全局目录需要 root 权限而失败（`EACCES`）。

## 根因

1. `deepharness` npm 包只是一个调用原生 `dh` 二进制文件的 JS 包装器，如果系统里没有真实的 `dh` 二进制，包装器会打印上述错误。
2. 包装器的 fallback 查找逻辑使用 `which dh`；当 npm 全局包已经被安装到 PATH 后，`which dh` 会返回包装器脚本自身，导致包装器无限递归启动自己，最终表现为命令挂起或超时。
3. npm 默认全局目录 `/usr/lib/node_modules` 需要 root 权限，普通用户直接 `npm install -g` 会权限失败。
4. npm 包 `package.json` 中的 homepage/repository URL 指向了不存在的 `github.com/deepharness/deepharness-ent-desktop`。

## 解决方案

### 1. 自动下载原生二进制

新增 GitHub Actions workflow（`.github/workflows/release-dh.yml`），在推送 `dh-v*` tag 时交叉编译 5 个平台：

- Linux x64 / arm64
- macOS x64 / arm64
- Windows x64

编译产物上传到 GitHub Release，命名规则为 `dh-{platform}-{arch}`（Windows 带 `.exe`）。

npm 包新增 `scripts/download-binary.js`，根据 `process.platform` 和 `process.arch` 自动从对应 Release 下载二进制到 `~/.local/bin/dh`：

```javascript
const PLATFORM_ASSET_NAMES = {
  'linux:x64': 'dh-linux-x64',
  'linux:arm64': 'dh-linux-arm64',
  'darwin:x64': 'dh-darwin-x64',
  'darwin:arm64': 'dh-darwin-arm64',
  'win32:x64': 'dh-windows-x64.exe',
};
```

- `npm/bin/dh.js`：运行时若找不到本地 `dh`，自动触发下载。
- `npm/scripts/postinstall.js`：安装 npm 包时也会尝试自动下载或从源码构建。
- 支持 `DH_BINARY_PATH` 环境变量显式指定二进制路径。

### 2. 修复包装器无限递归

在 `npm/bin/dh.js` 中：

- 通过 `realpathSync` 识别包装器脚本自身，避免 `which dh` 返回自己造成递归。
- spawn 子进程时设置 `DH_NPM_WRAPPER=1`，子进程中跳过 `which dh` fallback。

### 3. 修复 GitHub 地址 404

将 `npm/package.json` 中的 `homepage`、`repository`、`bugs` URL 从 `github.com/deepharness/...` 改为正确的 `github.com/WraithN/...`。

### 4. 修复交叉编译错误

在实现 CI 过程中发现并修复了多个跨平台编译问题：

- `apps/cli/src/commands/gatewayd.rs`：macOS 没有 `__errno_location`，改用 `std::io::Error::last_os_error()`。
- `apps/cli/src/commands/exec.rs`：`libc::kill` 不是跨平台 API，提取为 `#[cfg(unix)]` 函数，Windows 暂跳过 graceful restart。
- `apps/cli/Cargo.toml`：添加 `windows-sys` 依赖。
- `gatewayd.rs` Windows 代码：`STILL_ACTIVE` 直接使用常量值 `259`。
- 根 `Cargo.toml`：`reqwest` 改为 `default-features = false` + `rustls-tls`，移除 OpenSSL 依赖，使 Linux ARM64 交叉编译成功。
- `.github/workflows/release-dh.yml`：对 cross 编译的 target 跳过宿主 `strip`。

### 5. npm 全局安装权限问题

配置 npm 使用用户级全局目录：

```bash
mkdir -p ~/.npm-global
npm config set prefix '~/.npm-global'
export PATH="$HOME/.npm-global/bin:$PATH"
```

并将该 PATH 写入 `~/.bashrc`。

## 验证结果

```bash
$ npm install -g deepharness
$ dh --version
dh 0.1.0
```

在干净的 Linux x64 环境删除 `~/.local/bin/dh` 后，执行 `dh --version` 会自动从 GitHub Release 下载二进制并运行：

```bash
$ rm -f ~/.local/bin/dh
$ dh --version
[deepharness] Downloading dh 0.0.3 for linux-x64...
[deepharness] URL: https://github.com/WraithN/deepharness-ent-desktop/releases/download/dh-v0.0.3/dh-linux-x64
dh 0.1.0
```

GitHub Release 验证：

```bash
$ gh release view dh-v0.0.3 --repo WraithN/deepharness-ent-desktop
title:	dh-v0.0.3
asset:	dh-darwin-arm64
asset:	dh-darwin-x64
asset:	dh-linux-arm64
asset:	dh-linux-x64
asset:	dh-windows-x64.exe
```

npm 包验证：

```bash
$ npm view deepharness@0.0.3 --registry https://registry.npmjs.org/
homepage: https://github.com/WraithN/deepharness-ent-desktop
```

- `cargo check -p deepharness-cli` 无 warning。
- `cargo check`（Tauri）无 warning。
- `npx tsc --noEmit -p tsconfig.check.json` 无 error。
- `npx biome lint` 通过。
- `.rules/check.sh` 通过。
