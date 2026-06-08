import { useRef, useCallback } from 'react';
import { useChatStore, useLogStore } from '@/stores';
import { db } from '@/db';
import { toast } from 'sonner';
import { sessionLogger } from '@/services/logger';
import type { Conversation } from '@/types/types';

export function useMessageHandlers(activeConversation: Conversation | null, activeAgentId: string | null) {
  const sendingMessageRef = useRef(false);
  const setMessages = useChatStore((s) => s.setMessages);
  const setIsStreaming = useChatStore((s) => s.setIsStreaming);
  const chatSendMessage = useChatStore((s) => s.sendMessage);
  const logStoreAppend = useLogStore((s) => s.appendLog);

  const handleSendMessage = useCallback(
    async (content: string) => {
      if (sendingMessageRef.current) { return; }
      if (!activeConversation) {
        toast.error('请先创建或选择一个会话');
        return;
      }

      sendingMessageRef.current = true;
      let streamingMessageId: string | null = null;

      try {
        const userMsg = await db.createMessage({
          conversation_id: activeConversation.id,
          role: 'user',
          content,
        });

        if (!userMsg) {
          toast.error('发送消息失败');
          return;
        }

        setMessages((prev) => [...prev, userMsg]);
        setIsStreaming(true);

        const streamingMsg = {
          id: `streaming-${Date.now()}`,
          conversation_id: activeConversation.id,
          role: 'assistant' as const,
          content: '',
          steps: [],
          is_complete: false,
          created_at: new Date().toISOString(),
        };
        streamingMessageId = streamingMsg.id;
        setMessages((prev) => [...prev, streamingMsg]);

        const result = await chatSendMessage(content);
        if (result.text) {
          setMessages((prev) =>
            prev.map((msg) =>
              msg.id === streamingMsg.id ? { ...msg, content: result.text, is_complete: true } : msg
            )
          );
        }
      } catch (error) {
        const errMsg = error instanceof Error ? error.message : String(error);
        const stack = error instanceof Error ? error.stack : undefined;
        logStoreAppend({
          id: `log-${Date.now()}`,
          conversationId: activeConversation.id,
          instanceId: activeAgentId || undefined,
          timestamp: new Date().toLocaleTimeString(),
          level: 'error',
          source: 'chat',
          message: `通信错误: ${errMsg}`,
          detail: { stack },
        });
        void sessionLogger.logError(error instanceof Error ? error : errMsg, {
          conversationId: activeConversation.id,
          instanceId: activeAgentId,
          message: content,
        }).catch(() => undefined);
        toast.error('通信错误，详情已写入 Session Logs');
        if (streamingMessageId) {
          setMessages((prev) => prev.filter((msg) => msg.id !== streamingMessageId));
        }
      } finally {
        sendingMessageRef.current = false;
        setIsStreaming(false);
        useChatStore.getState().setIsTyping(false);
      }
    },
    [activeConversation, activeAgentId, setMessages, setIsStreaming, chatSendMessage, logStoreAppend]
  );

  const handleAnswerPermission = useCallback(async (_stepIndex: number, answer: 'once' | 'session' | 'deny') => {
    const label = answer === 'once' ? '本次同意' : answer === 'session' ? '本Session同意' : '不同意';
    toast.success(`已${label}`);
    try {
      await useChatStore.getState().sendInteractionResponse({ answer });
    } catch (e) {
      console.error('Failed to send permission response:', e);
      toast.error('发送回答失败');
    }
  }, []);

  const handleAnswerUserQuestions = useCallback(async (_stepIndex: number, answers: Record<string, string | string[]>) => {
    toast.success('已提交回答');
    try {
      const answerValues = Object.values(answers).flat();
      await useChatStore.getState().sendInteractionResponse({ answers: answerValues });
    } catch (e) {
      console.error('Failed to send question response:', e);
      toast.error('发送回答失败');
    }
  }, []);

  const handleRetryStep = useCallback((messageId: string, stepIndex: number) => {
    toast.success('正在重试...');
    setMessages((prev) =>
      prev.map((msg) => {
        if (msg.id !== messageId || !msg.steps) { return msg; }
        const newSteps = [...msg.steps];
        if (newSteps[stepIndex]) {
          newSteps[stepIndex] = { ...newSteps[stepIndex], failed: false, type: 'tool_use' };
        }
        return { ...msg, steps: newSteps };
      })
    );
  }, [setMessages]);

  const handleEditUserMessage = useCallback((content: string) => {
    toast.info('已加载到输入框，你可以修改后重新发送');
    return content;
  }, []);

  return {
    handleSendMessage,
    handleAnswerPermission,
    handleAnswerUserQuestions,
    handleRetryStep,
    handleEditUserMessage,
  };
}
