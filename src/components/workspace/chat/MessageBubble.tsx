import { useState, useEffect } from 'react';
import type { Message, } from '@/types/types';
import { Bot, User, Copy, Check, Pencil, Loader2, Clock } from 'lucide-react';
import { StepItem, } from './ChatSteps';
import { invoke } from '@tauri-apps/api/core';

// 读文件内容渲染（支持Markdown和代码高亮）
// ========================== 消息气泡 ==========================

function MessageBubble({ message, onAnswerPermission, onAnswerUser, onEditUserMessage, onRetryStep }: {
  message: Message;
  onAnswerPermission?: (stepIndex: number, answer: 'once' | 'session' | 'deny') => void;
  onAnswerUser?: (stepIndex: number, answers: Record<string, string | string[]>) => void;
  onEditUserMessage?: (content: string) => void;
  onRetryStep?: (stepIndex: number) => void;
}) {
  const [copied, setCopied] = useState(false);
  const isUser = message.role === 'user';
  const isComplete = message.is_complete ?? true;

  // Debug: log to Rust when assistant content changes
  useEffect(() => {
    if (!isUser) {
      void invoke('console_logs', {
        logs: [{
          type: 'info',
          message: `[MessageBubble] msg=${message.id.slice(-8)} content="${message.content.slice(-40)}" len=${message.content.length} isComplete=${isComplete} steps=${message.steps?.length || 0}`
        }]
      });
    }
  }, [message.content, isUser, message.id, message.conversation_id, isComplete, message.steps]);

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
              <span className="text-xs text-muted-foreground font-mono">{lang}</span>
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
            <span className="text-xs text-muted-foreground">你</span>
          </div>
          <div className="bg-primary/15 rounded-lg px-4 py-2.5 text-sm text-foreground leading-relaxed">{renderContent(message.content)}</div>
          {/* 用户消息底部操作按钮 */}
          <div className="flex items-center justify-end gap-1.5 mt-1.5 opacity-0 group-hover:opacity-100 transition-opacity">
            <button
              type="button"
              onClick={handleCopy}
              className="flex items-center gap-1 text-xs text-muted-foreground hover:text-foreground transition-colors px-1.5 py-0.5 rounded hover:bg-secondary/50"
            >
              {copied ? <Check className="w-3 h-3" /> : <Copy className="w-3 h-3" />}
              复制
            </button>
            <button
              type="button"
              onClick={() => onEditUserMessage?.(message.content)}
              className="flex items-center gap-1 text-xs text-muted-foreground hover:text-foreground transition-colors px-1.5 py-0.5 rounded hover:bg-secondary/50"
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
          <span className="text-xs text-muted-foreground">AI助手</span>
          {!isComplete && (
            <span className="flex items-center gap-1 text-xs text-primary">
              <Loader2 className="w-2.5 h-2.5 animate-spin" />
              进行中
            </span>
          )}
          {isComplete && (
            <span className="flex items-center gap-1 text-xs text-green-400">
              <Check className="w-2.5 h-2.5" />
              编程已完成
            </span>
          )}
        </div>
        {isComplete ? message.steps && message.steps.length > 0 ? (
          <>
            <div className="space-y-1.5 mb-2">{message.steps.map((step, i) => (
              <StepItem
                key={`${message.id}-step-${i}`}
                step={step}
                index={i}
                onAnswerPermission={onAnswerPermission ? (answer) => onAnswerPermission(i, answer) : undefined}
                onAnswerUser={onAnswerUser ? (answers) => onAnswerUser(i, answers) : undefined}
                onRetry={onRetryStep ? () => onRetryStep(i) : undefined}
              />
            ))}</div>
            <div className="text-sm text-foreground leading-relaxed">{renderContent(message.content)}</div>
          </>
        ) : (
          <div className="text-sm text-foreground leading-relaxed">{renderContent(message.content)}</div>
        ) : (
          <>
            {message.steps && message.steps.length > 0 && (
              <div className="space-y-1.5 mb-2">{message.steps.map((step, i) => (
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
            <div className="text-sm text-foreground leading-relaxed">
              {message.content ? renderContent(message.content) : (
                <div className="flex items-center gap-1 py-1">
                  <div className="w-1.5 h-1.5 rounded-full bg-primary animate-pulse" />
                  <div className="w-1.5 h-1.5 rounded-full bg-primary animate-pulse [animation-delay:0.2s]" />
                  <div className="w-1.5 h-1.5 rounded-full bg-primary animate-pulse [animation-delay:0.4s]" />
                </div>
              )}
            </div>
          </>
        )}
        {/* AI消息底部Token和耗时统计 */}
        {isComplete && (message.token_in || message.token_out || message.duration_ms) && (
          <div className="flex items-center gap-3 mt-2 pt-2 border-t border-border/50">
            {message.token_in !== undefined && (
              <span className="text-xs text-muted-foreground">
                输入 {message.token_in} tokens
              </span>
            )}
            {message.token_out !== undefined && (
              <span className="text-xs text-muted-foreground">
                输出 {message.token_out} tokens
              </span>
            )}
            {message.duration_ms !== undefined && (
              <span className="flex items-center gap-0.5 text-xs text-muted-foreground">
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


export default MessageBubble;
