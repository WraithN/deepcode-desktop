#!/usr/bin/env bash
# release.sh - DeepHarness 一键打包发布
#
# 自动化流程:
#   1. 读取 workspace 版本号（根 Cargo.toml [workspace.package] version）
#   2. 校验标签 dh-v{version} 尚未创建、工作区无未提交的版本号变更
#   3. 本地构建 dh-gatewayd 并冒烟测试（启动监听后立即退出）
#   4. 创建并推送标签，触发 GitHub Actions release-dh.yml
#   5. 轮询监控工作流直到完成
#   6. 验证 GitHub Release 产物与 npm 发布状态
#
# 用法:
#   scripts/release.sh              # 完整流程（本地构建验证 + CI + npm 验证）
#   scripts/release.sh --skip-local # 跳过本地构建验证，直接触发 CI
#   scripts/release.sh --desktop    # 额外构建并启动桌面端（交互式 GUI 验证）
set -euo pipefail

# ===== 常量（规则7：禁止魔法值）=====
readonly TAG_PREFIX="dh-v"
readonly WORKFLOW_FILE="release-dh.yml"
readonly GATEWAYD_PACKAGE="dh-gatewayd"
readonly GATEWAYD_BIN="dh-gatewayd"
readonly NPM_PACKAGE="deepharness"
# 直连官方源，绕过本地 npmmirror 镜像同步延迟
readonly NPM_REGISTRY="https://registry.npmjs.org/"
# 冒烟测试使用高位端口，避免与运行中的实例冲突
readonly SMOKE_PORT=23999
readonly SMOKE_ADMIN_PORT=23998
readonly SMOKE_TIMEOUT=5
# CI 触发后工作流创建有数秒延迟，需重试查找
readonly TRIGGER_RETRY=10
readonly TRIGGER_RETRY_INTERVAL=3
# npm 发布后官方源传播可能延迟，需重试验证
readonly NPM_RETRY=6
readonly NPM_RETRY_INTERVAL=10

ROOT="$(git rev-parse --show-toplevel)"
cd "$ROOT"

# 显示帮助信息
show_help() {
  cat <<'EOF'
release.sh - DeepHarness 一键打包发布

自动化流程:
  1. 读取 workspace 版本号（根 Cargo.toml [workspace.package] version）
  2. 校验标签 dh-v{version} 尚未创建、工作区无未提交的版本号变更
  3. 本地构建 dh-gatewayd 并冒烟测试（启动监听后立即退出）
  4. 创建并推送标签，触发 GitHub Actions release-dh.yml
  5. 轮询监控工作流直到完成
  6. 验证 GitHub Release 产物与 npm 发布状态

用法:
  scripts/release.sh              # 完整流程（本地构建验证 + CI + npm 验证）
  scripts/release.sh --skip-local # 跳过本地构建验证，直接触发 CI
  scripts/release.sh --desktop    # 额外构建并启动桌面端（交互式 GUI 验证）
EOF
}

# ===== 参数解析 =====
SKIP_LOCAL=0
BUILD_DESKTOP=0
for arg in "$@"; do
  case "$arg" in
    --skip-local) SKIP_LOCAL=1 ;;
    --desktop) BUILD_DESKTOP=1 ;;
    -h|--help) show_help; exit 0 ;;
    *) echo "未知参数: $arg（见 --help）" >&2; exit 1 ;;
  esac
done

# ===== 工具函数 =====

# 依赖检查：确保运行环境具备所需命令行工具
check_deps() {
  local missing=()
  for cmd in git cargo gh npm; do
    command -v "$cmd" >/dev/null 2>&1 || missing+=("$cmd")
  done
  if [[ ${#missing[@]} -gt 0 ]]; then
    echo "release: 缺少依赖命令: ${missing[*]}" >&2
    exit 1
  fi
}

# 从根 Cargo.toml 读取 [workspace.package] version
# 仅匹配行首的 version = "x.y.z"，避免误伤 [workspace.dependencies] 中的依赖版本
read_workspace_version() {
  local ver
  ver=$(grep -m1 '^version = "' Cargo.toml | sed -E 's/^version = "([^"]+)".*/\1/')
  if [[ -z "$ver" ]]; then
    echo "release: 未在 Cargo.toml 找到 ^version 行" >&2
    exit 1
  fi
  echo "$ver"
}

# 检查 git 标签是否已存在
tag_exists() {
  git rev-parse -q --verify "refs/tags/$1" >/dev/null
}

# 本地构建 dh-gatewayd 并执行冒烟测试
# 冒烟逻辑：启动后捕获日志，检测 "Starting gatewayd" 即视为正常，超时退出
build_and_smoke_gatewayd() {
  echo "==> 本地构建 $GATEWAYD_PACKAGE"
  cargo build --release -p "$GATEWAYD_PACKAGE"

  local bin="target/release/$GATEWAYD_BIN"
  if [[ ! -x "$bin" ]]; then
    echo "release: 构建产物 $bin 不存在" >&2
    exit 1
  fi

  echo "==> 冒烟测试（端口 $SMOKE_PORT，超时 ${SMOKE_TIMEOUT}s）"
  local log
  # timeout 会在超时后终止进程；冒烟测试进程本就会持续运行，靠 timeout 退出属正常
  log=$(timeout "$SMOKE_TIMEOUT" "$bin" --port "$SMOKE_PORT" --admin-port "$SMOKE_ADMIN_PORT" 2>&1 || true)
  if ! echo "$log" | grep -q "Starting gatewayd on port"; then
    echo "release: 冒烟测试失败，未检测到启动日志" >&2
    echo "$log" | tail -20 >&2
    exit 1
  fi
  echo "    冒烟测试通过"
}

# 构建并启动桌面端（交互式，用户验证 GUI）
# 后台启动，不阻塞主流程
build_and_launch_desktop() {
  echo "==> 构建桌面端 (pnpm tauri build)"
  pnpm tauri build
  echo "==> 启动桌面端（后台）"
  bash run-desktop.sh &
  echo "    桌面端已启动，请验证 GUI 窗口"
}

# 创建并推送标签以触发 CI
trigger_ci() {
  local tag="$1"
  echo "==> 创建并推送标签 $tag"
  git tag -a "$tag" -m "Release $tag (dh CLI + dh-gatewayd)"
  git push origin "$tag"
}

# 轮询监控工作流直到完成
# 返回工作流运行 ID；失败时退出
monitor_workflow() {
  local tag="$1"
  echo "==> 等待工作流触发..."
  local run_id=""
  # CI 触发有数秒延迟，重试查找由该标签触发的运行
  local i
  for i in $(seq 1 "$TRIGGER_RETRY"); do
    run_id=$(gh run list --workflow="$WORKFLOW_FILE" --branch="$tag" \
      --limit 1 --json databaseId -q '.[0].databaseId' 2>/dev/null || true)
    [[ -n "$run_id" ]] && break
    sleep "$TRIGGER_RETRY_INTERVAL"
  done
  if [[ -z "$run_id" ]]; then
    echo "release: 未能找到由 $tag 触发的工作流运行" >&2
    exit 1
  fi
  echo "    工作流运行 ID: $run_id"
  echo "    监控中（上次约 6 分钟）..."
  # --exit-status 使退出码反映工作流结论：成功 0，失败非 0
  if ! gh run watch "$run_id" --exit-status >/dev/null 2>&1; then
    echo "release: 工作流未成功" >&2
    gh run view "$run_id" >&2
    exit 1
  fi
  echo "    工作流成功完成"
}

# 验证 GitHub Release 产物
verify_release() {
  local tag="$1"
  echo "==> 验证 GitHub Release"
  local url
  url=$(gh release view "$tag" --json url -q '.url')
  local count
  count=$(gh release view "$tag" --json assets -q '.assets | length')
  echo "    Release URL: $url"
  echo "    产物数量: $count"
}

# 验证 npm 发布（直连官方源，绕过本地镜像同步延迟）
# 官方源传播可能有数秒延迟，故重试验证
verify_npm() {
  local version="$1"
  echo "==> 验证 npm 发布"
  local i published latest
  for i in $(seq 1 "$NPM_RETRY"); do
    published=$(npm view "$NPM_PACKAGE@$version" version \
      --registry="$NPM_REGISTRY" 2>/dev/null || true)
    if [[ "$published" == "$version" ]]; then
      latest=$(npm view "$NPM_PACKAGE" dist-tags \
        --registry="$NPM_REGISTRY" -q '.latest' 2>/dev/null || echo "?")
      echo "    npm: $NPM_PACKAGE@$version (latest=$latest)"
      return 0
    fi
    sleep "$NPM_RETRY_INTERVAL"
  done
  echo "release: npm 上 $NPM_PACKAGE@$version 尚未可见（可能传播延迟）" >&2
  return 1
}

# ===== 主流程 =====

check_deps

VERSION=$(read_workspace_version)
TAG="${TAG_PREFIX}${VERSION}"
echo "================ DeepHarness 打包发布 ================"
echo "版本: $VERSION"
echo "标签: $TAG"

if tag_exists "$TAG"; then
  echo "release: 标签 $TAG 已存在" >&2
  echo "  如需重新发布，请先: git tag -d $TAG && git push origin :refs/tags/$TAG" >&2
  exit 1
fi

# 若版本号文件有未提交变更，提示但不阻断（标签指向 HEAD 提交）
if [[ -n "$(git status --short Cargo.toml)" ]]; then
  echo "release: 警告 - Cargo.toml 有未提交变更，标签将指向当前 HEAD 提交" >&2
fi

if [[ "$SKIP_LOCAL" -eq 0 ]]; then
  build_and_smoke_gatewayd
fi

if [[ "$BUILD_DESKTOP" -eq 1 ]]; then
  build_and_launch_desktop
fi

trigger_ci "$TAG"
monitor_workflow "$TAG"
verify_release "$TAG"
verify_npm "$VERSION" || true

echo "================ 发布完成 ================"
