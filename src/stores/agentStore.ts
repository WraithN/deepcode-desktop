import { create } from 'zustand';
import { useWebSocketStore } from './websocketStore';

export interface AgentInstance {
  instanceId: string;
  status: 'stopped' | 'starting' | 'running' | 'crashed';
  pluginKey: string;
  name: string;
  workspace: string;
  pid?: number;
}

interface AgentState {
  instances: AgentInstance[];
  activeInstanceId: string | null;

  createInstance: (config: { pluginKey: string; name: string; workspace: string }) => Promise<AgentInstance>;
  stopInstance: (instanceId: string) => Promise<void>;
  setActiveInstance: (instanceId: string | null) => void;
  updateInstanceStatus: (instanceId: string, status: AgentInstance['status'], pid?: number) => void;
  removeInstance: (instanceId: string) => void;
}

export const useAgentStore = create<AgentState>((set, get) => ({
  instances: [],
  activeInstanceId: null,

  createInstance: async (config) => {
    const ws = useWebSocketStore.getState();
    const result = await ws.sendRequest<AgentInstance>('agent.createInstance', config);

    set((state) => ({
      instances: [...state.instances, result],
      activeInstanceId: result.instanceId,
    }));

    return result;
  },

  stopInstance: async (instanceId) => {
    const ws = useWebSocketStore.getState();
    await ws.sendRequest('agent.stopInstance', { instanceId });

    set((state) => ({
      instances: state.instances.filter((i) => i.instanceId !== instanceId),
      activeInstanceId: state.activeInstanceId === instanceId ? null : state.activeInstanceId,
    }));
  },

  setActiveInstance: (instanceId) => {
    set({ activeInstanceId: instanceId });
  },

  updateInstanceStatus: (instanceId, status, pid) => {
    set((state) => ({
      instances: state.instances.map((i) =>
        i.instanceId === instanceId ? { ...i, status, ...(pid && { pid }) } : i
      ),
    }));
  },

  removeInstance: (instanceId) => {
    set((state) => ({
      instances: state.instances.filter((i) => i.instanceId !== instanceId),
      activeInstanceId: state.activeInstanceId === instanceId ? null : state.activeInstanceId,
    }));
  },
}));
