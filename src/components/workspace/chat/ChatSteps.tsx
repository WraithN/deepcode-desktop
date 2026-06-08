import { useState } from 'react';
import type { MessageStep, } from '@/types/types';
import {
  Brain, Wrench, FileCheck, Lightbulb,
  ShieldCheck, HelpCircle,
  RefreshCw, AlertCircle,
  FileCode, Timer, AlignLeft,
  ListTodo, Minimize2, 
} from 'lucide-react';

export const stepConfig: Record<string, { label: string; icon: React.ElementType; border: string; bg: string; text: string; labelColor: string }> = {
  thinking: {
    label: '思考中', icon: Brain, border: 'border-muted-foreground/20', bg: 'bg-secondary/30', text: 'text-muted-foreground', labelColor: 'text-muted-foreground',
  },
  tool_use: {
    label: '使用工具', icon: Wrench, border: 'border-primary/30', bg: 'bg-primary/5', text: 'text-foreground', labelColor: 'text-primary',
  },
  tool_result: {
    label: '工具结果', icon: FileCheck, border: 'border-green-400/30', bg: 'bg-green-400/5', text: 'text-foreground', labelColor: 'text-green-400',
  },
  ask_permission: {
    label: '权限询问', icon: ShieldCheck, border: 'border-yellow-400/30', bg: 'bg-yellow-400/5', text: 'text-foreground', labelColor: 'text-yellow-400',
  },
  ask_user: {
    label: '用户确认', icon: HelpCircle, border: 'border-blue-400/30', bg: 'bg-blue-400/5', text: 'text-foreground', labelColor: 'text-blue-400',
  },
  final: {
    label: '结果', icon: Lightbulb, border: 'border-border', bg: 'bg-transparent', text: 'text-foreground', labelColor: 'text-foreground',
  },
  compress: {
    label: '压缩中', icon: Minimize2, border: 'border-purple-400/30', bg: 'bg-purple-400/5', text: 'text-foreground', labelColor: 'text-purple-400',
  },
  retry: {
    label: '重试', icon: RefreshCw, border: 'border-orange-400/30', bg: 'bg-orange-400/5', text: 'text-foreground', labelColor: 'text-orange-400',
  },
};

// 读文件内容渲染（支持Markdown和代码高亮）
export function ReadFileContent({ content }: { content: string }) {
  const parts = content.split(/(```[\s\S]*?```)/g);
  return (
    <div className="text-xs text-foreground leading-relaxed">
      {parts.map((part, i) => {
        if (part.startsWith('```') && part.endsWith('```')) {
          const lines = part.slice(3, -3).split('\n');
          const lang = lines[0]?.trim() || '';
          const code = lines.slice(lang ? 1 : 0).join('\n');
          return (
            <div key={i} className="my-2 rounded overflow-hidden border border-border">
              <div className="flex items-center justify-between px-3 py-1 bg-secondary/50 border-b border-border">
                <span className="text-xs text-muted-foreground font-mono">{lang || 'text'}</span>
              </div>
              <pre className="p-3 text-[12px] font-mono leading-relaxed overflow-x-auto bg-background/50"><code>{code}</code></pre>
            </div>
          );
        }
        // Markdown 内联格式
        const formatted = part
          .replace(/`([^`]+)`/g, (_, code) => `<code class="px-1 py-0.5 rounded bg-secondary text-xs font-mono text-primary">${code}</code>`)
          .replace(/\*\*([^*]+)\*\*/g, (_, text) => `<strong class="font-semibold">${text}</strong>`)
          .replace(/\*([^*]+)\*/g, (_, text) => `<em class="italic">${text}</em>`);
        return (
          <span
            key={i}
            className="whitespace-pre-wrap"
            dangerouslySetInnerHTML={{ __html: formatted }}
          />
        );
      })}
    </div>
  );
}

export function PermissionStep({ step, onAnswer }: {
  step: MessageStep;
  onAnswer: (answer: 'once' | 'session' | 'deny') => void;
}) {
  const config = stepConfig.ask_permission;
  const Icon = config.icon;
  const interaction = step.interaction;
  const toolName = interaction?.toolName || step.permissionType || 'unknown';
  const action = interaction?.action || step.content;

  return (
    <div className={`rounded-md border ${config.border} ${config.bg} overflow-hidden`}>
      <div className="flex items-center gap-2 px-3 py-1.5">
        <Icon className={`w-3 h-3 shrink-0 ${config.labelColor}`} />
        <span className={`text-[12px] font-medium ${config.labelColor}`}>
          {config.label} · {toolName}
        </span>
      </div>
      <div className="px-3 pb-2 text-xs text-foreground leading-relaxed whitespace-pre-wrap">
        {action}
      </div>
      <div className="flex flex-col gap-1.5 px-3 pb-3">
        <button
          type="button"
          onClick={() => onAnswer('once')}
          className="w-full py-2 px-3 text-xs rounded-md bg-primary/15 text-primary hover:bg-primary/25 transition-colors border border-primary/25 font-medium"
        >
          本次同意 (once)
        </button>
        <button
          type="button"
          onClick={() => onAnswer('session')}
          className="w-full py-2 px-3 text-xs rounded-md bg-secondary text-foreground hover:bg-secondary/80 transition-colors border border-border"
        >
          本 Session 同意 (always)
        </button>
        <button
          type="button"
          onClick={() => onAnswer('deny')}
          className="w-full py-2 px-3 text-xs rounded-md bg-destructive/10 text-destructive hover:bg-destructive/20 transition-colors border border-destructive/20"
        >
          不同意 (reject)
        </button>
      </div>
    </div>
  );
}

export function UserQuestionsStep({ step, onSubmit }: {
  step: MessageStep;
  onSubmit: (answers: Record<string, string | string[]>) => void;
}) {
  const config = stepConfig.ask_user;
  const Icon = config.icon;
  const questions = step.interaction?.questions || [];
  const [answers, setAnswers] = useState<Record<string, string | string[]>>({});
  const [customInputs, setCustomInputs] = useState<Record<string, string>>({});

  const toggleOption = (qIdx: number, label: string, multiple: boolean) => {
    const key = String(qIdx);
    setAnswers(prev => {
      const current = prev[key];
      if (multiple) {
        const arr = Array.isArray(current) ? [...current] : [];
        if (arr.includes(label)) {
          return { ...prev, [key]: arr.filter(l => l !== label) };
        }
        return { ...prev, [key]: [...arr, label] };
      }
      return { ...prev, [key]: label };
    });
  };

  const handleCustomSubmit = (qIdx: number) => {
    const text = customInputs[qIdx]?.trim();
    if (!text) { return; }
    setAnswers(prev => ({ ...prev, [String(qIdx)]: text }));
  };

  const canSubmit = questions.every((_, idx) => {
    const ans = answers[idx];
    return ans !== undefined && (typeof ans === 'string' ? ans.length > 0 : ans.length > 0);
  });

  return (
    <div className={`rounded-md border ${config.border} ${config.bg} overflow-hidden`}>
      <div className="flex items-center gap-2 px-3 py-1.5">
        <Icon className={`w-3 h-3 shrink-0 ${config.labelColor}`} />
        <span className={`text-[12px] font-medium ${config.labelColor}`}>{config.label}</span>
      </div>
      <div className="flex flex-col gap-3 px-3 pb-3">
        {questions.map((q, qIdx) => (
          <div key={qIdx} className="flex flex-col gap-1.5">
            <div className="text-xs font-medium text-foreground">{q.header}</div>
            <div className="text-xs text-muted-foreground">{q.question}</div>
            {q.multiple && (
              <div className="text-[10px] text-muted-foreground">可多选</div>
            )}
            <div className="flex flex-col gap-1">
              {q.options.map((opt, oIdx) => {
                const ans = answers[qIdx];
                const isSelected = q.multiple
                  ? Array.isArray(ans) && ans.includes(opt.label)
                  : ans === opt.label;
                return (
                  <button
                    key={oIdx}
                    type="button"
                    onClick={() => toggleOption(qIdx, opt.label, q.multiple)}
                    className={`text-left px-2.5 py-1.5 text-xs rounded-md border transition-colors ${
                      isSelected
                        ? 'bg-primary/15 text-primary border-primary/25'
                        : 'bg-secondary text-foreground border-border hover:bg-secondary/80'
                    }`}
                  >
                    <span className="font-medium">{opt.label}</span>
                    {opt.description && (
                      <span className="text-muted-foreground ml-1">· {opt.description}</span>
                    )}
                  </button>
                );
              })}
              {/* 自定义输入（opencode 默认启用 custom） */}
              <div className="flex items-center gap-2 mt-1">
                <input
                  type="text"
                  value={customInputs[qIdx] || ''}
                  onChange={(e) => setCustomInputs(prev => ({ ...prev, [qIdx]: e.target.value }))}
                  onKeyDown={(e) => {
                    if (e.key === 'Enter') {
                      e.preventDefault();
                      handleCustomSubmit(qIdx);
                    }
                  }}
                  placeholder="输入自定义答案..."
                  className="flex-1 min-w-0 px-2.5 py-1.5 text-xs bg-secondary border border-border rounded-md text-foreground placeholder:text-muted-foreground focus:outline-none focus:border-primary"
                />
                <button
                  type="button"
                  onClick={() => handleCustomSubmit(qIdx)}
                  disabled={!customInputs[qIdx]?.trim()}
                  className="shrink-0 px-2.5 py-1.5 text-xs rounded-md bg-secondary text-foreground hover:bg-secondary/80 transition-colors border border-border disabled:opacity-40"
                >
                  使用
                </button>
              </div>
            </div>
          </div>
        ))}
        <button
          type="button"
          onClick={() => onSubmit(answers)}
          disabled={!canSubmit}
          className="w-full py-2 px-3 text-xs rounded-md bg-primary text-primary-foreground hover:bg-primary/90 transition-colors font-medium disabled:opacity-40"
        >
          提交回答
        </button>
      </div>
    </div>
  );
}

export function TodoWriteStep({ step }: { step: MessageStep }) {
  const todos = step.interaction?.todos || [];

  const priorityColor: Record<string, string> = {
    high: 'text-red-400',
    medium: 'text-yellow-400',
    low: 'text-muted-foreground',
  };

  const statusConfig: Record<string, { icon: string; color: string }> = {
    pending: { icon: '☐', color: 'text-muted-foreground' },
    in_progress: { icon: '◐', color: 'text-primary' },
    completed: { icon: '☑', color: 'text-green-400' },
    cancelled: { icon: '☒', color: 'text-red-400' },
  };

  return (
    <div className="rounded-md border border-border/60 bg-card/50 overflow-hidden">
      {/* Header: 参考 Kimi 的 SetTodoList 样式 */}
      <div className="flex items-center gap-2 px-3 py-2 border-b border-border/40">
        <div className="w-2 h-2 rounded-full bg-green-400 shrink-0" />
        <ListTodo className="w-3.5 h-3.5 shrink-0 text-primary" />
        <span className="text-[12px] font-medium text-foreground">Update Todos</span>
        <span className="text-[11px] text-muted-foreground ml-1">{todos.length} 项任务</span>
      </div>
      {/* Todo 列表 */}
      <div className="flex flex-col">
        {todos.map((todo, idx) => {
          const status = statusConfig[todo.status] || statusConfig.pending;
          const isCompleted = todo.status === 'completed';
          return (
            <div
              key={todo.id}
              className={`flex items-start gap-2.5 px-3 py-2 ${idx !== todos.length - 1 ? 'border-b border-border/30' : ''} ${isCompleted ? 'opacity-60' : ''}`}
            >
              <span className={`text-sm mt-0.5 shrink-0 ${status.color}`}>{status.icon}</span>
              <div className="flex-1 min-w-0 flex flex-col">
                <span className={`text-xs leading-relaxed ${isCompleted ? 'text-muted-foreground line-through' : 'text-foreground'}`}>
                  {todo.content}
                </span>
                <div className="flex items-center gap-2 mt-0.5">
                  <span className={`text-[10px] font-medium px-1 py-0.5 rounded ${priorityColor[todo.priority] || ''} bg-secondary/40`}>
                    {todo.priority === 'high' ? '高优先级' : todo.priority === 'medium' ? '中优先级' : '低优先级'}
                  </span>
                  <span className="text-[10px] text-muted-foreground">
                    {todo.status === 'completed' ? '已完成' : todo.status === 'in_progress' ? '进行中' : todo.status === 'cancelled' ? '已取消' : '待处理'}
                  </span>
                </div>
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}

export function StepItem({ step, index, onAnswerPermission, onAnswerUser, onRetry }: {
  step: MessageStep; index: number;
  onAnswerPermission?: (answer: 'once' | 'session' | 'deny') => void;
  onAnswerUser?: (answers: Record<string, string | string[]>) => void;
  onRetry?: () => void;
}) {
  const config = stepConfig[step.type] || stepConfig.thinking;
  const Icon = config.icon;
  const [expanded, setExpanded] = useState(step.type === 'final' || step.type === 'compress');
  const [showDiff, setShowDiff] = useState(false);

  if (step.type === 'ask_permission' && onAnswerPermission) { return <PermissionStep step={step} onAnswer={onAnswerPermission} />; }
  if (step.type === 'ask_user' && onAnswerUser) { return <UserQuestionsStep step={step} onSubmit={onAnswerUser} />; }
  if (step.type === 'tool_result' && step.toolName === 'todowrite') { return <TodoWriteStep step={step} />; }

  const isCollapsible = step.type !== 'final' && step.type !== 'compress';
  const isFailed = step.failed === true;

  // 压缩区块
  if (step.type === 'compress' && step.compressInfo) {
    const { status, originalSize, compressedSize, ratio } = step.compressInfo;
    const isDone = status === 'done';
    return (
      <div className={`rounded-md border ${config.border} ${config.bg} overflow-hidden animate-in fade-in slide-in-from-bottom-1 duration-300`} style={{ animationDelay: `${index * 150}ms`, animationFillMode: 'both' }}>
        <div className="flex items-center gap-2 px-3 py-1.5">
          <Icon className={`w-3 h-3 shrink-0 ${config.labelColor} ${isDone ? '' : 'animate-pulse'}`} />
          <span className={`text-[12px] font-medium ${config.labelColor}`}>{isDone ? '压缩完成' : '压缩中'}</span>
          {isDone && (
            <span className="ml-auto text-xs text-green-400">
              {((1 - ratio) * 100).toFixed(1)}% 压缩比
            </span>
          )}
        </div>
        <div className="px-3 pb-2 text-xs text-foreground leading-relaxed">
          <div className="flex items-center gap-3 mt-1">
            <span className="text-xs text-muted-foreground">原始 {originalSize.toLocaleString()} bytes</span>
            {isDone && (
              <>
                <span className="text-xs text-muted-foreground">→</span>
                <span className="text-xs text-muted-foreground">压缩后 {compressedSize.toLocaleString()} bytes</span>
              </>
            )}
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className={`rounded-md border ${isFailed ? 'border-destructive/30' : config.border} ${isFailed ? 'bg-destructive/5' : config.bg} overflow-hidden animate-in fade-in slide-in-from-bottom-1 duration-300`} style={{ animationDelay: `${index * 150}ms`, animationFillMode: 'both' }}>
      <div className="flex items-center gap-2 px-3 py-1.5">
        {isFailed ? <AlertCircle className="w-3 h-3 shrink-0 text-destructive" /> : <Icon className={`w-3 h-3 shrink-0 ${config.labelColor}`} />}
        <span className={`text-[12px] font-medium ${isFailed ? 'text-destructive' : config.labelColor}`}>
          {isFailed ? '执行失败' : config.label}{step.toolName && ` · ${step.toolName}`}
        </span>
        {/* 工具调用摘要 */}
        {step.summary && (
          <span className="ml-2 flex items-center gap-1 text-xs text-muted-foreground">
            {step.summary.file && <><FileCode className="w-2.5 h-2.5" />{step.summary.file}</>}
            {step.summary.lines !== undefined && <><AlignLeft className="w-2.5 h-2.5" />{step.summary.lines}行</>}
            {step.summary.durationMs !== undefined && <><Timer className="w-2.5 h-2.5" />{(step.summary.durationMs / 1000).toFixed(1)}s</>}
          </span>
        )}
        {isCollapsible && <button type="button" onClick={() => setExpanded(!expanded)} className="ml-auto text-xs text-muted-foreground hover:text-foreground transition-colors">{expanded ? '收起' : '展开'}</button>}
      </div>
      {expanded && (
        <div className="px-3 pb-2">
          {/* 失败重试按钮 */}
          {isFailed && onRetry && (
            <button
              type="button"
              onClick={onRetry}
              className="flex items-center gap-1 mb-2 px-2 py-1 text-[12px] rounded bg-primary/10 text-primary hover:bg-primary/20 transition-colors border border-primary/20"
            >
              <RefreshCw className="w-3 h-3" />
              点击重试
            </button>
          )}
          {/* diff展示（写文件） */}
          {step.diff && (
            <div className="mb-2">
              <button
                type="button"
                onClick={() => setShowDiff(!showDiff)}
                className="flex items-center gap-1 text-xs text-muted-foreground hover:text-foreground transition-colors mb-1"
              >
                <FileCode className="w-3 h-3" />
                {showDiff ? '隐藏Diff' : '查看Diff'}
              </button>
              {showDiff && (
                <div className="rounded border border-border overflow-hidden text-xs font-mono leading-relaxed max-h-48 overflow-y-auto">
                  {step.diff.split('\n').map((line, i) => {
                    if (line.startsWith('+')) { return <div key={i} className="px-2 bg-green-400/10 text-green-400">{line}</div>; }
                    if (line.startsWith('-')) { return <div key={i} className="px-2 bg-red-400/10 text-red-400">{line}</div>; }
                    if (line.startsWith('@@')) { return <div key={i} className="px-2 text-primary bg-secondary/30">{line}</div>; }
                    return <div key={i} className="px-2 text-muted-foreground">{line}</div>;
                  })}
                </div>
              )}
            </div>
          )}
          {/* 读文件Markdown/代码高亮 */}
          {(step.toolName === 'read_file' || step.content.includes('```')) ? (
            <ReadFileContent content={step.content} />
          ) : (
            <div className={`text-xs ${isFailed ? 'text-destructive' : config.text} leading-relaxed whitespace-pre-wrap`}>{step.content}</div>
          )}
        </div>
      )}
    </div>
  );
}
