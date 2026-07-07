# dh gwd start 报错 No such file or directory (os error 2)

## 现象

执行全局安装并运行 `dh gwd start` 后，CLI 直接报错：

```
dh error: No such file or directory (os error 2)
```

而 `dh --version` 可以正常输出版本号，`which dh` 也能找到 `/usr/local/bin/dh`。

## 根因

`dh gwd start` 在启动网关守护进程时，内部会调用另一个原生二进制 `dh-gatewayd`：

```rust
// apps/cli/src/commands/gatewayd.rs
let mut cmd = std::process::Command::new("dh-gatewayd");
```

`deepharness` npm 包此前只打包/安装了 `dh` 这一个二进制，没有同时提供 `dh-gatewayd`。因此当用户通过 `npm -g install deepharness` 安装后，`dh` 虽然在 PATH 中，但 `dh-gatewayd` 缺失，导致上述报错。

## 解决方案

1. **Rust CLI 侧**：`dh` 启动 `dh-gatewayd` 时，优先从 `dh` 自身所在目录查找，再回退到 PATH，并支持 `DH_GATEWAYD_PATH` 环境变量覆盖。
2. **npm 包侧**：
   - `npm/scripts/prebuild.js` 在发布前同时准备 `dh` 和 `dh-gatewayd` 的所有平台二进制。
   - `npm/bin/dh.js` 在运行 `dh gwd` 子命令前，自动检查并下载 `dh-gatewayd`。
   - `npm/scripts/download-binary.js` 增加 `dh-gatewayd` 的下载逻辑。
3. **CI 侧**：`.github/workflows/release-dh.yml` 增加 `dh-gatewayd` 的构建矩阵，确保每个 Release 都包含 `dh-gatewayd-{platform}-{arch}` 资产。

## 验证

- `cargo build --release -p deepharness-cli -p dh-gatewayd` 成功。
- 将 `dh` 与 `dh-gatewayd` 放在同一目录，`./dh gwd start` 可正常启动守护进程。
- `npm pack --dry-run` 确认发布包同时包含 `dh` 和 `dh-gatewayd` 二进制。
- `pnpm tauri build` 与 `bash run-desktop.sh` 可正常构建并启动桌面端。
