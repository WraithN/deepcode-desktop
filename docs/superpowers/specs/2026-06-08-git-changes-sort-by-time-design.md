# 变更列表按时间倒序排序 — 设计文档

**日期**：2026-06-08
**范围**：RightPanel 变更文件列表排序 + Rust 后端数据增强

---

## 背景

当前工作区右侧面板的"变更"列表展示 git 变更文件，但：
1. 文件按 git status 的默认顺序（路径字母序）排列，用户难以快速找到最新修改的文件
2. 已有 5 秒轮询的实时更新机制，但缺少时间维度信息

## 目标

- 变更文件列表按**修改时间倒序**排列（最新的在最前面）
- 保持现有 5 秒轮询实时更新机制不变

## 方案

### 数据层

在 `GitChangedFile`（TS）和 `GitStatusEntry` / `GitChangedFile`（Rust）中增加 `modified_at` 字段：

```typescript
export interface GitChangedFile {
  path: string;
  status: 'M' | 'U' | 'A' | 'D' | 'R';
  additions: number;
  deletions: number;
  diff: string;
  modified_at: number; // Unix timestamp in milliseconds
}
```

### Rust 后端

在 `src-tauri/src/commands/git.rs` 的 `git_changed_files` 函数中：
1. 对每个变更文件调用 `std::fs::metadata(path)?.modified()`
2. 将 `SystemTime` 转换为毫秒级 Unix 时间戳
3. 将时间戳注入返回的 `GitChangedFile` 结构

### 前端

在 `RightPanel.tsx` 中：
1. `displayFiles` 不再直接使用 `gitFiles`
2. 使用 `useMemo` 对 `gitFiles` 按 `modified_at` 倒序排序
3. 保持现有 5 秒轮询和 diff 展开交互不变

## 影响面

| 文件 | 变更 |
|------|------|
| `src/types/types.ts` | `GitChangedFile` 增加 `modified_at` |
| `src-tauri/src/commands/git.rs` | `git_changed_files` 注入 mtime |
| `src/components/workspace/RightPanel.tsx` | `displayFiles` 倒序排序 |

## 测试验证

1. `cargo check` 通过
2. `npx tsc --noEmit` 通过
3. 变更文件列表按修改时间倒序排列
