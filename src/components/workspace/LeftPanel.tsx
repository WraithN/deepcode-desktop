import type { Conversation, Message, FileItem } from '@/types/types';
import {
  Folder, MessageSquare, Bot, Plus, FileCode2, Check,
  ChevronLeft, ChevronRight, Info,
  FolderOpen, FileText, FileJson, FileType, FileImage,
  Settings, Braces, Hash, Globe, Coffee,
  RefreshCw, Search, Trash2,
} from 'lucide-react';
import { useState, useRef, useEffect, useCallback } from 'react';
import {
  Dialog, DialogContent, DialogHeader, DialogTitle,
} from '@/components/ui/dialog';
import { useAgentStore } from '@/stores';
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

const rawMockFiles = [
  'src/App.tsx',
  'src/main.tsx',
  'src/index.css',
  'src/components/ui/button.tsx',
  'src/components/ui/card.tsx',
  'src/components/ui/dialog.tsx',
  'src/components/ui/sheet.tsx',
  'src/components/ui/tabs.tsx',
  'src/pages/Home.tsx',
  'src/pages/About.tsx',
  'src/lib/utils.ts',
  'public/vite.svg',
  'package.json',
  'tsconfig.json',
  'vite.config.ts',
];

// 扁平路径转文件树
function buildFileTree(paths: string[]): FileItem[] {
  const root: FileItem[] = [];
  for (const path of paths) {
    const parts = path.split('/');
    let current = root;
    for (let i = 0; i < parts.length; i++) {
      const name = parts[i];
      const isLast = i === parts.length - 1;
      const existing = current.find((item) => item.name === name);
      if (existing) {
        if (!isLast) current = existing.children!;
      } else {
        const newItem: FileItem = {
          name,
          path: parts.slice(0, i + 1).join('/'),
          type: isLast ? 'file' : 'folder',
          children: isLast ? undefined : [],
        };
        current.push(newItem);
        if (!isLast) current = newItem.children!;
      }
    }
  }
  return root;
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
}: {
  items: FileItem[];
  depth?: number;
  hoveredFile: string | null;
  setHoveredFile: (f: string | null) => void;
}) {
  const [expanded, setExpanded] = useState<Record<string, boolean>>(() => {
    // 默认展开第一层
    const init: Record<string, boolean> = {};
    items.forEach((item) => { if (item.type === 'folder') init[item.path] = true; });
    return init;
  });

  const sorted = [...items].sort((a, b) => {
    if (a.type === b.type) return a.name.localeCompare(b.name);
    return a.type === 'folder' ? -1 : 1;
  });

  return (
    <>
      {sorted.map((item) => {
        const isFolder = item.type === 'folder';
        const isHovered = hoveredFile === item.path;
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
                <span className="text-[12px] text-foreground">{item.name}</span>
              </button>
              {isOpen && item.children && (
                <FileTreeNode
                  items={item.children}
                  depth={depth + 1}
                  hoveredFile={hoveredFile}
                  setHoveredFile={setHoveredFile}
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
            onMouseEnter={() => setHoveredFile(item.path)}
            onMouseLeave={() => setHoveredFile(null)}
            className={`w-full flex items-center gap-1 px-3 py-1 text-left hover:bg-secondary/40 transition-colors ${depth > 0 ? 'pl-6' : ''}`}
          >
            <span className="w-3 shrink-0" />
            <Icon className={`w-3.5 h-3.5 shrink-0 ${isHovered ? 'text-primary' : 'text-muted-foreground'}`} />
            <span className={`text-[12px] truncate font-mono ${isHovered ? 'text-foreground' : 'text-muted-foreground'}`}>{item.name}</span>
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
  const resizeStartX = useRef(0);
  const resizeStartWidth = useRef(224);

  const handleActivateAgentAndSwitch = (id: string) => {
    onActivateAgent(id);
    setActiveTab('chat');
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
                  onClick={() => setFileSearch('')}
                  className="w-5 h-5 flex items-center justify-center rounded hover:bg-secondary text-muted-foreground hover:text-foreground transition-colors"
                  title="刷新"
                >
                  <RefreshCw className="w-3 h-3" />
                </button>
                <span className="text-[11px] text-muted-foreground">{rawMockFiles.length} 个文件</span>
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
              <FileTreeNode
                items={buildFileTree(rawMockFiles.filter((f) => !fileSearch || f.toLowerCase().includes(fileSearch.toLowerCase())))}
                hoveredFile={hoveredFile}
                setHoveredFile={setHoveredFile}
              />
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
    </div>
  );
}
