import { useState, useRef, useEffect } from 'react';
import type { Message, MessageStep, Conversation } from '@/types/types';
import {
  Send, Bot, User, Code2, Copy, Check,
  Brain, Wrench, FileCheck, Lightbulb,
  ShieldCheck, HelpCircle,
  ChevronUp, Settings2, Zap, ChevronDown,
  FileCode, Pencil, Loader2, Clock,
  MessageSquare, RefreshCw, Search, FileMinus, FilePlus,
  Minimize2, AlertCircle, Eye, Timer, AlignLeft,
} from 'lucide-react';
import {
  Popover, PopoverContent, PopoverTrigger,
} from '@/components/ui/popover';

interface ChatPanelProps {
  messages: Message[];
  isTyping: boolean;
  activeConversation: Conversation | null;
  conversations: Conversation[];
  activeAgentName: string;
  currentModel: string;
  contextPercent: number;
  agentMode: 'plan' | 'build';
  currentSkill: string;
  editContent?: string;
  onSendMessage: (content: string) => void;
  onAgentModeChange: (mode: 'plan' | 'build') => void;
  onModelChange: (model: string) => void;
  onSkillChange: (skill: string) => void;
  onSelectConversation: (conv: Conversation) => void;
  onAnswerPermission: (stepIndex: number, answer: 'once' | 'session' | 'deny') => void;
  onAnswerUserQuestions: (stepIndex: number, answers: Record<string, string>) => void;
  onEditUserMessage: (content: string) => void;
  onRetryStep?: (messageId: string, stepIndex: number) => void;
}

const stepConfig: Record<string, { label: string; icon: React.ElementType; border: string; bg: string; text: string; labelColor: string }> = {
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

// ========================== 步骤组件 ==========================

function PermissionStep({ step, onAnswer }: {
  step: MessageStep;
  onAnswer: (answer: 'once' | 'session' | 'deny') => void;
}) {
  const config = stepConfig.ask_permission;
  const Icon = config.icon;
  return (
    <div className={`rounded-md border ${config.border} ${config.bg} overflow-hidden`}>
      <div className="flex items-center gap-2 px-3 py-1.5">
        <Icon className={`w-3 h-3 shrink-0 ${config.labelColor}`} />
        <span className={`text-[11px] font-medium ${config.labelColor}`}>{config.label}{step.permissionType && ` · ${step.permissionType}`}</span>
      </div>
      <div className="px-3 pb-2 text-xs text-foreground leading-relaxed whitespace-pre-wrap">{step.content}</div>
      <div className="flex flex-col gap-1.5 px-3 pb-3">
        <button type="button" onClick={() => onAnswer('once')} className="w-full py-2 px-3 text-xs rounded-md bg-primary/15 text-primary hover:bg-primary/25 transition-colors border border-primary/25 font-medium">本次同意</button>
        <button type="button" onClick={() => onAnswer('session')} className="w-full py-2 px-3 text-xs rounded-md bg-secondary text-foreground hover:bg-secondary/80 transition-colors border border-border">本Session同意</button>
        <button type="button" onClick={() => onAnswer('deny')} className="w-full py-2 px-3 text-xs rounded-md bg-destructive/10 text-destructive hover:bg-destructive/20 transition-colors border border-destructive/20">不同意</button>
      </div>
    </div>
  );
}

function UserQuestionsStep({ onSubmit }: {
  onSubmit: (answers: Record<string, string>) => void;
}) {
  const config = stepConfig.ask_user;
  const Icon = config.icon;
  const [customInput, setCustomInput] = useState('');

  return (
    <div className={`rounded-md border ${config.border} ${config.bg} overflow-hidden`}>
      <div className="flex items-center gap-2 px-3 py-1.5">
        <Icon className={`w-3 h-3 shrink-0 ${config.labelColor}`} />
        <span className={`text-[11px] font-medium ${config.labelColor}`}>{config.label}</span>
      </div>
      <div className="px-3 pb-2 text-xs text-foreground leading-relaxed">请选择一个选项或输入自定义答案</div>
      <div className="flex flex-col gap-1.5 px-3 pb-3">
        <button
          type="button"
          onClick={() => onSubmit({ answer: 'recommended' })}
          className="w-full py-2 px-3 text-xs rounded-md bg-primary/15 text-primary hover:bg-primary/25 transition-colors border border-primary/25 font-medium text-left"
        >
          推荐方案（默认）
        </button>
        <div className="w-full flex items-center gap-2">
          <input
            type="text"
            value={customInput}
            onChange={(e) => setCustomInput(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter' && customInput.trim()) {
                e.preventDefault();
                onSubmit({ answer: customInput.trim() });
              }
            }}
            placeholder="输入自定义方案..."
            className="flex-1 min-w-0 px-2.5 py-2 text-xs bg-secondary border border-border rounded-md text-foreground placeholder:text-muted-foreground focus:outline-none focus:border-primary"
          />
          <button
            type="button"
            onClick={() => {
              if (customInput.trim()) onSubmit({ answer: customInput.trim() });
            }}
            disabled={!customInput.trim()}
            className="shrink-0 px-3 py-2 text-xs rounded-md bg-secondary text-foreground hover:bg-secondary/80 transition-colors border border-border disabled:opacity-40"
          >
            确认
          </button>
        </div>
      </div>
    </div>
  );
}

function StepItem({ step, index, onAnswerPermission, onAnswerUser, onRetry }: {
  step: MessageStep; index: number;
  onAnswerPermission?: (answer: 'once' | 'session' | 'deny') => void;
  onAnswerUser?: (answers: Record<string, string>) => void;
  onRetry?: () => void;
}) {
  const config = stepConfig[step.type] || stepConfig.thinking;
  const Icon = config.icon;
  const [expanded, setExpanded] = useState(step.type === 'final' || step.type === 'compress');
  const [showDiff, setShowDiff] = useState(false);

  if (step.type === 'ask_permission' && onAnswerPermission) return <PermissionStep step={step} onAnswer={onAnswerPermission} />;
  if (step.type === 'ask_user' && onAnswerUser) return <UserQuestionsStep onSubmit={onAnswerUser} />;

  const isCollapsible = step.type !== 'final' && step.type !== 'compress';
  const isFailed = step.failed === true;

  // 压缩区块
  if (step.type === 'compress' && step.compressInfo) {
    const { status, originalSize, compressedSize, ratio } = step.compressInfo;
    const isDone = status === 'done';
    return (
      <div className={`rounded-md border ${config.border} ${config.bg} overflow-hidden animate-in fade-in slide-in-from-bottom-1 duration-300`} style={{ animationDelay: `${index * 150}ms`, animationFillMode: 'both' }}>
        <div className="flex items-center gap-2 px-3 py-1.5">
          <Icon className={`w-3 h-3 shrink-0 ${config.labelColor} ${!isDone ? 'animate-pulse' : ''}`} />
          <span className={`text-[11px] font-medium ${config.labelColor}`}>{isDone ? '压缩完成' : '压缩中'}</span>
          {isDone && (
            <span className="ml-auto text-[10px] text-green-400">
              {((1 - ratio) * 100).toFixed(1)}% 压缩比
            </span>
          )}
        </div>
        <div className="px-3 pb-2 text-xs text-foreground leading-relaxed">
          <div className="flex items-center gap-3 mt-1">
            <span className="text-[10px] text-muted-foreground">原始 {originalSize.toLocaleString()} bytes</span>
            {isDone && (
              <>
                <span className="text-[10px] text-muted-foreground">→</span>
                <span className="text-[10px] text-muted-foreground">压缩后 {compressedSize.toLocaleString()} bytes</span>
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
        <span className={`text-[11px] font-medium ${isFailed ? 'text-destructive' : config.labelColor}`}>
          {isFailed ? '执行失败' : config.label}{step.toolName && ` · ${step.toolName}`}
        </span>
        {/* 工具调用摘要 */}
        {step.summary && (
          <span className="ml-2 flex items-center gap-1 text-[10px] text-muted-foreground">
            {step.summary.file && <><FileCode className="w-2.5 h-2.5" />{step.summary.file}</>}
            {step.summary.lines !== undefined && <><AlignLeft className="w-2.5 h-2.5" />{step.summary.lines}行</>}
            {step.summary.durationMs !== undefined && <><Timer className="w-2.5 h-2.5" />{(step.summary.durationMs / 1000).toFixed(1)}s</>}
          </span>
        )}
        {isCollapsible && <button type="button" onClick={() => setExpanded(!expanded)} className="ml-auto text-[10px] text-muted-foreground hover:text-foreground transition-colors">{expanded ? '收起' : '展开'}</button>}
      </div>
      {expanded && (
        <div className="px-3 pb-2">
          {/* 失败重试按钮 */}
          {isFailed && onRetry && (
            <button
              type="button"
              onClick={onRetry}
              className="flex items-center gap-1 mb-2 px-2 py-1 text-[11px] rounded bg-primary/10 text-primary hover:bg-primary/20 transition-colors border border-primary/20"
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
                className="flex items-center gap-1 text-[10px] text-muted-foreground hover:text-foreground transition-colors mb-1"
              >
                <FileCode className="w-3 h-3" />
                {showDiff ? '隐藏Diff' : '查看Diff'}
              </button>
              {showDiff && (
                <div className="rounded border border-border overflow-hidden text-[10px] font-mono leading-relaxed max-h-48 overflow-y-auto">
                  {step.diff.split('\n').map((line, i) => {
                    if (line.startsWith('+')) return <div key={i} className="px-2 bg-green-400/10 text-green-400">{line}</div>;
                    if (line.startsWith('-')) return <div key={i} className="px-2 bg-red-400/10 text-red-400">{line}</div>;
                    if (line.startsWith('@@')) return <div key={i} className="px-2 text-primary bg-secondary/30">{line}</div>;
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

// 读文件内容渲染（支持Markdown和代码高亮）
function ReadFileContent({ content }: { content: string }) {
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
                <span className="text-[10px] text-muted-foreground font-mono">{lang || 'text'}</span>
              </div>
              <pre className="p-3 text-[11px] font-mono leading-relaxed overflow-x-auto bg-background/50"><code>{code}</code></pre>
            </div>
          );
        }
        // Markdown 内联格式
        const formatted = part
          .replace(/`([^`]+)`/g, (_, code) => `<code class="px-1 py-0.5 rounded bg-secondary text-[10px] font-mono text-primary">${code}</code>`)
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

// ========================== 消息气泡 ==========================

function MessageBubble({ message, onAnswerPermission, onAnswerUser, onEditUserMessage, onRetryStep }: {
  message: Message;
  onAnswerPermission?: (stepIndex: number, answer: 'once' | 'session' | 'deny') => void;
  onAnswerUser?: (stepIndex: number, answers: Record<string, string>) => void;
  onEditUserMessage?: (content: string) => void;
  onRetryStep?: (stepIndex: number) => void;
}) {
  const [copied, setCopied] = useState(false);
  const isUser = message.role === 'user';
  const isComplete = message.is_complete ?? true;

  const handleCopy = () => {
    navigator.clipboard.writeText(message.content);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  const renderContent = (content: string) => {
    const parts = content.split(/(```[\s\S]*?```)/g);
    return parts.map((part, i) => {
      if (part.startsWith('```') && part.endsWith('```')) {
        const lines = part.slice(3, -3).split('\n');
        const lang = lines[0]?.trim() || '';
        const code = lines.slice(lang ? 1 : 0).join('\n');
        return (
          <div key={i} className="my-2 rounded overflow-hidden border border-border">
            <div className="flex items-center justify-between px-3 py-1.5 bg-secondary/50 border-b border-border">
              <span className="text-[10px] text-muted-foreground font-mono">{lang}</span>
              <button type="button" onClick={handleCopy} className="text-muted-foreground hover:text-foreground transition-colors">{copied ? <Check className="w-3 h-3" /> : <Copy className="w-3 h-3" />}</button>
            </div>
            <pre className="p-3 text-xs font-mono leading-relaxed overflow-x-auto bg-background/50"><code>{code}</code></pre>
          </div>
        );
      }
      return part.split('\n').map((line, j) => (
        <span key={`${i}-${j}`}>{line}{j < part.split('\n').length - 1 && <br />}</span>
      ));
    });
  };

  if (isUser) {
    return (
      <div className="flex gap-3 px-4 py-3 justify-end group">
        <div className="flex-1 min-w-0 max-w-[85%]">
          <div className="flex items-center justify-end gap-2 mb-1">
            <span className="text-[10px] text-muted-foreground">你</span>
          </div>
          <div className="bg-primary/15 rounded-lg px-4 py-2.5 text-sm text-foreground leading-relaxed">{renderContent(message.content)}</div>
          {/* 用户消息底部操作按钮 */}
          <div className="flex items-center justify-end gap-1.5 mt-1.5 opacity-0 group-hover:opacity-100 transition-opacity">
            <button
              type="button"
              onClick={handleCopy}
              className="flex items-center gap-1 text-[10px] text-muted-foreground hover:text-foreground transition-colors px-1.5 py-0.5 rounded hover:bg-secondary/50"
            >
              {copied ? <Check className="w-3 h-3" /> : <Copy className="w-3 h-3" />}
              复制
            </button>
            <button
              type="button"
              onClick={() => onEditUserMessage?.(message.content)}
              className="flex items-center gap-1 text-[10px] text-muted-foreground hover:text-foreground transition-colors px-1.5 py-0.5 rounded hover:bg-secondary/50"
            >
              <Pencil className="w-3 h-3" />
              编辑
            </button>
          </div>
        </div>
        <div className="w-7 h-7 rounded shrink-0 flex items-center justify-center bg-primary/20"><User className="w-3.5 h-3.5 text-primary" /></div>
      </div>
    );
  }

  // AI消息
  return (
    <div className="flex gap-3 px-4 py-3">
      <div className="w-7 h-7 rounded shrink-0 flex items-center justify-center bg-accent relative">
        <Bot className="w-3.5 h-3.5 text-primary" />
        {!isComplete && (
          <div className="absolute -bottom-0.5 -right-0.5 w-2.5 h-2.5 rounded-full bg-primary border-2 border-background animate-pulse" />
        )}
      </div>
      <div className="flex-1 min-w-0 max-w-[85%]">
        <div className="flex items-center gap-1.5 mb-1">
          <span className="text-[10px] text-muted-foreground">AI助手</span>
          {!isComplete && (
            <span className="flex items-center gap-1 text-[10px] text-primary">
              <Loader2 className="w-2.5 h-2.5 animate-spin" />
              进行中
            </span>
          )}
          {isComplete && (
            <span className="flex items-center gap-1 text-[10px] text-green-400">
              <Check className="w-2.5 h-2.5" />
              已完成
            </span>
          )}
        </div>
        {!isComplete ? (
          // 流式生成中：同时显示实时文本和步骤
          <>
            <div className="text-sm text-foreground leading-relaxed">{renderContent(message.content)}</div>
            {message.steps && message.steps.length > 0 && (
              <div className="space-y-1.5 mt-2">{message.steps.map((step, i) => (
                <StepItem
                  key={`${message.id}-step-${i}`}
                  step={step}
                  index={i}
                  onAnswerPermission={onAnswerPermission ? (answer) => onAnswerPermission(i, answer) : undefined}
                  onAnswerUser={onAnswerUser ? (answers) => onAnswerUser(i, answers) : undefined}
                  onRetry={onRetryStep ? () => onRetryStep(i) : undefined}
                />
              ))}</div>
            )}
          </>
        ) : message.steps && message.steps.length > 0 ? (
          // 已完成且有 steps：只显示步骤摘要
          <div className="space-y-1.5">{message.steps.map((step, i) => (
            <StepItem
              key={`${message.id}-step-${i}`}
              step={step}
              index={i}
              onAnswerPermission={onAnswerPermission ? (answer) => onAnswerPermission(i, answer) : undefined}
              onAnswerUser={onAnswerUser ? (answers) => onAnswerUser(i, answers) : undefined}
              onRetry={onRetryStep ? () => onRetryStep(i) : undefined}
            />
          ))}</div>
        ) : (
          // 已完成且无 steps：显示文本内容
          <div className="text-sm text-foreground leading-relaxed">{renderContent(message.content)}</div>
        )}
        {/* AI消息底部Token和耗时统计 */}
        {isComplete && (message.token_in || message.token_out || message.duration_ms) && (
          <div className="flex items-center gap-3 mt-2 pt-2 border-t border-border/50">
            {message.token_in !== undefined && (
              <span className="text-[10px] text-muted-foreground">
                输入 {message.token_in} tokens
              </span>
            )}
            {message.token_out !== undefined && (
              <span className="text-[10px] text-muted-foreground">
                输出 {message.token_out} tokens
              </span>
            )}
            {message.duration_ms !== undefined && (
              <span className="flex items-center gap-0.5 text-[10px] text-muted-foreground">
                <Clock className="w-2.5 h-2.5" />
                {(message.duration_ms / 1000).toFixed(1)}s
              </span>
            )}
          </div>
        )}
      </div>
    </div>
  );
}

// ========================== 工具栏选择器组件 ==========================

function ModeSelector({ value, onChange }: { value: 'plan' | 'build'; onChange: (v: 'plan' | 'build') => void }) {
  const [open, setOpen] = useState(false);
  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <button type="button" className="flex items-center gap-1 px-2 py-0.5 text-[11px] rounded bg-secondary border border-border text-foreground hover:bg-secondary/80 transition-colors">
          <Settings2 className="w-2.5 h-2.5 text-muted-foreground" />
          {value === 'plan' ? 'Plan' : 'Build'}
          <ChevronUp className={`w-2.5 h-2.5 text-muted-foreground transition-transform ${open ? 'rotate-180' : ''}`} />
        </button>
      </PopoverTrigger>
      <PopoverContent side="top" align="start" className="w-28 p-1">
        <button type="button" onClick={() => { onChange('plan'); setOpen(false); }} className={`w-full text-left px-2.5 py-1.5 text-xs rounded transition-colors ${value === 'plan' ? 'bg-primary/10 text-primary' : 'text-foreground hover:bg-secondary'}`}>Plan</button>
        <button type="button" onClick={() => { onChange('build'); setOpen(false); }} className={`w-full text-left px-2.5 py-1.5 text-xs rounded transition-colors ${value === 'build' ? 'bg-primary/10 text-primary' : 'text-foreground hover:bg-secondary'}`}>Build</button>
      </PopoverContent>
    </Popover>
  );
}

function ModelSelector({ value, onChange }: { value: string; onChange: (v: string) => void }) {
  const [open, setOpen] = useState(false);
  const models = [
    { value: 'gpt-4', label: 'GPT-4' },
    { value: 'gpt-4-turbo', label: 'GPT-4 Turbo' },
    { value: 'claude-3-opus', label: 'Claude 3 Opus' },
    { value: 'claude-3-sonnet', label: 'Claude 3 Sonnet' },
    { value: 'deepseek-v3', label: 'DeepSeek V3' },
  ];
  const current = models.find((m) => m.value === value);

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <button type="button" className="flex items-center gap-1 px-2 py-0.5 text-[11px] rounded bg-secondary border border-border text-foreground hover:bg-secondary/80 transition-colors">
          {current?.label || value}
          <ChevronUp className={`w-2.5 h-2.5 text-muted-foreground transition-transform ${open ? 'rotate-180' : ''}`} />
        </button>
      </PopoverTrigger>
      <PopoverContent side="top" align="start" className="w-36 p-1">
        {models.map((m) => (
          <button key={m.value} type="button" onClick={() => { onChange(m.value); setOpen(false); }} className={`w-full text-left px-2.5 py-1.5 text-xs rounded transition-colors ${value === m.value ? 'bg-primary/10 text-primary' : 'text-foreground hover:bg-secondary'}`}>{m.label}</button>
        ))}
      </PopoverContent>
    </Popover>
  );
}

// ========================== 技能选择 ==========================

interface SkillItem {
  name: string;
  desc: string;
}

const skillCategories = [
  { key: 'all', label: '全部' },
  { key: 'frontend', label: '前端' },
  { key: 'backend', label: '后端' },
  { key: 'fullstack', label: '全栈' },
  { key: 'design', label: '设计' },
  { key: 'other', label: '其他' },
];

const skillData: Record<string, SkillItem[]> = {
  all: [
    { name: 'React', desc: 'React 组件开发与状态管理' },
    { name: 'Vue', desc: 'Vue 组合式API与响应式系统' },
    { name: 'Tailwind CSS', desc: '原子化CSS样式设计与实现' },
    { name: 'Next.js', desc: '全栈React框架与SSR开发' },
    { name: 'Node.js', desc: '服务端JavaScript运行环境' },
    { name: 'Python', desc: 'Python脚本与数据处理' },
    { name: 'TypeScript', desc: '静态类型检查与高级类型' },
    { name: 'Docker', desc: '容器化部署与镜像管理' },
    { name: 'Git', desc: '版本控制与分支策略' },
    { name: 'Figma', desc: '设计稿转代码与UI还原' },
  ],
  frontend: [
    { name: 'React', desc: 'React 组件开发与状态管理' },
    { name: 'Vue', desc: 'Vue 组合式API与响应式系统' },
    { name: 'Angular', desc: '企业级前端框架开发' },
    { name: 'Tailwind CSS', desc: '原子化CSS样式设计' },
    { name: 'Sass', desc: 'CSS预处理器与变量管理' },
    { name: 'shadcn/ui', desc: '基于Radix的组件库' },
    { name: 'Vite', desc: '前端构建工具与热更新' },
    { name: 'Webpack', desc: '模块打包与资源优化' },
  ],
  backend: [
    { name: 'Node.js', desc: '服务端JavaScript运行环境' },
    { name: 'Python', desc: 'Python脚本与数据处理' },
    { name: 'Go', desc: '高并发服务端开发' },
    { name: 'Rust', desc: '系统级编程与内存安全' },
    { name: 'Java', desc: 'Spring生态与企业级开发' },
    { name: 'PostgreSQL', desc: '关系型数据库设计与优化' },
    { name: 'Redis', desc: '缓存与高性能数据存储' },
    { name: 'GraphQL', desc: '查询语言与API设计' },
  ],
  fullstack: [
    { name: 'Next.js', desc: '全栈React框架与SSR开发' },
    { name: 'Nuxt', desc: '全栈Vue框架与SSR开发' },
    { name: 'Django', desc: 'Python全栈Web框架' },
    { name: 'Spring Boot', desc: 'Java全栈开发框架' },
    { name: 'NestJS', desc: 'Node.js企业级框架' },
    { name: 'Express', desc: '轻量级Node.js服务端' },
  ],
  design: [
    { name: 'UI/UX', desc: '用户界面与体验设计' },
    { name: 'Figma', desc: '设计稿转代码与UI还原' },
    { name: 'Framer Motion', desc: 'React动画库与交互设计' },
    { name: 'CSS Animation', desc: 'CSS3动画与过渡效果' },
    { name: 'Responsive Design', desc: '响应式布局与多端适配' },
  ],
  other: [
    { name: 'Docker', desc: '容器化部署与镜像管理' },
    { name: 'Git', desc: '版本控制与分支策略' },
    { name: 'CI/CD', desc: '持续集成与自动化部署' },
    { name: 'Nginx', desc: '反向代理与负载均衡' },
    { name: 'Linux', desc: 'Linux系统操作与Shell' },
    { name: 'Shell Script', desc: '自动化脚本编写' },
    { name: 'Testing', desc: '单元测试与集成测试' },
  ],
};

function SkillSelector({ value, onChange, onInsertSkill }: { value: string; onChange: (v: string) => void; onInsertSkill: (skill: string) => void }) {
  const [open, setOpen] = useState(false);
  const [activeCategory, setActiveCategory] = useState('all');

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <button type="button" className="flex items-center gap-1 px-2 py-0.5 text-[11px] rounded bg-secondary border border-border text-foreground hover:bg-secondary/80 transition-colors">
          <Zap className="w-2.5 h-2.5 text-muted-foreground" />
          技能
          <ChevronUp className={`w-2.5 h-2.5 text-muted-foreground transition-transform ${open ? 'rotate-180' : ''}`} />
        </button>
      </PopoverTrigger>
      <PopoverContent side="top" align="start" className="w-72 p-0 overflow-hidden">
        <div className="flex border-b border-border overflow-x-auto">
          {skillCategories.map((cat) => (
            <button
              key={cat.key}
              type="button"
              onClick={() => setActiveCategory(cat.key)}
              className={`px-3 py-2 text-[11px] whitespace-nowrap transition-colors flex-shrink-0 ${
                activeCategory === cat.key
                  ? 'text-primary border-b-2 border-primary'
                  : 'text-muted-foreground hover:text-foreground'
              }`}
            >
              {cat.label}
            </button>
          ))}
        </div>
        <div className="max-h-56 overflow-y-auto overflow-x-auto">
          <div className="min-w-full">
            {skillData[activeCategory]?.map((skill) => (
              <button
                key={skill.name}
                type="button"
                onClick={() => {
                  onChange(skill.name);
                  onInsertSkill(skill.name);
                  setOpen(false);
                }}
                className={`w-full text-left px-3 py-2 text-xs transition-colors border-b border-border last:border-b-0 whitespace-nowrap hover:bg-secondary/50 ${
                  value === skill.name ? 'bg-primary/5 text-primary' : 'text-foreground'
                }`}
              >
                <span className="font-medium">{skill.name}</span>
                <span className="text-muted-foreground ml-2 text-[11px]">{skill.desc}</span>
              </button>
            ))}
          </div>
        </div>
      </PopoverContent>
    </Popover>
  );
}

// ========================== 主组件 ==========================

export default function ChatPanel({
  messages,
  isTyping,
  activeConversation,
  conversations,
  activeAgentName,
  currentModel,
  contextPercent,
  agentMode,
  currentSkill,
  editContent,
  onSendMessage,
  onAgentModeChange,
  onModelChange,
  onSkillChange,
  onSelectConversation,
  onAnswerPermission,
  onAnswerUserQuestions,
  onEditUserMessage,
  onRetryStep,
}: ChatPanelProps) {
  const [input, setInput] = useState('');
  const [convSearch, setConvSearch] = useState('');
  const messagesEndRef = useRef<HTMLDivElement>(null);
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  // 监听编辑内容变化，填入输入框
  useEffect(() => {
    if (editContent !== undefined) {
      setInput(editContent);
      if (textareaRef.current) {
        textareaRef.current.style.height = 'auto';
        textareaRef.current.style.height = Math.min(textareaRef.current.scrollHeight, 200) + 'px';
        textareaRef.current.focus();
      }
    }
  }, [editContent]);

  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [messages, isTyping]);

  const handleSend = () => {
    if (!input.trim()) return;
    onSendMessage(input.trim());
    setInput('');
    if (textareaRef.current) textareaRef.current.style.height = 'auto';
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      handleSend();
    }
  };

  const handleTextareaChange = (e: React.ChangeEvent<HTMLTextAreaElement>) => {
    setInput(e.target.value);
    const target = e.target;
    target.style.height = 'auto';
    target.style.height = Math.min(target.scrollHeight, 200) + 'px';
  };

  const handleInsertSkill = (skill: string) => {
    setInput((prev) => {
      const trimmed = prev.trim();
      return trimmed ? `${trimmed} @${skill} ` : `@${skill} `;
    });
    setTimeout(() => textareaRef.current?.focus(), 0);
  };

  const modelLabel = currentModel === 'gpt-4' ? 'GPT-4' :
    currentModel === 'gpt-4-turbo' ? 'GPT-4 Turbo' :
    currentModel === 'claude-3-opus' ? 'Claude 3 Opus' :
    currentModel === 'claude-3-sonnet' ? 'Claude 3 Sonnet' :
    currentModel === 'deepseek-v3' ? 'DeepSeek V3' : currentModel;

  const truncateTitle = (title: string, maxLen = 3) =>
    title.length > maxLen ? title.slice(0, maxLen) + '...' : title;

  const sessionLabel = activeConversation
    ? `当前会话：${truncateTitle(activeConversation.title)}`
    : '当前会话：新建会话';

  return (
    <div className="flex-1 flex flex-col min-w-0 bg-background">
      {/* 会话标题下拉 */}
      <div className="h-10 border-b border-border flex items-center px-4 shrink-0">
        <Code2 className="w-4 h-4 text-primary mr-2" />
        <Popover>
          <PopoverTrigger asChild>
            <button
              type="button"
              className="flex items-center gap-1.5 text-xs px-2.5 py-1 rounded-md border border-border bg-secondary/40 text-muted-foreground hover:text-foreground hover:bg-secondary/60 transition-colors"
            >
              <span>{sessionLabel}</span>
              <ChevronDown className="w-3 h-3" />
            </button>
          </PopoverTrigger>
          <PopoverContent className="w-56 p-0" align="start">
            <div className="p-2 border-b border-border">
              <div className="flex items-center gap-1.5 px-2 py-1 rounded-md border border-border bg-secondary/40">
                <Search className="w-3 h-3 text-muted-foreground shrink-0" />
                <input
                  type="text"
                  value={convSearch}
                  onChange={(e) => setConvSearch(e.target.value)}
                  placeholder="搜索会话..."
                  className="flex-1 min-w-0 bg-transparent text-xs text-foreground placeholder:text-muted-foreground focus:outline-none"
                />
              </div>
            </div>
            <div className="py-1 max-h-52 overflow-y-auto">
              {(!conversations || conversations.length === 0) && (
                <div className="px-3 py-2 text-xs text-muted-foreground">暂无会话</div>
              )}
              {(conversations || []).filter((c) => !convSearch || c.title.toLowerCase().includes(convSearch.toLowerCase())).map((conv) => {
                const isActive = activeConversation?.id === conv.id;
                return (
                  <button
                    key={conv.id}
                    type="button"
                    onClick={() => { onSelectConversation(conv); setConvSearch(''); }}
                    className={`w-full flex items-center gap-2 px-3 py-2 text-left transition-colors ${
                      isActive ? 'bg-primary/5 text-foreground font-medium' : 'text-foreground hover:bg-secondary/40'
                    }`}
                  >
                    <MessageSquare className="w-3.5 h-3.5 shrink-0 text-muted-foreground" />
                    <span className="text-xs truncate">{conv.title}</span>
                  </button>
                );
              })}
            </div>
          </PopoverContent>
        </Popover>
      </div>

      {/* 消息列表 */}
      <div className="flex-1 overflow-y-auto">
        {messages.length === 0 && !activeConversation ? (
          <div className="flex flex-col items-center justify-center h-full text-muted-foreground">
            <Bot className="w-12 h-12 mb-4 opacity-30" />
            <p className="text-sm">欢迎使用 AI Coding</p>
            <p className="text-xs mt-1">点击左侧会话图标开始编码</p>
          </div>
        ) : messages.length === 0 ? (
          <div className="flex flex-col items-center justify-center h-full text-muted-foreground">
            <Bot className="w-12 h-12 mb-4 opacity-30" />
            <p className="text-sm">开始新的对话</p>
            <p className="text-xs mt-1">输入你的编码需求</p>
          </div>
        ) : (
          messages.map((msg) => (
            <MessageBubble
              key={msg.id}
              message={msg}
              onAnswerPermission={onAnswerPermission}
              onAnswerUser={onAnswerUserQuestions}
              onEditUserMessage={onEditUserMessage}
              onRetryStep={onRetryStep ? (stepIndex) => onRetryStep(msg.id, stepIndex) : undefined}
            />
          ))
        )}

        {isTyping && (
          <div className="flex gap-3 px-4 py-3">
            <div className="w-7 h-7 rounded shrink-0 flex items-center justify-center bg-accent relative">
              <Bot className="w-3.5 h-3.5 text-primary" />
              <div className="absolute -bottom-0.5 -right-0.5 w-2.5 h-2.5 rounded-full bg-primary border-2 border-background animate-pulse" />
            </div>
            <div className="flex-1 min-w-0">
              <div className="text-[10px] text-muted-foreground mb-1">AI助手</div>
              <div className="flex items-center gap-1">
                <div className="w-1.5 h-1.5 rounded-full bg-primary animate-pulse" />
                <div className="w-1.5 h-1.5 rounded-full bg-primary animate-pulse [animation-delay:0.2s]" />
                <div className="w-1.5 h-1.5 rounded-full bg-primary animate-pulse [animation-delay:0.4s]" />
              </div>
            </div>
          </div>
        )}
        <div ref={messagesEndRef} />
      </div>

      {/* 底部输入区域 */}
      <div className="border-t border-border p-3 shrink-0 bg-card">
        {/* Agent执行中来回走的进度条 */}
        {isTyping && (
          <div className="w-full h-0.5 bg-secondary/50 rounded-full overflow-hidden mb-2 relative">
            <div className="absolute top-0 bottom-0 w-1/3 bg-primary rounded-full animate-[loading-bar_1.2s_ease-in-out_infinite]" />
          </div>
        )}
        {/* 状态栏：模型 + 上下文 + Token统计 */}
        <div className="flex items-center justify-between mb-2 px-0.5">
          <div className="flex items-center gap-3">
            <span className="text-[10px] text-muted-foreground">
              模型: <span className="text-foreground font-medium">{modelLabel}</span>
            </span>
            <div className="flex items-center gap-1.5">
              <span className="text-[10px] text-muted-foreground">上下文</span>
              <div className="w-16 h-1.5 bg-secondary rounded-full overflow-hidden">
                <div className="h-full bg-primary rounded-full transition-all" style={{ width: `${contextPercent}%` }} />
              </div>
              <span className="text-[10px] text-muted-foreground w-6 text-right">{contextPercent}%</span>
            </div>
          </div>
          <div className="flex items-center gap-3">
            {(() => {
              const totalIn = messages.filter((m) => m.role === 'assistant').reduce((sum, m) => sum + (m.token_in || 0), 0);
              const totalOut = messages.filter((m) => m.role === 'assistant').reduce((sum, m) => sum + (m.token_out || 0), 0);
              return (
                <>
                  <span className="text-[10px] text-muted-foreground">
                    输入 <span className="text-foreground font-medium">{totalIn.toLocaleString()}</span> tokens
                  </span>
                  <span className="text-[10px] text-muted-foreground">
                    输出 <span className="text-foreground font-medium">{totalOut.toLocaleString()}</span> tokens
                  </span>
                </>
              );
            })()}
          </div>
        </div>

        {/* 输入框容器 */}
        <div className="rounded-lg border border-border bg-secondary/30 p-2">
          <textarea
            ref={textareaRef}
            value={input}
            onChange={handleTextareaChange}
            onKeyDown={handleKeyDown}
            placeholder="输入编码需求... (Enter发送, Shift+Enter换行)"
            className="w-full min-h-[40px] max-h-[200px] resize-none bg-transparent border-0 text-sm px-1 py-1 text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-0"
            rows={1}
          />

          {/* 底部工具栏 */}
          <div className="flex items-center justify-between mt-1">
            {/* 左侧工具按钮 */}
            <div className="flex items-center gap-1">
              <button type="button" className="flex items-center gap-1 px-2 py-0.5 text-[11px] rounded bg-secondary border border-border text-foreground hover:bg-secondary/80 transition-colors">
                <FileCode className="w-2.5 h-2.5 text-muted-foreground" />
                代码
              </button>
              <ModeSelector value={agentMode} onChange={onAgentModeChange} />
              <ModelSelector value={currentModel} onChange={onModelChange} />
              <SkillSelector value={currentSkill} onChange={onSkillChange} onInsertSkill={handleInsertSkill} />
            </div>

            {/* 右侧执行按钮 */}
            <button
              type="button"
              onClick={handleSend}
              disabled={!input.trim() || isTyping}
              className="flex items-center gap-1.5 px-3 py-1.5 text-[11px] rounded-full bg-primary text-primary-foreground hover:bg-primary/90 transition-colors disabled:opacity-50 disabled:cursor-not-allowed font-medium"
            >
              <Send className="w-3.5 h-3.5" />
              执行
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
