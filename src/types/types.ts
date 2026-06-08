export interface Profile {
  id: string;
  username: string | null;
  email: string | null;
  phone: string | null;
  role: 'user' | 'admin';
  created_at: string;
}

export interface Conversation {
  id: string;
  user_id: string;
  title: string;
  agent: string;
  model: string;
  created_at: string;
  updated_at: string;
}

export interface MessageStep {
  type: 'thinking' | 'tool_use' | 'tool_result' | 'ask_permission' | 'ask_user' | 'final' | 'compress' | 'retry';
  content: string;
  toolName?: string;
  questions?: AskQuestion[];
  permissionType?: string; // 权限询问的工具名称
  failed?: boolean; // 步骤是否失败
  summary?: ToolSummary; // 工具调用摘要
  compressInfo?: { originalSize: number; compressedSize: number; ratio: number; status: 'compressing' | 'done' };
  diff?: string; // 写文件时的diff内容
  interaction?: InteractionPayload;
  partId?: string; // 关联的 part ID，用于去重
}

export interface InteractionPayload {
  type: 'question' | 'permission' | 'todowrite';
  questions?: QuestionItem[];
  toolName?: string;
  action?: string;
  todos?: TodoItem[];
}

export interface QuestionItem {
  question: string;
  header: string;
  options: QuestionOption[];
  multiple: boolean;
}

export interface QuestionOption {
  label: string;
  description: string;
}

export interface TodoItem {
  id: string;
  content: string;
  status: 'pending' | 'in_progress' | 'completed' | 'cancelled';
  priority: 'high' | 'medium' | 'low';
}

export interface ToolSummary {
  file?: string;
  lines?: number;
  durationMs?: number;
}

export interface AskQuestion {
  id: string;
  label: string;
  type: 'choice' | 'custom';
  options?: string[];
  required?: boolean;
}

export interface Message {
  id: string;
  conversation_id: string;
  role: 'user' | 'assistant';
  content: string;
  steps?: MessageStep[]; // 仅前端使用，AI流式步骤
  is_complete?: boolean; // AI消息是否已完成
  token_in?: number;
  token_out?: number;
  duration_ms?: number;
  created_at: string;
}

export interface Task {
  id: string;
  user_id: string;
  conversation_id: string | null;
  title: string;
  status: 'pending' | 'in_progress' | 'completed';
  created_at: string;
}

export interface ModifiedFile {
  id: string;
  user_id: string;
  conversation_id: string | null;
  file_path: string;
  change_type: 'created' | 'modified' | 'deleted';
  diff?: string;
  created_at: string;
}

export interface UserSettings {
  id: string;
  user_id: string;
  agent: string;
  model: string;
  theme: string;
  skills: Record<string, string>;
  created_at: string;
  updated_at: string;
}

export interface PromptCard {
  id: string;
  title: string;
  content: string;
  tags: string[];
}

export interface FileItem {
  name: string;
  path: string;
  type: 'file' | 'folder';
  children?: FileItem[];
}

export interface WorkspaceFileNode {
  name: string;
  path: string;
  is_dir: boolean;
  ignored: boolean;
  children?: WorkspaceFileNode[] | null;
}

export interface WorkspaceFileContent {
  path: string;
  content: string;
  truncated: boolean;
  is_image: boolean;
}

export interface GitStatusEntry {
  path: string;
  status: 'M' | 'U' | 'A' | 'D' | 'R';
}

export interface GitChangedFile {
  path: string;
  status: 'M' | 'U' | 'A' | 'D' | 'R';
  additions: number;
  deletions: number;
  diff: string;
}
