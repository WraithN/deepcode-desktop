import { OpenCode, ClaudeCode, Cursor, Codex } from '@lobehub/icons';
import { Bot } from 'lucide-react';

interface AgentIconProps {
  agentKey: string;
  size?: number;
  className?: string;
}

export default function AgentIcon({ agentKey, size = 20, className = '' }: AgentIconProps) {
  switch (agentKey) {
    case 'opencode':
      return <OpenCode size={size} className={className} />;
    case 'claude-code':
      return <ClaudeCode size={size} className={className} />;
    case 'cursor-agent':
      return <Cursor size={size} className={className} />;
    case 'codex':
      return <Codex size={size} className={className} />;
    case 'custom':
      return <Bot size={size} className={className} />;
    default:
      return <Bot size={size} className={className} />;
  }
}
