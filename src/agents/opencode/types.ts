export interface OpencodeMessageRequest {
  message: string;
  session_id?: string;
}

export interface OpencodeSSEEvent {
  event: string;
  data: string;
}

export type OpencodeEventType =
  | 'thinking'
  | 'tool_use'
  | 'tool_result'
  | 'permission_request'
  | 'question'
  | 'content_delta'
  | 'done'
  | 'error';
