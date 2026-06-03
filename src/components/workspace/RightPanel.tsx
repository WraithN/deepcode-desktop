import type { Task, ModifiedFile } from '@/types/types';
import { useState, useMemo } from 'react';
import {
  ListTodo, FileEdit, CheckCircle2, Clock, Loader2,
  ChevronDown, ChevronRight,
  ChevronLeft, FileCode2, ChevronUp, FilePlus, FileMinus,
} from 'lucide-react';
import {
  Sheet, SheetContent, SheetHeader, SheetTitle,
} from '@/components/ui/sheet';

// Split diff 行
interface DiffLine {
  type: 'context' | 'add' | 'del' | 'empty' | 'header';
  leftContent?: string;
  rightContent?: string;
  leftNum?: number;
  rightNum?: number;
}

function parseDiffToSplit(diff?: string): DiffLine[] {
  if (!diff) return [];
  const lines = diff.split('\n');
  const result: DiffLine[] = [];
  let leftNum = 0;
  let rightNum = 0;
  let inHunk = false;

  for (const rawLine of lines) {
    // 元信息行
    if (rawLine.startsWith('diff ') || rawLine.startsWith('index ') || rawLine.startsWith('---') || rawLine.startsWith('+++')) {
      result.push({ type: 'header', leftContent: rawLine, rightContent: rawLine });
      continue;
    }
    // hunk header
    const hunkMatch = rawLine.match(/^@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@/);
    if (hunkMatch) {
      leftNum = parseInt(hunkMatch[1], 10);
      rightNum = parseInt(hunkMatch[2], 10);
      inHunk = true;
      result.push({ type: 'header', leftContent: rawLine, rightContent: rawLine });
      continue;
    }
    if (!inHunk) {
      result.push({ type: 'context', leftContent: rawLine, rightContent: rawLine, leftNum, rightNum });
      leftNum++;
      rightNum++;
      continue;
    }
    const prefix = rawLine.charAt(0);
    if (prefix === '+') {
      result.push({ type: 'add', rightContent: rawLine.slice(1), rightNum });
      rightNum++;
    } else if (prefix === '-') {
      result.push({ type: 'del', leftContent: rawLine.slice(1), leftNum });
      leftNum++;
    } else if (prefix === ' ' || prefix === '\t') {
      result.push({ type: 'context', leftContent: rawLine.slice(1), rightContent: rawLine.slice(1), leftNum, rightNum });
      leftNum++;
      rightNum++;
    } else if (rawLine === '') {
      // 空行在 hunk 中可能是上下文也可能是分隔
      result.push({ type: 'context', leftContent: '', rightContent: '', leftNum, rightNum });
      leftNum++;
      rightNum++;
    } else {
      result.push({ type: 'context', leftContent: rawLine, rightContent: rawLine, leftNum, rightNum });
      leftNum++;
      rightNum++;
    }
  }
  return result;
}

interface RightPanelProps {
  tasks: Task[];
  modifiedFiles: ModifiedFile[];
  collapsed: boolean;
  onToggleCollapse: () => void;
}

// 默认Mock变更文件（无数据时展示效果）
const defaultMockFiles: ModifiedFile[] = [
  {
    id: 'mock-1',
    user_id: 'mock',
    conversation_id: null,
    file_path: 'src/components/App.tsx',
    change_type: 'modified',
    created_at: new Date().toISOString(),
    diff: `diff --git a/src/components/App.tsx b/src/components/App.tsx
index 1234567..abcdefg 100644
--- a/src/components/App.tsx
+++ b/src/components/App.tsx
@@ -1,7 +1,12 @@
 import React from "react";
+import { useState, useEffect } from "react";
+import { Button } from "@/components/ui/button";
 
-export default function App() {
+export default function App(): JSX.Element {
+  const [count, setCount] = useState(0);
+
   return (
     <div>
-      <h1>Hello World</h1>
+      <h1>Hello DeepHarness</h1>
+      <Button onClick={() => setCount(c => c + 1)}>
+        Count: {count}
+      </Button>
     </div>
   );
 }`,
  },
  {
    id: 'mock-2',
    user_id: 'mock',
    conversation_id: null,
    file_path: 'src/utils/helpers.ts',
    change_type: 'modified',
    created_at: new Date().toISOString(),
    diff: `diff --git a/src/utils/helpers.ts b/src/utils/helpers.ts
index 1111111..2222222 100644
--- a/src/utils/helpers.ts
+++ b/src/utils/helpers.ts
@@ -1,8 +1,20 @@
-export function formatDate(date: Date) {
-  return date.toISOString();
+export function formatDate(date: Date, format = 'iso'): string {
+  if (format === 'iso') return date.toISOString();
+  if (format === 'local') return date.toLocaleString();
+  return date.toString();
 }
 
-export function clamp(num: number, min: number, max: number) {
-  return Math.min(Math.max(num, min), max);
-}
+export function debounce<T extends (...args: unknown[]) => void>(
+  fn: T,
+  ms: number
+): (...args: Parameters<T>) => void {
+  let timer: ReturnType<typeof setTimeout>;
+  return (...args) => {
+    clearTimeout(timer);
+    timer = setTimeout(() => fn(...args), ms);
+  };
+}
+
+export function throttle<T extends (...args: unknown[]) => void>(
+  fn: T,
+  ms: number
+): (...args: Parameters<T>) => void {
+  let last = 0;
+  return (...args) => {
+    const now = Date.now();
+    if (now - last >= ms) {
+      last = now;
+      fn(...args);
+    }
+  };
+}`,
  },
  {
    id: 'mock-3',
    user_id: 'mock',
    conversation_id: null,
    file_path: 'src/hooks/useAuth.ts',
    change_type: 'created',
    created_at: new Date().toISOString(),
    diff: `diff --git a/src/hooks/useAuth.ts b/src/hooks/useAuth.ts
new file mode 100644
index 0000000..3333333
--- /dev/null
+++ b/src/hooks/useAuth.ts
@@ -0,0 +1,25 @@
+import { useState, useCallback } from "react";
+
+interface User {
+  id: string;
+  name: string;
+  email: string;
+}
+
+export function useAuth() {
+  const [user, setUser] = useState<User | null>(null);
+  const [loading, setLoading] = useState(false);
+
+  const login = useCallback(async (email: string, password: string) => {
+    setLoading(true);
+    try {
+      // API call
+      const res = await fetch("/api/login", { method: "POST", body: JSON.stringify({ email, password }) });
+      const data = await res.json();
+      setUser(data.user);
+    } finally {
+      setLoading(false);
+    }
+  }, []);
+
+  const logout = useCallback(() => setUser(null), []);
+
+  return { user, loading, login, logout };
+}`,
  },
];

const statusConfig = {
  pending: { label: '待处理', icon: Clock, color: 'text-muted-foreground' },
  in_progress: { label: '进行中', icon: Loader2, color: 'text-primary' },
  completed: { label: '已完成', icon: CheckCircle2, color: 'text-green-400' },
};

// 从diff解析统计
function parseDiffStats(diff?: string) {
  if (!diff) return { additions: 0, deletions: 0 };
  const lines = diff.split('\n');
  let additions = 0;
  let deletions = 0;
  for (const line of lines) {
    if (line.startsWith('+') && !line.startsWith('+++')) additions++;
    if (line.startsWith('-') && !line.startsWith('---')) deletions++;
  }
  return { additions, deletions };
}

export default function RightPanel({ tasks, modifiedFiles, collapsed, onToggleCollapse }: RightPanelProps) {
  const [expandedDiff, setExpandedDiff] = useState<ModifiedFile | null>(null);
  const [expandedFiles, setExpandedFiles] = useState(true);
  const [expandedTasks, setExpandedTasks] = useState(true);

  // 无真实数据时使用Mock数据展示效果
  const displayFiles = modifiedFiles.length > 0 ? modifiedFiles : defaultMockFiles;

  if (collapsed) {
    return (
      <div className="w-7 shrink-0 border-l border-border bg-card flex flex-col items-center py-2">
        <button
          type="button"
          onClick={onToggleCollapse}
          className="w-5 h-8 flex items-center justify-center rounded hover:bg-secondary/60 text-muted-foreground hover:text-foreground transition-colors"
        >
          <ChevronLeft className="w-3.5 h-3.5" />
        </button>
      </div>
    );
  }

  const totalAdditions = displayFiles.reduce((sum, f) => sum + parseDiffStats(f.diff).additions, 0);
  const totalDeletions = displayFiles.reduce((sum, f) => sum + parseDiffStats(f.diff).deletions, 0);
  const totalFiles = displayFiles.length;

  return (
    <>
      <div className="w-60 shrink-0 border-l border-border bg-card flex flex-col relative">
        {/* 收缩按钮 */}
        <button
          type="button"
          onClick={onToggleCollapse}
          className="absolute -left-2.5 top-3 z-10 w-5 h-8 flex items-center justify-center rounded-l bg-card border border-border border-r-0 text-muted-foreground hover:text-foreground transition-colors"
        >
          <ChevronRight className="w-3.5 h-3.5" />
        </button>

        {/* 上方：任务列表 */}
        <div className="flex-1 min-h-0 flex flex-col border-b border-border">
          <button
            type="button"
            onClick={() => setExpandedTasks(!expandedTasks)}
            className="flex items-center justify-between px-3 py-2 border-b border-border shrink-0 hover:bg-secondary/30 transition-colors"
          >
            <span className="text-xs font-medium text-foreground">
              <ListTodo className="w-3.5 h-3.5 inline mr-1.5 text-primary" />
              任务
            </span>
            {expandedTasks ? (
              <ChevronDown className="w-3 h-3 text-muted-foreground" />
            ) : (
              <ChevronRight className="w-3 h-3 text-muted-foreground" />
            )}
          </button>
          {expandedTasks && (
            <div className="flex-1 overflow-y-auto py-1">
              {tasks.length === 0 ? (
                <div className="px-3 py-6 text-center text-xs text-muted-foreground">暂无任务</div>
              ) : (
                tasks.map((task) => {
                  const config = statusConfig[task.status];
                  const StatusIcon = config.icon;
                  return (
                    <div key={task.id} className="flex items-start gap-2 px-3 py-2 hover:bg-secondary/30 transition-colors">
                      <StatusIcon className={`w-3.5 h-3.5 mt-0.5 shrink-0 ${config.color} ${task.status === 'in_progress' ? 'animate-spin' : ''}`} />
                      <div className="flex-1 min-w-0">
                        <div className="text-xs text-foreground truncate">{task.title}</div>
                        <div className="text-[11px] text-muted-foreground mt-0.5">{config.label}</div>
                      </div>
                    </div>
                  );
                })
              )}
            </div>
          )}
        </div>

        {/* 下方：变更文件列表 */}
        <div className="flex-1 min-h-0 flex flex-col">
          <button
            type="button"
            onClick={() => setExpandedFiles(!expandedFiles)}
            className="flex items-center justify-between px-3 py-2 border-b border-border shrink-0 hover:bg-secondary/30 transition-colors"
          >
            <span className="text-xs font-medium text-foreground">
              <FileEdit className="w-3.5 h-3.5 inline mr-1.5 text-primary" />
              变更
            </span>
            {expandedFiles ? (
              <ChevronDown className="w-3 h-3 text-muted-foreground" />
            ) : (
              <ChevronRight className="w-3 h-3 text-muted-foreground" />
            )}
          </button>
          {expandedFiles && (
            <div className="flex-1 overflow-y-auto py-1 font-mono">
              {displayFiles.length === 0 ? (
                <div className="px-3 py-6 text-center text-xs text-muted-foreground">暂无变更文件</div>
              ) : (
                <>
                  {displayFiles.map((file) => {
                    const stats = parseDiffStats(file.diff);
                    return (
                      <button
                        key={file.id}
                        type="button"
                        onClick={() => setExpandedDiff(file)}
                        className="w-full flex items-center gap-2 px-3 py-1.5 hover:bg-secondary/30 transition-colors text-left"
                      >
                        <span className="text-[12px] text-foreground truncate flex-1 min-w-0">{file.file_path}</span>
                        <span className="text-[11px] text-green-400 shrink-0">+{stats.additions}</span>
                        <span className="text-[11px] text-red-400 shrink-0">-{stats.deletions}</span>
                      </button>
                    );
                  })}
                  {/* 总计行 */}
                  <div className="border-t border-border mt-1 pt-1 px-3 py-1.5 flex items-center gap-2 text-[12px]">
                    <span className="text-muted-foreground flex-1">总计</span>
                    <span className="text-green-400 shrink-0">+{totalAdditions}</span>
                    <span className="text-red-400 shrink-0">-{totalDeletions}</span>
                    <span className="text-muted-foreground shrink-0">({totalFiles}个文件)</span>
                  </div>
                </>
              )}
            </div>
          )}
        </div>
      </div>

      {/* Diff Sheet 抽屉 */}
      <Sheet open={!!expandedDiff} onOpenChange={(open) => !open && setExpandedDiff(null)}>
        <SheetContent side="right" className="w-screen sm:w-screen p-0 bg-card border-l border-border flex flex-col">
          {/* 头部标题栏 */}
          <SheetHeader className="px-4 py-3 border-b border-border shrink-0 pr-12">
            <SheetTitle className="flex items-center gap-2 text-sm text-foreground font-mono">
              <FileCode2 className="w-4 h-4 text-primary" />
              {expandedDiff?.file_path || '文件变更'}
            </SheetTitle>
          </SheetHeader>

          {/* 导航工具栏（与关闭按钮错开） */}
          <div className="flex items-center justify-between px-4 py-2 border-b border-border shrink-0 bg-secondary/20">
            <span className="text-[11px] text-muted-foreground">
              {expandedDiff ? `${displayFiles.findIndex((f) => f.id === expandedDiff.id) + 1} / ${displayFiles.length}` : ''}
            </span>
            <div className="flex items-center gap-1">
              <button
                type="button"
                disabled={!expandedDiff || displayFiles.findIndex((f) => f.id === expandedDiff.id) <= 0}
                onClick={() => {
                  if (!expandedDiff) return;
                  const idx = displayFiles.findIndex((f) => f.id === expandedDiff.id);
                  if (idx > 0) setExpandedDiff(displayFiles[idx - 1]);
                }}
                className="flex items-center gap-0.5 px-2 py-1 text-[12px] rounded border border-border bg-card text-foreground hover:bg-secondary/60 disabled:opacity-30 disabled:cursor-not-allowed transition-colors"
              >
                <ChevronLeft className="w-3 h-3" />
                上一个
              </button>
              <button
                type="button"
                disabled={!expandedDiff || displayFiles.findIndex((f) => f.id === expandedDiff.id) >= displayFiles.length - 1}
                onClick={() => {
                  if (!expandedDiff) return;
                  const idx = displayFiles.findIndex((f) => f.id === expandedDiff.id);
                  if (idx < displayFiles.length - 1) setExpandedDiff(displayFiles[idx + 1]);
                }}
                className="flex items-center gap-0.5 px-2 py-1 text-[12px] rounded border border-border bg-card text-foreground hover:bg-secondary/60 disabled:opacity-30 disabled:cursor-not-allowed transition-colors"
              >
                下一个
                <ChevronRight className="w-3 h-3" />
              </button>
            </div>
          </div>

          {/* Split Diff 主体 */}
          <div className="flex-1 overflow-auto flex">
            {(() => {
              const diffLines = parseDiffToSplit(expandedDiff?.diff);
              return (
                <>
                  {/* 左侧：修改前 */}
                  <div className="flex-1 min-w-0 border-r border-border">
                    <div className="sticky top-0 z-10 px-3 py-1.5 text-[11px] text-muted-foreground bg-card border-b border-border flex items-center gap-1">
                      <FileMinus className="w-3 h-3" />
                      修改前
                    </div>
                    <div className="font-mono text-[12px] leading-relaxed">
                      {diffLines.map((line, i) => {
                        if (line.type === 'header') {
                          return (
                            <div key={`l-${i}`} className="px-2 py-0.5 text-muted-foreground bg-secondary/20">
                              {line.leftContent}
                            </div>
                          );
                        }
                        if (line.type === 'add') {
                          return <div key={`l-${i}`} className="h-[1.5em] bg-secondary/10" />;
                        }
                        const bg = line.type === 'del' ? 'bg-red-400/10' : '';
                        const textColor = line.type === 'del' ? 'text-red-400' : 'text-foreground';
                        return (
                          <div key={`l-${i}`} className={`flex ${bg}`}>
                            <span className="w-8 shrink-0 text-right pr-2 text-muted-foreground select-none">
                              {line.leftNum || ''}
                            </span>
                            <span className={`flex-1 px-1 ${textColor}`}>{line.leftContent || ''}</span>
                          </div>
                        );
                      })}
                    </div>
                  </div>
                  {/* 右侧：修改后 */}
                  <div className="flex-1 min-w-0">
                    <div className="sticky top-0 z-10 px-3 py-1.5 text-[11px] text-muted-foreground bg-card border-b border-border flex items-center gap-1">
                      <FilePlus className="w-3 h-3" />
                      修改后
                    </div>
                    <div className="font-mono text-[12px] leading-relaxed">
                      {diffLines.map((line, i) => {
                        if (line.type === 'header') {
                          return (
                            <div key={`r-${i}`} className="px-2 py-0.5 text-muted-foreground bg-secondary/20">
                              {line.rightContent}
                            </div>
                          );
                        }
                        if (line.type === 'del') {
                          return <div key={`r-${i}`} className="h-[1.5em] bg-secondary/10" />;
                        }
                        const bg = line.type === 'add' ? 'bg-green-400/10' : '';
                        const textColor = line.type === 'add' ? 'text-green-400' : 'text-foreground';
                        return (
                          <div key={`r-${i}`} className={`flex ${bg}`}>
                            <span className="w-8 shrink-0 text-right pr-2 text-muted-foreground select-none">
                              {line.rightNum || ''}
                            </span>
                            <span className={`flex-1 px-1 ${textColor}`}>{line.rightContent || ''}</span>
                          </div>
                        );
                      })}
                    </div>
                  </div>
                </>
              );
            })()}
          </div>
        </SheetContent>
      </Sheet>
    </>
  );
}
