import { invoke } from '@tauri-apps/api/core';
import type { AgentAdapter, AgentEvent, AgentStartConfig, AgentStatus } from '../types';
import { parseOpencodeEvent } from './parser';

export class OpencodeAdapter implements AgentAdapter {
  readonly agentKey = 'opencode';
  readonly displayName = 'OpenCode';

  async isInstalled(): Promise<boolean> {
    try {
      const result = await invoke<string>('check_opencode_installed');
      return !!result;
    } catch {
      return false;
    }
  }

  async start(config: AgentStartConfig): Promise<void> {
    await invoke('start_sidecar', {
      instanceId: config.instanceId,
      agentKey: this.agentKey,
      workspace: config.workspace,
    });
  }

  async stop(instanceId: string): Promise<void> {
    await invoke('stop_sidecar', { instanceId });
  }

  async *sendMessage(instanceId: string, message: string): AsyncGenerator<AgentEvent, void, unknown> {
    const status = await this.getStatus(instanceId);
    if (status.state !== 'running') {
      yield { type: 'error', message: '智能体未运行' };
      return;
    }

    const port = (status as Extract<AgentStatus, { state: 'running' }>).port;
    const url = `http://127.0.0.1:${port}/v1/messages`;

    const response = await fetch(url, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ message }),
    });

    if (!response.ok) {
      yield { type: 'error', message: `HTTP ${response.status}: ${response.statusText}` };
      return;
    }

    const reader = response.body?.getReader();
    if (!reader) {
      yield { type: 'error', message: '无法读取响应流' };
      return;
    }

    const decoder = new TextDecoder();
    let buffer = '';

    try {
      while (true) {
        const { done, value } = await reader.read();
        if (done) break;

        buffer += decoder.decode(value, { stream: true });
        const lines = buffer.split('\n');
        buffer = lines.pop() || '';

        let currentEvent: string | null = null;
        for (const line of lines) {
          if (line.startsWith('event:')) {
            currentEvent = line;
          } else if (line.startsWith('data:') && currentEvent) {
            const event = parseOpencodeEvent(currentEvent, line);
            if (event) yield event;
            currentEvent = null;
          }
        }
      }

      if (buffer.trim()) {
        const lines = buffer.split('\n');
        let currentEvent: string | null = null;
        for (const line of lines) {
          if (line.startsWith('event:')) {
            currentEvent = line;
          } else if (line.startsWith('data:') && currentEvent) {
            const event = parseOpencodeEvent(currentEvent, line);
            if (event) yield event;
            currentEvent = null;
          }
        }
      }
    } finally {
      reader.releaseLock();
    }

    yield { type: 'done' };
  }

  async setMode(instanceId: string, mode: 'build' | 'plan'): Promise<void> {
    const status = await this.getStatus(instanceId);
    if (status.state !== 'running') return;
    const port = (status as Extract<AgentStatus, { state: 'running' }>).port;
    const url = `http://127.0.0.1:${port}/v1/agents/mode`;
    await fetch(url, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ mode }),
    });
  }

  async getStatus(instanceId: string): Promise<AgentStatus> {
    try {
      const result = await invoke<{
        instanceId: string;
        agentKey: string;
        port: number;
        status: string;
        pid: number;
        workspace: string;
      }>('get_sidecar_status', { instanceId });

      if (result.status === 'running') {
        return { state: 'running', port: result.port, pid: result.pid };
      } else if (result.status === 'crashed') {
        return { state: 'crashed' };
      } else if (result.status === 'starting') {
        return { state: 'starting' };
      }
      return { state: 'stopped' };
    } catch {
      return { state: 'stopped' };
    }
  }
}
