import type { AgentEvent } from '../types';

export function parseOpencodeEvent(eventLine: string, dataLine: string): AgentEvent | null {
  const event = eventLine.replace('event: ', '').trim();
  const data = dataLine.replace('data: ', '').trim();

  try {
    const payload = JSON.parse(data);

    switch (event) {
      case 'thinking':
        return { type: 'thinking', content: payload.content || data };
      case 'tool_use':
        return {
          type: 'tool_use',
          toolName: payload.tool_name || payload.name || 'unknown',
          args: payload.args || payload.arguments || {},
        };
      case 'tool_result':
        return {
          type: 'tool_result',
          toolName: payload.tool_name || 'unknown',
          result: payload.result || payload.content || '',
          failed: payload.failed || payload.error != null,
        };
      case 'permission_request':
        return {
          type: 'ask_permission',
          toolName: payload.tool_name || 'unknown',
          message: payload.message || data,
        };
      case 'question':
        return {
          type: 'ask_user',
          questions: payload.questions || [],
        };
      case 'content_delta':
      case 'delta':
        return { type: 'text_delta', content: payload.content || payload.delta || data };
      case 'done':
      case 'complete':
        return { type: 'done' };
      case 'error':
        return { type: 'error', message: payload.message || payload.error || data };
      default:
        if (payload.content) {
          return { type: 'text_delta', content: payload.content };
        }
        return null;
    }
  } catch {
    if (event === 'message' || event === 'delta' || event === 'content') {
      return { type: 'text_delta', content: data };
    }
    return null;
  }
}
