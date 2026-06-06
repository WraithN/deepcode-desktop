#!/bin/bash
# 启动 dh（兼容无 GPU 环境）
export GDK_BACKEND=x11
export LIBGL_ALWAYS_SOFTWARE=1
export WEBKIT_DISABLE_DMABUF_RENDERER=1
export WEBKIT_DISABLE_COMPOSITING_MODE=1
export NO_AT_BRIDGE=1

./src-tauri/target/release/dh "$@"
