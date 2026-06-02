import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type { AgentEvent } from '@/agents/types';

export interface PluginInfo {
  key: string;
  name: string;
  installed: boolean;
}

export interface InstanceInfo {
  id: string;
  plugin_key: string;
  name: string;
  workspace: string;
  status: 'stopped' | 'starting' | 'running' | { crashed: string };
}

export interface AgentEventPayload {
  instance_id: string;
  event: AgentEvent;
}

export interface StatusChangePayload {
  instance_id: string;
  status: InstanceInfo['status'];
}

export async function agentListPlugins(): Promise<PluginInfo[]> {
  return invoke('agent_list_plugins');
}

export async function agentCreateInstance(
  pluginKey: string,
  name: string,
  workspace: string,
): Promise<InstanceInfo> {
  return invoke('agent_create_instance', {
    plugin_key: pluginKey,
    name,
    workspace,
  });
}

export async function agentSendMessage(
  instanceId: string,
  message: string,
  conversationId: string,
): Promise<void> {
  return invoke('agent_send_message', {
    instance_id: instanceId,
    message,
    conversation_id: conversationId,
  });
}

export async function agentStopInstance(instanceId: string): Promise<void> {
  return invoke('agent_stop_instance', { instance_id: instanceId });
}

export async function agentGetInstance(instanceId: string): Promise<InstanceInfo> {
  return invoke('agent_get_instance', { instance_id: instanceId });
}

export async function agentListInstances(): Promise<InstanceInfo[]> {
  return invoke('agent_list_instances');
}

export async function listenAgentEvents(
  callback: (payload: AgentEventPayload) => void,
): Promise<UnlistenFn> {
  return listen<AgentEventPayload>('agent:event', (event) => {
    callback(event.payload);
  });
}

export async function listenAgentStatusChanges(
  callback: (payload: StatusChangePayload) => void,
): Promise<UnlistenFn> {
  return listen<StatusChangePayload>('agent:status_changed', (event) => {
    callback(event.payload);
  });
}
