#!/bin/bash
# 启动 AI Coding Desktop（兼容无 GPU 环境）
export LIBGL_ALWAYS_SOFTWARE=1
export WEBKIT_DISABLE_COMPOSITING_MODE=1
export WEBKIT_DISABLE_DMABUF_RENDERER=1

./src-tauri/target/release/ai-coding-desktop "$@"
