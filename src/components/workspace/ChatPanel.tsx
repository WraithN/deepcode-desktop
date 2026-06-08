import { useState, useRef, useEffect } from 'react';
import type { Message, Conversation } from '@/types/types';
import { useChatStore } from '@/stores';
import {
  Send, Bot, Code2, ChevronDown, Search, MessageSquare,
} from 'lucide-react';
import {
  Popover, PopoverContent, PopoverTrigger,
} from '@/components/ui/popover';
import MessageBubble from './chat/MessageBubble';
import { ModeSelector, ModelSelector, SkillSelector } from './chat/ChatSelectors';

interface ChatPanelProps {
  messages?: Message[];
  isTyping?: boolean;
  activeConversation: Conversation | null;
  conversations: Conversation[];
  activeAgentName: string;
  activeAgentType: string;
  currentModel: string;
  contextPercent: number;
  agentMode: 'plan' | 'build';
  currentSkill: string;
  editContent?: string;
  onSendMessage?: (content: string) => void;
  onAgentModeChange: (mode: 'plan' | 'build') => void;
  onModelChange: (model: string) => void;
  onSkillChange: (skill: string) => void;
  onSelectConversation: (conv: Conversation) => void;
  onAnswerPermission: (stepIndex: number, answer: 'once' | 'session' | 'deny') => void;
  onAnswerUserQuestions: (stepIndex: number, answers: Record<string, string | string[]>) => void;
  onEditUserMessage: (content: string) => void;
  onRetryStep?: (messageId: string, stepIndex: number) => void;
}

const agentTypeLabels: Record<string, string> = {
  opencode: 'opencode',
  'claude-code': 'claudecode',
  'cursor-agent': 'cursor agent',
  codex: 'codex',
  custom: 'custom',
};

export default function ChatPanel({
  messages: messagesProp,
  isTyping: isTypingProp,
  activeConversation,
  conversations,
  activeAgentName: _activeAgentName,
  activeAgentType,
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
  const storeMessages = useChatStore((s) => s.messages);
  const storeIsStreaming = useChatStore((s) => s.isStreaming);
  const storeSendMessage = useChatStore((s) => s.sendMessage);

  const messages = messagesProp ?? storeMessages;
  const isTyping = isTypingProp ?? storeIsStreaming;

  const [input, setInput] = useState('');
  const [convSearch, setConvSearch] = useState('');
  const messagesEndRef = useRef<HTMLDivElement>(null);
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  useEffect(() => {
    if (editContent !== undefined) {
      setInput(editContent);
      if (textareaRef.current) {
        textareaRef.current.style.height = 'auto';
        textareaRef.current.style.height = `${Math.min(textareaRef.current.scrollHeight, 200)}px`;
        textareaRef.current.focus();
      }
    }
  }, [editContent]);

  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [messages, isTyping]);

  const handleSend = async () => {
    if (!input.trim()) { return; }
    try {
      if (onSendMessage) {
        await onSendMessage(input.trim());
      } else {
        await storeSendMessage(input.trim());
      }
    } catch (e) {
      console.error('[ChatPanel] sendMessage failed:', e);
    }
    setInput('');
    if (textareaRef.current) { textareaRef.current.style.height = 'auto'; }
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
    target.style.height = `${Math.min(target.scrollHeight, 200)}px`;
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
    title.length > maxLen ? `${title.slice(0, maxLen)}...` : title;

  const sessionLabel = activeConversation
    ? `当前会话：${truncateTitle(activeConversation.title)}`
    : '当前会话：新建会话';

  const totalInputTokens = messages.filter((m) => m.role === 'assistant').reduce((sum, m) => sum + (m.token_in || 0), 0);
  const totalOutputTokens = messages.filter((m) => m.role === 'assistant').reduce((sum, m) => sum + (m.token_out || 0), 0);

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
      <div className="flex-1 overflow-y-auto" data-workspace-context-menu="true">
        {messages.length === 0 && !activeConversation ? (
          <div className="flex flex-col items-center justify-center h-full text-muted-foreground">
            <Bot className="w-12 h-12 mb-4 opacity-30" />
            <p className="text-sm">欢迎使用 DeepHarness</p>
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

        <div ref={messagesEndRef} />
      </div>

      {/* 底部输入区域 */}
      <div className="border-t border-border p-3 shrink-0 bg-card">
        {/* 状态栏：智能体 + 上下文 + Token */}
        <div className="flex items-center justify-between gap-4 mb-2 px-0.5 text-xs text-muted-foreground">
          <div className="flex items-center gap-3 min-w-0">
            <span>
              当前正在使用: <span className="text-foreground font-medium">{agentTypeLabels[activeAgentType] || activeAgentType}</span>
            </span>
            <span>
              模型: <span className="text-foreground font-medium">{modelLabel}</span>
            </span>
          </div>
          <div className="flex items-center gap-3 shrink-0">
            <div className="flex items-center gap-1.5">
              <span>上下文</span>
              <div className="h-1.5 w-24 rounded-full bg-secondary overflow-hidden border border-border/60">
                <div className="h-full rounded-full bg-primary transition-all" style={{ width: `${contextPercent}%` }} />
              </div>
              <span className="text-foreground font-medium tabular-nums">{contextPercent}%</span>
            </div>
            <span>
              输入 <span className="text-foreground font-medium tabular-nums">{totalInputTokens.toLocaleString()}</span>
            </span>
            <span>
              输出 <span className="text-foreground font-medium tabular-nums">{totalOutputTokens.toLocaleString()}</span>
            </span>
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
              <ModeSelector value={agentMode} onChange={onAgentModeChange} />
              <ModelSelector value={currentModel} onChange={onModelChange} />
              <SkillSelector value={currentSkill} onChange={onSkillChange} onInsertSkill={handleInsertSkill} />
            </div>

            {/* 右侧执行按钮 */}
            <button
              type="button"
              onClick={handleSend}
              disabled={!input.trim() || isTyping}
              className="flex items-center gap-1.5 px-3 py-1.5 text-[12px] rounded-full bg-primary text-primary-foreground hover:bg-primary/90 transition-colors disabled:opacity-50 disabled:cursor-not-allowed font-medium"
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
