import { Command } from '@tauri-apps/plugin-shell';
import type { AgentAdapter, AgentEvent, AgentStartConfig, AgentStatus } from '../types';
import { parseOpencodeJsonLine } from './parser';
import { debugLogger } from '@/services/debug-logger';

function isTauri(): boolean {
  return !!(window as any).__TAURI_INTERNALS__ || !!(window as any).__TAURI__;
}

export class OpencodeAdapter implements AgentAdapter {
  readonly agentKey = 'opencode';
  readonly displayName = 'OpenCode';

  async isInstalled(): Promise<boolean> {
    try {
      await debugLogger.log('OpencodeAdapter', 'checking isInstalled');
      const result = await Command.create('opencode', ['--version']).execute();
      await debugLogger.log('OpencodeAdapter', 'isInstalled result', { code: result.code, stdout: result.stdout });
      return result.code === 0 && result.stdout.includes('.');
    } catch (e) {
      await debugLogger.log('OpencodeAdapter', 'isInstalled failed', { error: String(e) });
      return false;
    }
  }

  async start(_config: AgentStartConfig): Promise<void> {
    return;
  }

  async stop(_instanceId: string): Promise<void> {
    return;
  }

  async *sendMessage(
    _instanceId: string,
    message: string,
    options?: { workspace?: string; sessionId?: string; continueSession?: boolean },
  ): AsyncGenerator<AgentEvent, void, unknown> {
    await debugLogger.log('OpencodeAdapter', 'sendMessage called', { message, options });

    // 浏览器环境下返回模拟数据（用于测试）
    if (!isTauri()) {
      await debugLogger.log('OpencodeAdapter', 'browser mode, returning mock data');
      yield { type: 'thinking', content: '正在思考...' };
      yield { type: 'text_delta', content: `收到消息: ${message}` };
      yield { type: 'done' };
      return;
    }

    const args = ['run', '--format', 'json'];

    if (options?.workspace && options.workspace !== '.') {
      args.push('--dir', options.workspace);
    }

    if (options?.sessionId) {
      args.push('--session', options.sessionId);
    } else if (options?.continueSession) {
      args.push('--continue');
    }

    args.push(message);

    await debugLogger.log('OpencodeAdapter', 'executing command', { program: 'opencode', args });

    let result;
    try {
      result = await Command.create('opencode', args).execute();
    } catch (execError) {
      await debugLogger.log('OpencodeAdapter', 'execute failed', { error: String(execError) });
      yield {
        type: 'error',
        message: `启动 opencode 失败: ${execError instanceof Error ? execError.message : String(execError)}`,
      };
      return;
    }

    await debugLogger.log('OpencodeAdapter', 'execute completed', {
      code: result.code,
      signal: result.signal,
      stdoutLength: result.stdout?.length,
      stderrLength: result.stderr?.length,
    });

    if (result.code !== 0) {
      await debugLogger.log('OpencodeAdapter', 'non-zero exit code', { code: result.code, stderr: result.stderr });
      yield {
        type: 'error',
        message: `opencode 执行失败 (exit ${result.code}): ${result.stderr || '未知错误'}`,
      };
      return;
    }

    const lines = result.stdout.split('\n').filter((l) => l.trim());
    await debugLogger.log('OpencodeAdapter', 'parsed lines count', { count: lines.length });

    let eventCount = 0;
    for (let i = 0; i < lines.length; i++) {
      const line = lines[i];
      await debugLogger.log('OpencodeAdapter', `line ${i}`, { line: line.substring(0, 200) });

      const raw = parseOpencodeJsonLine(line);
      await debugLogger.log('OpencodeAdapter', `parsed line ${i}`, { raw });

      if (!raw) {
        await debugLogger.log('OpencodeAdapter', `line ${i} parse returned null`);
        continue;
      }

      const event = this.mapToAgentEvent(raw);
      await debugLogger.log('OpencodeAdapter', `mapped event ${i}`, { event });

      if (event) {
        eventCount++;
        await debugLogger.log('OpencodeAdapter', `yielding event ${i}`, { type: event.type });
        yield event;
      } else {
        await debugLogger.log('OpencodeAdapter', `event ${i} mapped to null, skipping`);
      }
    }

    // 兜底：如果没有任何事件被解析出来，将原始 stdout 作为 text_delta 返回
    if (eventCount === 0 && result.stdout) {
      await debugLogger.log('OpencodeAdapter', 'no events parsed, yielding raw stdout as fallback');
      yield { type: 'text_delta', content: `【原始输出】\n${result.stdout.substring(0, 2000)}` };
    }

    await debugLogger.log('OpencodeAdapter', 'yielding done event');
    yield { type: 'done' };
  }

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
        return null;
      }
      default:
        return null;
    }
  }

  async setMode(_instanceId: string, _mode: 'build' | 'plan'): Promise<void> {
    await debugLogger.log('OpencodeAdapter', 'OpenCode CLI 模式切换暂未实现');
  }

  async getStatus(_instanceId: string): Promise<AgentStatus> {
    const installed = await this.isInstalled();
    if (installed) {
      return { state: 'running', port: 0, pid: 0 };
    }
    return { state: 'stopped' };
  }
}
