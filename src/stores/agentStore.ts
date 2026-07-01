import { create } from 'zustand';
import { useWebSocketStore } from './websocketStore';

export interface AgentModelConfig {
  type: 'builtin' | 'custom';
  modelId?: string;
  name?: string;
  url?: string;
  apiKey?: string;
}

export interface AgentInstance {
  id: string;
  agentKey: string;
  displayName: string;
  workspace: string;
  modelConfig?: AgentModelConfig;
  status: 'stopped' | 'starting' | 'running' | 'crashed';
  pid?: number;
}

interface AgentState {
  instances: AgentInstance[];
  activeInstanceId: string | null;

  createInstance: (config: { agentKey: string; displayName: string; workspace: string; modelConfig?: AgentModelConfig }) => Promise<AgentInstance>;
  stopInstance: (instanceId: string) => Promise<void>;
  setActiveInstance: (instanceId: string | null) => void;
  updateInstanceStatus: (instanceId: string, status: AgentInstance['status'], pid?: number) => void;
  removeInstance: (instanceId: string) => void;
  setInstances: (instances: AgentInstance[]) => void;
  addInstance: (instance: AgentInstance) => void;
  updateInstance: (instanceId: string, updates: Partial<AgentInstance>) => void;
}

function mapBackendInstance(data: Record<string, unknown>): AgentInstance {
  return {
    id: (data.instanceId as string) || (data.id as string) || '',
    agentKey: (data.agentKey as string) || (data.pluginKey as string) || '',
    displayName: (data.name as string) || (data.displayName as string) || '',
    workspace: (data.workDirectory as string) || (data.workspace as string) || '.',
    modelConfig: (data.modelConfig as AgentModelConfig | undefined),
    status: (data.status as AgentInstance['status']) || 'stopped',
    pid: data.pid as number | undefined,
  };
}

export const useAgentStore = create<AgentState>((set, get) => ({
  instances: [],
  activeInstanceId: null,

  createInstance: async (config) => {
    const ws = useWebSocketStore.getState();
    const result = await ws.sendRequest<Record<string, unknown>>('agent.createInstance', {
      agentKey: config.agentKey,
      name: config.displayName,
      workDirectory: config.workspace,
      modelConfig: config.modelConfig,
    });

    const mapped = mapBackendInstance(result);

    set((state) => ({
      instances: [...state.instances, mapped],
      activeInstanceId: mapped.id,
    }));

    return mapped;
  },

  stopInstance: async (instanceId) => {
    const ws = useWebSocketStore.getState();
    await ws.sendRequest('agent.stopInstance', { instanceId });

    set((state) => ({
      instances: state.instances.filter((i) => i.id !== instanceId),
      activeInstanceId: state.activeInstanceId === instanceId ? null : state.activeInstanceId,
    }));
  },

  setActiveInstance: (instanceId) => {
    set({ activeInstanceId: instanceId });
  },

  updateInstanceStatus: (instanceId, status, pid) => {
    set((state) => ({
      instances: state.instances.map((i) =>
        i.id === instanceId ? { ...i, status, ...(pid !== undefined && { pid }) } : i
      ),
    }));
  },

  removeInstance: (instanceId) => {
    set((state) => ({
      instances: state.instances.filter((i) => i.id !== instanceId),
      activeInstanceId: state.activeInstanceId === instanceId ? null : state.activeInstanceId,
    }));
  },

  setInstances: (instances) => {
    set({ instances });
  },

  addInstance: (instance) => {
    set((state) => ({
      instances: [...state.instances, instance],
      activeInstanceId: instance.id,
    }));
  },

  updateInstance: (instanceId, updates) => {
    set((state) => ({
      instances: state.instances.map((i) =>
        i.id === instanceId ? { ...i, ...updates } : i
      ),
    }));
  },
}));
