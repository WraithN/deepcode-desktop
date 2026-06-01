import { useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { useAuth } from '@/contexts/AuthContext';
import { Button } from '@/components/ui/button';
import { toast } from 'sonner';
import { ArrowRight, Sparkles, Bot } from 'lucide-react';
import DirectoryPickerButton from '@/components/DirectoryPickerButton';
import AgentIcon from '@/components/workspace/AgentIcon';

const agents = [
  {
    id: 'opencode',
    name: 'OpenCode',
    description: '开源编码智能体，支持多种编程语言和框架，提供智能代码补全和重构建议。',
    bgColor: 'bg-green-400/10',
  },
  {
    id: 'claude-code',
    name: 'Claude Code',
    description: 'Anthropic推出的编码助手，擅长复杂逻辑推理和大型项目架构设计。',
    bgColor: 'bg-orange-400/10',
  },
  {
    id: 'cursor-agent',
    name: 'Cursor Agent',
    description: '基于GPT-4的智能编码代理，专注于代码生成和自动化测试。',
    bgColor: 'bg-blue-400/10',
  },
  {
    id: 'codex',
    name: 'Codex',
    description: 'OpenAI Codex，专为软件工程优化的AI模型，支持多文件上下文和高级代码理解。',
    bgColor: 'bg-purple-400/10',
  },
  {
    id: 'custom',
    name: '自定义智能体',
    description: '创建属于你自己的AI编码助手，自由配置模型和能力参数。',
    bgColor: 'bg-primary/10',
  },
];

const defaultNames = ['小智', '阿明', '小红', '小宇', '阿强', '小琳', '小凯', '小慧', '阿杰', '小燕', '小峰', '阿丽', '小龙', '小雪', '阿伟', '小芳'];
const nameChars = '小阿明红宇强琳凯慧杰燕峰龙雪伟芳文武涛静超磊洋敏';

function generateName(existing: string[] = []): string {
  const available = defaultNames.filter((n) => !existing.includes(n));
  if (available.length > 0) {
    return available[Math.floor(Math.random() * available.length)];
  }
  const len = 2 + Math.floor(Math.random() * 2);
  let name = '';
  for (let i = 0; i < len; i++) {
    name += nameChars[Math.floor(Math.random() * nameChars.length)];
  }
  return name;
}

export default function SelectAgentPage() {
  const [selected, setSelected] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [displayName, setDisplayName] = useState(generateName());
  const [workspace, setWorkspace] = useState('.');
  const { user } = useAuth();
  const navigate = useNavigate();

  const handleConfirm = async () => {
    if (!selected) {
      toast.error('请选择一个编码智能体');
      return;
    }
    setLoading(true);
    try {
      localStorage.setItem('selected_agent', selected);
      localStorage.setItem('should_create_new_session', 'true');

      // 保存默认智能体名称和工作目录
      const trimmed = displayName.trim().slice(0, 3);
      localStorage.setItem('default_agent_name', trimmed);
      localStorage.setItem('default_agent_workspace', workspace);

      toast.success(`已选择 ${agents.find((a) => a.id === selected)?.name}`);
      navigate('/workspace');
    } finally {
      setLoading(false);
    }
  };

  const handleRandomName = () => {
    setDisplayName(generateName([displayName]));
  };

  return (
    <div className="min-h-screen flex flex-col items-center justify-center bg-background p-4">
      <div className="w-full max-w-2xl">
        {/* 标题 */}
        <div className="text-center mb-8">
          <div className="w-12 h-12 rounded-lg bg-primary/10 flex items-center justify-center mx-auto mb-4">
            <Bot className="w-6 h-6 text-primary" />
          </div>
          <h1 className="text-2xl font-bold text-foreground">选择编码智能体</h1>
          <p className="text-sm text-muted-foreground mt-2">选择一个AI智能体开始你的编码之旅</p>
        </div>

        {/* 名称输入 */}
        <div className="mb-4">
          <label className="text-xs text-muted-foreground mb-1.5 block">智能体名称（最多3个字）</label>
          <div className="flex items-center gap-2">
            <input
              type="text"
              value={displayName}
              onChange={(e) => setDisplayName(e.target.value.slice(0, 3))}
              placeholder="输入名称..."
              className="flex-1 px-3 py-2 text-sm bg-card border border-border rounded-lg text-foreground placeholder:text-muted-foreground focus:outline-none focus:border-primary focus:ring-1 focus:ring-primary/20"
            />
            <button
              type="button"
              onClick={handleRandomName}
              className="flex items-center gap-1 px-3 py-2 text-xs rounded-lg bg-secondary border border-border text-foreground hover:bg-secondary/80 transition-colors"
            >
              <Sparkles className="w-3 h-3" />
              随机
            </button>
          </div>
        </div>

        {/* 工作区输入 */}
        <div className="mb-5">
          <label className="text-xs text-muted-foreground mb-1.5 block">工作区目录</label>
          <div className="flex items-center gap-2">
            <input
              type="text"
              value={workspace}
              onChange={(e) => setWorkspace(e.target.value)}
              placeholder="输入工作目录路径..."
              className="flex-1 px-3 py-2 text-sm bg-card border border-border rounded-lg text-foreground placeholder:text-muted-foreground focus:outline-none focus:border-primary focus:ring-1 focus:ring-primary/20"
            />
            <DirectoryPickerButton onSelect={setWorkspace} />
          </div>
          <span className="text-[10px] text-muted-foreground mt-1 block">智能体将以此目录作为工作根目录</span>
        </div>

        {/* 智能体列表 */}
        <div className="space-y-3 mb-8">
          {agents.map((agent) => {
            const isSelected = selected === agent.id;
            return (
              <button
                key={agent.id}
                type="button"
                onClick={() => setSelected(agent.id)}
                className={`w-full flex items-center gap-4 p-4 rounded-lg border transition-all text-left ${
                  isSelected
                    ? 'border-primary bg-primary/5 shadow-sm'
                    : 'border-border bg-card hover:border-primary/50 hover:bg-card/80'
                }`}
              >
                <div className={`w-12 h-12 rounded-lg ${agent.bgColor} flex items-center justify-center shrink-0`}>
                  <AgentIcon agentKey={agent.id} size={28} />
                </div>
                <div className="flex-1 min-w-0">
                  <h3 className="font-semibold text-foreground">{agent.name}</h3>
                  <p className="text-sm text-muted-foreground mt-0.5 line-clamp-2">{agent.description}</p>
                </div>
                <div className={`w-5 h-5 rounded-full border-2 shrink-0 flex items-center justify-center transition-colors ${
                  isSelected ? 'border-primary' : 'border-muted-foreground/30'
                }`}>
                  {isSelected && <div className="w-2.5 h-2.5 rounded-full bg-primary" />}
                </div>
              </button>
            );
          })}
        </div>

        {/* 确认按钮 */}
        <Button
          onClick={handleConfirm}
          disabled={!selected || loading}
          className="w-full bg-primary text-primary-foreground hover:bg-primary/90 h-11"
        >
          {loading ? '加载中...' : (
            <>
              开始编码
              <ArrowRight className="w-4 h-4 ml-2" />
            </>
          )}
        </Button>

        <p className="text-center text-xs text-muted-foreground mt-4">
          登录用户：{user?.email?.replace('@miaoda.com', '') || '未知'}
        </p>
      </div>
    </div>
  );
}
