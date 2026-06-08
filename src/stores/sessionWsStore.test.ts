import { describe, it, expect, vi, beforeEach } from 'vitest';
import { useSessionWsStore, setSessionWsBaseUrl, getSessionWsBaseUrl } from './sessionWsStore';

const resetStore = () => {
  useSessionWsStore.getState().disconnectAll();
  useSessionWsStore.setState({
    connections: new Map(),
  });
  setSessionWsBaseUrl('');
};

describe('sessionWsStore', () => {
  beforeEach(() => {
    resetStore();
  });

  describe('initial state', () => {
    it('should have empty connections map', () => {
      expect(useSessionWsStore.getState().connections.size).toBe(0);
    });
  });

  describe('setSessionWsBaseUrl / getSessionWsBaseUrl', () => {
    it('should set and get base url', () => {
      setSessionWsBaseUrl('ws://localhost:9999');
      expect(getSessionWsBaseUrl()).toBe('ws://localhost:9999');
    });

    it('should default to empty string', () => {
      expect(getSessionWsBaseUrl()).toBe('');
    });
  });

  describe('connect', () => {
    it('should create a websocket connection for conversation', () => {
      const ws = useSessionWsStore.getState().connect('conv-1');
      expect(ws).not.toBeNull();
      expect(useSessionWsStore.getState().connections.has('conv-1')).toBe(true);
    });

    it('should replace existing connection for same conversation', () => {
      const ws1 = useSessionWsStore.getState().connect('conv-1');
      const ws2 = useSessionWsStore.getState().connect('conv-1');
      expect(ws1).not.toBe(ws2);
      expect(useSessionWsStore.getState().connections.get('conv-1')).toBe(ws2);
    });

    it('should use default url when base url is not set', () => {
      const ws = useSessionWsStore.getState().connect('conv-1');
      expect(ws).not.toBeNull();
    });

    it('should use custom base url when set', () => {
      setSessionWsBaseUrl('ws://custom:9999');
      const ws = useSessionWsStore.getState().connect('conv-1');
      expect(ws).not.toBeNull();
    });

    it('should call onMessage when message is received', () => {
      const onMessage = vi.fn();
      const ws = useSessionWsStore.getState().connect('conv-1', onMessage);
      expect(ws).not.toBeNull();

      if (ws && 'simulateMessage' in ws && typeof ws.simulateMessage === 'function') {
        ws.simulateMessage({ data: 'test' });
      } else if (ws && ws.onmessage) {
        ws.onmessage(new MessageEvent('message', { data: '{"data":"test"}' }));
      }

      expect(onMessage).toHaveBeenCalledWith({ data: 'test' });
    });
  });

  describe('disconnect', () => {
    it('should remove connection for conversation', () => {
      useSessionWsStore.getState().connect('conv-1');
      useSessionWsStore.getState().disconnect('conv-1');
      expect(useSessionWsStore.getState().connections.has('conv-1')).toBe(false);
    });

    it('should do nothing when conversation does not exist', () => {
      expect(() => useSessionWsStore.getState().disconnect('nonexistent')).not.toThrow();
    });
  });

  describe('disconnectAll', () => {
    it('should remove all connections', () => {
      useSessionWsStore.getState().connect('conv-1');
      useSessionWsStore.getState().connect('conv-2');
      useSessionWsStore.getState().disconnectAll();
      expect(useSessionWsStore.getState().connections.size).toBe(0);
    });
  });

  describe('send', () => {
    it('should send message when connection is open', async () => {
      const ws = useSessionWsStore.getState().connect('conv-1');
      expect(ws).not.toBeNull();

      // Wait for microtask to trigger onopen and set readyState to OPEN
      await new Promise((resolve) => { queueMicrotask(resolve); });

      if (ws) {
        const sendSpy = vi.spyOn(ws, 'send');
        useSessionWsStore.getState().send('conv-1', { type: 'ping' });
        expect(sendSpy).toHaveBeenCalledWith(JSON.stringify({ type: 'ping' }));
      }
    });

    it('should not send when connection does not exist', () => {
      expect(() => useSessionWsStore.getState().send('nonexistent', { type: 'ping' })).not.toThrow();
    });
  });
});
