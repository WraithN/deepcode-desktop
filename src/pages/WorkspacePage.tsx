import { useState, useEffect, useCallback, useRef } from 'react';
import { useNavigate } from 'react-router-dom';
import { useAuth } from '@/contexts/AuthContext';
import { db } from '@/db';
import type { Conversation, Task, ModifiedFile } from '@/types/types';
import { Settings, LogOut, Bot, ChevronDown } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover';
import { toast } from 'sonner';
import LeftPanel from '@/components/workspace/LeftPanel';
import type { AgentInstance } from '@/stores';
import ChatPanel from '@/components/workspace/ChatPanel';
import RightPanel from '@/components/workspace/RightPanel';
import SettingsDialog from '@/components/workspace/SettingsDialog';
import AddAgentDialog from '@/components/workspace/AddAgentDialog';
import AgentIcon from '@/components/workspace/AgentIcon';
import SessionLogDrawer from '@/components/workspace/SessionLogDrawer';
import { invoke } from '@tauri-apps/api/core';
import { useWebSocketStore, useChatStore, useAgentStore, useLogStore } from '@/stores';
import type { AgentEvent } from '@/stores';
import { generateShortId, formatIdShort } from '@/lib/id';

const defaultAgents: AgentInstance[] = [
  { id: 'default-1', agentKey: 'opencode', displayName: '小智', workspace: '.', modelConfig: { type: 'builtin', modelId: 'gpt-4' }, status: 'stopped' },
  { id: 'default-2', agentKey: 'claude-code', displayName: '小文', workspace: '.', modelConfig: { type: 'builtin', modelId: 'claude-3-opus' }, status: 'stopped' },
  { id: 'default-3', agentKey: 'cursor-agent', displayName: '小游', workspace: '.', modelConfig: { type: 'builtin', modelId: 'deepseek-v3' }, status: 'stopped' },
  { id: 'default-4', agentKey: 'codex', displayName: '小柯', workspace: '.', modelConfig: { type: 'builtin', modelId: 'gpt-4' }, status: 'stopped' },
  { id: 'default-5', agentKey: 'custom', displayName: '小C', workspace: '.', modelConfig: { type: 'builtin', modelId: 'gpt-4' }, status: 'stopped' },
];

function getStoredAgents(): AgentInstance[] {
  try {
    const raw = localStorage.getItem('agent_instances');
    if (raw) {
      const parsed = JSON.parse(raw) as AgentInstance[];
      // 兼容旧数据：补全缺失字段
      return parsed.map((a) => ({
        ...a,
        workspace: a.workspace || '.',
        modelConfig: a.modelConfig || { type: 'builtin', modelId: 'gpt-4' },
        status: a.status || 'stopped',
      }));
    }
  } catch { /* ignore */ }
  return defaultAgents;
}

function getStoredActiveAgentId(): string {
  return localStorage.getItem('active_agent_id') || defaultAgents[0].id;
}

export default function WorkspacePage() {
  const { user, signOut } = useAuth();
  const navigate = useNavigate();
  const [conversations, setConversations] = useState<Conversation[]>([]);
  const [activeConversation, setActiveConversation] = useState<Conversation | null>(null);
  const [tasks, setTasks] = useState<Task[]>([]);
  const [modifiedFiles, setModifiedFiles] = useState<ModifiedFile[]>([]);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [currentModel, setCurrentModel] = useState('gpt-4');
  const [contextPercent, setContextPercent] = useState(15);
  const [agentMode, setAgentMode] = useState<'plan' | 'build'>('build');
  const [currentSkill, setCurrentSkill] = useState('auto');
  const [rightCollapsed, setRightCollapsed] = useState(false);
  const [leftCollapsed, setLeftCollapsed] = useState(false);
  const [addAgentOpen, setAddAgentOpen] = useState(false);
  const [editContent, setEditContent] = useState<string | undefined>(undefined);
  const [logDrawerOpen, setLogDrawerOpen] = useState(false);
  const clickCountRef = useRef(0);
  const clickTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Store hooks
  const messages = useChatStore((s) => s.messages);
  const setMessages = useChatStore((s) => s.setMessages);
  const isStreaming = useChatStore((s) => s.isStreaming);
  const setIsStreaming = useChatStore((s) => s.setIsStreaming);
  const chatSendMessage = useChatStore((s) => s.sendMessage);
  const appendEvent = useChatStore((s) => s.appendEvent);
  const setChatActiveInstanceId = useChatStore((s) => s.setActiveInstanceId);

  const agentInstances = useAgentStore((s) => s.instances);
  const setAgentInstances = useAgentStore((s) => s.setInstances);
  const addAgentInstance = useAgentStore((s) => s.addInstance);
  const updateAgentInstance = useAgentStore((s) => s.updateInstance);
  const removeAgentInstance = useAgentStore((s) => s.removeInstance);
  const activeAgentId = useAgentStore((s) => s.activeInstanceId);
  const setActiveAgentId = useAgentStore((s) => s.setActiveInstance);

  const logStoreAppend = useLogStore((s) => s.appendLog);
  const sessionLogs = useLogStore((s) => s.logs);
  const isTyping = isStreaming;

  // 智能体实例管理
  useEffect(() => {
    const stored = getStoredAgents();
    if (stored.length > 0) {
      setAgentInstances(stored);
      const activeId = getStoredActiveAgentId();
      setActiveAgentId(activeId);
      setChatActiveInstanceId(activeId);
    }
  }, [setAgentInstances, setActiveAgentId, setChatActiveInstanceId]);

  const activeAgent = agentInstances.find((a) => a.id === activeAgentId) || agentInstances[0] || defaultAgents[0];

  // 初始化 WebSocket 连接
  useEffect(() => {
    const init = async () => {
      try {
        const url = await invoke<string>('get_websocket_url');
        await useWebSocketStore.getState().connect(url);
      } catch (e) {
        console.error('Failed to connect WebSocket:', e);
      }
    };
    init();
  }, []);

  // 订阅 WebSocket 事件
  useEffect(() => {
    const ws = useWebSocketStore.getState();

    const unsubEvents = ws.subscribe('agent.event', (params) => {
      appendEvent(params as AgentEvent);
    });

    const unsubStatus = ws.subscribe('agent.status', (params) => {
      const { instanceId, status, pid } = params as { instanceId: string; status: string; pid?: number };
      useAgentStore.getState().updateInstanceStatus(instanceId, status as AgentInstance['status'], pid);
    });

    const unsubLogs = ws.subscribe('session.log', (params) => {
      logStoreAppend(params as Parameters<typeof logStoreAppend>[0]);
    });

    return () => {
      unsubEvents();
      unsubStatus();
      unsubLogs();
    };
  }, [appendEvent, logStoreAppend]);

  // 将相对路径的工作目录解析为绝对路径
  useEffect(() => {
    const resolveWorkspaces = async () => {
      const needsResolve = agentInstances.some((a) => a.workspace === '.' || !a.workspace.startsWith('/'));
      if (!needsResolve) return;
      try {
        const cwd = await invoke<string>('get_current_dir');
        for (const a of agentInstances) {
          if (a.workspace === '.' || a.workspace === '') {
            updateAgentInstance(a.id, { workspace: cwd });
          } else if (!a.workspace.startsWith('/')) {
            updateAgentInstance(a.id, { workspace: `${cwd}/${a.workspace}` });
          }
        }
      } catch {
        // ignore
      }
    };
    resolveWorkspaces();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // 持久化智能体状态
  useEffect(() => {
    localStorage.setItem('agent_instances', JSON.stringify(agentInstances));
  }, [agentInstances]);
  useEffect(() => {
    if (activeAgentId) {
      localStorage.setItem('active_agent_id', activeAgentId);
    }
    if (activeAgent) {
      localStorage.setItem('selected_agent', activeAgent.agentKey);
    }
  }, [activeAgentId, activeAgent]);

  // 加载会话列表
  const loadConversations = useCallback(async () => {
    if (!user) return;
    const data = await db.loadConversations(user.id, 50);
    setConversations(Array.isArray(data) ? data : []);
  }, [user]);

  // 加载任务
  const loadTasks = useCallback(async () => {
    if (!user) return;
    const data = await db.loadTasks(user.id, 20);
    setTasks(Array.isArray(data) ? data : []);
  }, [user]);

  // 加载修改文件
  const loadModifiedFiles = useCallback(async () => {
    if (!user) return;
    const data = await db.loadModifiedFiles(user.id, 20);
    setModifiedFiles(Array.isArray(data) ? data : []);
  }, [user]);

  // 加载消息
  const loadMessages = useCallback(async (conversationId: string) => {
    const data = await db.loadMessages(conversationId, 100);
    setMessages(Array.isArray(data) ? data : []);
  }, [setMessages]);

  // 初始加载数据
  useEffect(() => {
    loadConversations();
    loadTasks();
    loadModifiedFiles();
  }, [loadConversations, loadTasks, loadModifiedFiles]);

  // Load logs when active conversation changes
  useEffect(() => {
    if (!activeConversation) {
      useLogStore.setState({ logs: [], filteredLogs: [] });
      return;
    }
    useLogStore.getState().loadHistory(activeConversation.id);
  }, [activeConversation]);

  // 自动测试发送消息（开发调试用）
  // useEffect(() => {
  //   if (!activeConversation || !activeAgentId) return;
  //   const timer = setTimeout(() => {
  //     console.log('[WorkspacePage] auto-sending test message');
  //     handleSendMessage('你好，这是一个测试消息');
  //   }, 3000);
  //   return () => clearTimeout(timer);
  //   // eslint-disable-next-line react-hooks/exhaustive-deps
  // }, [activeConversation, activeAgentId]);

  // 如果没有任务，创建mock任务（三种状态各一个）
  useEffect(() => {
    if (!user || tasks.length > 0) return;
    const createMockTasks = async () => {
      const mockTasks = [
        { title: '初始化项目环境', status: 'completed' as const },
        { title: '实现核心功能模块', status: 'in_progress' as const },
        { title: '等待代码审查', status: 'pending' as const },
      ];
      for (const task of mockTasks) {
        const data = await db.createTask({
          user_id: user.id,
          title: task.title,
          status: task.status,
          conversation_id: null,
        });
        if (data) setTasks((prev) => [data, ...prev]);
      }
    };
    createMockTasks();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [user]);

  // 从 select-agent 返回后加载会话
  useEffect(() => {
    if (!user) return;
    // SelectAgentPage 已经创建了 instance，这里只需创建会话
    const timer = setTimeout(async () => {
      if (conversations.length === 0 && agentInstances.length > 0) {
        const activeAgent = agentInstances[0];
        const data = await db.createConversation({
          user_id: user.id,
          title: '新会话',
          agent: activeAgent.agentKey,
          model: 'gpt-4',
        });

        if (data) {
          setConversations((prev) => [data, ...prev]);
          setActiveConversation(data);
          setMessages([]);
        }
      }
    }, 200);

    return () => clearTimeout(timer);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [user]);

  // 选择会话（单击预览，不切换主窗口）
  const handleSelectConversation = async (conv: Conversation) => {
    // 单击仅做预览，不加载消息到主窗口
    setActiveConversation(conv);
    await loadMessages(conv.id);
  };

  // 双击会话切换主窗口
  const handleDoubleClickConversation = async (conv: Conversation) => {
    setActiveConversation(conv);
    await loadMessages(conv.id);
    // 同步激活该会话所属的智能体
    const agentInstance = agentInstances.find((a) => a.agentKey === conv.agent);
    if (agentInstance) {
      setActiveAgentId(agentInstance.id);
      setChatActiveInstanceId(agentInstance.id);
    }
  };

  // 新建会话（使用当前激活智能体）
  const handleNewConversation = async () => {
    if (!user) return;
    const agentKey = activeAgent?.agentKey || 'opencode';
    const data = await db.createConversation({
      user_id: user.id,
      title: '新会话',
      agent: agentKey,
      model: 'gpt-4',
    });
    if (!data) {
      toast.error('创建会话失败');
      return;
    }
    setConversations((prev) => [data, ...prev]);
    setActiveConversation(data);
    setMessages([]);
  };

  // 新建智能体会话（点击+号）
  const handleNewAgentSession = () => {
    localStorage.setItem('should_create_new_session', 'true');
    navigate('/select-agent');
  };

  // 添加智能体
  const handleAddAgent = () => {
    setAddAgentOpen(true);
  };

  // 确认添加智能体
  const handleConfirmAddAgent = async (agentKey: string, displayName: string, workspace: string) => {
    const instanceId = generateShortId();
    const newInstance: AgentInstance = {
      id: instanceId,
      agentKey,
      displayName,
      workspace: workspace || '.',
      modelConfig: { type: 'builtin', modelId: 'gpt-4' },
      status: 'stopped',
    };
    addAgentInstance(newInstance);
    toast.success(`已添加智能体 ${displayName} [${formatIdShort(instanceId)}]`);

    // 自动为该智能体创建一个新会话
    if (user) {
      const data = await db.createConversation({
        user_id: user.id,
        title: '新会话',
        agent: agentKey,
        model: 'gpt-4',
      });
      if (data) {
        setConversations((prev) => [data, ...prev]);
        setActiveConversation(data);
        setMessages([]);
      }
    }
  };

  // 删除智能体
  const handleDeleteAgent = (instanceId: string) => {
    const instance = agentInstances.find((a) => a.id === instanceId);
    if (!instance) return;
    
    // 删除相关会话
    setConversations((prev) => prev.filter((c) => c.agent !== instance.agentKey));
    
    // 如果删除的是当前活跃的，清空状态
    if (activeAgentId === instanceId) {
      setActiveAgentId(null);
      setChatActiveInstanceId(null);
      setActiveConversation(null);
      setMessages([]);
    }
    
    // 从 store 中删除
    removeAgentInstance(instanceId);
    toast.success(`已删除智能体 ${instance.displayName}`);
  };

  // 清空所有 mock 数据
  const handleClearAllData = () => {
    // 清空智能体
    setAgentInstances([]);
    setActiveAgentId(null);
    setChatActiveInstanceId(null);
    
    // 清空会话和消息
    setConversations([]);
    setActiveConversation(null);
    setMessages([]);
    
    // 清空本地存储
    localStorage.removeItem('agent_instances');
    localStorage.removeItem('active_agent_id');
    localStorage.removeItem('selected_agent');
    localStorage.removeItem('should_create_new_session');
    localStorage.removeItem('default_agent_name');
    localStorage.removeItem('default_agent_workspace');
    
    toast.success('已清空所有数据');
  };

  // 激活智能体
  const handleActivateAgent = async (id: string) => {
    setActiveAgentId(id);
    setChatActiveInstanceId(id);
    const instance = agentInstances.find((a) => a.id === id);
    if (!instance) return;

    // 查找该智能体类型的最新会话
    const agentConvs = conversations.filter((c) => c.agent === instance.agentKey);
    if (agentConvs.length > 0) {
      setActiveConversation(agentConvs[0]);
      await loadMessages(agentConvs[0].id);
    } else if (user) {
      // 没有会话则自动创建
      const data = await db.createConversation({
        user_id: user.id,
        title: '新会话',
        agent: instance.agentKey,
        model: 'gpt-4',
      });
      if (data) {
        setConversations((prev) => [data, ...prev]);
        setActiveConversation(data);
        setMessages([]);
      }
    }
  };

  const handleLogoClick = () => {
    clickCountRef.current += 1;
    if (clickTimerRef.current) clearTimeout(clickTimerRef.current);
    clickTimerRef.current = setTimeout(() => {
      clickCountRef.current = 0;
    }, 1000);

    if (clickCountRef.current >= 5) {
      clickCountRef.current = 0;
      if (clickTimerRef.current) clearTimeout(clickTimerRef.current);
      setLogDrawerOpen((v) => !v);
    }
  };

  // 底部栏切换智能体
  const [agentSwitcherOpen, setAgentSwitcherOpen] = useState(false);
  const handleSwitchAgentFromFooter = (id: string) => {
    handleActivateAgent(id);
    setAgentSwitcherOpen(false);
  };

  // 发送消息
  const handleSendMessage = async (content: string) => {
    if (!activeConversation) {
      toast.error('请先创建或选择一个会话');
      return;
    }
    if (!user) {
      toast.error('请先创建或选择一个会话');
      return;
    }

    const userMsg = await db.createMessage({
      conversation_id: activeConversation.id,
      role: 'user',
      content,
    });

    if (!userMsg) {
      toast.error('发送消息失败');
      return;
    }

    setMessages((prev) => [...prev, userMsg]);
    setIsStreaming(true);

    // 创建流式临时消息
    const streamingMsg = {
      id: `streaming-${Date.now()}`,
      conversation_id: activeConversation.id,
      role: 'assistant' as const,
      content: '',
      steps: [],
      is_complete: false,
      created_at: new Date().toISOString(),
    };
    setMessages((prev) => [...prev, streamingMsg]);

    try {
      await chatSendMessage(content);
    } catch (error) {
      const errMsg = String(error);
      toast.error(`通信错误: ${errMsg}`);
      setMessages((prev) => prev.filter((msg) => msg.id !== streamingMsg.id));
    }

    setIsStreaming(false);
  };

  // 处理权限询问回答
  const handleAnswerPermission = (stepIndex: number, answer: 'once' | 'session' | 'deny') => {
    const label = answer === 'once' ? '本次同意' : answer === 'session' ? '本Session同意' : '不同意';
    toast.success(`已${label}`);
    setContextPercent((prev) => Math.min(prev + 5, 100));
  };

  // 处理用户问题回答
  const handleAnswerUserQuestions = (stepIndex: number, answers: Record<string, string>) => {
    toast.success('已提交回答');
    setContextPercent((prev) => Math.min(prev + 3, 100));
  };


  // 编辑用户消息（将内容填入输入区域）
  const handleEditUserMessage = (content: string) => {
    setEditContent(content);
    toast.info('已加载到输入框，你可以修改后重新发送');
  };

  // 重试失败的步骤
  const handleRetryStep = (messageId: string, stepIndex: number) => {
    toast.success('正在重试...');
    setMessages((prev) =>
      prev.map((msg) => {
        if (msg.id !== messageId || !msg.steps) return msg;
        const newSteps = [...msg.steps];
        if (newSteps[stepIndex]) {
          newSteps[stepIndex] = { ...newSteps[stepIndex], failed: false, type: 'tool_use' };
        }
        return { ...msg, steps: newSteps };
      })
    );
  };

  // 退出登录
  const handleLogout = async () => {
    await signOut();
    navigate('/login');
  };

  return (
    <div className="flex flex-col h-screen bg-background">
      {/* 顶部栏 */}
      <header className="h-10 border-b border-border flex items-center justify-between px-4 shrink-0 bg-card">
        <div
          className="flex items-center gap-2 cursor-pointer select-none"
          onClick={handleLogoClick}
          title="快速点击5次打开日志抽屉"
        >
          <Bot className="w-5 h-5 text-primary" />
          <span className="font-semibold text-sm text-foreground">DeepHarness</span>
        </div>
        <div className="flex items-center gap-2">
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
      </header>

      {/* 主内容区 */}
      <div className="flex flex-col flex-1 min-h-0">
        <div className="flex flex-1 min-h-0">
          {/* 左侧两级边栏 */}
          <LeftPanel
            conversations={conversations}
            activeConversation={activeConversation}
            agentInstances={agentInstances}
            activeAgentId={activeAgentId}
            messages={messages}
            collapsed={leftCollapsed}
            onToggleCollapse={() => setLeftCollapsed((v) => !v)}
            onSelectConversation={handleSelectConversation}
            onDoubleClickConversation={handleDoubleClickConversation}
            onNewConversation={handleNewConversation}
            onAddAgent={handleAddAgent}
            onActivateAgent={handleActivateAgent}
            onDeleteAgent={handleDeleteAgent}
          />

          {/* 中间会话区 */}
          <ChatPanel
            messages={messages}
            isTyping={isTyping}
            activeConversation={activeConversation}
            conversations={conversations}
            activeAgentName={activeAgent.displayName}
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
            onEditUserMessage={handleEditUserMessage}
            onRetryStep={handleRetryStep}
          />

          {/* 右侧栏 */}
          <RightPanel
            tasks={tasks}
            modifiedFiles={modifiedFiles}
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

      {/* 底部状态栏 */}
      <div className="h-7 border-t border-border bg-card flex items-center justify-between px-4 shrink-0">
        <div className="flex items-center gap-3">
          <Popover open={agentSwitcherOpen} onOpenChange={setAgentSwitcherOpen}>
            <PopoverTrigger asChild>
              <button
                type="button"
                className="flex items-center gap-1 text-[11px] text-muted-foreground hover:text-foreground transition-colors"
              >
                <Bot className="w-2.5 h-2.5" />
                <span>{activeAgent.displayName}</span>
                <ChevronDown className="w-2.5 h-2.5" />
              </button>
            </PopoverTrigger>
            <PopoverContent side="top" align="start" className="w-48 p-1">
              {agentInstances.map((instance) => {
                const agentConfig: Record<string, { color: string; bg: string; letter: string }> = {
                  opencode: { color: 'text-green-400', bg: 'bg-green-400/15', letter: 'O' },
                  'claude-code': { color: 'text-orange-400', bg: 'bg-orange-400/15', letter: 'C' },
                  'cursor-agent': { color: 'text-blue-400', bg: 'bg-blue-400/15', letter: 'C' },
                  codex: { color: 'text-purple-400', bg: 'bg-purple-400/15', letter: 'X' },
                  custom: { color: 'text-primary', bg: 'bg-primary/15', letter: 'C' },
                };
                const config = agentConfig[instance.agentKey] || agentConfig.opencode;
                const isActive = activeAgentId === instance.id;
                return (
                  <button
                    key={instance.id}
                    type="button"
                    onClick={() => handleSwitchAgentFromFooter(instance.id)}
                    className={`w-full flex items-center gap-2 px-2.5 py-1.5 text-xs rounded transition-colors ${
                      isActive ? 'bg-primary/10 text-primary' : 'text-foreground hover:bg-secondary'
                    }`}
                  >
                    <span className={`w-5 h-5 rounded text-[11px] font-bold flex items-center justify-center ${config.bg} ${config.color} overflow-hidden`}>
                      <AgentIcon agentKey={instance.agentKey} size={14} />
                    </span>
                    <span className="flex-1 text-left truncate">{instance.displayName}</span>
                    {isActive && <span className="text-[11px] text-primary">当前</span>}
                  </button>
                );
              })}
            </PopoverContent>
          </Popover>
          <span className="text-[11px] text-muted-foreground truncate max-w-[200px]" title={activeAgent.workspace}>
            工作目录：{activeAgent.workspace}
          </span>
        </div>
        <div className="flex items-center gap-3 text-[11px] text-muted-foreground">
          <span>DeepHarness v1.0</span>
        </div>
      </div>

      {/* 设置面板 */}
      <SettingsDialog
        open={settingsOpen}
        onOpenChange={setSettingsOpen}
        agents={agentInstances}
        onUpdateAgents={setAgentInstances}
      />

      {/* 添加智能体弹窗 */}
      <AddAgentDialog
        open={addAgentOpen}
        onOpenChange={setAddAgentOpen}
        onAddAgent={handleConfirmAddAgent}
        existingNames={agentInstances.map((a) => a.displayName)}
      />
    </div>
  );
}
