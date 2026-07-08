#!/usr/bin/env bash
# bump-version.sh — 递增 workspace patch 版本号并提交
#
# 版本号单一事实来源: 根 Cargo.toml 的 [workspace.package] version。
# 每次合并并推送到 main 时由 githooks/pre-push 自动调用；也可手动执行。
#
# 用法:
#   scripts/bump-version.sh           # 递增 patch（HEAD 已是 bump 提交时跳过）
#   scripts/bump-version.sh --force   # 强制递增，忽略重复 guard
set -euo pipefail

FORCE=0
[[ "${1:-}" == "--force" ]] && FORCE=1

ROOT="$(git rev-parse --show-toplevel)"
CARGO="$ROOT/Cargo.toml"
NPM_PKG="$ROOT/npm/package.json"

# 仅匹配行首的 version = "x.y.z"（位于 [workspace.package] 下），
# 避免误伤 [workspace.dependencies] 中带 version 字段的依赖项。
CUR=$(grep -m1 '^version = "' "$CARGO" | sed -E 's/^version = "([^"]+)".*/\1/')
if [[ -z "$CUR" ]]; then
  echo "bump-version: 未在 $CARGO 找到 ^version 行" >&2
  exit 1
fi

# guard: 若 HEAD 已是 version-bump 提交则跳过，避免重复递增。
LAST_MSG=$(git log -1 --pretty=%s 2>/dev/null || true)
if [[ "$FORCE" -eq 0 && "$LAST_MSG" == "chore: bump version to "* ]]; then
  echo "bump-version: HEAD 已是版本提交 ($LAST_MSG)，跳过"
  exit 0
fi

# 拆分语义化版本并递增 patch 段。
IFS='.' read -r MAJOR MINOR PATCH <<< "$CUR"
PATCH=$((PATCH + 1))
NEW="$MAJOR.$MINOR.$PATCH"

# perl -i 跨平台原地编辑行为一致（sed -i 在 macOS 需额外空串参数，易出错）。
perl -i -pe "s/^version = \"[^\"]+\"/version = \"$NEW\"/" "$CARGO"

# 同步 npm 包版本号，确保发布的 npm 包与 Rust workspace 版本一致。
if [[ -f "$NPM_PKG" ]]; then
  perl -i -pe "s/\"version\": \"[^\"]+\"/\"version\": \"$NEW\"/" "$NPM_PKG"
  git add "$NPM_PKG"
fi

git add "$CARGO"
git commit -m "chore: bump version to $NEW" >/dev/null
echo "bump-version: $CUR -> $NEW"
