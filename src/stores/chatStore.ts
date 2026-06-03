import { create } from 'zustand';
import { useWebSocketStore } from './websocketStore';

export interface Message {
  id: string;
  role: 'user' | 'assistant';
  content: string;
  steps?: AgentEvent[];
  createdAt: string;
}

export type AgentEvent =
  | { type: 'thinking'; content: string }
  | { type: 'text_delta'; content: string }
  | { type: 'tool_use'; toolName: string; args: unknown }
  | { type: 'tool_result'; toolName: string; result: string; failed: boolean }
  | { type: 'ask_permission'; message: string; toolName: string }
  | { type: 'ask_user'; questions: string[] }
  | { type: 'error'; message: string }
  | { type: 'done' };

interface ChatState {
  conversations: Array<{ id: string; title: string }>;
  currentConversationId: string | null;
  messages: Message[];
  isStreaming: boolean;
  activeInstanceId: string | null;

  sendMessage: (content: string) => Promise<void>;
  appendEvent: (event: AgentEvent) => void;
  loadConversation: (conversationId: string) => Promise<void>;
  setCurrentConversation: (conversationId: string | null) => void;
  setActiveInstanceId: (instanceId: string | null) => void;
}

export const useChatStore = create<ChatState>((set, get) => ({
  conversations: [],
  currentConversationId: null,
  messages: [],
  isStreaming: false,
  activeInstanceId: null,

  sendMessage: async (content: string) => {
    const ws = useWebSocketStore.getState();
    const { activeInstanceId } = get();

    if (!activeInstanceId) {
      throw new Error('No active agent instance');
    }

    const conversationId = get().currentConversationId || `conv-${Date.now()}`;

    // Add user message immediately
    const userMessage: Message = {
      id: `msg-${Date.now()}`,
      role: 'user',
      content,
      createdAt: new Date().toISOString(),
    };

    set((state) => ({
      messages: [...state.messages, userMessage],
      isStreaming: true,
      currentConversationId: conversationId,
    }));

    // Send via WebSocket
    await ws.sendRequest('agent.sendMessage', {
      instanceId: activeInstanceId,
      conversationId,
      message: content,
    });
  },

  appendEvent: (event: AgentEvent) => {
    set((state) => {
      const messages = [...state.messages];
      const lastMessage = messages[messages.length - 1];

      if (event.type === 'done') {
        return { isStreaming: false };
      }

      if (event.type === 'error') {
        return {
          isStreaming: false,
          messages: [
            ...messages,
            {
              id: `msg-${Date.now()}`,
              role: 'assistant',
              content: `Error: ${event.message}`,
              createdAt: new Date().toISOString(),
            },
          ],
        };
      }

      if (lastMessage && lastMessage.role === 'assistant') {
        // Update existing assistant message
        const updatedLastMessage = { ...lastMessage };

        if (event.type === 'text_delta') {
          updatedLastMessage.content = (updatedLastMessage.content || '') + event.content;
        }

        if (!updatedLastMessage.steps) {
          updatedLastMessage.steps = [];
        }
        updatedLastMessage.steps.push(event);

        messages[messages.length - 1] = updatedLastMessage;
        return { messages };
      } else {
        // Create new assistant message
        const assistantMessage: Message = {
          id: `msg-${Date.now()}`,
          role: 'assistant',
          content: event.type === 'text_delta' ? event.content : '',
          steps: [event],
          createdAt: new Date().toISOString(),
        };
        return { messages: [...messages, assistantMessage] };
      }
    });
  },

  loadConversation: async (conversationId: string) => {
    set({ currentConversationId: conversationId });
  },

  setCurrentConversation: (conversationId) => {
    set({ currentConversationId: conversationId });
  },

  setActiveInstanceId: (instanceId) => {
    set({ activeInstanceId: instanceId });
  },
}));
