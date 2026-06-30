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

## 解决方案

1. 将原生 `dh` 二进制安装到标准用户目录：
   ```bash
   cargo build --release -p deepharness-cli
   mkdir -p ~/.local/bin
   cp target/release/dh ~/.local/bin/dh
   ```
2. 改进 `npm/bin/dh.js`：
   - 通过 `realpathSync` 识别包装器脚本自身，避免 `which dh` 返回自己造成递归。
   - 当已经处于包装器启动的子进程中（`DH_NPM_WRAPPER=1`）时，跳过 `which dh` fallback。
   - 支持 `DH_BINARY_PATH` 环境变量显式指定二进制路径。
   - 优化错误提示，给出三种明确的解决方式。
3. 新增 `npm/scripts/postinstall.js`：
   - 安装 npm 包时自动检查 `dh` 二进制是否存在。
   - 如果从源码仓库安装（`npm link` / 本地安装），自动执行 `cargo build --release -p deepharness-cli`。
   - 同样避免把包装器自身误判为可用的 `dh` 二进制。
4. 更新 `npm/package.json`：
   - 添加 `postinstall` 脚本。
   - 添加 `files` 字段确保 `bin/`、`scripts/`、`README.md` 随包发布。
5. 更新 `npm/README.md`：
   - 明确说明必须先安装原生 `dh` 二进制。
   - 提供 npm 全局安装权限问题的两种解决方案：修改 npm prefix 或使用 `npx`。
6. 将 `~/.npm-global/bin` 加入 `~/.bashrc` 的 PATH，使新 shell 可以直接使用全局 `dh` 命令。

## 验证结果

```bash
$ dh --version
dh 0.1.0

$ bash -lc 'which dh && dh --version'
/home/nan/deepharness-ent-desktop/target/release/dh
dh 0.1.0
```

- `cargo check -p deepharness-cli` 无 warning。
- `npx tsc --noEmit -p tsconfig.check.json` 无 error。
- `npx biome lint` 通过。
- `.rules/check.sh` 通过。
