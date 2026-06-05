import type { Task, ModifiedFile, GitChangedFile } from '@/types/types';
import { useEffect, useState } from 'react';
import {
  ListTodo, FileEdit, CheckCircle2, Clock, Loader2,
  ChevronDown, ChevronRight,
  ChevronLeft, FileCode2, FilePlus, FileMinus,
} from 'lucide-react';
import { invoke } from '@tauri-apps/api/core';
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
  workspace: string;
  collapsed: boolean;
  onToggleCollapse: () => void;
}

const statusConfig = {
  pending: { label: '待处理', icon: Clock, color: 'text-muted-foreground' },
  in_progress: { label: '进行中', icon: Loader2, color: 'text-primary' },
  completed: { label: '已完成', icon: CheckCircle2, color: 'text-green-400' },
};

const gitStatusConfig: Record<GitChangedFile['status'], { label: string; className: string }> = {
  M: { label: 'M', className: 'text-orange-400' },
  U: { label: 'U', className: 'text-green-400' },
  A: { label: 'A', className: 'text-green-400' },
  D: { label: 'D', className: 'text-red-400' },
  R: { label: 'R', className: 'text-blue-400' },
};

export default function RightPanel({ tasks, modifiedFiles, workspace, collapsed, onToggleCollapse }: RightPanelProps) {
  const [expandedDiff, setExpandedDiff] = useState<GitChangedFile | null>(null);
  const [expandedFiles, setExpandedFiles] = useState(true);
  const [expandedTasks, setExpandedTasks] = useState(true);
  const [gitFiles, setGitFiles] = useState<GitChangedFile[]>([]);
  const [gitError, setGitError] = useState<string | null>(null);

  useEffect(() => {
    if (!workspace) return;
    invoke<GitChangedFile[]>('git_changed_files', { workspace })
      .then((data) => {
        setGitFiles(Array.isArray(data) ? data : []);
        setGitError(null);
      })
      .catch((error) => {
        setGitFiles([]);
        setGitError(error instanceof Error ? error.message : String(error));
      });
  }, [workspace, modifiedFiles]);

  const displayFiles = gitFiles;

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

  const totalAdditions = displayFiles.reduce((sum, f) => sum + f.additions, 0);
  const totalDeletions = displayFiles.reduce((sum, f) => sum + f.deletions, 0);
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
                        <div className="text-xs text-muted-foreground mt-0.5">{config.label}</div>
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
            <div className="flex-1 min-h-0 flex flex-col font-mono">
              <div className="flex-1 min-h-0 overflow-y-auto py-1">
                {displayFiles.length === 0 ? (
                  <div className={`px-3 py-6 text-center text-xs ${gitError ? 'text-red-400' : 'text-muted-foreground'}`}>{gitError || '暂无变更文件'}</div>
                ) : (
                  <>
                    {gitError && <div className="px-3 py-2 text-xs text-red-400">{gitError}</div>}
                    {displayFiles.map((file) => {
                      const status = gitStatusConfig[file.status];
                      return (
                        <button
                          key={file.path}
                          type="button"
                          onClick={() => setExpandedDiff(file)}
                          className="w-full flex items-center gap-2 px-3 py-1.5 hover:bg-secondary/30 transition-colors text-left"
                          title={file.path}
                        >
                          <span className="text-[12px] text-foreground truncate flex-1 min-w-0">{file.path}</span>
                          <span className={`text-xs shrink-0 ${status.className}`}>{status.label}</span>
                          <span className="text-xs text-green-400 shrink-0">+{file.additions}</span>
                          <span className="text-xs text-red-400 shrink-0">-{file.deletions}</span>
                        </button>
                      );
                    })}
                  </>
                )}
              </div>
              <div className="border-t border-border px-3 py-1.5 flex items-center gap-2 text-[12px] shrink-0 bg-card">
                <span className="text-muted-foreground flex-1">总计</span>
                <span className="text-green-400 shrink-0">+{totalAdditions}</span>
                <span className="text-red-400 shrink-0">-{totalDeletions}</span>
                <span className="text-muted-foreground shrink-0">({totalFiles}个文件)</span>
              </div>
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
              {expandedDiff?.path || '文件变更'}
            </SheetTitle>
          </SheetHeader>

          {/* 导航工具栏（与关闭按钮错开） */}
          <div className="flex items-center justify-between px-3 py-0.5 border-b border-border shrink-0 bg-secondary/20">
            <span className="text-xs text-muted-foreground">
              {expandedDiff ? `${displayFiles.findIndex((f) => f.path === expandedDiff.path) + 1} / ${displayFiles.length}` : ''}
            </span>
            <div className="flex items-center gap-1">
              <button
                type="button"
                disabled={!expandedDiff || displayFiles.findIndex((f) => f.path === expandedDiff.path) <= 0}
                onClick={() => {
                  if (!expandedDiff) return;
                  const idx = displayFiles.findIndex((f) => f.path === expandedDiff.path);
                  if (idx > 0) setExpandedDiff(displayFiles[idx - 1]);
                }}
                className="flex items-center gap-0.5 px-1 py-0 text-xs rounded border border-border bg-card text-foreground hover:bg-secondary/60 disabled:opacity-30 disabled:cursor-not-allowed transition-colors"
              >
                <ChevronLeft className="w-3 h-3" />
                上一个
              </button>
              <button
                type="button"
                disabled={!expandedDiff || displayFiles.findIndex((f) => f.path === expandedDiff.path) >= displayFiles.length - 1}
                onClick={() => {
                  if (!expandedDiff) return;
                  const idx = displayFiles.findIndex((f) => f.path === expandedDiff.path);
                  if (idx < displayFiles.length - 1) setExpandedDiff(displayFiles[idx + 1]);
                }}
                className="flex items-center gap-0.5 px-1 py-0 text-xs rounded border border-border bg-card text-foreground hover:bg-secondary/60 disabled:opacity-30 disabled:cursor-not-allowed transition-colors"
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
                    <div className="sticky top-0 z-10 px-3 py-1.5 text-xs text-muted-foreground bg-card border-b border-border flex items-center gap-1">
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
                    <div className="sticky top-0 z-10 px-3 py-1.5 text-xs text-muted-foreground bg-card border-b border-border flex items-center gap-1">
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
