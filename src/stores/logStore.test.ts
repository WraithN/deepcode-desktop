import { describe, it, expect, vi, beforeEach } from 'vitest';
import { useLogStore } from './logStore';
import type { SessionLogEntry } from './logStore';

const resetStore = () => {
  useLogStore.setState({
    logs: [],
    filteredLogs: [],
    filterLevel: 'all',
  });
};

vi.mock('./websocketStore', () => ({
  useWebSocketStore: {
    getState: vi.fn(() => ({
      sendRequest: vi.fn(),
    })),
  },
}));

describe('logStore', () => {
  beforeEach(() => {
    resetStore();
    vi.clearAllMocks();
  });

  describe('initial state', () => {
    it('should have empty logs and all filter level', () => {
      const state = useLogStore.getState();
      expect(state.logs).toEqual([]);
      expect(state.filteredLogs).toEqual([]);
      expect(state.filterLevel).toBe('all');
    });
  });

  describe('appendLog', () => {
    it('should append log to logs and filteredLogs when filter is all', () => {
      const log = createMockLog({ level: 'info' });
      useLogStore.getState().appendLog(log);

      const state = useLogStore.getState();
      expect(state.logs).toHaveLength(1);
      expect(state.filteredLogs).toHaveLength(1);
      expect(state.logs[0].message).toBe(log.message);
    });

    it('should append log to logs but not filteredLogs when level does not match', () => {
      useLogStore.getState().setFilterLevel('error');
      const log = createMockLog({ level: 'info' });
      useLogStore.getState().appendLog(log);

      const state = useLogStore.getState();
      expect(state.logs).toHaveLength(1);
      expect(state.filteredLogs).toHaveLength(0);
    });

    it('should append log to both logs and filteredLogs when level matches', () => {
      useLogStore.getState().setFilterLevel('warn');
      const log = createMockLog({ level: 'warn' });
      useLogStore.getState().appendLog(log);

      const state = useLogStore.getState();
      expect(state.logs).toHaveLength(1);
      expect(state.filteredLogs).toHaveLength(1);
    });
  });

  describe('setFilterLevel', () => {
    it('should set filter level to error and filter logs', () => {
      const infoLog = createMockLog({ level: 'info' });
      const errorLog = createMockLog({ level: 'error' });
      useLogStore.getState().appendLog(infoLog);
      useLogStore.getState().appendLog(errorLog);

      useLogStore.getState().setFilterLevel('error');

      const state = useLogStore.getState();
      expect(state.filterLevel).toBe('error');
      expect(state.filteredLogs).toHaveLength(1);
      expect(state.filteredLogs[0].level).toBe('error');
    });

    it('should show all logs when filter is set to all', () => {
      useLogStore.getState().appendLog(createMockLog({ level: 'info' }));
      useLogStore.getState().appendLog(createMockLog({ level: 'error' }));
      useLogStore.getState().setFilterLevel('error');
      useLogStore.getState().setFilterLevel('all');

      expect(useLogStore.getState().filteredLogs).toHaveLength(2);
    });
  });

  describe('loadHistory', () => {
    it('should load history logs via websocket', async () => {
      const { useWebSocketStore } = await import('./websocketStore');
      const mockSendRequest = vi.fn().mockResolvedValue([
        {
          id: 'log-1',
          conversationId: 'conv-1',
          timestamp: '2024-01-01T00:00:00Z',
          level: 'info',
          source: 'test',
          message: 'Hello',
        },
      ]);
      vi.mocked(useWebSocketStore.getState).mockReturnValue({
        sendRequest: mockSendRequest,
      } as unknown as ReturnType<typeof useWebSocketStore.getState>);

      await useLogStore.getState().loadHistory('conv-1');

      expect(mockSendRequest).toHaveBeenCalledWith('session.logLoad', { conversationId: 'conv-1' });
      const state = useLogStore.getState();
      expect(state.logs).toHaveLength(1);
      expect(state.logs[0].message).toBe('Hello');
    });

    it('should parse string payload into detail object', async () => {
      const { useWebSocketStore } = await import('./websocketStore');
      const mockSendRequest = vi.fn().mockResolvedValue([
        {
          id: 'log-1',
          conversationId: 'conv-1',
          timestamp: '2024-01-01T00:00:00Z',
          level: 'info',
          source: 'test',
          message: 'Hello',
          payload: '{"key":"value"}',
        },
      ]);
      vi.mocked(useWebSocketStore.getState).mockReturnValue({
        sendRequest: mockSendRequest,
      } as unknown as ReturnType<typeof useWebSocketStore.getState>);

      await useLogStore.getState().loadHistory('conv-1');

      const state = useLogStore.getState();
      expect(state.logs[0].detail).toEqual({ key: 'value' });
    });
  });
});

function createMockLog(overrides: Partial<SessionLogEntry> = {}): SessionLogEntry {
  return {
    id: `log-${Math.random().toString(36).slice(2)}`,
    conversationId: 'conv-1',
    timestamp: new Date().toISOString(),
    level: 'info',
    source: 'test',
    message: 'Test log message',
    ...overrides,
  };
}
