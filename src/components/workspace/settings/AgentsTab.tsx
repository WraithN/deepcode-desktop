import { useState } from 'react';
import { Label } from '@/components/ui/label';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Input } from '@/components/ui/input';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Switch } from '@/components/ui/switch';
import { Bot, Save, Eye, EyeOff, Brain } from 'lucide-react';
import type { AgentInstance } from '@/stores';
import { AGENT_TYPES, AgentTypeConfig, builtinModels } from './settings-utils';

function AgentTypeConfigCard({
  agentType,
  config,
  onChange,
}: {
  agentType: { key: string; name: string; desc: string };
  config: AgentTypeConfig;
  onChange: (c: AgentTypeConfig) => void;
}) {
  const [showKey, setShowKey] = useState(false);

  return (
    <div className="rounded-lg border border-border bg-secondary/20 p-3 space-y-3">
      <div className="flex items-center gap-2">
        <span className="text-xs font-medium text-foreground">{agentType.name}</span>
        <Badge variant="secondary" className="text-xs px-1.5 h-4">{agentType.key}</Badge>
      </div>
      <p className="text-[12px] text-muted-foreground">{agentType.desc}</p>

      <div className="flex items-center gap-2">
        {[
          { key: 'builtin', label: '内置模型' },
          { key: 'custom', label: '自定义' },
        ].map((opt) => (
          <button
            key={opt.key}
            type="button"
            onClick={() => onChange({ ...config, type: opt.key as 'builtin' | 'custom' })}
            className={`px-2.5 py-1 text-[12px] rounded border transition-colors ${
              config.type === opt.key
                ? 'border-primary bg-primary/10 text-primary'
                : 'border-border bg-secondary text-foreground hover:bg-secondary/80'
            }`}
          >
            {opt.label}
          </button>
        ))}
      </div>

      {config.type === 'builtin' ? (
        <div className="space-y-1">
          <Label className="text-xs text-muted-foreground">选择模型</Label>
          <Select
            value={config.modelId || 'gpt-4'}
            onValueChange={(v) => onChange({ ...config, modelId: v })}
          >
            <SelectTrigger className="bg-secondary border-border text-xs h-8">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {builtinModels.map((m) => (
                <SelectItem key={m.id} value={m.id} className="text-xs">{m.name}</SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>
      ) : (
        <div className="space-y-2">
          <div className="space-y-1">
            <Label className="text-xs text-muted-foreground">模型名称</Label>
            <Input value={config.name || ''} onChange={(e) => onChange({ ...config, name: e.target.value })} placeholder="例如：自定义 GPT" className="bg-secondary border-border text-xs h-8" />
          </div>
          <div className="space-y-1">
            <Label className="text-xs text-muted-foreground">API URL</Label>
            <Input value={config.url || ''} onChange={(e) => onChange({ ...config, url: e.target.value })} placeholder="https://api.example.com/v1/chat/completions" className="bg-secondary border-border text-xs h-8" />
          </div>
          <div className="space-y-1">
            <Label className="text-xs text-muted-foreground">API KEY</Label>
            <div className="flex items-center gap-1.5">
              <Input type={showKey ? 'text' : 'password'} value={config.apiKey || ''} onChange={(e) => onChange({ ...config, apiKey: e.target.value })} placeholder="sk-..." className="bg-secondary border-border text-xs h-8" />
              <button type="button" onClick={() => setShowKey(!showKey)} className="w-8 h-8 flex items-center justify-center rounded border border-border bg-secondary text-muted-foreground hover:text-foreground transition-colors shrink-0" title={showKey ? '隐藏' : '显示'}>
                {showKey ? <EyeOff className="w-3.5 h-3.5" /> : <Eye className="w-3.5 h-3.5" />}
              </button>
            </div>
          </div>
        </div>
      )}

      <div className="flex items-center justify-between pt-2 border-t border-border/40">
        <div className="flex items-center gap-2">
          <Brain className="w-3.5 h-3.5 text-muted-foreground" />
          <span className="text-[12px] text-muted-foreground">展示思考过程</span>
        </div>
        <Switch
          checked={config.showThinking !== false}
          onCheckedChange={(v) => onChange({ ...config, showThinking: v })}
          className="scale-75"
        />
      </div>
    </div>
  );
}

interface AgentsTabProps {
  agents: AgentInstance[];
  agentTypeConfigs: Record<string, AgentTypeConfig>;
  onAgentTypeConfigChange: (key: string, config: AgentTypeConfig) => void;
  onSave: () => void;
}

export default function AgentsTab({
  agents,
  agentTypeConfigs,
  onAgentTypeConfigChange,
  onSave,
}: AgentsTabProps) {
  return (
    <div className="space-y-4">
      <div className="flex items-center gap-2 mb-2">
        <Bot className="w-4 h-4 text-primary" />
        <span className="text-sm font-medium text-foreground">智能体模型设置</span>
      </div>
      <div className="space-y-3 max-h-[500px] overflow-y-auto">
        {AGENT_TYPES.map((at) => (
          <AgentTypeConfigCard
            key={at.key}
            agentType={at}
            config={agentTypeConfigs[at.key] || { type: 'builtin', modelId: 'gpt-4' }}
            onChange={(c) => onAgentTypeConfigChange(at.key, c)}
          />
        ))}
      </div>
      <Button onClick={onSave} className="w-full bg-primary text-primary-foreground hover:bg-primary/90">
        <Save className="w-3.5 h-3.5 mr-1.5" /> 保存智能体配置
      </Button>
    </div>
  );
}
