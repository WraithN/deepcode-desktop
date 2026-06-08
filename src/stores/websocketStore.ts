import { create } from 'zustand';
import { invoke } from '@tauri-apps/api/core';

interface JsonRpcRequest {
  jsonrpc: '2.0';
  id: string;
  method: string;
  params?: unknown;
}

interface JsonRpcResponse {
  jsonrpc: '2.0';
  id: string;
  result?: unknown;
  error?: {
    code: number;
    message: string;
    data?: unknown;
  };
}

interface JsonRpcNotification {
  jsonrpc: '2.0';
  method: string;
  params?: unknown;
}

type WebSocketStatus = 'idle' | 'connecting' | 'connected' | 'reconnecting' | 'error';

let connectPromise: Promise<void> | null = null;

interface WebSocketState {
  url: string | null;
  status: WebSocketStatus;
  reconnectAttempts: number;
  ws: WebSocket | null;
  pendingRequests: Map<string, { resolve: (value: unknown) => void; reject: (reason: Error) => void }>;
  notificationHandlers: Map<string, Set<(params: unknown) => void>>;

  connect: (url: string) => Promise<void>;
  disconnect: () => void;
  sendRequest: <T>(method: string, params?: unknown) => Promise<T>;
  subscribe: (method: string, handler: (params: unknown) => void) => () => void;
}

export const useWebSocketStore = create<WebSocketState>((set, get) => ({
  url: null,
  status: 'idle',
  reconnectAttempts: 0,
  ws: null,
  pendingRequests: new Map(),
  notificationHandlers: new Map(),

  connect: async (url: string) => {
    const state = get();
    if (state.ws?.readyState === WebSocket.OPEN) {
      return;
    }
    if (connectPromise && state.status === 'connecting') {
      return connectPromise;
    }

    set({ status: 'connecting', url });

    connectPromise = new Promise<void>((resolve, reject) => {
      const ws = new WebSocket(url);

      ws.onopen = () => {
        connectPromise = null;
        set({ status: 'connected', reconnectAttempts: 0, ws });
        console.log('[WebSocket] connected to', url);
        resolve();
      };

      ws.onmessage = (event) => {
        try {
          const data = JSON.parse(event.data) as JsonRpcResponse | JsonRpcNotification;
          // Log to Rust for debugging
          try {
            const method = ((data as unknown) as Record<string, unknown>).method || ((data as unknown) as Record<string, unknown>).id || 'unknown';
            void invoke('console_logs', {
              logs: [{ type: 'info', message: `[WS raw] ${method}: ${JSON.stringify(data).slice(0, 200)}` }]
            });
          } catch (_) { /* ignore */ }

          // Check if it's a response with an id
          if ('id' in data && data.id !== undefined && data.id !== null) {
            const pending = get().pendingRequests.get(data.id);
            if (pending) {
              get().pendingRequests.delete(data.id);
              if ('error' in data && data.error) {
                pending.reject(new Error(data.error.message));
              } else {
                pending.resolve(data.result);
              }
            }
          } else if ('method' in data) {
            // It's a notification
            const handlers = get().notificationHandlers.get(data.method);
            if (handlers) {
              handlers.forEach((handler) => handler(data.params));
            }
          }
        } catch (e) {
          console.error('Failed to parse WebSocket message:', e);
        }
      };

      ws.onclose = () => {
        connectPromise = null;
        set({ status: 'idle', ws: null });
        // Auto-reconnect logic
        const currentState = get();
        if (currentState.url && currentState.status !== 'error') {
          const attempts = currentState.reconnectAttempts + 1;
          const delay = Math.min(1000 * Math.pow(2, attempts - 1), 30000);

          set({ status: 'reconnecting', reconnectAttempts: attempts });

          setTimeout(() => {
            if (get().url) {
              get().connect(get().url as string);
            }
          }, delay);
        }
      };

      ws.onerror = () => {
        connectPromise = null;
        const message = `WebSocket connection failed: ${url}`;
        console.error(message);
        set({ status: 'error' });
        reject(new Error(message));
      };
    });

    return connectPromise;
  },

  disconnect: () => {
    const { ws } = get();
    if (ws) {
      ws.close();
      set({ ws: null, status: 'idle', url: null });
    }
  },

  sendRequest: async <T>(method: string, params?: unknown): Promise<T> => {
    let { ws } = get();

    if (!ws || ws.readyState !== WebSocket.OPEN) {
      const { url } = get();
      if (!url) {
        throw new Error('WebSocket not connected');
      }
      await get().connect(url);
      ws = get().ws;
    }

    if (!ws || ws.readyState !== WebSocket.OPEN) {
      throw new Error('WebSocket not connected');
    }

    const { pendingRequests } = get();
    const id = `req-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`;
    const request: JsonRpcRequest = {
      jsonrpc: '2.0',
      id,
      method,
      params,
    };

    return new Promise<T>((resolve, reject) => {
      pendingRequests.set(id, { resolve: resolve as (value: unknown) => void, reject });
      ws.send(JSON.stringify(request));

      // Timeout after 30 seconds
      setTimeout(() => {
        if (pendingRequests.has(id)) {
          pendingRequests.delete(id);
          reject(new Error(`Request timeout: ${method}`));
        }
      }, 30000);
    });
  },

  subscribe: (method: string, handler: (params: unknown) => void) => {
    const { notificationHandlers } = get();

    if (!notificationHandlers.has(method)) {
      notificationHandlers.set(method, new Set());
    }

    notificationHandlers.get(method)?.add(handler);

    // Return unsubscribe function
    return () => {
      const handlers = get().notificationHandlers.get(method);
      if (handlers) {
        handlers.delete(handler);
        if (handlers.size === 0) {
          get().notificationHandlers.delete(method);
        }
      }
    };
  },
}));
