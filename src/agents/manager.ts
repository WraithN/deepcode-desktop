import type { AgentAdapter, AgentEvent, AgentStatus } from './types';
import { agentRegistry } from './registry';

export interface ManagedAgent {
  instanceId: string;
  agentKey: string;
  displayName: string;
  workspace: string;
  status: AgentStatus;
  adapter: AgentAdapter;
  sessionId?: string;
}

type Listener = () => void;

class AgentManager {
  private agents = new Map<string, ManagedAgent>();
  private listeners = new Set<Listener>();

  subscribe(listener: Listener): () => void {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  }

  private notify(): void {
    for (const listener of this.listeners) {
      listener();
    }
  }

  getAgents(): ManagedAgent[] {
    return Array.from(this.agents.values());
  }

  getAgent(instanceId: string): ManagedAgent | undefined {
    return this.agents.get(instanceId);
  }

  async addAgent(
    agentKey: string,
    instanceId: string,
    displayName: string,
    workspace: string,
  ): Promise<void> {
    const adapter = agentRegistry.get(agentKey);
    if (!adapter) {
      throw new Error(`未知智能体类型: ${agentKey}`);
    }

    const installed = await adapter.isInstalled();
    if (!installed) {
      throw new Error('智能体尚未安装');
    }

    const managedAgent: ManagedAgent = {
      instanceId,
      agentKey,
      displayName,
      workspace,
      status: { state: 'stopped' },
      adapter,
    };

    this.agents.set(instanceId, managedAgent);
    this.notify();
  }

  async startAgent(instanceId: string): Promise<void> {
    const managedAgent = this.agents.get(instanceId);
    if (!managedAgent) {
      throw new Error(`智能体实例不存在: ${instanceId}`);
    }

    managedAgent.status = { state: 'starting' };
    this.notify();

    try {
      await managedAgent.adapter.start({
        instanceId,
        workspace: managedAgent.workspace,
      });

      const status = await managedAgent.adapter.getStatus(instanceId);
      managedAgent.status = status;
    } catch (error) {
      managedAgent.status = {
        state: 'crashed',
        error: error instanceof Error ? error.message : String(error),
      };
    }

    this.notify();
  }

  async stopAgent(instanceId: string): Promise<void> {
    const managedAgent = this.agents.get(instanceId);
    if (!managedAgent) {
      throw new Error(`智能体实例不存在: ${instanceId}`);
    }

    await managedAgent.adapter.stop(instanceId);
    managedAgent.status = { state: 'stopped' };
    this.notify();
  }

  async removeAgent(instanceId: string): Promise<void> {
    const managedAgent = this.agents.get(instanceId);
    if (!managedAgent) {
      return;
    }

    if (managedAgent.status.state === 'running' || managedAgent.status.state === 'starting') {
      try {
        await managedAgent.adapter.stop(instanceId);
      } catch {
        // 忽略停止时的错误，强制移除
      }
    }

    this.agents.delete(instanceId);
    this.notify();
  }

  async *sendMessage(instanceId: string, message: string): AsyncGenerator<AgentEvent> {
    const managedAgent = this.agents.get(instanceId);
    if (!managedAgent) {
      throw new Error(`智能体实例不存在: ${instanceId}`);
    }

    const options = {
      workspace: managedAgent.workspace,
      sessionId: managedAgent.sessionId,
      continueSession: !!managedAgent.sessionId,
    };

    for await (const event of managedAgent.adapter.sendMessage(instanceId, message, options)) {
      yield event;
    }
  }

  async setMode(instanceId: string, mode: 'build' | 'plan'): Promise<void> {
    const managedAgent = this.agents.get(instanceId);
    if (!managedAgent) {
      throw new Error(`智能体实例不存在: ${instanceId}`);
    }

    await managedAgent.adapter.setMode(instanceId, mode);
  }
}

export const agentManager = new AgentManager();
