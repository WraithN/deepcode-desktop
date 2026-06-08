import type { Conversation, Message, WorkspaceFileNode, WorkspaceFileContent, GitStatusEntry } from '@/types/types';
import { Folder, MessageSquare, Bot, FileCode2, ChevronLeft, ChevronRight } from 'lucide-react';
import { useState, useRef, useEffect, useCallback } from 'react';
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { Sheet, SheetContent, SheetHeader, SheetTitle } from '@/components/ui/sheet';
import { useAgentStore } from '@/stores';
import { invoke } from '@tauri-apps/api/core';
import type { AgentInstance } from '@/stores';
import FileTreePanel from './left/FileTreePanel';
import ConversationPanel from './left/ConversationPanel';
import AgentPanel from './left/AgentPanel';
import {
  collectFilePaths,
  collectFolderPaths,
  filterWorkspaceTree,
  highlightCodeLine,
  isMarkdownFile,
  renderMarkdown,
} from './left/tree-utils';

export interface AgentModelConfig {
  type: 'builtin' | 'custom';
  modelId?: string;
  name?: string;
  url?: string;
  apiKey?: string;
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
  const [panelWidth, setPanelWidth] = useState(224);
  const [isResizing, setIsResizing] = useState(false);
  const [fileSearch, setFileSearch] = useState('');
  const [convSearch, setConvSearch] = useState('');
  const [fileTree, setFileTree] = useState<WorkspaceFileNode[]>([]);
  const [fileTreeLoading, setFileTreeLoading] = useState(false);
  const [fileTreeError, setFileTreeError] = useState<string | null>(null);
  const [gitStatus, setGitStatus] = useState<Map<string, GitStatusEntry['status']>>(new Map());
  const [expandedFolders, setExpandedFolders] = useState<Record<string, boolean>>({});
  const [preview, setPreview] = useState<WorkspaceFileContent | null>(null);
  const [previewError, setPreviewError] = useState<string | null>(null);
  const [detailConv, setDetailConv] = useState<Conversation | null>(null);
  const resizeStartX = useRef(0);
  const resizeStartWidth = useRef(224);

  const handleActivateAgentAndSwitch = (id: string) => {
    onActivateAgent(id);
    setActiveTab('chat');
  };

  const loadFileTree = useCallback(async () => {
    if (!workspace) { return; }
    setFileTreeLoading(true);
    setFileTreeError(null);
    try {
      const [data, status] = await Promise.all([
        invoke<WorkspaceFileNode[]>('list_workspace_tree', { workspace }),
        invoke<GitStatusEntry[]>('git_status_workspace', { workspace }),
      ]);
      setFileTree(Array.isArray(data) ? data : []);
      setGitStatus(
        new Map((Array.isArray(status) ? status : []).map((entry) => [entry.path, entry.status]))
      );
    } catch (error) {
      setFileTreeError(error instanceof Error ? error.message : String(error));
    } finally {
      setFileTreeLoading(false);
    }
  }, [workspace]);

  useEffect(() => {
    loadFileTree();
  }, [loadFileTree]);

  // 文件树定时刷新：每 5 秒轮询一次，保持文件列表与文件系统同步
  useEffect(() => {
    const interval = setInterval(() => {
      if (!fileTreeLoading) {
        loadFileTree();
      }
    }, 5000);
    return () => clearInterval(interval);
  }, [loadFileTree, fileTreeLoading]);

  const openFile = async (path: string) => {
    if (!workspace) { return; }
    setPreviewError(null);
    try {
      const data = await invoke<WorkspaceFileContent>('read_workspace_file', { workspace, path });
      setPreview(data);
    } catch (error) {
      setPreview(null);
      setPreviewError(error instanceof Error ? error.message : String(error));
    }
  };

  const handleToggleFolder = (path: string, open: boolean) => {
    setExpandedFolders((prev) => ({ ...prev, [path]: open }));
  };

  const handleExpandAllFolders = () => {
    const next = Object.fromEntries(
      collectFolderPaths(filterWorkspaceTree(fileTree, fileSearch)).map((path) => [path, true])
    );
    setExpandedFolders(next);
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
  const handleResizeStart = useCallback(
    (e: React.MouseEvent) => {
      setIsResizing(true);
      resizeStartX.current = e.clientX;
      resizeStartWidth.current = panelWidth;
    },
    [panelWidth]
  );

  useEffect(() => {
    const handleMouseMove = (e: MouseEvent) => {
      if (!isResizing) { return; }
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

  const _truncateTitle = (title: string, maxLen = 5) =>
    title.length > maxLen ? `${title.slice(0, maxLen)}...` : title;

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

  const filteredTree = filterWorkspaceTree(fileTree, fileSearch);

  const tabButtonClass = (tab: TabType) =>
    `w-7 h-7 rounded-md flex items-center justify-center transition-all ${
      activeTab === tab
        ? 'bg-primary/10 text-primary'
        : 'text-muted-foreground hover:bg-secondary/60 hover:text-foreground'
    }`;

  // 收缩状态：只显示图标栏 + 展开按钮
  if (collapsed) {
    return (
      <div
        className="w-10 shrink-0 border-r border-border bg-card flex flex-col items-center py-3 gap-3 relative"
        onContextMenu={(event) => event.preventDefault()}
      >
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
          className={tabButtonClass('files')}
          title="文件"
        >
          <Folder className="w-4 h-4" />
        </button>
        <button
          type="button"
          onClick={() => {
            setActiveTab('chat');
            onToggleCollapse();
          }}
          className={tabButtonClass('chat')}
          title="会话"
        >
          <MessageSquare className="w-4 h-4" />
        </button>
        <button
          type="button"
          onClick={() => {
            setActiveTab('agent');
            onToggleCollapse();
          }}
          className={tabButtonClass('agent')}
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
      <div
        className="w-10 shrink-0 border-r border-border bg-card flex flex-col items-center py-3 gap-3"
        onContextMenu={(event) => event.preventDefault()}
      >
        <button
          type="button"
          onClick={() => setActiveTab('files')}
          className={tabButtonClass('files')}
          title="文件"
        >
          <Folder className="w-4 h-4" />
        </button>
        <button
          type="button"
          onClick={() => setActiveTab('chat')}
          className={tabButtonClass('chat')}
          title="会话"
        >
          <MessageSquare className="w-4 h-4" />
        </button>
        <button
          type="button"
          onClick={() => setActiveTab('agent')}
          className={tabButtonClass('agent')}
          title="智能体"
        >
          <Bot className="w-4 h-4" />
        </button>
      </div>

      {/* 第二级：内容面板（可拉伸） */}
      <div
        className="shrink-0 border-r border-border bg-card flex flex-col relative"
        style={{ width: panelWidth }}
      >
        {/* 拉伸把手 */}
        <div
          className="absolute -right-1 top-0 bottom-0 w-2 cursor-col-resize z-20"
          onMouseDown={handleResizeStart}
          title="拖拽调整宽度"
        />

        {activeTab === 'files' && (
          <FileTreePanel
            fileTree={fileTree}
            gitStatus={gitStatus}
            fileSearch={fileSearch}
            onFileSearchChange={setFileSearch}
            fileTreeLoading={fileTreeLoading}
            fileTreeError={fileTreeError}
            expandedFolders={expandedFolders}
            filteredTree={filteredTree}
            onLoadFileTree={loadFileTree}
            onExpandAll={handleExpandAllFolders}
            onOpenFile={openFile}
            onToggleFolder={handleToggleFolder}
          />
        )}

        {activeTab === 'chat' && (
          <ConversationPanel
            conversations={conversations}
            activeConversation={activeConversation}
            messages={messages}
            convSearch={convSearch}
            onConvSearchChange={setConvSearch}
            onSelectConversation={onSelectConversation}
            onDoubleClickConversation={onDoubleClickConversation}
            onNewConversation={onNewConversation}
            onShowDetail={setDetailConv}
          />
        )}

        {activeTab === 'agent' && (
          <AgentPanel
            agentInstances={agentInstances}
            activeAgentId={activeAgentId}
            onActivateAgent={onActivateAgent}
            onActivateAgentAndSwitch={handleActivateAgentAndSwitch}
            onAddAgent={onAddAgent}
            onDeleteAgent={onDeleteAgent}
          />
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
            const contextPct =
              isActive && messages.length > 0
                ? Math.min(100, Math.round((messages.filter((m) => m.role === 'assistant').length / 50) * 100))
                : 0;
            return (
              <div className="space-y-3 text-sm">
                <div>
                  <div className="text-xs text-muted-foreground uppercase tracking-wider">会话ID</div>
                  <div className="text-xs font-mono text-foreground mt-0.5 break-all">{detailConv.id}</div>
                </div>
                <div>
                  <div className="text-xs text-muted-foreground uppercase tracking-wider">消息数</div>
                  <div className="text-foreground mt-0.5">{stats.msgCount}</div>
                </div>
                <div>
                  <div className="text-xs text-muted-foreground uppercase tracking-wider mb-1">Token 使用</div>
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

      {/* 文件预览 Sheet */}
      <Sheet
        open={!!preview || !!previewError}
        onOpenChange={(open) => !open && (setPreview(null), setPreviewError(null))}
      >
        <SheetContent
          side="right"
          className="w-screen sm:w-[72vw] p-0 bg-card border-l border-border flex flex-col"
        >
          <SheetHeader className="px-4 py-3 border-b border-border shrink-0 pr-12">
            <SheetTitle className="flex items-center gap-2 text-sm text-foreground font-mono min-w-0">
              <FileCode2 className="w-4 h-4 text-primary shrink-0" />
              <span className="truncate" title={preview ? `${workspace}/${preview.path}` : '文件预览'}>
                {preview ? `${workspace}/${preview.path}` : '文件预览'}
              </span>
            </SheetTitle>
          </SheetHeader>
          <div className="flex items-center justify-between px-3 py-0.5 border-b border-border shrink-0 bg-secondary/20">
            <span className="text-xs text-muted-foreground">
              {preview && previewIndex >= 0 ? `${previewIndex + 1} / ${filePaths.length}` : ''}
            </span>
            <div className="flex items-center gap-1">
              <button
                type="button"
                disabled={previewIndex <= 0}
                onClick={() => openAdjacentFile(-1)}
                className="flex items-center gap-0.5 px-1 py-0 text-xs rounded border border-border bg-card text-foreground hover:bg-secondary/60 disabled:opacity-30 disabled:cursor-not-allowed transition-colors"
              >
                <ChevronLeft className="w-3 h-3" />
                上一个
              </button>
              <button
                type="button"
                disabled={previewIndex < 0 || previewIndex >= filePaths.length - 1}
                onClick={() => openAdjacentFile(1)}
                className="flex items-center gap-0.5 px-1 py-0 text-xs rounded border border-border bg-card text-foreground hover:bg-secondary/60 disabled:opacity-30 disabled:cursor-not-allowed transition-colors"
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
                <img
                  src={preview.content}
                  alt={preview.path}
                  className="max-w-full max-h-full object-contain rounded border border-border bg-card"
                />
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
