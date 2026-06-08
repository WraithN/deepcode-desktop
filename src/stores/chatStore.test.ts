import { describe, it, expect, vi, beforeEach } from 'vitest';
import { useChatStore } from './chatStore';
import type { Message } from '@/types/types';

const resetStore = () => {
  useChatStore.setState({
    conversations: [],
    currentConversationId: null,
    opencodeSessionId: null,
    messages: [],
    isStreaming: false,
    isTyping: false,
    activeInstanceId: null,
    pendingInteraction: null,
    todos: [],
  });
};

vi.mock('./websocketStore', () => ({
  useWebSocketStore: {
    getState: vi.fn(() => ({
      ws: { readyState: 1 },
      url: 'ws://localhost:1234',
      sendRequest: vi.fn(),
      connect: vi.fn().mockResolvedValue(undefined),
    })),
  },
}));

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

describe('chatStore', () => {
  beforeEach(() => {
    resetStore();
    vi.clearAllMocks();
  });

  describe('initial state', () => {
    it('should have empty messages and no active conversation', () => {
      const state = useChatStore.getState();
      expect(state.messages).toEqual([]);
      expect(state.conversations).toEqual([]);
      expect(state.currentConversationId).toBeNull();
      expect(state.activeInstanceId).toBeNull();
      expect(state.isStreaming).toBe(false);
      expect(state.isTyping).toBe(false);
      expect(state.opencodeSessionId).toBeNull();
      expect(state.pendingInteraction).toBeNull();
      expect(state.todos).toEqual([]);
    });
  });

  describe('setCurrentConversation', () => {
    it('should set current conversation id', () => {
      useChatStore.getState().setCurrentConversation('conv-1');
      expect(useChatStore.getState().currentConversationId).toBe('conv-1');
    });

    it('should clear current conversation id when null', () => {
      useChatStore.getState().setCurrentConversation('conv-1');
      useChatStore.getState().setCurrentConversation(null);
      expect(useChatStore.getState().currentConversationId).toBeNull();
    });
  });

  describe('setActiveInstanceId', () => {
    it('should set active instance id', () => {
      useChatStore.getState().setActiveInstanceId('inst-1');
      expect(useChatStore.getState().activeInstanceId).toBe('inst-1');
    });
  });

  describe('setMessages', () => {
    it('should replace messages with array', () => {
      const msgs = [createMockMessage({ content: 'hello' })];
      useChatStore.getState().setMessages(msgs);
      expect(useChatStore.getState().messages).toHaveLength(1);
      expect(useChatStore.getState().messages[0].content).toBe('hello');
    });

    it('should update messages with function', () => {
      useChatStore.getState().setMessages([createMockMessage({ content: 'a' })]);
      useChatStore.getState().setMessages((prev) => [...prev, createMockMessage({ content: 'b' })]);
      expect(useChatStore.getState().messages).toHaveLength(2);
    });
  });

  describe('setConversations', () => {
    it('should set conversations list', () => {
      const conversations = [{ id: 'conv-1', title: 'Test' }];
      useChatStore.getState().setConversations(conversations);
      expect(useChatStore.getState().conversations).toEqual(conversations);
    });
  });

  describe('setIsStreaming / setIsTyping', () => {
    it('should toggle streaming state', () => {
      useChatStore.getState().setIsStreaming(true);
      expect(useChatStore.getState().isStreaming).toBe(true);
      useChatStore.getState().setIsStreaming(false);
      expect(useChatStore.getState().isStreaming).toBe(false);
    });

    it('should toggle typing state', () => {
      useChatStore.getState().setIsTyping(true);
      expect(useChatStore.getState().isTyping).toBe(true);
    });
  });

  describe('setOpencodeSessionId', () => {
    it('should set opencode session id', () => {
      useChatStore.getState().setOpencodeSessionId('sess-1');
      expect(useChatStore.getState().opencodeSessionId).toBe('sess-1');
    });
  });

  describe('setPendingInteraction', () => {
    it('should set pending interaction', () => {
      const interaction = { sessionId: 'sess-1', type: 'ask', payload: { question: 'test' } };
      useChatStore.getState().setPendingInteraction(interaction);
      expect(useChatStore.getState().pendingInteraction).toEqual(interaction);
    });

    it('should clear pending interaction', () => {
      useChatStore.getState().setPendingInteraction({ sessionId: 'sess-1', type: 'ask', payload: {} });
      useChatStore.getState().setPendingInteraction(null);
      expect(useChatStore.getState().pendingInteraction).toBeNull();
    });
  });

  describe('setTodos', () => {
    it('should set todos list', () => {
      const todos = [{ id: 't1', content: 'Task 1', status: 'pending' as const, priority: 'high' as const }];
      useChatStore.getState().setTodos(todos);
      expect(useChatStore.getState().todos).toEqual(todos);
    });
  });

  describe('loadConversation', () => {
    it('should set current conversation id', async () => {
      await useChatStore.getState().loadConversation('conv-1');
      expect(useChatStore.getState().currentConversationId).toBe('conv-1');
    });
  });

  describe('appendToken', () => {
    it('should append token to last assistant message', () => {
      useChatStore.getState().setMessages([
        createMockMessage({ role: 'user', content: 'hi' }),
        createMockMessage({ role: 'assistant', content: 'Hel' }),
      ]);
      useChatStore.getState().appendToken('lo');
      expect(useChatStore.getState().messages[1].content).toBe('Hello');
    });

    it('should not modify last message if it is from user', () => {
      useChatStore.getState().setMessages([
        createMockMessage({ role: 'user', content: 'hi' }),
      ]);
      useChatStore.getState().appendToken('test');
      expect(useChatStore.getState().messages[0].content).toBe('hi');
    });

    it('should not crash when messages are empty', () => {
      useChatStore.getState().appendToken('test');
      expect(useChatStore.getState().messages).toEqual([]);
    });
  });

  describe('sendMessage', () => {
    it('should throw when no active instance', async () => {
      await expect(useChatStore.getState().sendMessage('hello')).rejects.toThrow('No active agent instance');
    });

    it('should send message via websocket and return result', async () => {
      const { useWebSocketStore } = await import('./websocketStore');
      const mockSendRequest = vi.fn().mockResolvedValue({
        sessionID: 'sess-1',
        parts: [{ type: 'text', text: 'Response' }],
      });
      vi.mocked(useWebSocketStore.getState).mockReturnValue({
        ws: { readyState: 1 },
        url: 'ws://localhost:1234',
        sendRequest: mockSendRequest,
        connect: vi.fn().mockResolvedValue(undefined),
      } as unknown as ReturnType<typeof useWebSocketStore.getState>);

      useChatStore.getState().setActiveInstanceId('inst-1');
      const result = await useChatStore.getState().sendMessage('hello');

      expect(mockSendRequest).toHaveBeenCalledWith('agent.sendMessage', expect.objectContaining({
        instanceId: 'inst-1',
        message: 'hello',
      }));
      expect(result.sessionID).toBe('sess-1');
      expect(result.text).toBe('Response');
      expect(useChatStore.getState().isStreaming).toBe(false);
      expect(useChatStore.getState().isTyping).toBe(false);
    });

    it('should fallback to tauri invoke when websocket fails', async () => {
      const { useWebSocketStore } = await import('./websocketStore');
      const { invoke } = await import('@tauri-apps/api/core');

      vi.mocked(useWebSocketStore.getState).mockReturnValue({
        ws: null,
        url: null,
        sendRequest: vi.fn().mockRejectedValue(new Error('WS down')),
        connect: vi.fn().mockRejectedValue(new Error('Cannot connect')),
      } as unknown as ReturnType<typeof useWebSocketStore.getState>);

      vi.mocked(invoke).mockResolvedValue({
        sessionID: 'sess-fallback',
        parts: [{ type: 'text', text: 'Fallback' }],
      });

      useChatStore.getState().setActiveInstanceId('inst-1');
      const result = await useChatStore.getState().sendMessage('hello');

      expect(invoke).toHaveBeenCalledWith('agent_send_message_direct', expect.anything());
      expect(result.text).toBe('Fallback');
    });
  });

  describe('sendInteractionResponse', () => {
    it('should do nothing when no pending interaction', async () => {
      const { useWebSocketStore } = await import('./websocketStore');
      const mockSendRequest = vi.fn();
      vi.mocked(useWebSocketStore.getState).mockReturnValue({
        sendRequest: mockSendRequest,
      } as unknown as ReturnType<typeof useWebSocketStore.getState>);

      await useChatStore.getState().sendInteractionResponse({ answer: 'yes' });
      expect(mockSendRequest).not.toHaveBeenCalled();
    });

    it('should send response and clear pending interaction', async () => {
      const { useWebSocketStore } = await import('./websocketStore');
      const mockSendRequest = vi.fn().mockResolvedValue({});
      vi.mocked(useWebSocketStore.getState).mockReturnValue({
        sendRequest: mockSendRequest,
      } as unknown as ReturnType<typeof useWebSocketStore.getState>);

      useChatStore.getState().setPendingInteraction({ sessionId: 'sess-1', type: 'ask', payload: {} });
      await useChatStore.getState().sendInteractionResponse({ answer: 'yes' });

      expect(mockSendRequest).toHaveBeenCalledWith('agent.respond', {
        sessionId: 'sess-1',
        interactionType: 'ask',
        response: { answer: 'yes' },
      });
      expect(useChatStore.getState().pendingInteraction).toBeNull();
    });
  });
});

function createMockMessage(overrides: Partial<Message> = {}): Message {
  return {
    id: `msg-${Math.random().toString(36).slice(2)}`,
    conversation_id: 'conv-1',
    role: 'assistant',
    content: '',
    created_at: new Date().toISOString(),
    ...overrides,
  };
}
