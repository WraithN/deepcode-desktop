# dh CLI Linux 二进制 glibc 版本不兼容

## 现象

在目标宿主机上安装/运行 `deepharness` npm 包或直接使用 `dh` 二进制时，报错：

```
/usr/local/lib/node_modules/deepharness/binaries/dh-linux-x64:
/lib/x86_64-linux-gnu/libc.so.6: version `GLIBC_2.39' not found
(required by /usr/local/lib/node_modules/deepharness/binaries/dh-linux-x64)
```

GitHub Actions 使用 `ubuntu-latest`（当前为 Ubuntu 24.04，glibc 2.39）编译的
`x86_64-unknown-linux-gnu` 二进制，在 glibc 版本低于 2.39 的 Linux 发行版上无法运行。

## 根因

1. `.github/workflows/release-dh.yml` 中 Linux x64 / arm64 使用 `*-unknown-linux-gnu` target。
2. GNU target 默认动态链接宿主系统的 glibc，编译机 glibc 版本成为运行时下限。
3. `ubuntu-latest` 已升级到 Ubuntu 24.04 (glibc 2.39)，导致旧版 Ubuntu、Debian、RHEL 等宿主无法启动。

## 解决方案

将 Linux 构建 target 改为 `*-unknown-linux-musl`，通过 musl libc 进行静态链接：

- `x86_64-unknown-linux-gnu` → `x86_64-unknown-linux-musl`
- `aarch64-unknown-linux-gnu` → `aarch64-unknown-linux-musl`

同步调整：

- 两个 Linux target 均使用 `cross` 编译，确保 musl 工具链一致。
- 保持 asset 名称 `dh-linux-x64` / `dh-linux-arm64` 不变，npm 包装器与 `scripts/prebuild.js`
  无需修改。

修改文件：`.github/workflows/release-dh.yml`

```yaml
# Linux targets use musl for fully-static binaries so the released CLI
# does not depend on a specific host glibc version (e.g. GLIBC_2.39).
- target: x86_64-unknown-linux-musl
  os: ubuntu-latest
  asset_name: dh-linux-x64
  cross: true
- target: aarch64-unknown-linux-musl
  os: ubuntu-latest
  asset_name: dh-linux-arm64
  cross: true
```

## 验证结果

- `cargo check -p deepharness-cli` 无 warning。
- YAML 语法检查通过。
- 本地因缺少 `x86_64-linux-musl-gcc` 工具链无法直接验证 musl 构建；完整验证需推送 tag
  `dh-v*` 触发 GitHub Actions，下载产物后在低版本 glibc 环境运行：

```bash
file dh-linux-x64
# 期望输出：statically linked
ldd dh-linux-x64
# 期望输出：not a dynamic executable
./dh-linux-x64 --version
```

## 备注

musl 静态链接方案的替代方案：

- 使用 `ubuntu-22.04` runner 保留 gnu target，可兼容 glibc ≥ 2.35 的宿主，但无法覆盖更旧系统。
- 使用 `cargo-zigbuild` 指定最低 glibc 版本，保留 gnu 兼容性但配置更复杂。

综合考虑 CLI 工具需要覆盖尽可能多的 Linux 发行版，选择 musl 静态链接。
