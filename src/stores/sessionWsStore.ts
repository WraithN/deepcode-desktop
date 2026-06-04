import { create } from 'zustand';

interface SessionWsState {
  connections: Map<string, WebSocket>;

  connect: (conversationId: string, onMessage?: (data: unknown) => void) => WebSocket;
  disconnect: (conversationId: string) => void;
  disconnectAll: () => void;
  send: (conversationId: string, message: unknown) => void;
}

let wsBaseUrl = '';

export function setSessionWsBaseUrl(url: string) {
  wsBaseUrl = url;
}

export function getSessionWsBaseUrl(): string {
  return wsBaseUrl;
}

export const useSessionWsStore = create<SessionWsState>((set, get) => ({
  connections: new Map(),

  connect: (conversationId: string, onMessage?: (data: unknown) => void) => {
    const { connections } = get();

    // 如果已连接，先断开
    if (connections.has(conversationId)) {
      const oldWs = connections.get(conversationId);
      oldWs?.close();
    }

    const url = wsBaseUrl || `ws://127.0.0.1:9527`;
    const ws = new WebSocket(`${url}/ws/${conversationId}`);

    ws.onopen = () => {
      console.log(`[SessionWS] Connected to ${conversationId}`);
    };

    ws.onclose = () => {
      console.log(`[SessionWS] Disconnected from ${conversationId}`);
      set((state) => {
        const newConnections = new Map(state.connections);
        newConnections.delete(conversationId);
        return { connections: newConnections };
      });
    };

    ws.onerror = (error) => {
      console.error(`[SessionWS] Error for ${conversationId}:`, error);
    };

    ws.onmessage = (event) => {
      try {
        const data = JSON.parse(event.data);
        onMessage?.(data);
      } catch (e) {
        console.error('[SessionWS] Failed to parse message:', e);
      }
    };

    set((state) => {
      const newConnections = new Map(state.connections);
      newConnections.set(conversationId, ws);
      return { connections: newConnections };
    });

    return ws;
  },

  disconnect: (conversationId: string) => {
    const { connections } = get();
    const ws = connections.get(conversationId);
    if (ws) {
      ws.close();
    }
  },

  disconnectAll: () => {
    const { connections } = get();
    connections.forEach((ws) => ws.close());
    set({ connections: new Map() });
  },

  send: (conversationId: string, message: unknown) => {
    const { connections } = get();
    const ws = connections.get(conversationId);
    if (ws && ws.readyState === WebSocket.OPEN) {
      ws.send(JSON.stringify(message));
    }
  },
}));
