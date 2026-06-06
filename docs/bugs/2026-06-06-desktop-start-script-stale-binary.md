# 桌面启动脚本指向旧二进制

## 现象

用户通过启动脚本启动桌面应用后，系统中存在进程，但界面窗口没有展示出来。开启 `RUST_LOG=info` 后可见 Tauri setup 已完成，但 WebSocket server 随即输出 `shutting down`。

## 根因

存在三个问题：第一，应用程序名称改为 `dh` 后，Tauri 构建产物二进制名称会同步变为 `src-tauri/target/release/dh`，原启动脚本仍指向旧路径 `src-tauri/target/release/ai-coding-desktop`；第二，WebSocket shutdown sender 使用 `_shutdown_tx` 局部变量保存，setup 回调结束后被 drop，导致 `broadcast::Receiver` 立即收到关闭信号，网关后台任务退出；第三，在当前 Linux/WSLg 图形环境中，无边框窗口创建后虽然 `visible=Ok(true)`，但实际外部尺寸为 `0x0`，进程和后端服务正常但用户看不到界面。

## 解决方案

将 Tauri `productName`、Rust package name、前端 package name、窗口标题统一改为 `dh`，并把 `run-desktop.sh` 的启动目标更新为 `./src-tauri/target/release/dh`。同时新增 `WebSocketShutdown` 托管状态保存 shutdown sender，确保 WebSocket server 生命周期跟随 Tauri 应用，并将 Linux 桌面窗口恢复为系统原生装饰，避免 WSLg/GTK 下无边框窗口映射为 `0x0`；同时在事件循环 Ready 后使用物理像素显式设置主窗口最小尺寸和尺寸、居中、`show`、`unminimize` 和 `set_focus`，延迟重试一次布局，并在启动脚本中禁用 WebKit compositing mode 与 AT-SPI bridge 以提升 WSLg 兼容性。通过 TypeScript/Rust 检查、前端构建和 release 二进制启动验证。
