import { describe, it, expect, vi } from 'vitest';
import { sessionLog } from './session-log';

describe('SessionLogStore', () => {
  it('should add and retrieve logs for a conversation', () => {
    sessionLog.clear('conv-1');
    sessionLog.add('conv-1', 'info', 'TestSource', 'hello');
    const logs = sessionLog.getLogs('conv-1');
    expect(logs).toHaveLength(1);
    expect(logs[0].source).toBe('TestSource');
    expect(logs[0].message).toBe('hello');
    expect(logs[0].level).toBe('info');
  });

  it('should isolate conversations', () => {
    sessionLog.clear('conv-a');
    sessionLog.clear('conv-b');
    sessionLog.add('conv-a', 'info', 'Src', 'msg-a');
    sessionLog.add('conv-b', 'info', 'Src', 'msg-b');
    expect(sessionLog.getLogs('conv-a')).toHaveLength(1);
    expect(sessionLog.getLogs('conv-b')).toHaveLength(1);
    expect(sessionLog.getLogs('conv-a')[0].message).toBe('msg-a');
  });

  it('should notify subscribers', () => {
    sessionLog.clear('conv-sub');
    const listener = vi.fn();
    const unsubscribe = sessionLog.subscribe(listener);
    sessionLog.add('conv-sub', 'info', 'Src', 'test');
    expect(listener).toHaveBeenCalledTimes(1);
    unsubscribe();
    sessionLog.add('conv-sub', 'info', 'Src', 'test2');
    expect(listener).toHaveBeenCalledTimes(1);
  });

  it('should clear logs', () => {
    sessionLog.clear('conv-clear');
    sessionLog.add('conv-clear', 'info', 'Src', 'msg');
    expect(sessionLog.getLogs('conv-clear')).toHaveLength(1);
    sessionLog.clear('conv-clear');
    expect(sessionLog.getLogs('conv-clear')).toHaveLength(0);
  });

  it('should cap logs at 500 per session', () => {
    sessionLog.clear('conv-cap');
    for (let i = 0; i < 510; i++) {
      sessionLog.add('conv-cap', 'info', 'Src', `msg-${i}`);
    }
    expect(sessionLog.getLogs('conv-cap')).toHaveLength(500);
    expect(sessionLog.getLogs('conv-cap')[0].message).toBe('msg-10');
  });
});
