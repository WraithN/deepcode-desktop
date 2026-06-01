import type { AgentAdapter } from './types';

class AgentRegistry {
  private adapters = new Map<string, AgentAdapter>();

  register(adapter: AgentAdapter) {
    this.adapters.set(adapter.agentKey, adapter);
  }

  get(agentKey: string): AgentAdapter | undefined {
    return this.adapters.get(agentKey);
  }

  has(agentKey: string): boolean {
    return this.adapters.has(agentKey);
  }
}

export const agentRegistry = new AgentRegistry();

import { OpencodeAdapter } from './opencode/adapter';
agentRegistry.register(new OpencodeAdapter());
