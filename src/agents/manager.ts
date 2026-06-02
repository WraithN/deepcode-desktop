// 过渡版本：保留接口但委托给 Rust AgentService
import { agentSendMessage, agentStopInstance } from '@/hooks/use-agent-service';

class AgentManager {
  async sendMessage(instanceId: string, message: string, conversationId: string) {
    await agentSendMessage(instanceId, message, conversationId);
  }

  async stopAgent(instanceId: string) {
    await agentStopInstance(instanceId);
  }
}

export const agentManager = new AgentManager();
