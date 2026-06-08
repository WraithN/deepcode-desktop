import { describe, it, expect, vi, beforeEach } from 'vitest';
import { useWebSocketStore } from './websocketStore';

describe('websocketStore', () => {
  beforeEach(() => {
    // Reset store to initial state
    useWebSocketStore.getState().disconnect();
    useWebSocketStore.setState({
      url: null,
      status: 'idle',
      reconnectAttempts: 0,
      ws: null,
      pendingRequests: new Map(),
      notificationHandlers: new Map(),
    });
    vi.clearAllMocks();
  });

  describe('initial state', () => {
    it('should have idle status and null ws', () => {
      const state = useWebSocketStore.getState();
      expect(state.status).toBe('idle');
      expect(state.ws).toBeNull();
      expect(state.url).toBeNull();
      expect(state.reconnectAttempts).toBe(0);
    });
  });

  describe('connect', () => {
    it('should set status to connected when WebSocket opens', async () => {
      await useWebSocketStore.getState().connect('ws://localhost:1234');
      expect(useWebSocketStore.getState().status).toBe('connected');
      expect(useWebSocketStore.getState().ws).not.toBeNull();
    });

    it('should not reconnect when already connected', async () => {
      await useWebSocketStore.getState().connect('ws://localhost:1234');
      const result = await useWebSocketStore.getState().connect('ws://localhost:1234');

      // When already connected, connect resolves immediately to undefined
      expect(result).toBeUndefined();
    });
  });

  describe('disconnect', () => {
    it('should set status to idle and clear ws', async () => {
      await useWebSocketStore.getState().connect('ws://localhost:1234');
      useWebSocketStore.getState().disconnect();

      expect(useWebSocketStore.getState().status).toBe('idle');
      expect(useWebSocketStore.getState().ws).toBeNull();
    });
  });

  describe('sendRequest', () => {
    it('should send JSON-RPC request and resolve on response', async () => {
      await useWebSocketStore.getState().connect('ws://localhost:1234');

      const ws = useWebSocketStore.getState().ws;
      expect(ws).not.toBeNull();

      const sendPromise = useWebSocketStore.getState().sendRequest<string>('test.method', { foo: 'bar' });

      // Simulate response
      if (ws && 'simulateMessage' in ws && typeof ws.simulateMessage === 'function') {
        // Extract request id from pending requests
        const pending = useWebSocketStore.getState().pendingRequests;
        const id = Array.from(pending.keys())[0];
        ws.simulateMessage({ jsonrpc: '2.0', id, result: 'success' });
      }

      const result = await sendPromise;
      expect(result).toBe('success');
    });

    it('should reject on error response', async () => {
      await useWebSocketStore.getState().connect('ws://localhost:1234');

      const ws = useWebSocketStore.getState().ws;
      const sendPromise = useWebSocketStore.getState().sendRequest('test.method');

      if (ws && 'simulateMessage' in ws && typeof ws.simulateMessage === 'function') {
        const pending = useWebSocketStore.getState().pendingRequests;
        const id = Array.from(pending.keys())[0];
        ws.simulateMessage({ jsonrpc: '2.0', id, error: { code: -1, message: 'Test error' } });
      }

      await expect(sendPromise).rejects.toThrow('Test error');
    });

    it('should throw when url is not set and ws is not open', async () => {
      useWebSocketStore.setState({ url: null, ws: null, status: 'idle' });

      await expect(useWebSocketStore.getState().sendRequest('test.method')).rejects.toThrow('WebSocket not connected');
    });
  });

  describe('subscribe', () => {
    it('should register notification handler and call it on notification', async () => {
      await useWebSocketStore.getState().connect('ws://localhost:1234');

      const handler = vi.fn();
      const unsubscribe = useWebSocketStore.getState().subscribe('test.notification', handler);

      const ws = useWebSocketStore.getState().ws;
      if (ws && 'simulateMessage' in ws && typeof ws.simulateMessage === 'function') {
        ws.simulateMessage({ jsonrpc: '2.0', method: 'test.notification', params: { data: 123 } });
      }

      expect(handler).toHaveBeenCalledWith({ data: 123 });

      // Unsubscribe
      unsubscribe();

      if (ws && 'simulateMessage' in ws && typeof ws.simulateMessage === 'function') {
        ws.simulateMessage({ jsonrpc: '2.0', method: 'test.notification', params: { data: 456 } });
      }

      // Should still only have been called once
      expect(handler).toHaveBeenCalledTimes(1);
    });

    it('should return unsubscribe function that removes handler', () => {
      const handler = vi.fn();
      const unsubscribe = useWebSocketStore.getState().subscribe('test.notification', handler);
      unsubscribe();

      const handlers = useWebSocketStore.getState().notificationHandlers;
      expect(handlers.has('test.notification')).toBe(false);
    });
  });

  describe('circuit breaker', () => {
    it('should open circuit after 5 consecutive failures', async () => {
      // Simulate multiple failed connections
      for (let i = 0; i < 5; i++) {
        useWebSocketStore.setState({ consecutiveFailures: i, circuitOpen: false });
        // Trigger onerror path by creating a connection that fails
        const connectPromise = useWebSocketStore.getState().connect('ws://fail');
        const ws = useWebSocketStore.getState().ws;
        if (ws && ws.onerror) {
          ws.onerror(new Event('error'));
        }
        try { await connectPromise; } catch { /* ignore */ }
      }

      expect(useWebSocketStore.getState().circuitOpen).toBe(true);
    });

    it('should reject new connections when circuit is open', async () => {
      useWebSocketStore.setState({
        circuitOpen: true,
        circuitResetTime: Date.now() + 30000,
        consecutiveFailures: 5,
      });

      await expect(useWebSocketStore.getState().connect('ws://test')).rejects.toThrow('circuit breaker');
    });

    it('should reset circuit via resetCircuit', () => {
      useWebSocketStore.setState({
        circuitOpen: true,
        consecutiveFailures: 5,
        circuitResetTime: Date.now() + 30000,
      });

      useWebSocketStore.getState().resetCircuit();

      expect(useWebSocketStore.getState().circuitOpen).toBe(false);
      expect(useWebSocketStore.getState().consecutiveFailures).toBe(0);
    });
  });
});
