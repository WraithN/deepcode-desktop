import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { OpencodeAdapter } from './adapter';
import type { AgentEvent } from '../types';

// Mock @tauri-apps/plugin-shell
const mockExecute = vi.fn();
const mockCommandCreate = vi.fn().mockReturnValue({
  execute: mockExecute,
});

vi.mock('@tauri-apps/plugin-shell', () => ({
  Command: {
    create: (...args: unknown[]) => mockCommandCreate(...args),
  },
}));

// Mock debug-logger
vi.mock('@/services/debug-logger', () => ({
  debugLogger: {
    log: vi.fn().mockResolvedValue(undefined),
  },
}));

describe('OpencodeAdapter Tauri integration', () => {
  let adapter: OpencodeAdapter;

  beforeEach(() => {
    adapter = new OpencodeAdapter();
    mockExecute.mockReset();
    mockCommandCreate.mockClear();
    // Set Tauri environment
    (window as any).__TAURI_INTERNALS__ = {};
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('should execute opencode with correct args', async () => {
    mockExecute.mockResolvedValue({
      code: 0,
      signal: null,
      stdout: '',
      stderr: '',
    });

    const generator = adapter.sendMessage('test-id', 'hello', { workspace: '/test' });
    await generator.next(); // trigger execution

    expect(mockCommandCreate).toHaveBeenCalledWith('opencode', ['run', '--format', 'json', '--dir', '/test', 'hello']);
    expect(mockExecute).toHaveBeenCalled();
  });

  it('should not add --dir for default workspace', async () => {
    mockExecute.mockResolvedValue({
      code: 0,
      signal: null,
      stdout: '',
      stderr: '',
    });

    const generator = adapter.sendMessage('test-id', 'hello', { workspace: '.' });
    await generator.next();

    expect(mockCommandCreate).toHaveBeenCalledWith('opencode', ['run', '--format', 'json', 'hello']);
  });

  it('should parse stdout and yield events', async () => {
    const stdout = [
      JSON.stringify({ type: 'step_start', part: { type: 'step-start' } }),
      JSON.stringify({ type: 'text', part: { type: 'text', text: 'Hello user' } }),
      JSON.stringify({ type: 'step_finish', part: { type: 'step-finish', reason: 'stop' } }),
    ].join('\n');

    mockExecute.mockResolvedValue({
      code: 0,
      signal: null,
      stdout,
      stderr: '',
    });

    const events: AgentEvent[] = [];
    for await (const event of adapter.sendMessage('test-id', 'hello')) {
      events.push(event);
    }

    expect(events).toHaveLength(3);
    expect(events[0].type).toBe('thinking');
    expect(events[1].type).toBe('text_delta');
    expect(events[1]).toEqual({ type: 'text_delta', content: 'Hello user' });
    expect(events[2].type).toBe('done');
  });

  it('should handle opencode failure', async () => {
    mockExecute.mockResolvedValue({
      code: 1,
      signal: null,
      stdout: '',
      stderr: 'Error: something went wrong',
    });

    const events: AgentEvent[] = [];
    for await (const event of adapter.sendMessage('test-id', 'hello')) {
      events.push(event);
    }

    expect(events).toHaveLength(1);
    expect(events[0].type).toBe('error');
  });

  it('should handle execute exception', async () => {
    mockExecute.mockRejectedValue(new Error('Command not found'));

    const events: AgentEvent[] = [];
    for await (const event of adapter.sendMessage('test-id', 'hello')) {
      events.push(event);
    }

    expect(events).toHaveLength(1);
    expect(events[0].type).toBe('error');
    expect(events[0]).toEqual({ type: 'error', message: '启动 opencode 失败: Command not found' });
  });

  it('should handle empty stdout', async () => {
    mockExecute.mockResolvedValue({
      code: 0,
      signal: null,
      stdout: '',
      stderr: '',
    });

    const events: AgentEvent[] = [];
    for await (const event of adapter.sendMessage('test-id', 'hello')) {
      events.push(event);
    }

    expect(events).toHaveLength(1);
    expect(events[0].type).toBe('done');
  });

  it('should handle help text output (wrong args)', async () => {
    // This simulates what happens when --cwd is used instead of --dir
    const stdout = `opencode run [message..]

run opencode with a message

Positionals:
  message  message to send  [array] [default: []]`;

    mockExecute.mockResolvedValue({
      code: 0,
      signal: null,
      stdout,
      stderr: '',
    });

    const events: AgentEvent[] = [];
    for await (const event of adapter.sendMessage('test-id', 'hello')) {
      events.push(event);
    }

    // Should yield fallback text_delta + done since help text cannot be parsed as JSON events
    expect(events).toHaveLength(2);
    expect(events[0].type).toBe('text_delta');
    expect(events[1].type).toBe('done');
  });

  it('should yield done after events even with null event mapping', async () => {
    const stdout = [
      JSON.stringify({ type: 'step_start', part: { type: 'step-start' } }),
      JSON.stringify({ type: 'step_finish', part: { type: 'step-finish', reason: 'stop' } }),
    ].join('\n');

    mockExecute.mockResolvedValue({
      code: 0,
      signal: null,
      stdout,
      stderr: '',
    });

    const events: AgentEvent[] = [];
    for await (const event of adapter.sendMessage('test-id', 'hello')) {
      events.push(event);
    }

    // step_start -> thinking, step_finish -> null (skipped), done
    expect(events).toHaveLength(2);
    expect(events[0].type).toBe('thinking');
    expect(events[1].type).toBe('done');
  });
});
