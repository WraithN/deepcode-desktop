import { useEffect, useCallback } from 'react';
import { useAgentStore, useChatStore } from '@/stores';
import type { AgentInstance } from '@/stores';
import { invoke } from '@tauri-apps/api/core';
import { generateShortId, formatIdShort } from '@/lib/id';
import { toast } from 'sonner';
import { db } from '@/db';
import type { Conversation } from '@/types/types';

const defaultAgents: AgentInstance[] = [
  { id: 'default-1', agentKey: 'opencode', displayName: '小智', workspace: '.', modelConfig: { type: 'builtin', modelId: 'gpt-4' }, status: 'stopped' },
  { id: 'default-2', agentKey: 'claude-code', displayName: '小文', workspace: '.', modelConfig: { type: 'builtin', modelId: 'claude-3-opus' }, status: 'stopped' },
  { id: 'default-3', agentKey: 'cursor-agent', displayName: '小游', workspace: '.', modelConfig: { type: 'builtin', modelId: 'deepseek-v3' }, status: 'stopped' },
  { id: 'default-4', agentKey: 'codex', displayName: '小柯', workspace: '.', modelConfig: { type: 'builtin', modelId: 'gpt-4' }, status: 'stopped' },
  { id: 'default-5', agentKey: 'custom', displayName: '小C', workspace: '.', modelConfig: { type: 'builtin', modelId: 'gpt-4' }, status: 'stopped' },
];

function getStoredAgents(): AgentInstance[] {
  try {
    const raw = localStorage.getItem('agent_instances');
    if (raw) {
      const parsed = JSON.parse(raw) as AgentInstance[];
      return parsed.map((a) => ({
        ...a,
        workspace: a.workspace || '.',
        modelConfig: a.modelConfig || { type: 'builtin', modelId: 'gpt-4' },
        status: a.status || 'stopped',
      }));
    }
  } catch { /* ignore */ }
  return defaultAgents;
}

function getStoredActiveAgentId(): string {
  return localStorage.getItem('active_agent_id') || defaultAgents[0].id;
}

export function useAgentManagement(user: { id: string } | null) {
  const storeInstances = useAgentStore((s) => s.instances);
  const setAgentInstances = useAgentStore((s) => s.setInstances);
  const addAgentInstance = useAgentStore((s) => s.addInstance);
  const removeAgentInstance = useAgentStore((s) => s.removeInstance);
  const updateAgentInstance = useAgentStore((s) => s.updateInstance);
  const activeAgentId = useAgentStore((s) => s.activeInstanceId);
  const setActiveAgentId = useAgentStore((s) => s.setActiveInstance);
  const setChatActiveInstanceId = useChatStore((s) => s.setActiveInstanceId);
  const setMessages = useChatStore((s) => s.setMessages);

  // 从 localStorage 加载智能体
  useEffect(() => {
    const stored = getStoredAgents();
    if (stored.length > 0) {
      setAgentInstances(stored);
      const activeId = getStoredActiveAgentId();
      setActiveAgentId(activeId);
      setChatActiveInstanceId(activeId);
    }
  }, [setAgentInstances, setActiveAgentId, setChatActiveInstanceId]);

  const agentInstances = storeInstances;
  const activeAgent = agentInstances.find((a) => a.id === activeAgentId) || agentInstances[0] || defaultAgents[0];

  // 解析工作目录
  useEffect(() => {
    const resolveWorkspaces = async () => {
      const needsResolve = agentInstances.some((a) => a.workspace === '.' || !a.workspace.startsWith('/'));
      if (!needsResolve) { return; }
      try {
        const cwd = await invoke<string>('get_current_dir');
        for (const a of agentInstances) {
          if (a.workspace === '.' || a.workspace === '') {
            updateAgentInstance(a.id, { workspace: cwd });
          } else if (!a.workspace.startsWith('/')) {
            updateAgentInstance(a.id, { workspace: `${cwd}/${a.workspace}` });
          }
        }
      } catch {
        // ignore
      }
    };
    resolveWorkspaces();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // 持久化
  useEffect(() => {
    localStorage.setItem('agent_instances', JSON.stringify(agentInstances));
  }, [agentInstances]);

  useEffect(() => {
    if (activeAgentId) {
      localStorage.setItem('active_agent_id', activeAgentId);
    }
    if (activeAgent) {
      localStorage.setItem('selected_agent', activeAgent.agentKey);
    }
  }, [activeAgentId, activeAgent]);

  const handleAddAgent = useCallback(() => {
    // 仅返回标记，由调用方控制弹窗
    return true;
  }, []);

  const handleConfirmAddAgent = useCallback(
    async (agentKey: string, displayName: string, workspace: string) => {
      const instanceId = generateShortId();
      const newInstance: AgentInstance = {
        id: instanceId,
        agentKey,
        displayName,
        workspace: workspace || '.',
        modelConfig: { type: 'builtin', modelId: 'gpt-4' },
        status: 'stopped',
      };
      addAgentInstance(newInstance);
      toast.success(`已添加智能体 ${displayName} [${formatIdShort(instanceId)}]`);

      if (user) {
        const data = await db.createConversation({
          user_id: user.id,
          title: '新会话',
          agent: agentKey,
          model: 'gpt-4',
        });
        if (data) {
          return { newInstance, newConversation: data };
        }
      }
      return { newInstance, newConversation: null };
    },
    [addAgentInstance, user]
  );

  const handleDeleteAgent = useCallback(
    (instanceId: string) => {
      const instance = agentInstances.find((a) => a.id === instanceId);
      if (!instance) { return; }

      if (activeAgentId === instanceId) {
        setActiveAgentId(null);
        setChatActiveInstanceId(null);
      }

      removeAgentInstance(instanceId);
      toast.success(`已删除智能体 ${instance.displayName}`);
      return instance;
    },
    [agentInstances, activeAgentId, setActiveAgentId, setChatActiveInstanceId, removeAgentInstance]
  );

  const handleActivateAgent = useCallback(
    async (id: string, conversations: Conversation[]) => {
      setActiveAgentId(id);
      setChatActiveInstanceId(id);
      const instance = agentInstances.find((a) => a.id === id);
      if (!instance) { return null; }

      const agentConvs = conversations.filter((c) => c.agent === instance.agentKey);
      if (agentConvs.length > 0) {
        return { conversation: agentConvs[0], messages: [] as import('@/types/types').Message[] };
      }

      if (user) {
        const data = await db.createConversation({
          user_id: user.id,
          title: '新会话',
          agent: instance.agentKey,
          model: 'gpt-4',
        });
        if (data) {
          return { conversation: data, messages: [] as import('@/types/types').Message[] };
        }
      }
      return null;
    },
    [agentInstances, setActiveAgentId, setChatActiveInstanceId, user]
  );

  return {
    agentInstances,
    activeAgentId,
    activeAgent,
    setAgentInstances,
    setActiveAgentId,
    setChatActiveInstanceId,
    setMessages,
    handleAddAgent,
    handleConfirmAddAgent,
    handleDeleteAgent,
    handleActivateAgent,
    updateAgentInstance,
  };
}
