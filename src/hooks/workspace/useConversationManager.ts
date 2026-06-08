import { useState, useEffect, useCallback, useRef } from 'react';
import { useChatStore } from '@/stores';
import { db } from '@/db';
import { toast } from 'sonner';
import type { Conversation, } from '@/types/types';
import type { AgentInstance } from '@/stores';

export function useConversationManager(
  user: { id: string } | null,
  agentInstances: AgentInstance[],
  activeAgentId: string | null,
) {
  const [conversations, setConversations] = useState<Conversation[]>([]);
  const [conversationsLoaded, setConversationsLoaded] = useState(false);
  const [activeConversation, setActiveConversation] = useState<Conversation | null>(null);
  const setMessages = useChatStore((s) => s.setMessages);
  const setChatCurrentConversation = useChatStore((s) => s.setCurrentConversation);
  const creatingConversationRef = useRef(false);
  const initialConversationCreatingRef = useRef(false);

  const loadConversations = useCallback(async () => {
    if (!user) { return; }
    const data = await db.loadConversations(user.id, 50);
    setConversations(Array.isArray(data) ? data : []);
    setConversationsLoaded(true);
  }, [user]);

  const loadMessages = useCallback(async (conversationId: string) => {
    const data = await db.loadMessages(conversationId, 100);
    setMessages(Array.isArray(data) ? data : []);
  }, [setMessages]);

  // 初始加载数据
  useEffect(() => {
    loadConversations();
  }, [loadConversations]);

  // 同步 activeConversation 到 chat store
  useEffect(() => {
    setChatCurrentConversation(activeConversation?.id ?? null);
  }, [activeConversation?.id, setChatCurrentConversation]);

  // 初始化主会话
  useEffect(() => {
    if (!user || !conversationsLoaded || activeConversation || agentInstances.length === 0 || initialConversationCreatingRef.current) {
      return;
    }

    let cancelled = false;
    const initConversation = async () => {
      initialConversationCreatingRef.current = true;
      try {
        const createForAgentId = localStorage.getItem('create_conversation_for_agent_id');
        const targetInstance = agentInstances.find((instance) => instance.id === createForAgentId)
          || agentInstances.find((instance) => instance.id === activeAgentId)
          || agentInstances[0];

        if (!createForAgentId && conversations.length > 0) {
          const latestConversation = conversations[0];
          if (!cancelled) {
            setActiveConversation(latestConversation);
            await loadMessages(latestConversation.id);
          }
          return;
        }

        const data = await db.createConversation({
          user_id: user.id,
          title: '新会话',
          agent: targetInstance.agentKey,
          model: 'gpt-4',
        });

        if (!cancelled && data) {
          localStorage.removeItem('create_conversation_for_agent_id');
          setConversations((prev) => (prev.some((conv) => conv.id === data.id) ? prev : [data, ...prev]));
          setActiveConversation(data);
          setMessages([]);
        }
      } finally {
        initialConversationCreatingRef.current = false;
      }
    };

    initConversation();
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [user, conversationsLoaded, activeConversation, agentInstances, activeAgentId, conversations, loadMessages, setMessages]);

  const handleSelectConversation = useCallback(
    async (conv: Conversation) => {
      setActiveConversation(conv);
      await loadMessages(conv.id);
    },
    [loadMessages]
  );

  const handleDoubleClickConversation = useCallback(
    async (conv: Conversation, onActivateAgent: (id: string) => void) => {
      setActiveConversation(conv);
      await loadMessages(conv.id);
      const agentInstance = agentInstances.find((a) => a.agentKey === conv.agent);
      if (agentInstance) {
        onActivateAgent(agentInstance.id);
      }
    },
    [agentInstances, loadMessages]
  );

  const handleNewConversation = useCallback(
    async (agentKey: string) => {
      if (!user || creatingConversationRef.current) { return null; }
      creatingConversationRef.current = true;
      try {
        const data = await db.createConversation({
          user_id: user.id,
          title: '新会话',
          agent: agentKey,
          model: 'gpt-4',
        });
        if (!data) {
          toast.error('创建会话失败');
          return null;
        }
        setConversations((prev) => [data, ...prev]);
        setActiveConversation(data);
        setMessages([]);
        return data;
      } finally {
        creatingConversationRef.current = false;
      }
    },
    [user, setMessages]
  );

  return {
    conversations,
    setConversations,
    activeConversation,
    setActiveConversation,
    conversationsLoaded,
    loadConversations,
    loadMessages,
    handleSelectConversation,
    handleDoubleClickConversation,
    handleNewConversation,
  };
}
