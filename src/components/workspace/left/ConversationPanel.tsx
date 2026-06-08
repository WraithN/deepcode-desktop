import { Plus, Search, Info } from 'lucide-react';
import { useState } from 'react';
import type { Conversation, Message } from '@/types/types';
import AgentIcon from '../AgentIcon';
import { agentConfig } from './constants';

export interface ConversationPanelProps {
  conversations: Conversation[];
  activeConversation: Conversation | null;
  messages: Message[];
  convSearch: string;
  onConvSearchChange: (value: string) => void;
  onSelectConversation: (conv: Conversation) => void;
  onDoubleClickConversation: (conv: Conversation) => void;
  onNewConversation: () => void;
  onShowDetail: (conv: Conversation) => void;
}

export default function ConversationPanel({
  conversations,
  activeConversation,
  messages,
  convSearch,
  onConvSearchChange,
  onSelectConversation,
  onDoubleClickConversation,
  onNewConversation,
  onShowDetail,
}: ConversationPanelProps) {
  const [hoveredConv, setHoveredConv] = useState<string | null>(null);

  const truncateTitle = (title: string, maxLen = 5) =>
    title.length > maxLen ? `${title.slice(0, maxLen)}...` : title;

  const filteredConversations = convSearch
    ? conversations.filter((c) => c.title.toLowerCase().includes(convSearch.toLowerCase()))
    : conversations;

  return (
    <div className="flex flex-col h-full">
      <div className="flex items-center justify-between px-3 py-2 border-b border-border shrink-0">
        <span className="text-xs font-medium text-foreground">会话</span>
        <button
          type="button"
          onClick={onNewConversation}
          className="flex items-center gap-1 text-[12px] text-primary hover:text-primary/80 transition-colors"
        >
          <Plus className="w-3 h-3" />
          新建
        </button>
      </div>
      <div className="px-3 py-2 border-b border-border shrink-0">
        <div className="flex items-center gap-1.5 px-2 py-1 rounded-md border border-border bg-secondary/40">
          <Search className="w-3 h-3 text-muted-foreground shrink-0" />
          <input
            type="text"
            value={convSearch}
            onChange={(e) => onConvSearchChange(e.target.value)}
            placeholder="搜索会话..."
            className="flex-1 min-w-0 bg-transparent text-xs text-foreground placeholder:text-muted-foreground focus:outline-none"
          />
        </div>
      </div>
      <div className="flex-1 overflow-y-auto py-1">
        {filteredConversations.length === 0 ? (
          <div className="px-3 py-6 text-center text-xs text-muted-foreground">暂无会话</div>
        ) : (
          filteredConversations.map((conv) => {
            const cfg = agentConfig[conv.agent] || agentConfig.opencode;
            const isAct = activeConversation?.id === conv.id;
            const showDet = hoveredConv === conv.id;
            return (
              <div
                key={conv.id}
                role="button"
                tabIndex={0}
                onClick={() => onSelectConversation(conv)}
                onDoubleClick={() => onDoubleClickConversation(conv)}
                onMouseEnter={() => setHoveredConv(conv.id)}
                onMouseLeave={() => setHoveredConv(null)}
                onKeyDown={(e) => {
                  if (e.key === 'Enter' || e.key === ' ') {
                    onSelectConversation(conv);
                  }
                }}
                className={`w-full flex items-center gap-2 px-3 py-2 transition-colors text-left relative cursor-pointer ${isAct ? 'bg-primary/5' : 'hover:bg-secondary/40'}`}
              >
                <span className={`w-5 h-5 rounded text-xs font-medium flex items-center justify-center shrink-0 ${cfg.bg} ${cfg.color} overflow-hidden`}>
                  <AgentIcon agentKey={conv.agent} size={16} />
                </span>
                <div className="flex-1 min-w-0">
                  <div className={`text-[12px] truncate ${isAct ? 'text-foreground font-medium' : 'text-foreground'}`}>
                    {truncateTitle(conv.title)}
                  </div>
                  <div className="text-xs text-muted-foreground mt-0.5">{cfg.name}</div>
                </div>
                <div className="flex items-center gap-1 shrink-0">
                  {showDet && (
                    <div
                      role="button"
                      tabIndex={0}
                      onClick={(e) => {
                        e.stopPropagation();
                        onShowDetail(conv);
                      }}
                      onKeyDown={(e) => {
                        if (e.key === 'Enter' || e.key === ' ') {
                          e.stopPropagation();
                          onShowDetail(conv);
                        }
                      }}
                      className="w-5 h-5 flex items-center justify-center rounded hover:bg-secondary text-muted-foreground hover:text-foreground transition-colors cursor-pointer"
                      title="会话概要"
                    >
                      <Info className="w-3 h-3" />
                    </div>
                  )}
                  {isAct && <div className="w-1 h-4 rounded-full bg-primary" />}
                </div>
              </div>
            );
          })
        )}
      </div>
    </div>
  );
}
