import { create } from 'zustand';
import { invoke } from '@tauri-apps/api/core';
import { useWebSocketStore } from './websocketStore';
import type { Message } from '@/types/types';

interface ChatState {
  conversations: Array<{ id: string; title: string }>;
  currentConversationId: string | null;
  opencodeSessionId: string | null;
  messages: Message[];
  isStreaming: boolean;
  isTyping: boolean;
  activeInstanceId: string | null;

  sendMessage: (content: string) => Promise<{ sessionID?: string; text: string }>;
  loadConversation: (conversationId: string) => Promise<void>;
  setCurrentConversation: (conversationId: string | null) => void;
  setActiveInstanceId: (instanceId: string | null) => void;
  setMessages: (messages: Message[] | ((prev: Message[]) => Message[])) => void;
  setConversations: (conversations: Array<{ id: string; title: string }>) => void;
  setIsStreaming: (isStreaming: boolean) => void;
  setIsTyping: (isTyping: boolean) => void;
  setOpencodeSessionId: (sessionId: string | null) => void;
}

async function ensureWebSocketConnected() {
  const wsStore = useWebSocketStore.getState();
  if (wsStore.ws?.readyState === WebSocket.OPEN) {
    return;
  }

  const url = wsStore.url || await invoke<string>('get_websocket_url');
  await useWebSocketStore.getState().connect(url);
}

export const useChatStore = create<ChatState>((set, get) => ({
  conversations: [],
  currentConversationId: null,
  opencodeSessionId: null,
  messages: [],
  isStreaming: false,
  isTyping: false,
  activeInstanceId: null,

  sendMessage: async (content: string) => {
    const { activeInstanceId, opencodeSessionId } = get();

    if (!activeInstanceId) {
      throw new Error('No active agent instance');
    }

    const conversationId = get().currentConversationId || `conv-${Date.now()}`;

    set({
      isStreaming: true,
      isTyping: true,
      currentConversationId: conversationId,
    });

    try {
      let result: { sessionID?: string; parts?: Array<{ type?: string; text?: string }> };
      try {
        await ensureWebSocketConnected();

        result = await useWebSocketStore.getState().sendRequest<{ sessionID?: string; parts?: Array<{ type?: string; text?: string }> }>('agent.sendMessage', {
          instanceId: activeInstanceId,
          conversationId,
          message: content,
          opencodeSessionId,
        });
      } catch (error) {
        console.warn('[chatStore] WebSocket unavailable, falling back to Tauri invoke:', error);
        result = await invoke<{ sessionID?: string; parts?: Array<{ type?: string; text?: string }> }>('agent_send_message_direct', {
          message: content,
          sessionId: opencodeSessionId || null,
        });
      }

      if (result.sessionID) {
        set({ opencodeSessionId: result.sessionID });
      }

      const text = (result.parts || [])
        .filter((part) => part.type === 'text' && part.text)
        .map((part) => part.text)
        .join('');

      return { sessionID: result.sessionID, text };
    } finally {
      set({ isStreaming: false, isTyping: false });
    }
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

  setMessages: (messages) => {
    if (typeof messages === 'function') {
      set((state) => ({ messages: messages(state.messages) }));
    } else {
      set({ messages });
    }
  },

  setConversations: (conversations) => {
    set({ conversations });
  },

  setIsStreaming: (isStreaming) => {
    set({ isStreaming });
  },

  setIsTyping: (isTyping) => {
    set({ isTyping });
  },

  setOpencodeSessionId: (sessionId) => {
    set({ opencodeSessionId: sessionId });
  },
}));
