import { useState } from 'react';
import { Bot, ChevronDown } from 'lucide-react';
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover';
import AgentIcon from './AgentIcon';
import type { AgentInstance } from '@/stores';

const AGENT_SWITCHER_CONFIG: Record<string, { color: string; bg: string; letter: string }> = {
  opencode: { color: 'text-green-400', bg: 'bg-green-400/15', letter: 'O' },
  'claude-code': { color: 'text-orange-400', bg: 'bg-orange-400/15', letter: 'C' },
  'cursor-agent': { color: 'text-blue-400', bg: 'bg-blue-400/15', letter: 'C' },
  codex: { color: 'text-purple-400', bg: 'bg-purple-400/15', letter: 'X' },
  custom: { color: 'text-primary', bg: 'bg-primary/15', letter: 'C' },
};

interface WorkspaceFooterProps {
  agentInstances: AgentInstance[];
  activeAgentId: string | null;
  activeAgent: AgentInstance;
  onSwitchAgent: (id: string) => void;
}

export default function WorkspaceFooter({ agentInstances, activeAgentId, activeAgent, onSwitchAgent }: WorkspaceFooterProps) {
  const [open, setOpen] = useState(false);

  return (
    <div className="border-t border-border bg-card pb-2.5 shrink-0">
      <div className="h-7 flex items-center justify-between px-4 pointer-events-auto [-webkit-app-region:no-drag]">
        <div className="flex items-center gap-3">
          <Popover open={open} onOpenChange={setOpen}>
            <PopoverTrigger asChild>
              <button
                type="button"
                data-no-drag
                className="flex items-center gap-1.5 text-xs text-muted-foreground hover:text-foreground transition-colors pointer-events-auto [-webkit-app-region:no-drag]"
              >
                <Bot className="w-3.5 h-3.5" />
                <span>{activeAgent.displayName}</span>
                <ChevronDown className="w-3.5 h-3.5" />
              </button>
            </PopoverTrigger>
            <PopoverContent side="top" align="start" className="w-48 p-1">
              {agentInstances.map((instance) => {
                const config = AGENT_SWITCHER_CONFIG[instance.agentKey] || AGENT_SWITCHER_CONFIG.opencode;
                const isActive = activeAgentId === instance.id;
                return (
                  <button
                    key={instance.id}
                    type="button"
                    onClick={() => {
                      onSwitchAgent(instance.id);
                      setOpen(false);
                    }}
                    className={`w-full flex items-center gap-2 px-2.5 py-1.5 text-xs rounded transition-colors ${
                      isActive ? 'bg-primary/10 text-primary' : 'text-foreground hover:bg-secondary'
                    }`}
                  >
                    <span className={`w-5 h-5 rounded text-xs font-medium flex items-center justify-center ${config.bg} ${config.color} overflow-hidden`}>
                      <AgentIcon agentKey={instance.agentKey} size={14} />
                    </span>
                    <span className="flex-1 text-left truncate">{instance.displayName}</span>
                    {isActive && <span className="text-xs text-primary">当前</span>}
                  </button>
                );
              })}
            </PopoverContent>
          </Popover>
          <span className="text-xs text-muted-foreground truncate max-w-[260px]" title={activeAgent.workspace}>
            工作目录：{activeAgent.workspace}
          </span>
        </div>
        <div className="flex items-center gap-3 text-xs text-muted-foreground">
          <span>DeepHarness v1.0</span>
        </div>
      </div>
    </div>
  );
}
