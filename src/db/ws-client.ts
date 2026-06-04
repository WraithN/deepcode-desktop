import type { IDataStore, AuthUser, AuthSession, AuthStateChangeCallback } from './types';
import type { Conversation, Message, Task, ModifiedFile, Profile } from '@/types/types';

const WS_URL = 'ws://127.0.0.1:9527';

interface JsonRpcRequest {
  jsonrpc: '2.0';
  id: string;
  method: string;
  params?: Record<string, unknown>;
}

interface JsonRpcResponse {
  jsonrpc: '2.0';
  id: string;
  result?: unknown;
  error?: { code: number; message: string; data?: unknown };
}

class WsDataStore implements IDataStore {
  private ws: WebSocket | null = null;
  private _status: 'idle' | 'connecting' | 'connected' = 'idle';
  private pendingRequests = new Map<string, { resolve: (value: unknown) => void; reject: (reason: Error) => void }>();
  private connectPromise: Promise<void> | null = null;
  private callbacks: AuthStateChangeCallback[] = [];

  private async ensureConnected(): Promise<void> {
    if (this.ws?.readyState === WebSocket.OPEN) {
      return;
    }
    if (this.connectPromise) {
      return this.connectPromise;
    }
    this.connectPromise = this.doConnect();
    return this.connectPromise;
  }

  private doConnect(): Promise<void> {
    return new Promise((resolve, reject) => {
      this._status = 'connecting';
      const ws = new WebSocket(WS_URL);

      ws.onopen = () => {
        this._status = 'connected';
        this.ws = ws;
        this.connectPromise = null;
        resolve();
      };

      ws.onmessage = (event) => {
        try {
          const data = JSON.parse(event.data) as JsonRpcResponse;
          if (data.id !== undefined && data.id !== null) {
            const pending = this.pendingRequests.get(data.id);
            if (pending) {
              this.pendingRequests.delete(data.id);
              if (data.error) {
                pending.reject(new Error(data.error.message));
              } else {
                pending.resolve(data.result);
              }
            }
          }
        } catch {
          // ignore non-JSON messages
        }
      };

      ws.onclose = () => {
        this._status = 'idle';
        this.ws = null;
        this.connectPromise = null;
      };

      ws.onerror = (_err) => {
        this._status = 'idle';
        this.ws = null;
        this.connectPromise = null;
        reject(new Error('WebSocket connection failed'));
      };
    });
  }

  private async sendRequest<T>(method: string, params?: Record<string, unknown>): Promise<T> {
    await this.ensureConnected();
    const id = `db-${Date.now()}-${Math.random().toString(36).slice(2, 9)}`;
    const request: JsonRpcRequest = {
      jsonrpc: '2.0',
      id,
      method,
      params,
    };
    console.log('[ws-client] sendRequest', method, id, params);
    return new Promise<T>((resolve, reject) => {
      this.pendingRequests.set(id, { resolve: resolve as (value: unknown) => void, reject });
      this.ws!.send(JSON.stringify(request));
      setTimeout(() => {
        if (this.pendingRequests.has(id)) {
          console.warn('[ws-client] request timeout:', method, id);
          this.pendingRequests.delete(id);
          reject(new Error(`Request timeout: ${method}`));
        }
      }, 10000);
    });
  }

  // ========== Auth ==========
  async signIn(username: string, password: string): Promise<{ user: AuthUser | null; error: Error | null }> {
    try {
      const user = await this.sendRequest<AuthUser>('db.signIn', { username, password });
      const session: AuthSession = { user, token: `token-${Date.now()}` };
      localStorage.setItem('aicoding_session', JSON.stringify(session));
      this.callbacks.forEach((cb) => cb('SIGNED_IN', session));
      return { user, error: null };
    } catch (error) {
      return { user: null, error: error instanceof Error ? error : new Error(String(error)) };
    }
  }

  async signUp(username: string, password: string): Promise<{ user: AuthUser | null; error: Error | null }> {
    try {
      const user = await this.sendRequest<AuthUser>('db.signUp', { username, password });
      const session: AuthSession = { user, token: `token-${Date.now()}` };
      localStorage.setItem('aicoding_session', JSON.stringify(session));
      this.callbacks.forEach((cb) => cb('SIGNED_IN', session));
      return { user, error: null };
    } catch (error) {
      return { user: null, error: error instanceof Error ? error : new Error(String(error)) };
    }
  }

  async signOut(): Promise<void> {
    localStorage.removeItem('aicoding_session');
    this.callbacks.forEach((cb) => cb('SIGNED_OUT', null));
  }

  async getSession(): Promise<AuthSession | null> {
    try {
      const raw = localStorage.getItem('aicoding_session');
      return raw ? JSON.parse(raw) : null;
    } catch {
      return null;
    }
  }

  onAuthStateChange(callback: AuthStateChangeCallback): { unsubscribe: () => void } {
    this.callbacks.push(callback);
    // 立即触发一次当前状态
    this.getSession().then((session) => {
      if (session) callback('SIGNED_IN', session);
    });
    return {
      unsubscribe: () => {
        this.callbacks = this.callbacks.filter((cb) => cb !== callback);
      },
    };
  }

  async getProfile(userId: string): Promise<Profile | null> {
    try {
      return await this.sendRequest<Profile>('db.getProfile', { userId });
    } catch {
      return null;
    }
  }

  // ========== Conversations ==========
  async loadConversations(userId: string, limit = 50): Promise<Conversation[]> {
    try {
      return await this.sendRequest<Conversation[]>('db.loadConversations', { userId, limit });
    } catch {
      return [];
    }
  }

  async createConversation(data: Omit<Conversation, 'id' | 'created_at' | 'updated_at'>): Promise<Conversation | null> {
    try {
      return await this.sendRequest<Conversation>('db.createConversation', data as Record<string, unknown>);
    } catch {
      return null;
    }
  }

  async updateConversation(id: string, data: Partial<Conversation>): Promise<void> {
    try {
      await this.sendRequest<void>('db.updateConversation', { id, data });
    } catch {
      // ignore
    }
  }

  async deleteConversation(id: string): Promise<void> {
    try {
      await this.sendRequest<void>('db.deleteConversation', { id });
    } catch {
      // ignore
    }
  }

  // ========== Messages ==========
  async loadMessages(conversationId: string, limit = 100): Promise<Message[]> {
    try {
      return await this.sendRequest<Message[]>('db.loadMessages', { conversationId, limit });
    } catch {
      return [];
    }
  }

  async createMessage(data: Omit<Message, 'id' | 'created_at'>): Promise<Message | null> {
    try {
      return await this.sendRequest<Message>('db.createMessage', data as Record<string, unknown>);
    } catch {
      return null;
    }
  }

  // ========== Tasks ==========
  async loadTasks(userId: string, limit = 20): Promise<Task[]> {
    try {
      return await this.sendRequest<Task[]>('db.loadTasks', { userId, limit });
    } catch {
      return [];
    }
  }

  async createTask(data: Omit<Task, 'id' | 'created_at'>): Promise<Task | null> {
    try {
      return await this.sendRequest<Task>('db.createTask', data as Record<string, unknown>);
    } catch {
      return null;
    }
  }

  // ========== Modified Files ==========
  async loadModifiedFiles(userId: string, limit = 20): Promise<ModifiedFile[]> {
    try {
      return await this.sendRequest<ModifiedFile[]>('db.loadModifiedFiles', { userId, limit });
    } catch {
      return [];
    }
  }

  async createModifiedFile(data: Omit<ModifiedFile, 'id' | 'created_at'>): Promise<ModifiedFile | null> {
    try {
      return await this.sendRequest<ModifiedFile>('db.createModifiedFile', data as Record<string, unknown>);
    } catch {
      return null;
    }
  }
}

export const wsDataStore = new WsDataStore();
