import { describe, it, expect, vi, beforeEach } from 'vitest';
import { mockDataStore } from './mock';

function assertNonNull<T>(value: T | null | undefined): T {
  if (value === null || value === undefined) {
    throw new Error('Expected non-null value');
  }
  return value;
}

describe('mockDataStore', () => {
  beforeEach(() => {
    localStorage.clear();
  });

  describe('auth', () => {
    it('should sign up a new user', async () => {
      const result = await mockDataStore.signUp('alice', 'password');
      expect(result.error).toBeNull();
      expect(result.user).not.toBeNull();
      expect(result.user?.username).toBe('alice');
    });

    it('should reject sign up with duplicate username', async () => {
      await mockDataStore.signUp('alice', 'password');
      const result = await mockDataStore.signUp('alice', 'password');
      expect(result.error).not.toBeNull();
      expect(result.user).toBeNull();
    });

    it('should sign in existing user', async () => {
      await mockDataStore.signUp('alice', 'password');
      // Remove only session to simulate fresh sign in while keeping profile
      localStorage.removeItem('aicoding_session');
      const result = await mockDataStore.signIn('alice', 'password');
      expect(result.error).toBeNull();
      expect(result.user?.username).toBe('alice');
    });

    it('should reject sign in for nonexistent user', async () => {
      const result = await mockDataStore.signIn('bob', 'password');
      expect(result.error).not.toBeNull();
      expect(result.user).toBeNull();
    });

    it('should sign out and clear session', async () => {
      await mockDataStore.signUp('alice', 'password');
      await mockDataStore.signOut();
      const session = await mockDataStore.getSession();
      expect(session).toBeNull();
    });

    it('should get current session', async () => {
      await mockDataStore.signUp('alice', 'password');
      const session = await mockDataStore.getSession();
      expect(session).not.toBeNull();
      expect(session?.user.username).toBe('alice');
    });

    it('should notify auth state change on sign in', async () => {
      const callback = vi.fn();
      mockDataStore.onAuthStateChange(callback);
      await mockDataStore.signUp('alice', 'password');
      expect(callback).toHaveBeenCalledWith('SIGNED_IN', expect.objectContaining({ user: expect.anything() }));
    });

    it('should notify auth state change on sign out', async () => {
      await mockDataStore.signUp('alice', 'password');
      const callback = vi.fn();
      mockDataStore.onAuthStateChange(callback);
      await mockDataStore.signOut();
      expect(callback).toHaveBeenCalledWith('SIGNED_OUT', null);
    });

    it('should allow unsubscribing from auth state changes', async () => {
      await mockDataStore.signUp('alice', 'password');
      const callback = vi.fn();
      const subscription = mockDataStore.onAuthStateChange(callback);
      // Should be called once for existing session
      expect(callback).toHaveBeenCalledTimes(1);
      subscription.unsubscribe();
      await mockDataStore.signOut();
      // Should still only have been called once
      expect(callback).toHaveBeenCalledTimes(1);
    });
  });

  describe('profile', () => {
    it('should get profile by user id', async () => {
      const user = assertNonNull((await mockDataStore.signUp('alice', 'password')).user);
      const profile = await mockDataStore.getProfile(user.id);
      expect(profile).not.toBeNull();
      expect(profile?.username).toBe('alice');
    });

    it('should return null for nonexistent profile', async () => {
      const profile = await mockDataStore.getProfile('nonexistent');
      expect(profile).toBeNull();
    });
  });

  describe('conversations', () => {
    it('should create and load conversations', async () => {
      const user = assertNonNull((await mockDataStore.signUp('alice', 'password')).user);
      const conv = await mockDataStore.createConversation({
        user_id: user.id,
        title: 'Test',
        agent: 'opencode',
        model: 'gpt-4',
      });
      expect(conv).not.toBeNull();

      const conversations = await mockDataStore.loadConversations(user.id);
      expect(conversations).toHaveLength(1);
      expect(conversations[0].title).toBe('Test');
    });

    it('should update conversation', async () => {
      const user = assertNonNull((await mockDataStore.signUp('alice', 'password')).user);
      const conv = await mockDataStore.createConversation({
        user_id: user.id,
        title: 'Old',
        agent: 'opencode',
        model: 'gpt-4',
      });
      expect(conv).not.toBeNull();
      await mockDataStore.updateConversation(conv?.id ?? '', { title: 'New' });
      const conversations = await mockDataStore.loadConversations(user.id);
      expect(conversations[0].title).toBe('New');
    });

    it('should delete conversation', async () => {
      const user = assertNonNull((await mockDataStore.signUp('alice', 'password')).user);
      const conv = await mockDataStore.createConversation({
        user_id: user.id,
        title: 'Test',
        agent: 'opencode',
        model: 'gpt-4',
      });
      expect(conv).not.toBeNull();
      await mockDataStore.deleteConversation(conv?.id ?? '');
      const conversations = await mockDataStore.loadConversations(user.id);
      expect(conversations).toHaveLength(0);
    });
  });

  describe('messages', () => {
    it('should create and load messages', async () => {
      const msg = await mockDataStore.createMessage({
        conversation_id: 'conv-1',
        role: 'user',
        content: 'Hello',
      });
      expect(msg).not.toBeNull();

      const messages = await mockDataStore.loadMessages('conv-1');
      expect(messages).toHaveLength(1);
      expect(messages[0].content).toBe('Hello');
    });

    it('should filter messages by conversation id', async () => {
      await mockDataStore.createMessage({ conversation_id: 'conv-1', role: 'user', content: 'A' });
      await mockDataStore.createMessage({ conversation_id: 'conv-2', role: 'user', content: 'B' });
      const messages = await mockDataStore.loadMessages('conv-1');
      expect(messages).toHaveLength(1);
      expect(messages[0].content).toBe('A');
    });
  });

  describe('tasks', () => {
    it('should create and load tasks', async () => {
      const user = assertNonNull((await mockDataStore.signUp('alice', 'password')).user);
      const task = await mockDataStore.createTask({
        user_id: user.id,
        conversation_id: 'conv-1',
        title: 'Task 1',
        status: 'pending',
      });
      expect(task).not.toBeNull();

      const tasks = await mockDataStore.loadTasks(user.id);
      expect(tasks).toHaveLength(1);
      expect(tasks[0].title).toBe('Task 1');
    });
  });

  describe('modified files', () => {
    it('should create and load modified files', async () => {
      const user = assertNonNull((await mockDataStore.signUp('alice', 'password')).user);
      const file = await mockDataStore.createModifiedFile({
        user_id: user.id,
        conversation_id: 'conv-1',
        file_path: 'src/index.ts',
        change_type: 'modified',
        diff: '+console.log(1)',
      });
      expect(file).not.toBeNull();

      const files = await mockDataStore.loadModifiedFiles(user.id);
      expect(files).toHaveLength(1);
      expect(files[0].file_path).toBe('src/index.ts');
    });
  });
});
