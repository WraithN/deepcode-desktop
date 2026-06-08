import { invoke } from '@tauri-apps/api/core';
import type { IDataStore, AuthUser, AuthSession, AuthStateChangeCallback } from './types';
import type { Conversation, Message, Task, ModifiedFile, Profile } from '@/types/types';

const SESSION_KEY = 'aicoding_session';

class TauriDataStore implements IDataStore {
  private callbacks: AuthStateChangeCallback[] = [];

  private notify(event: 'SIGNED_IN' | 'SIGNED_OUT', session: AuthSession | null) {
    this.callbacks.forEach((cb) => cb(event, session));
  }

  private async invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
    return invoke<T>(cmd, args);
  }

  async signIn(username: string, password: string): Promise<{ user: AuthUser | null; error: Error | null }> {
    try {
      const user = await this.invoke<AuthUser>('db_sign_in', { username, password });
      const session: AuthSession = { user, token: `token-${Date.now()}` };
      localStorage.setItem(SESSION_KEY, JSON.stringify(session));
      this.notify('SIGNED_IN', session);
      return { user, error: null };
    } catch (e) {
      return { user: null, error: e as Error };
    }
  }

  async signUp(username: string, password: string): Promise<{ user: AuthUser | null; error: Error | null }> {
    try {
      const user = await this.invoke<AuthUser>('db_sign_up', { username, password });
      const session: AuthSession = { user, token: `token-${Date.now()}` };
      localStorage.setItem(SESSION_KEY, JSON.stringify(session));
      this.notify('SIGNED_IN', session);
      return { user, error: null };
    } catch (e) {
      return { user: null, error: e as Error };
    }
  }

  async signOut(): Promise<void> {
    localStorage.removeItem(SESSION_KEY);
    this.notify('SIGNED_OUT', null);
  }

  async getSession(): Promise<AuthSession | null> {
    try {
      const raw = localStorage.getItem(SESSION_KEY);
      if (!raw) { return null; }
      return JSON.parse(raw) as AuthSession;
    } catch {
      return null;
    }
  }

  onAuthStateChange(callback: AuthStateChangeCallback): { unsubscribe: () => void } {
    this.callbacks.push(callback);
    const session = this.getSession();
    session.then((s) => {
      if (s) { callback('SIGNED_IN', s); }
    });
    return {
      unsubscribe: () => {
        this.callbacks = this.callbacks.filter((cb) => cb !== callback);
      },
    };
  }

  async getProfile(userId: string): Promise<Profile | null> {
    return this.invoke<Profile | null>('db_get_profile', { userId });
  }

  async loadConversations(userId: string, limit = 50): Promise<Conversation[]> {
    return this.invoke<Conversation[]>('db_load_conversations', { userId, limit });
  }

  async createConversation(data: Omit<Conversation, 'id' | 'created_at' | 'updated_at'>): Promise<Conversation | null> {
    return this.invoke<Conversation | null>('db_create_conversation', { data });
  }

  async updateConversation(id: string, data: Partial<Conversation>): Promise<void> {
    await this.invoke<void>('db_update_conversation', { id, data });
  }

  async deleteConversation(id: string): Promise<void> {
    await this.invoke<void>('db_delete_conversation', { id });
  }

  async loadMessages(conversationId: string, limit = 100): Promise<Message[]> {
    return this.invoke<Message[]>('db_load_messages', { conversationId, limit });
  }

  async createMessage(data: Omit<Message, 'id' | 'created_at'>): Promise<Message | null> {
    return this.invoke<Message | null>('db_create_message', { data });
  }

  async loadTasks(userId: string, limit = 20): Promise<Task[]> {
    return this.invoke<Task[]>('db_load_tasks', { userId, limit });
  }

  async createTask(data: Omit<Task, 'id' | 'created_at'>): Promise<Task | null> {
    return this.invoke<Task | null>('db_create_task', { data });
  }

  async loadModifiedFiles(userId: string, limit = 20): Promise<ModifiedFile[]> {
    return this.invoke<ModifiedFile[]>('db_load_modified_files', { userId, limit });
  }

  async createModifiedFile(data: Omit<ModifiedFile, 'id' | 'created_at'>): Promise<ModifiedFile | null> {
    return this.invoke<ModifiedFile | null>('db_create_modified_file', { data });
  }
}

export const tauriDataStore = new TauriDataStore();
