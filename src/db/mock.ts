import type { IDataStore, AuthUser, AuthSession, AuthStateChangeCallback } from './types';
import type { Conversation, Message, Task, ModifiedFile, Profile } from '@/types/types';
import { generateId } from '@/lib/id';

const SESSION_KEY = 'aicoding_session';
const PROFILES_KEY = 'aicoding_profiles';
const CONVERSATIONS_KEY = 'aicoding_conversations';
const MESSAGES_KEY = 'aicoding_messages';
const TASKS_KEY = 'aicoding_tasks';
const MODIFIED_FILES_KEY = 'aicoding_modified_files';

function getLocalItem<T>(key: string, defaultValue: T): T {
  try {
    const raw = localStorage.getItem(key);
    return raw ? JSON.parse(raw) : defaultValue;
  } catch {
    return defaultValue;
  }
}

function setLocalItem<T>(key: string, value: T) {
  localStorage.setItem(key, JSON.stringify(value));
}

function now(): string {
  return new Date().toISOString();
}

class MockDataStore implements IDataStore {
  private callbacks: AuthStateChangeCallback[] = [];

  private notify(event: 'SIGNED_IN' | 'SIGNED_OUT', session: AuthSession | null) {
    this.callbacks.forEach((cb) => cb(event, session));
  }

  // ========== Auth ==========
  async signIn(username: string, _password: string): Promise<{ user: AuthUser | null; error: Error | null }> {
    const profiles: Profile[] = getLocalItem(PROFILES_KEY, []);
    const profile = profiles.find((p) => p.username === username);
    if (!profile) {
      return { user: null, error: new Error('用户不存在') };
    }
    const user: AuthUser = {
      id: profile.id,
      email: profile.email || `${username}@local.dev`,
      username: profile.username || username,
      created_at: profile.created_at,
    };
    const session: AuthSession = { user, token: `token-${generateId()}` };
    setLocalItem(SESSION_KEY, session);
    this.notify('SIGNED_IN', session);
    return { user, error: null };
  }

  async signUp(username: string, _password: string): Promise<{ user: AuthUser | null; error: Error | null }> {
    const profiles: Profile[] = getLocalItem(PROFILES_KEY, []);
    if (profiles.some((p) => p.username === username)) {
      return { user: null, error: new Error('用户名已存在') };
    }
    const id = generateId();
    const email = `${username}@local.dev`;
    const profile: Profile = {
      id,
      username,
      email,
      phone: null,
      role: 'user',
      created_at: now(),
    };
    profiles.push(profile);
    setLocalItem(PROFILES_KEY, profiles);

    const user: AuthUser = { id, email, username, created_at: profile.created_at };
    const session: AuthSession = { user, token: `token-${generateId()}` };
    setLocalItem(SESSION_KEY, session);
    this.notify('SIGNED_IN', session);
    return { user, error: null };
  }

  async signOut(): Promise<void> {
    localStorage.removeItem(SESSION_KEY);
    this.notify('SIGNED_OUT', null);
  }

  async getSession(): Promise<AuthSession | null> {
    return getLocalItem<AuthSession | null>(SESSION_KEY, null);
  }

  onAuthStateChange(callback: AuthStateChangeCallback): { unsubscribe: () => void } {
    this.callbacks.push(callback);
    // 立即触发一次当前状态
    const session = getLocalItem<AuthSession | null>(SESSION_KEY, null);
    if (session) {
      callback('SIGNED_IN', session);
    }
    return {
      unsubscribe: () => {
        this.callbacks = this.callbacks.filter((cb) => cb !== callback);
      },
    };
  }

  async getProfile(userId: string): Promise<Profile | null> {
    const profiles: Profile[] = getLocalItem(PROFILES_KEY, []);
    return profiles.find((p) => p.id === userId) || null;
  }

  // ========== Conversations ==========
  async loadConversations(userId: string, limit = 50): Promise<Conversation[]> {
    const all: Conversation[] = getLocalItem(CONVERSATIONS_KEY, []);
    return all
      .filter((c) => c.user_id === userId)
      .sort((a, b) => new Date(b.updated_at).getTime() - new Date(a.updated_at).getTime())
      .slice(0, limit);
  }

  async createConversation(data: Omit<Conversation, 'id' | 'created_at' | 'updated_at'>): Promise<Conversation | null> {
    const all: Conversation[] = getLocalItem(CONVERSATIONS_KEY, []);
    const item: Conversation = { ...data, id: generateId(), created_at: now(), updated_at: now() };
    all.push(item);
    setLocalItem(CONVERSATIONS_KEY, all);
    return item;
  }

  async updateConversation(id: string, data: Partial<Conversation>): Promise<void> {
    const all: Conversation[] = getLocalItem(CONVERSATIONS_KEY, []);
    const idx = all.findIndex((c) => c.id === id);
    if (idx !== -1) {
      all[idx] = { ...all[idx], ...data, updated_at: now() };
      setLocalItem(CONVERSATIONS_KEY, all);
    }
  }

  async deleteConversation(id: string): Promise<void> {
    const all: Conversation[] = getLocalItem(CONVERSATIONS_KEY, []);
    setLocalItem(
      CONVERSATIONS_KEY,
      all.filter((c) => c.id !== id)
    );
  }

  // ========== Messages ==========
  async loadMessages(conversationId: string, limit = 100): Promise<Message[]> {
    const all: Message[] = getLocalItem(MESSAGES_KEY, []);
    return all
      .filter((m) => m.conversation_id === conversationId)
      .sort((a, b) => new Date(a.created_at).getTime() - new Date(b.created_at).getTime())
      .slice(0, limit);
  }

  async createMessage(data: Omit<Message, 'id' | 'created_at'>): Promise<Message | null> {
    const all: Message[] = getLocalItem(MESSAGES_KEY, []);
    const item: Message = { ...data, id: generateId(), created_at: now() };
    all.push(item);
    setLocalItem(MESSAGES_KEY, all);
    return item;
  }

  // ========== Tasks ==========
  async loadTasks(userId: string, limit = 20): Promise<Task[]> {
    const all: Task[] = getLocalItem(TASKS_KEY, []);
    return all
      .filter((t) => t.user_id === userId)
      .sort((a, b) => new Date(b.created_at).getTime() - new Date(a.created_at).getTime())
      .slice(0, limit);
  }

  async createTask(data: Omit<Task, 'id' | 'created_at'>): Promise<Task | null> {
    const all: Task[] = getLocalItem(TASKS_KEY, []);
    const item: Task = { ...data, id: generateId(), created_at: now() };
    all.push(item);
    setLocalItem(TASKS_KEY, all);
    return item;
  }

  // ========== Modified Files ==========
  async loadModifiedFiles(userId: string, limit = 20): Promise<ModifiedFile[]> {
    const all: ModifiedFile[] = getLocalItem(MODIFIED_FILES_KEY, []);
    return all
      .filter((f) => f.user_id === userId)
      .sort((a, b) => new Date(b.created_at).getTime() - new Date(a.created_at).getTime())
      .slice(0, limit);
  }

  async createModifiedFile(data: Omit<ModifiedFile, 'id' | 'created_at'>): Promise<ModifiedFile | null> {
    const all: ModifiedFile[] = getLocalItem(MODIFIED_FILES_KEY, []);
    const item: ModifiedFile = { ...data, id: generateId(), created_at: now() };
    all.push(item);
    setLocalItem(MODIFIED_FILES_KEY, all);
    return item;
  }
}

export const mockDataStore = new MockDataStore();
