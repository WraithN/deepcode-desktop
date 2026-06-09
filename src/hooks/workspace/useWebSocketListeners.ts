import { useEffect } from 'react';
import { useWebSocketStore, useChatStore, useAgentStore, useLogStore } from '@/stores';
import { invoke } from '@tauri-apps/api/core';
import { setSessionWsBaseUrl } from '@/stores';

export function useWebSocketListeners() {
  const logStoreAppend = useLogStore((s) => s.appendLog);

  const debugLog = (message: string, detail?: Record<string, unknown>) => {
    void invoke('console_logs', {
      logs: [{ type: 'info', message: `[WS DEBUG] ${message}${detail ? ` ${JSON.stringify(detail)}` : ''}` }]
    }).catch((e) => {
      console.error('Failed to send debug log:', e);
    });
  };

  // 初始化 WebSocket 连接
  useEffect(() => {
    const init = async () => {
      try {
        console.log('[useWebSocketListeners] getting WebSocket URL...');
        const url = await invoke<string>('get_websocket_url');
        debugLog(`WebSocket URL: ${url}`);
        console.log('[useWebSocketListeners] WS URL:', url);
        const wsUrl = url.replace(/^ws:\/\//, '');
        setSessionWsBaseUrl(`ws://${wsUrl}`);
        console.log('[useWebSocketListeners] connecting to WS...');
        await useWebSocketStore.getState().connect(url);
        debugLog('WebSocket connected');
        console.log('[useWebSocketListeners] WS connected successfully');
      } catch (e) {
        console.error('Failed to connect WebSocket:', e);
        debugLog(`WebSocket connect failed: ${String(e)}`);
      }
    };
    init();
  }, []);

  // 订阅状态与日志事件
  useEffect(() => {
    const wsStore = useWebSocketStore.getState();

    const unsubStatus = wsStore.subscribe('agent.status', (params: unknown) => {
      const { instanceId, status, pid } = params as { instanceId: string; status: string; pid?: number };
      useAgentStore.getState().updateInstanceStatus(instanceId, status as import('@/stores').AgentInstance['status'], pid);
    });

    const unsubLogs = wsStore.subscribe('session.log', (params: unknown) => {
      logStoreAppend(params as Parameters<typeof logStoreAppend>[0]);
    });

    return () => {
      unsubStatus();
      unsubLogs();
    };
  }, []);

  // 订阅智能体交互事件
  useEffect(() => {
    const wsStore = useWebSocketStore.getState();

    const unsubToken = wsStore.subscribe('agent.token', (params) => {
      const p = params as Record<string, unknown>;
      const text = (p.text as string) || '';
      debugLog(`[agent.token] text="${text}"`);
      if (text) {
        useChatStore.getState().appendToken(text);
      }
    });

    const seenThinkingPartIds = new Set<string>();

    const unsubThinking = wsStore.subscribe('agent.thinking', (params) => {
      const activeId = useAgentStore.getState().activeInstanceId;
      if (!activeId) { return; }
      const instances = useAgentStore.getState().instances;
      const active = instances.find((i) => i.id === activeId);
      if (!active) { return; }
      const agentKey = active.agentKey;
      const raw = localStorage.getItem('agent_type_configs');
      let showThinking = true;
      if (raw) {
        try {
          const configs = JSON.parse(raw) as Record<string, { showThinking?: boolean }>;
          showThinking = configs[agentKey]?.showThinking !== false;
        } catch { /* ignore */ }
      }
      if (!showThinking) {
        return;
      }
      const p = params as Record<string, unknown>;
      const content = (p.content as string) || (p.text as string) || '';
      const partId = (p.id as string) || '';
      if (partId && seenThinkingPartIds.has(partId)) {
        return;
      }
      if (partId) {
        seenThinkingPartIds.add(partId);
      }
      useChatStore.getState().setMessages((prev) => {
        const lastMsg = prev[prev.length - 1];
        if (!lastMsg || lastMsg.role !== 'assistant') { return prev; }
        const alreadyHas = partId && lastMsg.steps?.some((s) => s.partId === partId);
        if (alreadyHas) { return prev; }
        return prev.map((msg, idx) =>
          idx === prev.length - 1
            ? {
                ...msg,
                steps: [
                  { type: 'thinking' as const, content: content || 'AI 正在思考...', partId: partId || undefined },
                  ...(msg.steps || []).filter((s) => s.type !== 'thinking'),
                ],
              }
            : msg
        );
      });
    });

    const unsubQuestion = wsStore.subscribe('agent.question', (params) => {
      const p = params as Record<string, unknown>;
      const interaction = p.interaction as Record<string, unknown>;
      const sessionId = (p.sessionID as string) || '';
      useChatStore.getState().setPendingInteraction({ sessionId, type: 'question', payload: interaction });
      useChatStore.getState().setMessages((prev) => {
        const lastMsg = prev[prev.length - 1];
        if (!lastMsg || lastMsg.role !== 'assistant') { return prev; }
        return prev.map((msg, idx) =>
          idx === prev.length - 1
            ? {
                ...msg,
                steps: [
                  ...(msg.steps || []),
                  {
                    type: 'ask_user' as const,
                    content: '用户确认',
                    interaction: interaction as unknown as import('@/types/types').InteractionPayload,
                  },
                ],
              }
            : msg
        );
      });
    });

    const unsubPermission = wsStore.subscribe('agent.permission', (params) => {
      const p = params as Record<string, unknown>;
      const interaction = p.interaction as Record<string, unknown>;
      const sessionId = (p.sessionID as string) || '';
      useChatStore.getState().setPendingInteraction({ sessionId, type: 'permission', payload: interaction });
      useChatStore.getState().setMessages((prev) => {
        const lastMsg = prev[prev.length - 1];
        if (!lastMsg || lastMsg.role !== 'assistant') { return prev; }
        return prev.map((msg, idx) =>
          idx === prev.length - 1
            ? {
                ...msg,
                steps: [
                  ...(msg.steps || []),
                  {
                    type: 'ask_permission' as const,
                    content: (interaction.action as string) || '权限询问',
                    interaction: interaction as unknown as import('@/types/types').InteractionPayload,
                  },
                ],
              }
            : msg
        );
      });
    });

    const unsubTodo = wsStore.subscribe('agent.todowrite', (params) => {
      const p = params as Record<string, unknown>;
      const interaction = p.interaction as Record<string, unknown>;
      const newTodos = (interaction.todos as import('@/types/types').TodoItem[]) || [];
      useChatStore.getState().setTodos(newTodos);
      useChatStore.getState().setMessages((prev) => {
        const lastMsg = prev[prev.length - 1];
        if (!lastMsg || lastMsg.role !== 'assistant') { return prev; }
        return prev.map((msg, idx) =>
          idx === prev.length - 1
            ? {
                ...msg,
                steps: [
                  ...(msg.steps || []),
                  {
                    type: 'tool_result' as const,
                    content: '任务列表更新',
                    toolName: 'todowrite',
                    interaction: interaction as unknown as import('@/types/types').InteractionPayload,
                  },
                ],
              }
            : msg
        );
      });
    });

    const unsubDone = wsStore.subscribe('agent.done', (params) => {
      debugLog('[agent.done] received', { params });
      const p = params as Record<string, unknown>;
      const sessionID = (p.sessionID as string) || '';
      useChatStore.getState().setMessages((prev) => {
        const lastMsg = prev[prev.length - 1];
        debugLog('[agent.done] lastMsg check', { role: lastMsg?.role, id: lastMsg?.id });
        if (!lastMsg || lastMsg.role !== 'assistant') {
          debugLog(`[agent.done] lastMsg is not assistant, role=${lastMsg?.role}`);
          return prev;
        }
        debugLog(`[agent.done] setting is_complete=true for msg ${lastMsg.id}`);
        return prev.map((msg, idx) =>
          idx === prev.length - 1 ? { ...msg, is_complete: true } : msg
        );
      });
      useChatStore.getState().setIsStreaming(false);
      useChatStore.getState().setIsTyping(false);
      if (sessionID) {
        useChatStore.getState().setOpencodeSessionId(sessionID);
      }
    });

    const unsubError = wsStore.subscribe('agent.error', (params) => {
      const p = params as Record<string, unknown>;
      const message = (p.message as string) || '处理出错';
      console.error('[WS agent.error]', message);
      debugLog(`[agent.error] ${message}`);
      useChatStore.getState().setMessages((prev) => {
        const lastMsg = prev[prev.length - 1];
        if (!lastMsg || lastMsg.role !== 'assistant') { return prev; }
        return prev.map((msg, idx) =>
          idx === prev.length - 1
            ? { ...msg, is_complete: true, content: msg.content || `出错: ${message}` }
            : msg
        );
      });
      useChatStore.getState().setIsStreaming(false);
      useChatStore.getState().setIsTyping(false);
    });

    return () => {
      unsubToken();
      unsubThinking();
      unsubQuestion();
      unsubPermission();
      unsubTodo();
      unsubDone();
      unsubError();
    };
  }, []);
}
