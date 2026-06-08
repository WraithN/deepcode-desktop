/**
 * Agent configuration map used by both conversation and agent panels.
 */
export const agentConfig: Record<
  string,
  { name: string; color: string; bg: string; letter: string; desc: string; border: string }
> = {
  opencode: {
    name: 'OpenCode',
    color: 'text-green-400',
    bg: 'bg-green-400/15',
    letter: 'O',
    desc: '全能型编程助手',
    border: 'border-green-400/20',
  },
  'claude-code': {
    name: 'Claude Code',
    color: 'text-orange-400',
    bg: 'bg-orange-400/15',
    letter: 'C',
    desc: '代码理解与重构专家',
    border: 'border-orange-400/20',
  },
  'cursor-agent': {
    name: 'Cursor Agent',
    color: 'text-blue-400',
    bg: 'bg-blue-400/15',
    letter: 'C',
    desc: '智能补全与生成',
    border: 'border-blue-400/20',
  },
  codex: {
    name: 'Codex',
    color: 'text-purple-400',
    bg: 'bg-purple-400/15',
    letter: 'X',
    desc: 'OpenAI软件工程模型',
    border: 'border-purple-400/20',
  },
  custom: {
    name: '自定义',
    color: 'text-primary',
    bg: 'bg-primary/15',
    letter: 'C',
    desc: '自由配置的智能体',
    border: 'border-primary/20',
  },
};
