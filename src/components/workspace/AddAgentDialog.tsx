import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import {
  Dialog, DialogContent, DialogHeader, DialogTitle,
} from '@/components/ui/dialog';
import { X, Sparkles } from 'lucide-react';
import DirectoryPickerButton from '@/components/DirectoryPickerButton';
import AgentIcon from './AgentIcon';

interface AddAgentDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onAddAgent: (agentKey: string, displayName: string, workspace: string) => void;
  existingNames: string[];
}

const agents = [
  {
    key: 'opencode',
    name: 'OpenCode',
    desc: '全能型编程助手，擅长代码生成、重构与调试',
    color: 'text-green-400',
    bg: 'bg-green-400/15',
    border: 'border-green-400/20',
  },
  {
    key: 'claude-code',
    name: 'Claude Code',
    desc: '代码理解与重构专家，深度分析复杂代码结构',
    color: 'text-orange-400',
    bg: 'bg-orange-400/15',
    border: 'border-orange-400/20',
  },
  {
    key: 'cursor-agent',
    name: 'Cursor Agent',
    desc: '智能补全与生成，实时预测你的编码意图',
    color: 'text-blue-400',
    bg: 'bg-blue-400/15',
    border: 'border-blue-400/20',
  },
  {
    key: 'codex',
    name: 'Codex',
    desc: 'OpenAI Codex，专为软件工程优化的AI模型',
    color: 'text-purple-400',
    bg: 'bg-purple-400/15',
    border: 'border-purple-400/20',
  },
  {
    key: 'custom',
    name: '自定义智能体',
    desc: '创建属于你的AI编码助手，自由配置模型和能力',
    color: 'text-primary',
    bg: 'bg-primary/15',
    border: 'border-primary/20',
  },
];

const defaultNames = ['小智', '阿明', '小红', '小宇', '阿强', '小琳', '小凯', '小慧', '阿杰', '小燕', '小峰', '阿丽', '小龙', '小雪', '阿伟', '小芳'];

function generateName(existing: string[]): string {
  const available = defaultNames.filter((n) => !existing.includes(n));
  if (available.length > 0) {
    return available[Math.floor(Math.random() * available.length)];
  }
  // 随机生成3字以内的名字
  const chars = '小阿明红宇强琳凯慧杰燕峰龙雪伟芳文武涛静超磊洋敏';
  const len = 2 + Math.floor(Math.random() * 2); // 2或3字
  let name = '';
  for (let i = 0; i < len; i++) {
    name += chars[Math.floor(Math.random() * chars.length)];
  }
  return name;
}

export default function AddAgentDialog({ open, onOpenChange, onAddAgent, existingNames }: AddAgentDialogProps) {
  const [selectedAgent, setSelectedAgent] = useState<string | null>(null);
  const [displayName, setDisplayName] = useState('');
  const [workspace, setWorkspace] = useState('');

  // 每次打开时重置
  useEffect(() => {
    if (open) {
      setSelectedAgent(null);
      setDisplayName(generateName(existingNames));
      invoke<string>('get_current_dir')
        .then((dir) => setWorkspace(dir))
        .catch(() => setWorkspace(''));
    }
  }, [open, existingNames]);

  const handleConfirm = () => {
    if (!selectedAgent) return;
    const trimmed = displayName.trim().slice(0, 3);
    const ws = workspace.trim() || '.';
    onAddAgent(selectedAgent, trimmed || generateName(existingNames), ws);
    onOpenChange(false);
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-[calc(100%-2rem)] md:max-w-lg p-0 bg-transparent border-0 shadow-none [&>button]:hidden">
        <div className="bg-card/95 backdrop-blur-sm border border-border rounded-xl p-5 shadow-2xl">
          <DialogHeader className="mb-4 pb-3 border-b border-border">
            <div className="flex items-center justify-between">
              <DialogTitle className="text-sm font-semibold text-foreground">添加智能体</DialogTitle>
              <button
                type="button"
                onClick={() => onOpenChange(false)}
                className="w-6 h-6 flex items-center justify-center rounded text-muted-foreground hover:text-foreground hover:bg-secondary transition-colors"
              >
                <X className="w-4 h-4" />
              </button>
            </div>
          </DialogHeader>

          {/* 人名输入 */}
          <div className="mb-4">
            <label className="text-xs text-muted-foreground mb-1.5 block">智能体名称（最多3个字）</label>
            <div className="flex items-center gap-2">
              <input
                type="text"
                value={displayName}
                onChange={(e) => setDisplayName(e.target.value.slice(0, 3))}
                placeholder="输入名称..."
                className="flex-1 px-3 py-1.5 text-sm bg-secondary border border-border rounded text-foreground placeholder:text-muted-foreground focus:outline-none focus:border-primary"
              />
              <button
                type="button"
                onClick={() => setDisplayName(generateName([...existingNames, displayName]))}
                className="flex items-center gap-1 px-2.5 py-1.5 text-[11px] rounded bg-secondary border border-border text-foreground hover:bg-secondary/80 transition-colors"
              >
                <Sparkles className="w-3 h-3" />
                随机
              </button>
            </div>
          </div>

          {/* 工作区输入 */}
          <div className="mb-4">
            <label className="text-xs text-muted-foreground mb-1.5 block">工作区目录</label>
            <div className="flex items-center gap-2">
              <input
                type="text"
                value={workspace}
                onChange={(e) => setWorkspace(e.target.value)}
                placeholder="输入工作目录路径..."
                className="flex-1 px-3 py-1.5 text-sm bg-secondary border border-border rounded text-foreground placeholder:text-muted-foreground focus:outline-none focus:border-primary"
              />
              <DirectoryPickerButton onSelect={setWorkspace} />
            </div>
            <span className="text-[10px] text-muted-foreground mt-1 block">智能体将以此目录作为工作根目录</span>
          </div>

          {/* 选择智能体类型 */}
          <div className="space-y-2">
            <label className="text-xs text-muted-foreground block">选择智能体类型</label>
            <div className="space-y-2">
              {agents.map((agent) => (
                <button
                  key={agent.key}
                  type="button"
                  onClick={() => setSelectedAgent(agent.key)}
                  className={`w-full flex items-center gap-3 p-3 rounded-lg border transition-all text-left ${
                    selectedAgent === agent.key
                      ? `${agent.border} ${agent.bg} ring-1 ring-primary/30`
                      : 'border-border bg-secondary/20 hover:bg-secondary/40'
                  }`}
                >
                  <span className={`w-10 h-10 rounded-lg flex items-center justify-center ${agent.bg} ${agent.color}`}>
                    <AgentIcon agentKey={agent.key} size={22} />
                  </span>
                  <div className="flex-1 min-w-0">
                    <div className="text-sm font-medium text-foreground">{agent.name}</div>
                    <div className="text-xs text-muted-foreground mt-0.5">{agent.desc}</div>
                  </div>
                </button>
              ))}
            </div>
          </div>

          <div className="mt-4 pt-3 border-t border-border flex justify-end gap-2">
            <button
              type="button"
              onClick={() => onOpenChange(false)}
              className="px-4 py-1.5 text-xs rounded-md bg-secondary text-foreground hover:bg-secondary/80 transition-colors border border-border"
            >
              取消
            </button>
            <button
              type="button"
              onClick={handleConfirm}
              disabled={!selectedAgent}
              className="px-4 py-1.5 text-xs rounded-md bg-primary text-primary-foreground hover:bg-primary/90 transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
            >
              确认添加
            </button>
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}
