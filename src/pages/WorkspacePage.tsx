import { useState, useEffect, useRef, useCallback } from 'react';
import { useNavigate } from 'react-router-dom';
import { useAuth } from '@/contexts/AuthContext';
import { db } from '@/db';
import type { Task, ModifiedFile } from '@/types/types';
import { Settings, LogOut, Bot, Plus, MessageSquare } from 'lucide-react';
import { Button } from '@/components/ui/button';
import LeftPanel from '@/components/workspace/LeftPanel';
import ChatPanel from '@/components/workspace/ChatPanel';
import RightPanel from '@/components/workspace/RightPanel';
import SettingsDialog from '@/components/workspace/SettingsDialog';
import AddAgentDialog from '@/components/workspace/AddAgentDialog';
import SessionLogDrawer from '@/components/workspace/SessionLogDrawer';
import WorkspaceFooter from '@/components/workspace/WorkspaceFooter';
import WindowTitleBar from '@/components/common/WindowTitleBar';
import { useChatStore, useLogStore } from '@/stores';
import { useAgentManagement } from '@/hooks/workspace/useAgentManagement';
import { useConversationManager } from '@/hooks/workspace/useConversationManager';
import { useWebSocketListeners } from '@/hooks/workspace/useWebSocketListeners';
import { useMessageHandlers } from '@/hooks/workspace/useMessageHandlers';

const CONTEXT_MENU_OFFSET = 8;
const CONTEXT_MENU_AREA_SELECTOR = '[data-workspace-context-menu]';
const CONTEXT_MENU_ENABLED = 'true';

interface ContextMenuState {
  x: number;
  y: number;
}

export default function WorkspacePage() {
  const { user, signOut } = useAuth();
  const navigate = useNavigate();

  console.log('[WorkspacePage] Rendering, user=', user?.id);

  // Hooks
  const agentMgmt = useAgentManagement(user);
  const {
    agentInstances,
    activeAgentId,
    activeAgent,
    setAgentInstances,
    setActiveAgentId,
    setChatActiveInstanceId,
    setMessages,
    handleConfirmAddAgent,
    handleDeleteAgent,
    handleActivateAgent,
  } = agentMgmt;

  console.log('[WorkspacePage] activeAgent=', activeAgent?.id, activeAgent?.displayName);

  const convMgr = useConversationManager(user, agentInstances, activeAgentId);
  const {
    conversations,
    setConversations,
    activeConversation,
    setActiveConversation,
    conversationsLoaded,
    loadMessages,
    handleSelectConversation,
    handleDoubleClickConversation,
    handleNewConversation,
  } = convMgr;
  console.log('[WorkspacePage] conversationsLoaded=', conversationsLoaded, 'activeConv=', activeConversation?.id);

  useWebSocketListeners();

  const {
    handleSendMessage,
    handleAnswerPermission,
    handleAnswerUserQuestions,
    handleRetryStep,
    handleEditUserMessage,
  } = useMessageHandlers(activeConversation, activeAgentId);

  // UI State
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [addAgentOpen, setAddAgentOpen] = useState(false);
  const [logDrawerOpen, setLogDrawerOpen] = useState(false);
  const [leftCollapsed, setLeftCollapsed] = useState(false);
  const [rightCollapsed, setRightCollapsed] = useState(false);
  const [contextMenu, setContextMenu] = useState<ContextMenuState | null>(null);
  const [editContent, setEditContent] = useState<string | undefined>(undefined);
  const [currentModel, setCurrentModel] = useState('gpt-4');
  const [contextPercent, _setContextPercent] = useState(15);
  const [agentMode, setAgentMode] = useState<'plan' | 'build'>('build');
  const [currentSkill, setCurrentSkill] = useState('auto');

  const clickCountRef = useRef(0);
  const clickTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Store selectors
  const messages = useChatStore((s) => s.messages);
  const isTyping = useChatStore((s) => s.isTyping);
  const todos = useChatStore((s) => s.todos);
  const sessionLogs = useLogStore((s) => s.logs);

  // 任务与文件变更
  const [tasks, setTasks] = useState<Task[]>([]);
  const [modifiedFiles, setModifiedFiles] = useState<ModifiedFile[]>([]);

  const loadTasks = useCallback(async () => {
    if (!user) { return; }
    const data = await db.loadTasks(user.id, 20);
    setTasks(Array.isArray(data) ? data : []);
  }, [user]);

  const loadModifiedFiles = useCallback(async () => {
    if (!user) { return; }
    const data = await db.loadModifiedFiles(user.id, 20);
    setModifiedFiles(Array.isArray(data) ? data : []);
  }, [user]);

  useEffect(() => {
    loadTasks();
    loadModifiedFiles();
  }, [loadTasks, loadModifiedFiles]);

  // 日志加载
  useEffect(() => {
    if (!activeConversation) {
      useLogStore.setState({ logs: [], filteredLogs: [] });
      return;
    }
    useLogStore.getState().loadHistory(activeConversation.id);
  }, [activeConversation]);

  // 上下文菜单关闭
  useEffect(() => {
    if (!contextMenu) { return; }
    const closeContextMenu = () => setContextMenu(null);
    window.addEventListener('click', closeContextMenu);
    window.addEventListener('keydown', closeContextMenu);
    window.addEventListener('resize', closeContextMenu);
    return () => {
      window.removeEventListener('click', closeContextMenu);
      window.removeEventListener('keydown', closeContextMenu);
      window.removeEventListener('resize', closeContextMenu);
    };
  }, [contextMenu]);

  const handleLogoClick = () => {
    clickCountRef.current += 1;
    if (clickTimerRef.current) { clearTimeout(clickTimerRef.current); }
    clickTimerRef.current = setTimeout(() => {
      clickCountRef.current = 0;
    }, 1000);
    if (clickCountRef.current >= 5) {
      clickCountRef.current = 0;
      if (clickTimerRef.current) { clearTimeout(clickTimerRef.current); }
      setLogDrawerOpen((v) => !v);
    }
  };

  // 删除智能体（需要同时清理会话和状态）
  const handleDeleteAgentWithCleanup = useCallback(
    (instanceId: string) => {
      const instance = handleDeleteAgent(instanceId);
      if (!instance) { return; }
      setConversations((prev) => prev.filter((c) => c.agent !== instance.agentKey));
      if (activeAgentId === instanceId) {
        setActiveConversation(null);
        setMessages([]);
      }
    },
    [handleDeleteAgent, activeAgentId, setConversations, setActiveConversation, setMessages]
  );

  // 激活智能体（需要同步会话）
  const handleActivateAgentWithConversation = useCallback(
    async (id: string) => {
      const result = await handleActivateAgent(id, conversations);
      if (result?.conversation) {
        setActiveConversation(result.conversation);
        await loadMessages(result.conversation.id);
      }
    },
    [handleActivateAgent, conversations, setActiveConversation, loadMessages]
  );

  // 双击会话（需要同步智能体）
  const handleDoubleClickWithAgentSwitch = useCallback(
    async (conv: import('@/types/types').Conversation) => {
      await handleDoubleClickConversation(conv, (agentId: string) => {
        setActiveAgentId(agentId);
        setChatActiveInstanceId(agentId);
      });
    },
    [handleDoubleClickConversation, setActiveAgentId, setChatActiveInstanceId]
  );

  // 新建会话
  const handleNewConversationWithAgent = useCallback(async () => {
    const agentKey = activeAgent?.agentKey || 'opencode';
    const data = await handleNewConversation(agentKey);
    if (data) {
      // 如果新建会话的智能体不是当前激活的，切换过去
      const instance = agentInstances.find((a) => a.agentKey === agentKey);
      if (instance && instance.id !== activeAgentId) {
        setActiveAgentId(instance.id);
        setChatActiveInstanceId(instance.id);
      }
    }
  }, [activeAgent, agentInstances, activeAgentId, handleNewConversation, setActiveAgentId, setChatActiveInstanceId]);

  // 添加智能体确认回调
  const handleConfirmAddAgentWithConversation = useCallback(
    async (agentKey: string, displayName: string, workspace: string) => {
      const result = await handleConfirmAddAgent(agentKey, displayName, workspace);
      if (result?.newConversation) {
        setConversations((prev) => [result.newConversation, ...prev]);
        setActiveConversation(result.newConversation);
        setMessages([]);
      }
    },
    [handleConfirmAddAgent, setConversations, setActiveConversation, setMessages]
  );

  // 编辑用户消息
  const handleEditUserMessageWithState = useCallback(
    (content: string) => {
      const result = handleEditUserMessage(content);
      setEditContent(result);
    },
    [handleEditUserMessage]
  );

  // 底部切换智能体
  const handleSwitchAgentFromFooter = useCallback(
    (id: string) => {
      void handleActivateAgentWithConversation(id);
    },
    [handleActivateAgentWithConversation]
  );

  // 上下文菜单处理
  const handleWorkspaceContextMenu = (event: React.MouseEvent<HTMLDivElement>) => {
    const target = event.target;
    if (!(target instanceof HTMLElement)) { return; }
    const menuArea = target.closest(CONTEXT_MENU_AREA_SELECTOR);
    event.preventDefault();
    if (!(menuArea instanceof HTMLElement)) { return; }
    if (menuArea.dataset.workspaceContextMenu !== CONTEXT_MENU_ENABLED) { return; }
    setContextMenu({ x: event.clientX + CONTEXT_MENU_OFFSET, y: event.clientY + CONTEXT_MENU_OFFSET });
  };

  const handleLogout = async () => {
    await signOut();
    navigate('/login');
  };

  return (
    <div className="flex flex-col h-screen bg-background" onContextMenu={handleWorkspaceContextMenu}>
      <WindowTitleBar title="">
        <div
          data-no-drag
          className="flex items-center gap-2 cursor-pointer select-none px-4 h-full [-webkit-app-region:no-drag]"
          onClick={handleLogoClick}
          title="快速点击5次打开日志抽屉"
        >
          <Bot className="w-5 h-5 text-primary" />
          <span className="font-medium text-sm text-foreground">DeepHarness</span>
        </div>
        <div
          data-no-drag
          onMouseDown={(event) => event.stopPropagation()}
          className="flex items-center gap-2 ml-auto mr-2 h-full [-webkit-app-region:no-drag]"
        >
          <span className="text-xs text-muted-foreground mr-2">
            {user?.email?.replace('@local.dev', '')}
          </span>
          <Button
            variant="ghost"
            size="icon"
            onClick={() => setSettingsOpen(true)}
            className="h-7 w-7 text-muted-foreground hover:text-foreground"
          >
            <Settings className="w-4 h-4" />
          </Button>
          <Button
            variant="ghost"
            size="icon"
            onClick={handleLogout}
            className="h-7 w-7 text-muted-foreground hover:text-foreground"
          >
            <LogOut className="w-4 h-4" />
          </Button>
        </div>
      </WindowTitleBar>

      {/* 主内容区 */}
      <div className="flex flex-col flex-1 min-h-0">
        <div className="flex flex-1 min-h-0">
          <LeftPanel
            conversations={conversations}
            activeConversation={activeConversation}
            agentInstances={agentInstances}
            activeAgentId={activeAgentId}
            messages={messages}
            workspace={activeAgent.workspace}
            collapsed={leftCollapsed}
            onToggleCollapse={() => setLeftCollapsed((v) => !v)}
            onSelectConversation={handleSelectConversation}
            onDoubleClickConversation={handleDoubleClickWithAgentSwitch}
            onNewConversation={handleNewConversationWithAgent}
            onAddAgent={() => setAddAgentOpen(true)}
            onActivateAgent={handleActivateAgentWithConversation}
            onDeleteAgent={handleDeleteAgentWithCleanup}
          />

          <ChatPanel
            messages={messages}
            isTyping={isTyping}
            activeConversation={activeConversation}
            conversations={conversations}
            activeAgentName={activeAgent.displayName}
            activeAgentType={activeAgent.agentKey}
            currentModel={currentModel}
            contextPercent={contextPercent}
            agentMode={agentMode}
            currentSkill={currentSkill}
            editContent={editContent}
            onSendMessage={handleSendMessage}
            onAgentModeChange={setAgentMode}
            onModelChange={setCurrentModel}
            onSkillChange={setCurrentSkill}
            onSelectConversation={handleSelectConversation}
            onAnswerPermission={handleAnswerPermission}
            onAnswerUserQuestions={handleAnswerUserQuestions}
            onEditUserMessage={handleEditUserMessageWithState}
            onRetryStep={handleRetryStep}
          />

          <RightPanel
            tasks={tasks}
            todos={todos}
            modifiedFiles={modifiedFiles}
            workspace={activeAgent.workspace}
            collapsed={rightCollapsed}
            onToggleCollapse={() => setRightCollapsed((v) => !v)}
          />
        </div>

        {logDrawerOpen && activeConversation && (
          <SessionLogDrawer
            logs={sessionLogs}
            onClose={() => setLogDrawerOpen(false)}
            onClear={() => useLogStore.setState({ logs: [], filteredLogs: [] })}
          />
        )}
      </div>

      {/* 上下文菜单 */}
      {contextMenu && (
        <div
          className="fixed z-[2147483646] min-w-36 rounded-md border border-border bg-popover p-1 text-popover-foreground shadow-lg"
          style={{ left: contextMenu.x, top: contextMenu.y }}
          onClick={(event) => event.stopPropagation()}
        >
          <button
            type="button"
            onClick={() => { setContextMenu(null); setAddAgentOpen(true); }}
            className="flex w-full items-center gap-2 rounded-sm px-2.5 py-1.5 text-xs text-foreground transition-colors hover:bg-accent hover:text-accent-foreground"
          >
            <Plus className="w-3.5 h-3.5 text-muted-foreground" />
            智能体
          </button>
          <button
            type="button"
            onClick={() => { setContextMenu(null); void handleNewConversationWithAgent(); }}
            className="flex w-full items-center gap-2 rounded-sm px-2.5 py-1.5 text-xs text-foreground transition-colors hover:bg-accent hover:text-accent-foreground"
          >
            <MessageSquare className="w-3.5 h-3.5 text-muted-foreground" />
            新建会话
          </button>
        </div>
      )}

      <WorkspaceFooter
        agentInstances={agentInstances}
        activeAgentId={activeAgentId}
        activeAgent={activeAgent}
        onSwitchAgent={handleSwitchAgentFromFooter}
      />

      <SettingsDialog
        open={settingsOpen}
        onOpenChange={setSettingsOpen}
        agents={agentInstances}
        onUpdateAgents={setAgentInstances}
      />

      <AddAgentDialog
        open={addAgentOpen}
        onOpenChange={setAddAgentOpen}
        onAddAgent={handleConfirmAddAgentWithConversation}
        existingNames={agentInstances.map((a) => a.displayName)}
      />
    </div>
  );
}
