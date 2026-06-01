import { Command } from '@tauri-apps/plugin-shell';
import type { AgentAdapter, AgentEvent, AgentStartConfig, AgentStatus } from '../types';
import { parseOpencodeJsonLine } from './parser';

export class OpencodeAdapter implements AgentAdapter {
  readonly agentKey = 'opencode';
  readonly displayName = 'OpenCode';

  async isInstalled(): Promise<boolean> {
    try {
      const result = await Command.create('opencode', ['--version']).execute();
      return result.code === 0 && result.stdout.includes('.');
    } catch {
      return false;
    }
  }

  async start(_config: AgentStartConfig): Promise<void> {
    // OpenCode 不需要长期运行的 sidecar 进程
    return;
  }

  async stop(_instanceId: string): Promise<void> {
    // 无需停止进程
    return;
  }

  async *sendMessage(
    _instanceId: string,
    message: string,
    options?: { workspace?: string; sessionId?: string; continueSession?: boolean },
  ): AsyncGenerator<AgentEvent, void, unknown> {
    const args = ['run', '--format', 'json'];

    if (options?.workspace) {
      args.push('--cwd', options.workspace);
    }

    if (options?.sessionId) {
      args.push('--session', options.sessionId);
    } else if (options?.continueSession) {
      args.push('--continue');
    }

    args.push(message);

    const result = await Command.create('opencode', args).execute();

    if (result.code !== 0) {
      yield {
        type: 'error',
        message: `opencode 执行失败 (exit ${result.code}): ${result.stderr || '未知错误'}`,
      };
      return;
    }

    const lines = result.stdout.split('\n').filter((l) => l.trim());
    const textBuffer: string[] = [];
    let currentStepHasTool = false;

    for (const line of lines) {
      const raw = parseOpencodeJsonLine(line);
      if (!raw) continue;

      const event = this.mapOpencodeEvent(raw, textBuffer, () => currentStepHasTool);
      if (!event) continue;

      if (event.type === 'tool_use') currentStepHasTool = true;
      yield event;
    }

    yield { type: 'done' };
  }

  private mapOpencodeEvent(
    raw: Record<string, unknown>,
    textBuffer: string[],
    hasTool: () => boolean,
  ): AgentEvent | null {
    const type = raw.type as string;

    switch (type) {
      case 'step_start': {
        textBuffer.length = 0;
        return { type: 'thinking', content: '思考中...' };
      }

      case 'text': {
        const part = raw.part as Record<string, unknown> | undefined;
        const text = part?.text as string | undefined;
        if (text) {
          textBuffer.push(text);
          return { type: 'text_delta', content: text };
        }
        return null;
      }

      case 'tool_use': {
        const part = raw.part as Record<string, unknown> | undefined;
        const toolName = (part?.tool as string) || 'unknown';
        const state = (part?.state as Record<string, unknown>) || {};
        const input = (state?.input as Record<string, unknown>) || {};
        const output = state?.output as string | undefined;
        const status = state?.status as string | undefined;

        if (output || status === 'completed') {
          return {
            type: 'tool_result',
            toolName,
            result: output || '完成',
            failed: status === 'failed' || status === 'error',
          };
        }

        return {
          type: 'tool_use',
          toolName,
          args: input,
        };
      }

      case 'step_finish': {
        if (!hasTool() && textBuffer.length > 0) {
          const content = textBuffer.join('');
          textBuffer.length = 0;
          return { type: 'text_delta', content };
        }
        return null;
      }

      case 'error': {
        const error = raw.error as Record<string, unknown> | undefined;
        return {
          type: 'error',
          message: (error?.message as string) || (error?.data as Record<string, unknown>)?.message as string || 'OpenCode 错误',
        };
      }

      default:
        return null;
    }
  }

  async setMode(_instanceId: string, _mode: 'build' | 'plan'): Promise<void> {
    console.warn('OpenCode CLI 模式切换暂未实现');
  }

  async getStatus(_instanceId: string): Promise<AgentStatus> {
    const installed = await this.isInstalled();
    if (installed) {
      return { state: 'running', port: 0, pid: 0 };
    }
    return { state: 'stopped' };
  }
}
