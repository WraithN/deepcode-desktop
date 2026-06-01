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

    for (const line of lines) {
      const raw = parseOpencodeJsonLine(line);
      if (!raw) continue;

      const event = this.mapToAgentEvent(raw);
      if (event) yield event;
    }

    yield { type: 'done' };
  }

  /**
   * 将 OpenCode 扁平化事件映射为前端统一的 AgentEvent
   *
   * 映射关系：
   * - text          → text_delta
   * - tool_start    → tool_use
   * - tool_result   → tool_result
   * - step_start    → thinking
   * - step_complete → 忽略（前端不显示步骤完成事件）
   */
  private mapToAgentEvent(raw: import('./parser').OpencodeRawEvent): AgentEvent | null {
    switch (raw.type) {
      case 'text': {
        if (!raw.text) return null;
        return { type: 'text_delta', content: raw.text };
      }

      case 'tool_start': {
        return {
          type: 'tool_use',
          toolName: raw.tool || 'unknown',
          args: raw.args || {},
        };
      }

      case 'tool_result': {
        return {
          type: 'tool_result',
          toolName: raw.tool || 'unknown',
          result: raw.content || '',
          failed: false,
        };
      }

      case 'step_start': {
        const content = raw.description
          ? `${raw.step}: ${raw.description}`
          : raw.step || '思考中...';
        return { type: 'thinking', content };
      }

      case 'step_complete': {
        // 前端不显示步骤完成事件，返回 null
        return null;
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
