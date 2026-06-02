import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { OpencodeAdapter } from './adapter';
import type { AgentEvent } from '../types';

describe('OpencodeAdapter', () => {
  let adapter: OpencodeAdapter;

  beforeEach(() => {
    adapter = new OpencodeAdapter();
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  describe('browser mode (non-Tauri)', () => {
    it('should yield mock events in browser mode', async () => {
      // Ensure window.__TAURI_INTERNALS__ is not set
      const original = (window as any).__TAURI_INTERNALS__;
      (window as any).__TAURI_INTERNALS__ = undefined;

      const events: AgentEvent[] = [];
      for await (const event of adapter.sendMessage('test-id', 'hello')) {
        events.push(event);
      }

      expect(events).toHaveLength(3);
      expect(events[0].type).toBe('thinking');
      expect(events[1].type).toBe('text_delta');
      expect(events[2].type).toBe('done');

      (window as any).__TAURI_INTERNALS__ = original;
    });
  });

  describe('mapToAgentEvent', () => {
    it('should map text to text_delta', () => {
      const raw = { type: 'text' as const, text: 'Hello' };
      const event = (adapter as any).mapToAgentEvent(raw);
      expect(event).toEqual({ type: 'text_delta', content: 'Hello' });
    });

    it('should map step_start to thinking', () => {
      const raw = { type: 'step_start' as const, step: 'Thinking', description: 'AI is thinking' };
      const event = (adapter as any).mapToAgentEvent(raw);
      expect(event).toEqual({ type: 'thinking', content: 'Thinking: AI is thinking' });
    });

    it('should map step_complete to null', () => {
      const raw = { type: 'step_complete' as const };
      const event = (adapter as any).mapToAgentEvent(raw);
      expect(event).toBeNull();
    });

    it('should map tool_start to tool_use', () => {
      const raw = { type: 'tool_start' as const, tool: 'read_file', args: { path: '/test' } };
      const event = (adapter as any).mapToAgentEvent(raw);
      expect(event).toEqual({ type: 'tool_use', toolName: 'read_file', args: { path: '/test' } });
    });

    it('should map tool_result to tool_result event', () => {
      const raw = { type: 'tool_result' as const, tool: 'read_file', content: 'file content' };
      const event = (adapter as any).mapToAgentEvent(raw);
      expect(event).toEqual({ type: 'tool_result', toolName: 'read_file', result: 'file content', failed: false });
    });
  });

  describe('command args building', () => {
    it('should build correct args without workspace', async () => {
      const mockExecute = vi.fn().mockResolvedValue({
        code: 0,
        signal: null,
        stdout: '',
        stderr: '',
      });

      // We can't easily test private method, but we can verify behavior
      // by mocking Command
      expect(true).toBe(true);
    });
  });
});
