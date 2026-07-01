import { describe, it, expect, vi, beforeEach } from 'vitest';
import { useAgentStore } from './agentStore';

// Reset store state before each test
const resetStore = () => {
  useAgentStore.setState({
    instances: [],
    activeInstanceId: null,
  });
};

// Mock websocket store
vi.mock('./websocketStore', () => ({
  useWebSocketStore: {
    getState: vi.fn(() => ({
      sendRequest: vi.fn(),
    })),
  },
}));

describe('agentStore', () => {
  beforeEach(() => {
    resetStore();
    vi.clearAllMocks();
  });

  describe('initial state', () => {
    it('should have empty instances and no active instance', () => {
      const state = useAgentStore.getState();
      expect(state.instances).toEqual([]);
      expect(state.activeInstanceId).toBeNull();
    });
  });

  describe('setActiveInstance', () => {
    it('should set active instance id', () => {
      useAgentStore.getState().setActiveInstance('agent-1');
      expect(useAgentStore.getState().activeInstanceId).toBe('agent-1');
    });

    it('should clear active instance when null is passed', () => {
      useAgentStore.getState().setActiveInstance('agent-1');
      useAgentStore.getState().setActiveInstance(null);
      expect(useAgentStore.getState().activeInstanceId).toBeNull();
    });
  });

  describe('addInstance', () => {
    it('should add instance and set it as active', () => {
      const instance = createMockInstance({ id: 'inst-1' });
      useAgentStore.getState().addInstance(instance);

      const state = useAgentStore.getState();
      expect(state.instances).toHaveLength(1);
      expect(state.instances[0].id).toBe('inst-1');
      expect(state.activeInstanceId).toBe('inst-1');
    });
  });

  describe('setInstances', () => {
    it('should replace all instances', () => {
      useAgentStore.getState().addInstance(createMockInstance({ id: 'inst-1' }));
      useAgentStore.getState().setInstances([
        createMockInstance({ id: 'inst-2' }),
        createMockInstance({ id: 'inst-3' }),
      ]);

      const state = useAgentStore.getState();
      expect(state.instances).toHaveLength(2);
      expect(state.instances.map((i) => i.id)).toEqual(['inst-2', 'inst-3']);
    });
  });

  describe('updateInstanceStatus', () => {
    it('should update instance status and pid', () => {
      useAgentStore.getState().addInstance(createMockInstance({ id: 'inst-1', status: 'stopped' }));
      useAgentStore.getState().updateInstanceStatus('inst-1', 'running', 1234);

      const instance = useAgentStore.getState().instances[0];
      expect(instance.status).toBe('running');
      expect(instance.pid).toBe(1234);
    });

    it('should not modify other instances', () => {
      useAgentStore.getState().addInstance(createMockInstance({ id: 'inst-1', status: 'stopped' }));
      useAgentStore.getState().addInstance(createMockInstance({ id: 'inst-2', status: 'stopped' }));
      useAgentStore.getState().updateInstanceStatus('inst-1', 'running');

      const state = useAgentStore.getState();
      expect(state.instances[0].status).toBe('running');
      expect(state.instances[1].status).toBe('stopped');
    });
  });

  describe('updateInstance', () => {
    it('should merge updates into existing instance', () => {
      useAgentStore.getState().addInstance(createMockInstance({ id: 'inst-1', displayName: 'Old Name' }));
      useAgentStore.getState().updateInstance('inst-1', { displayName: 'New Name' });

      const instance = useAgentStore.getState().instances[0];
      expect(instance.displayName).toBe('New Name');
      expect(instance.id).toBe('inst-1');
    });
  });

  describe('removeInstance', () => {
    it('should remove instance by id', () => {
      useAgentStore.getState().addInstance(createMockInstance({ id: 'inst-1' }));
      useAgentStore.getState().addInstance(createMockInstance({ id: 'inst-2' }));
      useAgentStore.getState().removeInstance('inst-1');

      const state = useAgentStore.getState();
      expect(state.instances).toHaveLength(1);
      expect(state.instances[0].id).toBe('inst-2');
    });

    it('should clear active instance if removed instance was active', () => {
      useAgentStore.getState().addInstance(createMockInstance({ id: 'inst-1' }));
      useAgentStore.getState().removeInstance('inst-1');

      expect(useAgentStore.getState().activeInstanceId).toBeNull();
    });

    it('should keep active instance if removed instance was not active', () => {
      useAgentStore.getState().addInstance(createMockInstance({ id: 'inst-1' }));
      useAgentStore.getState().addInstance(createMockInstance({ id: 'inst-2' }));
      useAgentStore.getState().setActiveInstance('inst-2');
      useAgentStore.getState().removeInstance('inst-1');

      expect(useAgentStore.getState().activeInstanceId).toBe('inst-2');
    });
  });

  describe('createInstance', () => {
    it('should create instance via websocket and add to store', async () => {
      const { useWebSocketStore } = await import('./websocketStore');
      const mockSendRequest = vi.fn().mockResolvedValue({
        instanceId: 'new-inst',
        agentKey: 'opencode',
        name: 'Test Agent',
        workDirectory: '/tmp',
        status: 'running',
      });
      vi.mocked(useWebSocketStore.getState).mockReturnValue({
        sendRequest: mockSendRequest,
      } as unknown as ReturnType<typeof useWebSocketStore.getState>);

      const result = await useAgentStore.getState().createInstance({
        agentKey: 'opencode',
        displayName: 'Test Agent',
        workspace: '/tmp',
      });

      expect(mockSendRequest).toHaveBeenCalledWith('agent.createInstance', {
        agentKey: 'opencode',
        name: 'Test Agent',
        workDirectory: '/tmp',
        modelConfig: undefined,
      });
      expect(result.id).toBe('new-inst');
      expect(useAgentStore.getState().instances).toHaveLength(1);
      expect(useAgentStore.getState().activeInstanceId).toBe('new-inst');
    });
  });

  describe('stopInstance', () => {
    it('should stop instance via websocket and remove from store', async () => {
      useAgentStore.getState().addInstance(createMockInstance({ id: 'inst-1' }));

      const { useWebSocketStore } = await import('./websocketStore');
      const mockSendRequest = vi.fn().mockResolvedValue({});
      vi.mocked(useWebSocketStore.getState).mockReturnValue({
        sendRequest: mockSendRequest,
      } as unknown as ReturnType<typeof useWebSocketStore.getState>);

      await useAgentStore.getState().stopInstance('inst-1');

      expect(mockSendRequest).toHaveBeenCalledWith('agent.stopInstance', { instanceId: 'inst-1' });
      expect(useAgentStore.getState().instances).toHaveLength(0);
      expect(useAgentStore.getState().activeInstanceId).toBeNull();
    });
  });
});

function createMockInstance(overrides: Partial<Parameters<typeof useAgentStore.getState>['0']['instances'][0]> = {}) {
  return {
    id: 'default-id',
    agentKey: 'opencode',
    displayName: 'Mock Agent',
    workspace: '.',
    status: 'stopped' as const,
    ...overrides,
  };
}
