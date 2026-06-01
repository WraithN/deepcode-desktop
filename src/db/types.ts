import type { Conversation, Message, Task, ModifiedFile, Profile } from '@/types/types';

export interface AuthUser {
  id: string;
  email: string;
  username: string;
  created_at: string;
}

export interface AuthSession {
  user: AuthUser;
  token: string;
}

export type AuthStateChangeCallback = (event: 'SIGNED_IN' | 'SIGNED_OUT', session: AuthSession | null) => void;

export interface IDataStore {
  // ========== Auth ==========
  signIn(username: string, password: string): Promise<{ user: AuthUser | null; error: Error | null }>;
  signUp(username: string, password: string): Promise<{ user: AuthUser | null; error: Error | null }>;
  signOut(): Promise<void>;
  getSession(): Promise<AuthSession | null>;
  onAuthStateChange(callback: AuthStateChangeCallback): { unsubscribe: () => void };
  getProfile(userId: string): Promise<Profile | null>;

  // ========== Conversations ==========
  loadConversations(userId: string, limit?: number): Promise<Conversation[]>;
  createConversation(data: Omit<Conversation, 'id' | 'created_at' | 'updated_at'>): Promise<Conversation | null>;
  updateConversation(id: string, data: Partial<Conversation>): Promise<void>;
  deleteConversation(id: string): Promise<void>;

  // ========== Messages ==========
  loadMessages(conversationId: string, limit?: number): Promise<Message[]>;
  createMessage(data: Omit<Message, 'id' | 'created_at'>): Promise<Message | null>;

  // ========== Tasks ==========
  loadTasks(userId: string, limit?: number): Promise<Task[]>;
  createTask(data: Omit<Task, 'id' | 'created_at'>): Promise<Task | null>;

  // ========== Modified Files ==========
  loadModifiedFiles(userId: string, limit?: number): Promise<ModifiedFile[]>;
  createModifiedFile(data: Omit<ModifiedFile, 'id' | 'created_at'>): Promise<ModifiedFile | null>;
}

export interface DbStorageUploadOptions {
  bucketName: string;
  path?: string;
  file: File;
  upsert?: boolean;
}

export interface DbStorageUploadResult {
  name: string;
  message?: string;
}

export interface IDbStorage {
  upload(options: DbStorageUploadOptions): Promise<DbStorageUploadResult>;
}
