import { Plus, Check, Trash2 } from 'lucide-react';
import type { AgentInstance } from '@/stores';
import { formatIdShort } from '@/lib/id';
import AgentIcon from '../AgentIcon';
import { agentConfig } from './constants';

export interface AgentPanelProps {
  agentInstances: AgentInstance[];
  activeAgentId: string | null;
  onActivateAgent: (id: string) => void;
  onActivateAgentAndSwitch: (id: string) => void;
  onAddAgent: () => void;
  onDeleteAgent?: (id: string) => void;
}

export default function AgentPanel({
  agentInstances,
  activeAgentId,
  onActivateAgent,
  onActivateAgentAndSwitch,
  onAddAgent,
  onDeleteAgent,
}: AgentPanelProps) {
  return (
    <div className="flex flex-col h-full">
      <div className="flex items-center justify-between px-3 py-2 border-b border-border shrink-0">
        <span className="text-xs font-medium text-foreground">智能体</span>
        <button
          type="button"
          onClick={onAddAgent}
          className="flex items-center gap-1 text-[12px] text-primary hover:text-primary/80 transition-colors"
        >
          <Plus className="w-3 h-3" />
          添加
        </button>
      </div>
      <div className="flex-1 overflow-y-auto p-2 space-y-2">
        {agentInstances.map((instance) => {
          const config = agentConfig[instance.agentKey] || agentConfig.opencode;
          const isActive = activeAgentId === instance.id;
          return (
            <div
              key={instance.id}
              className={`w-full flex items-center gap-2 p-2 rounded-lg border transition-all ${
                isActive
                  ? `${config.border} ${config.bg} ring-1 ring-primary/40`
                  : 'border-border bg-secondary/20 hover:bg-secondary/40'
              }`}
            >
              <button
                type="button"
                onClick={() => onActivateAgent(instance.id)}
                onDoubleClick={() => onActivateAgentAndSwitch(instance.id)}
                className="flex-1 flex items-center gap-2 text-left min-w-0"
              >
                <span className={`w-7 h-7 rounded-md text-sm font-medium flex items-center justify-center shrink-0 ${config.bg} ${config.color} overflow-hidden`}>
                  <AgentIcon agentKey={instance.agentKey} size={18} />
                </span>
                <div className="flex-1 min-w-0">
                  <div className="text-xs font-medium text-foreground">{instance.displayName}</div>
                  <div className="text-xs text-muted-foreground">
                    {config.name} · {formatIdShort(instance.id)}
                  </div>
                </div>
                {isActive && <Check className="w-3.5 h-3.5 text-primary shrink-0" />}
              </button>
              {onDeleteAgent && (
                <button
                  type="button"
                  onClick={(e) => {
                    e.stopPropagation();
                    onDeleteAgent(instance.id);
                  }}
                  className="p-1 rounded hover:bg-destructive/20 text-muted-foreground hover:text-destructive transition-colors shrink-0"
                  title="删除"
                >
                  <Trash2 className="w-3 h-3" />
                </button>
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
}
