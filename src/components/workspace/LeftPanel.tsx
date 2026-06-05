import type { Conversation, Message, WorkspaceFileNode, WorkspaceFileContent, GitStatusEntry } from '@/types/types';
import {
  Folder, MessageSquare, Bot, Plus, FileCode2, Check,
  ChevronLeft, ChevronRight, Info,
  FolderOpen, FileText, FileJson, FileType, FileImage,
  Settings, Braces, Hash, Globe, Coffee,
  RefreshCw, Search, Trash2,
} from 'lucide-react';
import { useState, useRef, useEffect, useCallback, type ReactNode } from 'react';
import {
  Dialog, DialogContent, DialogHeader, DialogTitle,
} from '@/components/ui/dialog';
import {
  Sheet, SheetContent, SheetHeader, SheetTitle,
} from '@/components/ui/sheet';
import { useAgentStore } from '@/stores';
import { invoke } from '@tauri-apps/api/core';
import type { AgentInstance } from '@/stores';
import { formatIdShort } from '@/lib/id';

export interface AgentModelConfig {
  type: 'builtin' | 'custom';
  modelId?: string; // builtin model id
  name?: string; // custom model name
  url?: string; // custom api url
  apiKey?: string; // custom api key
}

interface LeftPanelProps {
  conversations: Conversation[];
  activeConversation: Conversation | null;
  agentInstances?: AgentInstance[];
  activeAgentId?: string | null;
  messages: Message[];
  workspace: string;
  collapsed: boolean;
  onToggleCollapse: () => void;
  onSelectConversation: (conv: Conversation) => void;
  onDoubleClickConversation: (conv: Conversation) => void;
  onNewConversation: () => void;
  onAddAgent: () => void;
  onActivateAgent: (id: string) => void;
  onDeleteAgent?: (id: string) => void;
}

import AgentIcon from './AgentIcon';

const agentConfig: Record<string, { name: string; color: string; bg: string; letter: string; desc: string; border: string }> = {
  opencode: { name: 'OpenCode', color: 'text-green-400', bg: 'bg-green-400/15', letter: 'O', desc: '全能型编程助手', border: 'border-green-400/20' },
  'claude-code': { name: 'Claude Code', color: 'text-orange-400', bg: 'bg-orange-400/15', letter: 'C', desc: '代码理解与重构专家', border: 'border-orange-400/20' },
  'cursor-agent': { name: 'Cursor Agent', color: 'text-blue-400', bg: 'bg-blue-400/15', letter: 'C', desc: '智能补全与生成', border: 'border-blue-400/20' },
  codex: { name: 'Codex', color: 'text-purple-400', bg: 'bg-purple-400/15', letter: 'X', desc: 'OpenAI软件工程模型', border: 'border-purple-400/20' },
  custom: { name: '自定义', color: 'text-primary', bg: 'bg-primary/15', letter: 'C', desc: '自由配置的智能体', border: 'border-primary/20' },
};

function highlightCodeLine(line: string, path: string): ReactNode {
  const text = line || ' ';
  const lowerPath = path.toLowerCase();
  const isCode = /\.(ts|tsx|js|jsx|json|rs|css|scss|html|md|py|go|java|c|cpp|h|hpp|sh|bash|zsh|ya?ml|toml|xml|env|dockerfile)$/.test(lowerPath) || lowerPath.endsWith('dockerfile');
  if (!isCode) return text;

  const isYaml = /\.(ya?ml|toml)$/.test(lowerPath);
  const isShell = /\.(sh|bash|zsh|env)$/.test(lowerPath) || lowerPath.endsWith('dockerfile');
  const pattern = isYaml
    ? /(#[^\n]*$)|("(?:\\.|[^"\\])*"|'(?:\\.|[^'\\])*')|\b(true|false|null|yes|no|on|off)\b|\b([0-9]+(?:\.[0-9]+)?)\b|^(\s*)([A-Za-z0-9_.-]+)(:)/g
    : /("(?:\\.|[^"\\])*"|'(?:\\.|[^'\\])*'|`(?:\\.|[^`\\])*`)|(\/\/[^\n]*$|#[^\n]*$)|(\$[A-Za-z_][A-Za-z0-9_]*|\$\{[^}]+\})|\b(import|export|from|const|let|var|function|return|if|else|elif|fi|then|for|while|do|done|case|esac|class|interface|type|async|await|match|pub|struct|enum|impl|use|fn|def|package|func|public|private|protected|static|new|try|catch|finally|throw|throws|echo|cd|pwd|ls|cat|grep|rg|find|mkdir|rm|cp|mv|chmod|chown|source|alias|local|readonly|printf|test)\b|\b(true|false|null|undefined|None|Some|Ok|Err|nil)\b|\b([0-9]+(?:\.[0-9]+)?)\b/g;

  const nodes: ReactNode[] = [];
  let lastIndex = 0;
  for (const match of text.matchAll(pattern)) {
    const index = match.index ?? 0;
    if (index > lastIndex) nodes.push(text.slice(lastIndex, index));
    const value = match[0];
    let className = 'text-foreground';
    if (isYaml) {
      if (match[7]) {
        nodes.push(match[5] || '');
        nodes.push(<span key={`${index}-yaml-key`} className="text-blue-300">{match[6]}</span>);
        nodes.push(match[7]);
        lastIndex = index + value.length;
        continue;
      }
      if (match[1]) className = 'text-muted-foreground';
      else if (match[2]) className = 'text-green-300';
      else if (match[3]) className = 'text-orange-300';
      else if (match[4]) className = 'text-cyan-300';
    } else {
      if (match[2]) className = 'text-muted-foreground';
      else if (match[1]) className = 'text-green-300';
      else if (match[3] && isShell) className = 'text-yellow-300';
      else if (match[4]) className = 'text-purple-300';
      else if (match[5]) className = 'text-orange-300';
      else if (match[6]) className = 'text-cyan-300';
    }
    nodes.push(<span key={`${index}-${value}`} className={className}>{value}</span>);
    lastIndex = index + value.length;
  }
  if (lastIndex < text.length) nodes.push(text.slice(lastIndex));
  return nodes;
}


function renderMarkdown(content: string) {
  const lines = content.split('\n');
  const blocks: ReactNode[] = [];
  let listItems: string[] = [];
  let codeLines: string[] = [];
  let inCode = false;

  const flushList = () => {
    if (listItems.length > 0) {
      blocks.push(
        <ul key={`list-${blocks.length}`} className="list-disc pl-5 my-2 space-y-1">
          {listItems.map((item, index) => <li key={`${item}-${index}`}>{item}</li>)}
        </ul>
      );
      listItems = [];
    }
  };

  const flushCode = () => {
    if (codeLines.length > 0) {
      blocks.push(
        <pre key={`code-${blocks.length}`} className="my-2 p-3 rounded bg-secondary/40 overflow-auto text-[12px]">
          {codeLines.join('\n')}
        </pre>
      );
      codeLines = [];
    }
  };

  lines.forEach((line, index) => {
    if (line.startsWith('```')) {
      if (inCode) {
        flushCode();
        inCode = false;
      } else {
        flushList();
        inCode = true;
      }
      return;
    }

    if (inCode) {
      codeLines.push(line);
      return;
    }

    const heading = line.match(/^(#{1,6})\s+(.*)$/);
    if (heading) {
      flushList();
      const size = heading[1].length <= 2 ? 'text-base' : 'text-sm';
      blocks.push(<div key={`h-${index}`} className={`${size} font-semibold mt-3 mb-1 text-foreground`}>{heading[2]}</div>);
      return;
    }

    const list = line.match(/^\s*[-*+]\s+(.*)$/);
    if (list) {
      listItems.push(list[1]);
      return;
    }

    flushList();
    if (line.trim()) {
      blocks.push(<p key={`p-${index}`} className="my-1 text-foreground leading-relaxed">{line}</p>);
    } else {
      blocks.push(<div key={`br-${index}`} className="h-2" />);
    }
  });

  flushList();
  flushCode();
  return blocks;
}

function isMarkdownFile(path: string): boolean {
  return /\.(md|markdown|mdx)$/i.test(path);
}

function collectFilePaths(nodes: WorkspaceFileNode[]): string[] {
  return nodes.flatMap((node) => {
    if (node.is_dir) return collectFilePaths(node.children || []);
    return [node.path];
  });
}

function getNodeGitStatus(node: WorkspaceFileNode, gitStatus: Map<string, GitStatusEntry['status']>): GitStatusEntry['status'] | 'dot' | null {
  if (!node.is_dir) return gitStatus.get(node.path) || null;
  const hasChangedChild = collectFilePaths(node.children || []).some((path) => gitStatus.has(path));
  return hasChangedChild ? 'dot' : null;
}

function filterWorkspaceTree(nodes: WorkspaceFileNode[], query: string): WorkspaceFileNode[] {
  const keyword = query.trim().toLowerCase();
  if (!keyword) return nodes;

  return nodes.flatMap((node) => {
    const children = node.children ? filterWorkspaceTree(node.children, keyword) : [];
    if (node.name.toLowerCase().includes(keyword) || node.path.toLowerCase().includes(keyword) || children.length > 0) {
      return [{ ...node, children }];
    }
    return [];
  });
}

// 根据扩展名返回图标
function getFileIcon(path: string) {
  if (path.endsWith('.tsx') || path.endsWith('.ts') || path.endsWith('.jsx') || path.endsWith('.js')) return Braces;
  if (path.endsWith('.css') || path.endsWith('.scss') || path.endsWith('.less')) return Hash;
  if (path.endsWith('.json')) return FileJson;
  if (path.endsWith('.html') || path.endsWith('.htm')) return Globe;
  if (path.endsWith('.md')) return FileText;
  if (path.endsWith('.png') || path.endsWith('.jpg') || path.endsWith('.svg') || path.endsWith('.jpeg')) return FileImage;
  if (path.endsWith('.java')) return Coffee;
  if (path.endsWith('.config.ts') || path.endsWith('.config.js')) return Settings;
  return FileCode2;
}

// 递归渲染文件树
function FileTreeNode({
  items,
  depth = 0,
  hoveredFile,
  setHoveredFile,
  onOpenFile,
  gitStatus,
}: {
  items: WorkspaceFileNode[];
  depth?: number;
  hoveredFile: string | null;
  setHoveredFile: (f: string | null) => void;
  onOpenFile: (path: string) => void;
  gitStatus: Map<string, GitStatusEntry['status']>;
}) {
  const [expanded, setExpanded] = useState<Record<string, boolean>>({});

  const sorted = [...items].sort((a, b) => {
    if (a.is_dir === b.is_dir) return a.name.localeCompare(b.name);
    return a.is_dir ? -1 : 1;
  });

  return (
    <>
      {sorted.map((item) => {
        const isFolder = item.is_dir;
        const isHovered = hoveredFile === item.path;
        const nodeStatus = getNodeGitStatus(item, gitStatus);
        if (isFolder) {
          const isOpen = expanded[item.path] ?? true;
          return (
            <div key={item.path}>
              <button
                type="button"
                onClick={() => setExpanded((prev) => ({ ...prev, [item.path]: !isOpen }))}
                className={`w-full flex items-center gap-1 px-3 py-1 text-left hover:bg-secondary/40 transition-colors ${depth > 0 ? 'pl-6' : ''}`}
              >
                <ChevronRight className={`w-3 h-3 shrink-0 text-muted-foreground transition-transform ${isOpen ? 'rotate-90' : ''}`} />
                {isOpen ? (
                  <FolderOpen className="w-3.5 h-3.5 shrink-0 text-primary" />
                ) : (
                  <Folder className="w-3.5 h-3.5 shrink-0 text-muted-foreground" />
                )}
                <span className={`text-[12px] truncate flex-1 min-w-0 ${item.ignored ? 'text-muted-foreground/50' : 'text-foreground'}`} title={item.path}>{item.name}</span>
                {nodeStatus === 'dot' && <span className="w-1.5 h-1.5 rounded-full bg-primary shrink-0" />}
              </button>
              {isOpen && item.children && (
                <FileTreeNode
                  items={item.children}
                  depth={depth + 1}
                  hoveredFile={hoveredFile}
                  setHoveredFile={setHoveredFile}
                  onOpenFile={onOpenFile}
                  gitStatus={gitStatus}
                />
              )}
            </div>
          );
        }
        const Icon = getFileIcon(item.path);
        return (
          <button
            key={item.path}
            type="button"
            onDoubleClick={() => onOpenFile(item.path)}
            onMouseEnter={() => setHoveredFile(item.path)}
            onMouseLeave={() => setHoveredFile(null)}
            className={`w-full flex items-center gap-1 px-3 py-1 text-left hover:bg-secondary/40 transition-colors ${depth > 0 ? 'pl-6' : ''}`}
            title={item.path}
          >
            <span className="w-3 shrink-0" />
            <Icon className={`w-3.5 h-3.5 shrink-0 ${item.ignored ? 'text-muted-foreground/40' : isHovered ? 'text-primary' : 'text-muted-foreground'}`} />
            <span className={`text-[12px] truncate font-mono flex-1 min-w-0 ${item.ignored ? 'text-muted-foreground/50' : isHovered ? 'text-foreground' : 'text-muted-foreground'}`}>{item.name}</span>
            {nodeStatus && nodeStatus !== 'dot' && (
              <span className={`text-[10px] shrink-0 ${nodeStatus === 'U' || nodeStatus === 'A' ? 'text-green-400' : nodeStatus === 'D' ? 'text-red-400' : 'text-orange-400'}`}>
                {nodeStatus}
              </span>
            )}
          </button>
        );
      })}
    </>
  );
}

type TabType = 'files' | 'chat' | 'agent';

export default function LeftPanel({
  conversations,
  activeConversation,
  agentInstances: agentInstancesProp,
  activeAgentId: activeAgentIdProp,
  messages,
  workspace,
  collapsed,
  onToggleCollapse,
  onSelectConversation,
  onDoubleClickConversation,
  onNewConversation,
  onAddAgent,
  onActivateAgent,
  onDeleteAgent,
}: LeftPanelProps) {
  const storeInstances = useAgentStore((s) => s.instances);
  const storeActiveId = useAgentStore((s) => s.activeInstanceId);
  const agentInstances = agentInstancesProp ?? storeInstances;
  const activeAgentId = activeAgentIdProp ?? storeActiveId;

  const [activeTab, setActiveTab] = useState<TabType>('chat');
  const [hoveredFile, setHoveredFile] = useState<string | null>(null);
  const [hoveredConv, setHoveredConv] = useState<string | null>(null);
  const [detailConv, setDetailConv] = useState<Conversation | null>(null);
  const [panelWidth, setPanelWidth] = useState(224); // 默认 w-56 = 14rem = 224px
  const [isResizing, setIsResizing] = useState(false);
  const [fileSearch, setFileSearch] = useState('');
  const [convSearch, setConvSearch] = useState('');
  const [fileTree, setFileTree] = useState<WorkspaceFileNode[]>([]);
  const [fileTreeLoading, setFileTreeLoading] = useState(false);
  const [fileTreeError, setFileTreeError] = useState<string | null>(null);
  const [gitStatus, setGitStatus] = useState<Map<string, GitStatusEntry['status']>>(new Map());
  const [preview, setPreview] = useState<WorkspaceFileContent | null>(null);
  const [previewError, setPreviewError] = useState<string | null>(null);
  const resizeStartX = useRef(0);
  const resizeStartWidth = useRef(224);

  const handleActivateAgentAndSwitch = (id: string) => {
    onActivateAgent(id);
    setActiveTab('chat');
  };

  const loadFileTree = useCallback(async () => {
    if (!workspace) return;
    setFileTreeLoading(true);
    setFileTreeError(null);
    try {
      const [data, status] = await Promise.all([
        invoke<WorkspaceFileNode[]>('list_workspace_tree', { workspace }),
        invoke<GitStatusEntry[]>('git_status_workspace', { workspace }),
      ]);
      setFileTree(Array.isArray(data) ? data : []);
      setGitStatus(new Map((Array.isArray(status) ? status : []).map((entry) => [entry.path, entry.status])));
    } catch (error) {
      setFileTreeError(error instanceof Error ? error.message : String(error));
    } finally {
      setFileTreeLoading(false);
    }
  }, [workspace]);

  useEffect(() => {
    loadFileTree();
  }, [loadFileTree]);

  const openFile = async (path: string) => {
    if (!workspace) return;
    setPreviewError(null);
    try {
      const data = await invoke<WorkspaceFileContent>('read_workspace_file', { workspace, path });
      setPreview(data);
    } catch (error) {
      setPreview(null);
      setPreviewError(error instanceof Error ? error.message : String(error));
    }
  };

  const filePaths = collectFilePaths(filterWorkspaceTree(fileTree, fileSearch));
  const previewIndex = preview ? filePaths.indexOf(preview.path) : -1;
  const openAdjacentFile = (offset: number) => {
    const nextPath = filePaths[previewIndex + offset];
    if (nextPath) {
      openFile(nextPath);
    }
  };

  // 拖拽拉伸
  const handleResizeStart = useCallback((e: React.MouseEvent) => {
    setIsResizing(true);
    resizeStartX.current = e.clientX;
    resizeStartWidth.current = panelWidth;
  }, [panelWidth]);

  useEffect(() => {
    const handleMouseMove = (e: MouseEvent) => {
      if (!isResizing) return;
      const delta = e.clientX - resizeStartX.current;
      const newWidth = Math.max(180, Math.min(400, resizeStartWidth.current + delta));
      setPanelWidth(newWidth);
    };
    const handleMouseUp = () => setIsResizing(false);
    if (isResizing) {
      window.addEventListener('mousemove', handleMouseMove);
      window.addEventListener('mouseup', handleMouseUp);
      document.body.style.cursor = 'col-resize';
      document.body.style.userSelect = 'none';
    }
    return () => {
      window.removeEventListener('mousemove', handleMouseMove);
      window.removeEventListener('mouseup', handleMouseUp);
      document.body.style.cursor = '';
      document.body.style.userSelect = '';
    };
  }, [isResizing]);

  const truncateTitle = (title: string, maxLen = 5) =>
    title.length > maxLen ? title.slice(0, maxLen) + '...' : title;

  // 计算会话统计
  const getConvStats = (conv: Conversation) => {
    const isActive = activeConversation?.id === conv.id;
    const convMessages = isActive ? messages : [];
    const msgCount = convMessages.length;
    const aiMessages = convMessages.filter((m) => m.role === 'assistant');
    const totalIn = aiMessages.reduce((sum, m) => sum + (m.token_in || 0), 0);
    const totalOut = aiMessages.reduce((sum, m) => sum + (m.token_out || 0), 0);
    const totalDuration = aiMessages.reduce((sum, m) => sum + (m.duration_ms || 0), 0);
    const completedCount = aiMessages.filter((m) => m.is_complete).length;
    return { msgCount, totalIn, totalOut, totalDuration, completedCount };
  };

  // 收缩状态：只显示图标栏 + 展开按钮
  if (collapsed) {
    return (
      <div className="w-10 shrink-0 border-r border-border bg-card flex flex-col items-center py-3 gap-3 relative">
        <button
          type="button"
          onClick={onToggleCollapse}
          className="w-5 h-8 flex items-center justify-center rounded hover:bg-secondary/60 text-muted-foreground hover:text-foreground transition-colors"
          title="展开"
        >
          <ChevronRight className="w-3.5 h-3.5" />
        </button>
        <div className="h-px w-6 bg-border" />
        <button
          type="button"
          onClick={() => setActiveTab('files')}
          className={`w-7 h-7 rounded-md flex items-center justify-center transition-all ${
            activeTab === 'files' ? 'bg-primary/10 text-primary' : 'text-muted-foreground hover:bg-secondary/60 hover:text-foreground'
          }`}
          title="文件"
        >
          <Folder className="w-4 h-4" />
        </button>
        <button
          type="button"
          onClick={() => { setActiveTab('chat'); onToggleCollapse(); }}
          className={`w-7 h-7 rounded-md flex items-center justify-center transition-all ${
            activeTab === 'chat' ? 'bg-primary/10 text-primary' : 'text-muted-foreground hover:bg-secondary/60 hover:text-foreground'
          }`}
          title="会话"
        >
          <MessageSquare className="w-4 h-4" />
        </button>
        <button
          type="button"
          onClick={() => { setActiveTab('agent'); onToggleCollapse(); }}
          className={`w-7 h-7 rounded-md flex items-center justify-center transition-all ${
            activeTab === 'agent' ? 'bg-primary/10 text-primary' : 'text-muted-foreground hover:bg-secondary/60 hover:text-foreground'
          }`}
          title="智能体"
        >
          <Bot className="w-4 h-4" />
        </button>
      </div>
    );
  }

  return (
    <div className="flex shrink-0 h-full relative">
      {/* 收缩按钮 */}
      <button
        type="button"
        onClick={onToggleCollapse}
        className="absolute -right-2.5 top-3 z-10 w-5 h-8 flex items-center justify-center rounded-r bg-card border border-border border-l-0 text-muted-foreground hover:text-foreground transition-colors"
        title="收缩"
      >
        <ChevronLeft className="w-3.5 h-3.5" />
      </button>

      {/* 第一级：图标栏 */}
      <div className="w-10 shrink-0 border-r border-border bg-card flex flex-col items-center py-3 gap-3">
        <button
          type="button"
          onClick={() => setActiveTab('files')}
          className={`w-7 h-7 rounded-md flex items-center justify-center transition-all ${
            activeTab === 'files' ? 'bg-primary/10 text-primary' : 'text-muted-foreground hover:bg-secondary/60 hover:text-foreground'
          }`}
          title="文件"
        >
          <Folder className="w-4 h-4" />
        </button>
        <button
          type="button"
          onClick={() => setActiveTab('chat')}
          className={`w-7 h-7 rounded-md flex items-center justify-center transition-all ${
            activeTab === 'chat' ? 'bg-primary/10 text-primary' : 'text-muted-foreground hover:bg-secondary/60 hover:text-foreground'
          }`}
          title="会话"
        >
          <MessageSquare className="w-4 h-4" />
        </button>
        <button
          type="button"
          onClick={() => setActiveTab('agent')}
          className={`w-7 h-7 rounded-md flex items-center justify-center transition-all ${
            activeTab === 'agent' ? 'bg-primary/10 text-primary' : 'text-muted-foreground hover:bg-secondary/60 hover:text-foreground'
          }`}
          title="智能体"
        >
          <Bot className="w-4 h-4" />
        </button>
      </div>

      {/* 第二级：内容面板（可拉伸） */}
      <div className="shrink-0 border-r border-border bg-card flex flex-col relative" style={{ width: panelWidth }}>
        {/* 拉伸把手 */}
        <div
          className="absolute -right-1 top-0 bottom-0 w-2 cursor-col-resize z-20"
          onMouseDown={handleResizeStart}
          title="拖拽调整宽度"
        />

        {/* 文件 Tab */}
        {activeTab === 'files' && (
          <div className="flex flex-col h-full">
            <div className="flex items-center justify-between px-3 py-2 border-b border-border shrink-0">
              <span className="text-xs font-medium text-foreground">文件</span>
              <div className="flex items-center gap-1">
                <button
                  type="button"
                  onClick={loadFileTree}
                  disabled={fileTreeLoading}
                  className="w-5 h-5 flex items-center justify-center rounded hover:bg-secondary text-muted-foreground hover:text-foreground transition-colors disabled:opacity-50"
                  title="刷新"
                >
                  <RefreshCw className={`w-3 h-3 ${fileTreeLoading ? 'animate-spin' : ''}`} />
                </button>

              </div>
            </div>
            <div className="px-3 py-2 border-b border-border shrink-0">
              <div className="flex items-center gap-1.5 px-2 py-1 rounded-md border border-border bg-secondary/40">
                <Search className="w-3 h-3 text-muted-foreground shrink-0" />
                <input
                  type="text"
                  value={fileSearch}
                  onChange={(e) => setFileSearch(e.target.value)}
                  placeholder="搜索文件..."
                  className="flex-1 min-w-0 bg-transparent text-xs text-foreground placeholder:text-muted-foreground focus:outline-none"
                />
              </div>
            </div>
            <div className="flex-1 overflow-y-auto py-1">
              {fileTreeError ? (
                <div className="px-3 py-6 text-center text-xs text-red-400">{fileTreeError}</div>
              ) : fileTree.length === 0 ? (
                <div className="px-3 py-6 text-center text-xs text-muted-foreground">暂无文件</div>
              ) : (
                <FileTreeNode
                  items={filterWorkspaceTree(fileTree, fileSearch)}
                  hoveredFile={hoveredFile}
                  setHoveredFile={setHoveredFile}
                  onOpenFile={openFile}
                  gitStatus={gitStatus}
                />
              )}
            </div>
          </div>
        )}

        {/* 会话 Tab */}
        {activeTab === 'chat' && (
          <div className="flex flex-col h-full">
            <div className="flex items-center justify-between px-3 py-2 border-b border-border shrink-0">
              <span className="text-xs font-medium text-foreground">会话</span>
              <button
                type="button"
                onClick={onNewConversation}
                className="flex items-center gap-1 text-[12px] text-primary hover:text-primary/80 transition-colors"
              >
                <Plus className="w-3 h-3" />
                新建
              </button>
            </div>
            <div className="px-3 py-2 border-b border-border shrink-0">
              <div className="flex items-center gap-1.5 px-2 py-1 rounded-md border border-border bg-secondary/40">
                <Search className="w-3 h-3 text-muted-foreground shrink-0" />
                <input
                  type="text"
                  value={convSearch}
                  onChange={(e) => setConvSearch(e.target.value)}
                  placeholder="搜索会话..."
                  className="flex-1 min-w-0 bg-transparent text-xs text-foreground placeholder:text-muted-foreground focus:outline-none"
                />
              </div>
            </div>
            <div className="flex-1 overflow-y-auto py-1">
              {conversations.length === 0 ? (
                <div className="px-3 py-6 text-center text-xs text-muted-foreground">暂无会话</div>
              ) : (
                conversations.filter((c) => !convSearch || c.title.toLowerCase().includes(convSearch.toLowerCase())).map((conv) => {
                  const cfg = agentConfig[conv.agent] || agentConfig.opencode;
                  const isAct = activeConversation?.id === conv.id;
                  const showDet = hoveredConv === conv.id;
                  return (
                    <div
                      key={conv.id}
                      role="button"
                      tabIndex={0}
                      onClick={() => onSelectConversation(conv)}
                      onDoubleClick={() => onDoubleClickConversation(conv)}
                      onMouseEnter={() => setHoveredConv(conv.id)}
                      onMouseLeave={() => setHoveredConv(null)}
                      onKeyDown={(e) => {
                        if (e.key === 'Enter' || e.key === ' ') {
                          onSelectConversation(conv);
                        }
                      }}
                      className={`w-full flex items-center gap-2 px-3 py-2 transition-colors text-left relative cursor-pointer ${
                        isAct ? 'bg-primary/5' : 'hover:bg-secondary/40'
                      }`}
                    >
                      <span className={`w-5 h-5 rounded text-[11px] font-bold flex items-center justify-center shrink-0 ${cfg.bg} ${cfg.color} overflow-hidden`}>
                        <AgentIcon agentKey={conv.agent} size={16} />
                      </span>
                      <div className="flex-1 min-w-0">
                        <div className={`text-[12px] truncate ${isAct ? 'text-foreground font-medium' : 'text-foreground'}`}>
                          {truncateTitle(conv.title)}
                        </div>
                        <div className="text-[11px] text-muted-foreground mt-0.5">{cfg.name}</div>
                      </div>
                      <div className="flex items-center gap-1 shrink-0">
                        {showDet && (
                          <div
                            role="button"
                            tabIndex={0}
                            onClick={(e) => {
                              e.stopPropagation();
                              setDetailConv(conv);
                            }}
                            onKeyDown={(e) => {
                              if (e.key === 'Enter' || e.key === ' ') {
                                e.stopPropagation();
                                setDetailConv(conv);
                              }
                            }}
                            className="w-5 h-5 flex items-center justify-center rounded hover:bg-secondary text-muted-foreground hover:text-foreground transition-colors cursor-pointer"
                            title="会话概要"
                          >
                            <Info className="w-3 h-3" />
                          </div>
                        )}
                        {isAct && <div className="w-1 h-4 rounded-full bg-primary" />}
                      </div>
                    </div>
                  );
                })
              )}
            </div>
          </div>
        )}

        {/* Agent Tab */}
        {activeTab === 'agent' && (
          <div className="flex flex-col h-full">
            <div className="flex items-center justify-between px-3 py-2 border-b border-border shrink-0">
              <span className="text-xs font-medium text-foreground">智能体</span>
              <button
                type="button"
                onClick={onAddAgent}
                className="flex items-center gap-1 text-[12px] text-primary hover:text-primary/80 transition-colors"
              >
                <Plus className="w-3 h-3" />
                添加
              </button>
            </div>
            <div className="flex-1 overflow-y-auto p-2 space-y-2">
              {agentInstances.map((instance) => {
                const config = agentConfig[instance.agentKey] || agentConfig.opencode;
                const isActive = activeAgentId === instance.id;
                return (
                  <div
                    key={instance.id}
                    className={`w-full flex items-center gap-2 p-2 rounded-lg border transition-all ${
                      isActive
                        ? `${config.border} ${config.bg} ring-1 ring-primary/40`
                        : 'border-border bg-secondary/20 hover:bg-secondary/40'
                    }`}
                  >
                    <button
                      type="button"
                      onClick={() => onActivateAgent(instance.id)}
                      onDoubleClick={() => handleActivateAgentAndSwitch(instance.id)}
                      className="flex-1 flex items-center gap-2 text-left min-w-0"
                    >
                      <span className={`w-7 h-7 rounded-md text-sm font-bold flex items-center justify-center shrink-0 ${config.bg} ${config.color} overflow-hidden`}>
                        <AgentIcon agentKey={instance.agentKey} size={18} />
                      </span>
                      <div className="flex-1 min-w-0">
                        <div className="text-xs font-medium text-foreground">{instance.displayName}</div>
                        <div className="text-[11px] text-muted-foreground">{config.name} · {formatIdShort(instance.id)}</div>
                      </div>
                      {isActive && (
                        <Check className="w-3.5 h-3.5 text-primary shrink-0" />
                      )}
                    </button>
                    {onDeleteAgent && (
                      <button
                        type="button"
                        onClick={(e) => {
                          e.stopPropagation();
                          onDeleteAgent(instance.id);
                        }}
                        className="p-1 rounded hover:bg-destructive/20 text-muted-foreground hover:text-destructive transition-colors shrink-0"
                        title="删除"
                      >
                        <Trash2 className="w-3 h-3" />
                      </button>
                    )}
                  </div>
                );
              })}
            </div>
          </div>
        )}
      </div>

      {/* 会话详情弹窗 */}
      <Dialog open={!!detailConv} onOpenChange={(open) => !open && setDetailConv(null)}>
        <DialogContent className="max-w-[calc(100%-2rem)] md:max-w-sm">
          <DialogHeader>
            <DialogTitle className="text-sm font-medium">会话概要</DialogTitle>
          </DialogHeader>
          {detailConv && (() => {
            const stats = getConvStats(detailConv);
            const isActive = activeConversation?.id === detailConv.id;
            const contextPct = isActive && messages.length > 0
              ? Math.min(100, Math.round((messages.filter((m) => m.role === 'assistant').length / 50) * 100))
              : 0;
            return (
              <div className="space-y-3 text-sm">
                <div>
                  <div className="text-[11px] text-muted-foreground uppercase tracking-wider">会话ID</div>
                  <div className="text-xs font-mono text-foreground mt-0.5 break-all">{detailConv.id}</div>
                </div>
                <div>
                  <div className="text-[11px] text-muted-foreground uppercase tracking-wider">消息数</div>
                  <div className="text-foreground mt-0.5">{stats.msgCount}</div>
                </div>
                <div>
                  <div className="text-[11px] text-muted-foreground uppercase tracking-wider mb-1">Token 使用</div>
                  <div className="space-y-1">
                    <div className="flex items-center justify-between">
                      <span className="text-xs text-muted-foreground">上下文</span>
                      <span className="text-xs text-foreground">{contextPct}%</span>
                    </div>
                    <div className="flex items-center justify-between">
                      <span className="text-xs text-muted-foreground">输入</span>
                      <span className="text-xs text-foreground">{stats.totalIn.toLocaleString()}</span>
                    </div>
                    <div className="flex items-center justify-between">
                      <span className="text-xs text-muted-foreground">输出</span>
                      <span className="text-xs text-foreground">{stats.totalOut.toLocaleString()}</span>
                    </div>
                  </div>
                </div>
              </div>
            );
          })()}
        </DialogContent>
      </Dialog>

      <Sheet open={!!preview || !!previewError} onOpenChange={(open) => !open && (setPreview(null), setPreviewError(null))}>
        <SheetContent side="right" className="w-screen sm:w-[72vw] p-0 bg-card border-l border-border flex flex-col">
          <SheetHeader className="px-4 py-3 border-b border-border shrink-0 pr-12">
            <SheetTitle className="flex items-center gap-2 text-sm text-foreground font-mono min-w-0">
              <FileCode2 className="w-4 h-4 text-primary shrink-0" />
              <span className="truncate" title={preview ? `${workspace}/${preview.path}` : '文件预览'}>
                {preview ? `${workspace}/${preview.path}` : '文件预览'}
              </span>
            </SheetTitle>
          </SheetHeader>
          <div className="flex items-center justify-between px-3 py-0.5 border-b border-border shrink-0 bg-secondary/20">
            <span className="text-[11px] text-muted-foreground">
              {preview && previewIndex >= 0 ? `${previewIndex + 1} / ${filePaths.length}` : ''}
            </span>
            <div className="flex items-center gap-1">
              <button
                type="button"
                disabled={previewIndex <= 0}
                onClick={() => openAdjacentFile(-1)}
                className="flex items-center gap-0.5 px-1 py-0 text-[10px] rounded border border-border bg-card text-foreground hover:bg-secondary/60 disabled:opacity-30 disabled:cursor-not-allowed transition-colors"
              >
                <ChevronLeft className="w-3 h-3" />
                上一个
              </button>
              <button
                type="button"
                disabled={previewIndex < 0 || previewIndex >= filePaths.length - 1}
                onClick={() => openAdjacentFile(1)}
                className="flex items-center gap-0.5 px-1 py-0 text-[10px] rounded border border-border bg-card text-foreground hover:bg-secondary/60 disabled:opacity-30 disabled:cursor-not-allowed transition-colors"
              >
                下一个
                <ChevronRight className="w-3 h-3" />
              </button>
            </div>
          </div>
          {previewError ? (
            <div className="p-4 text-sm text-red-400">{previewError}</div>
          ) : preview ? (
            preview.is_image ? (
              <div className="flex-1 overflow-auto p-6 flex items-center justify-center bg-background/60">
                <img src={preview.content} alt={preview.path} className="max-w-full max-h-full object-contain rounded border border-border bg-card" />
              </div>
            ) : isMarkdownFile(preview.path) ? (
              <div className="flex-1 overflow-auto px-6 py-4 text-sm text-foreground">
                {preview.truncated && (
                  <div className="mb-3 px-3 py-2 rounded bg-orange-400/10 text-orange-300 border border-orange-400/20">
                    文件较大，仅显示前 512KB
                  </div>
                )}
                <div className="max-w-4xl mx-auto">{renderMarkdown(preview.content)}</div>
              </div>
            ) : (
              <div className="flex-1 overflow-auto font-mono text-[12px] leading-relaxed">
                {preview.truncated && (
                  <div className="sticky top-0 z-10 px-3 py-2 bg-orange-400/10 text-orange-300 border-b border-border">
                    文件较大，仅显示前 512KB
                  </div>
                )}
                {preview.content.split('\n').map((line, index) => (
                  <div key={`${preview.path}-${index}`} className="flex hover:bg-secondary/20">
                    <span className="w-12 shrink-0 text-right pr-3 text-muted-foreground select-none border-r border-border/60">
                      {index + 1}
                    </span>
                    <pre className="flex-1 min-w-max px-3 whitespace-pre text-foreground">
                      {highlightCodeLine(line || ' ', preview.path)}
                    </pre>
                  </div>
                ))}
              </div>
            )
          ) : null}
        </SheetContent>
      </Sheet>
    </div>
  );
}
