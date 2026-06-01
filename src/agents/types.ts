export interface Question {
  id: string;
  label: string;
  type: 'choice' | 'custom';
  options?: string[];
  required?: boolean;
}

export type AgentEvent =
  | { type: 'thinking'; content: string }
  | { type: 'tool_use'; toolName: string; args: Record<string, unknown> }
  | { type: 'tool_result'; toolName: string; result: string; failed?: boolean }
  | { type: 'ask_permission'; toolName: string; message: string }
  | { type: 'ask_user'; questions: Question[] }
  | { type: 'text_delta'; content: string }
  | { type: 'done' }
  | { type: 'error'; message: string };

export interface AgentStartConfig {
  instanceId: string;
  workspace: string;
  port?: number;
}

export type AgentStatus =
  | { state: 'stopped' }
  | { state: 'starting' }
  | { state: 'running'; port: number; pid: number }
  | { state: 'crashed'; error?: string };

export interface AgentAdapter {
  readonly agentKey: string;
  readonly displayName: string;

  isInstalled(): Promise<boolean>;
  start(config: AgentStartConfig): Promise<void>;
  stop(instanceId: string): Promise<void>;
  sendMessage(instanceId: string, message: string): AsyncGenerator<AgentEvent, void, unknown>;
  setMode(instanceId: string, mode: 'build' | 'plan'): Promise<void>;
  getStatus(instanceId: string): Promise<AgentStatus>;
}
