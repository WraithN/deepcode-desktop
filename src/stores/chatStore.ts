import { create } from 'zustand';
import { useWebSocketStore } from './websocketStore';
import type { Message, MessageStep, AskQuestion } from '@/types/types';

export type AgentEvent =
  | { type: 'thinking'; content: string }
  | { type: 'text_delta'; content: string }
  | { type: 'tool_use'; toolName: string; args: unknown }
  | { type: 'tool_result'; toolName: string; result: string; failed: boolean }
  | { type: 'ask_permission'; message: string; toolName: string }
  | { type: 'ask_user'; questions: string[] }
  | { type: 'error'; message: string }
  | { type: 'done' };

export type StreamEvent =
  | { method: 'agent.thinking'; params: { content?: string } }
  | { method: 'agent.token'; params: { text?: string } }
  | { method: 'agent.done'; params: { sessionID?: string } }
  | { method: 'agent.error'; params: { message?: string } };

interface ChatState {
  conversations: Array<{ id: string; title: string }>;
  currentConversationId: string | null;
  opencodeSessionId: string | null;
  messages: Message[];
  isStreaming: boolean;
  isTyping: boolean;
  activeInstanceId: string | null;

  sendMessage: (content: string) => Promise<void>;
  appendEvent: (event: AgentEvent) => void;
  handleStreamEvent: (event: StreamEvent) => void;
  loadConversation: (conversationId: string) => Promise<void>;
  setCurrentConversation: (conversationId: string | null) => void;
  setActiveInstanceId: (instanceId: string | null) => void;
  setMessages: (messages: Message[] | ((prev: Message[]) => Message[])) => void;
  setConversations: (conversations: Array<{ id: string; title: string }>) => void;
  setIsStreaming: (isStreaming: boolean) => void;
  setIsTyping: (isTyping: boolean) => void;
  setOpencodeSessionId: (sessionId: string | null) => void;
  appendToken: (token: string) => void;
}

function agentEventToMessageStep(event: AgentEvent): MessageStep | null {
  switch (event.type) {
    case 'thinking':
      return { type: 'thinking', content: event.content };
    case 'tool_use': {
      const args = event.args as Record<string, unknown> | undefined;
      return {
        type: 'tool_use',
        content: `使用工具 ${event.toolName}...`,
        toolName: event.toolName,
        summary: { file: args?.file as string | undefined, lines: 0, durationMs: 0 },
      };
    }
    case 'tool_result':
      return { type: 'tool_result', content: event.result, toolName: event.toolName, failed: event.failed };
    case 'ask_permission':
      return { type: 'ask_permission', content: event.message, permissionType: event.toolName };
    case 'ask_user': {
      const questions: AskQuestion[] = event.questions.map((q, i) => ({
        id: `q-${i}`,
        label: q,
        type: 'custom',
      }));
      return { type: 'ask_user', content: '请选择一个选项或输入自定义答案', questions };
    }
    case 'text_delta':
    case 'error':
    case 'done':
      return null;
    default:
      return null;
  }
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
    const ws = useWebSocketStore.getState();
    const { activeInstanceId } = get();

    if (!activeInstanceId) {
      throw new Error('No active agent instance');
    }

    const conversationId = get().currentConversationId || `conv-${Date.now()}`;

    // Add user message immediately
    const userMessage: Message = {
      id: `msg-${Date.now()}`,
      conversation_id: conversationId,
      role: 'user',
      content,
      created_at: new Date().toISOString(),
    };

    set((state) => ({
      messages: [...state.messages, userMessage],
      isStreaming: true,
      isTyping: true,
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
        if (lastMessage && lastMessage.role === 'assistant') {
          messages[messages.length - 1] = { ...lastMessage, is_complete: true };
        }
        return { isStreaming: false, isTyping: false, messages };
      }

      if (event.type === 'error') {
        return {
          isStreaming: false,
          isTyping: false,
          messages: [
            ...messages,
            {
              id: `msg-${Date.now()}`,
              conversation_id: state.currentConversationId || '',
              role: 'assistant',
              content: `Error: ${event.message}`,
              is_complete: true,
              created_at: new Date().toISOString(),
            },
          ],
        };
      }

      if (event.type === 'text_delta') {
        if (lastMessage && lastMessage.role === 'assistant' && !lastMessage.is_complete) {
          messages[messages.length - 1] = {
            ...lastMessage,
            content: (lastMessage.content || '') + event.content,
          };
          return { messages };
        }
        // Create new streaming assistant message
        const assistantMessage: Message = {
          id: `msg-${Date.now()}`,
          conversation_id: state.currentConversationId || '',
          role: 'assistant',
          content: event.content,
          is_complete: false,
          created_at: new Date().toISOString(),
        };
        return { messages: [...messages, assistantMessage] };
      }

      const step = agentEventToMessageStep(event);
      if (!step) return { messages };

      if (lastMessage && lastMessage.role === 'assistant' && !lastMessage.is_complete) {
        const updatedLastMessage = {
          ...lastMessage,
          steps: [...(lastMessage.steps || []), step],
        };
        messages[messages.length - 1] = updatedLastMessage;
        return { messages };
      }

      // Create new assistant message with step
      const assistantMessage: Message = {
        id: `msg-${Date.now()}`,
        conversation_id: state.currentConversationId || '',
        role: 'assistant',
        content: '',
        steps: [step],
        is_complete: false,
        created_at: new Date().toISOString(),
      };
      return { messages: [...messages, assistantMessage] };
    });
  },

  handleStreamEvent: (event: StreamEvent) => {
    const { appendToken, setIsTyping, setIsStreaming, setOpencodeSessionId } = get();

    switch (event.method) {
      case 'agent.thinking':
        // 可以记录思考内容，保持 isTyping = true
        break;

      case 'agent.token': {
        const params = event.params as { text?: string };
        if (params.text) {
          // 第一个 token 到达，关闭 isTyping
          setIsTyping(false);
          appendToken(params.text);
        }
        break;
      }

      case 'agent.done': {
        const params = event.params as { sessionID?: string };
        if (params.sessionID) {
          setOpencodeSessionId(params.sessionID);
        }
        setIsStreaming(false);
        setIsTyping(false);

        // 标记最后一条消息完成
        set((state) => {
          const msgs = [...state.messages];
          const lastMsg = msgs[msgs.length - 1];
          if (lastMsg && lastMsg.role === 'assistant') {
            lastMsg.is_complete = true;
          }
          return { messages: msgs };
        });
        break;
      }

      case 'agent.error': {
        const params = event.params as { message?: string };
        console.error('Agent error:', params.message);
        setIsStreaming(false);
        setIsTyping(false);
        break;
      }
    }
  },

  appendToken: (token: string) => {
    set((state) => {
      const msgs = [...state.messages];
      const lastMsg = msgs[msgs.length - 1];

      if (lastMsg && lastMsg.role === 'assistant' && !lastMsg.is_complete) {
        lastMsg.content = (lastMsg.content || '') + token;
      }

      return { messages: msgs };
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
