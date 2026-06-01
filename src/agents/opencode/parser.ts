/**
 * OpenCode 原始事件（嵌套 JSON 格式）的扁平化中间表示
 *
 * 示例输入（来自 opencode run --format json）：
 * {"type":"step_start","part":{"type":"step-start","..."}}
 * {"type":"text","part":{"type":"text","text":"Hello"}}
 * {"type":"tool_use","part":{"type":"tool","tool":"read","state":{"status":"completed","input":{...},"output":"..."}}}
 * {"type":"step_finish","part":{"type":"step-finish","..."}}
 *
 * 解析后统一为扁平格式，便于 adapter 映射到前端 AgentEvent
 */

export interface OpencodeRawEvent {
  type: 'text' | 'tool_start' | 'tool_result' | 'step_start' | 'step_complete';
  text?: string;
  tool?: string;
  action?: string;
  path?: string;
  content?: string;
  step?: string;
  description?: string;
  duration_ms?: number;
  args?: Record<string, unknown>;
}

export function parseOpencodeJsonLine(line: string): OpencodeRawEvent | null {
  const trimmed = line.trim();
  if (!trimmed) return null;

  let raw: Record<string, unknown>;
  try {
    raw = JSON.parse(trimmed) as Record<string, unknown>;
  } catch {
    return null;
  }

  const topType = raw.type as string;
  const part = (raw.part as Record<string, unknown>) || {};

  switch (topType) {
    case 'text': {
      const text = (part.text as string) || (raw.text as string);
      if (!text) return null;
      return { type: 'text', text };
    }

    case 'tool_use': {
      const toolName = (part.tool as string) || 'unknown';
      const state = (part.state as Record<string, unknown>) || {};
      const input = (state.input as Record<string, unknown>) || {};
      const output = state.output as string | undefined;
      const status = state.status as string | undefined;

      // 如果工具已完成且有输出，视为 tool_result
      if (status === 'completed' && output !== undefined) {
        return {
          type: 'tool_result',
          tool: toolName,
          content: output,
          args: input,
        };
      }

      // 否则视为 tool_start
      return {
        type: 'tool_start',
        tool: toolName,
        action: input.action as string | undefined,
        path: (input.filePath as string) || (input.path as string),
        args: input,
      };
    }

    case 'step_start': {
      // OpenCode 的 step_start 没有具体的 step 名称，用占位符
      return { type: 'step_start', step: '思考中', description: 'AI 正在分析请求...' };
    }

    case 'step_finish': {
      return { type: 'step_complete', step: '完成', duration_ms: 0 };
    }

    // 兼容可能的直接扁平格式（未来扩展）
    case 'tool_start':
    case 'tool_result':
    case 'step_complete': {
      return raw as unknown as OpencodeRawEvent;
    }

    default:
      return null;
  }
}
