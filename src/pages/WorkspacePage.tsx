import { useState, useEffect, useCallback } from 'react';
import { useNavigate } from 'react-router-dom';
import { useAuth } from '@/contexts/AuthContext';
import { db } from '@/db';
import { agentManager } from '@/agents/manager';
import { sessionLogger } from '@/services/logger';
import type { Conversation, Message, MessageStep, Task, ModifiedFile } from '@/types/types';
import { Settings, LogOut, Bot, ChevronDown } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover';
import { toast } from 'sonner';
import LeftPanel from '@/components/workspace/LeftPanel';
import type { AgentInstance } from '@/components/workspace/LeftPanel';
import ChatPanel from '@/components/workspace/ChatPanel';
import RightPanel from '@/components/workspace/RightPanel';
import SettingsDialog from '@/components/workspace/SettingsDialog';
import AddAgentDialog from '@/components/workspace/AddAgentDialog';
import AgentIcon from '@/components/workspace/AgentIcon';

const defaultAgents: AgentInstance[] = [
  { id: 'default-1', agentKey: 'opencode', displayName: '小智', workspace: '.', modelConfig: { type: 'builtin', modelId: 'gpt-4' } },
  { id: 'default-2', agentKey: 'claude-code', displayName: '小文', workspace: '.', modelConfig: { type: 'builtin', modelId: 'claude-3-opus' } },
  { id: 'default-3', agentKey: 'cursor-agent', displayName: '小游', workspace: '.', modelConfig: { type: 'builtin', modelId: 'deepseek-v3' } },
  { id: 'default-4', agentKey: 'codex', displayName: '小柯', workspace: '.', modelConfig: { type: 'builtin', modelId: 'gpt-4' } },
  { id: 'default-5', agentKey: 'custom', displayName: '小C', workspace: '.', modelConfig: { type: 'builtin', modelId: 'gpt-4' } },
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
  const [messages, setMessages] = useState<Message[]>([]);
  const [tasks, setTasks] = useState<Task[]>([]);
  const [modifiedFiles, setModifiedFiles] = useState<ModifiedFile[]>([]);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [isTyping, setIsTyping] = useState(false);
  const [currentModel, setCurrentModel] = useState('gpt-4');
  const [contextPercent, setContextPercent] = useState(15);
  const [agentMode, setAgentMode] = useState<'plan' | 'build'>('build');
  const [currentSkill, setCurrentSkill] = useState('auto');
  const [rightCollapsed, setRightCollapsed] = useState(false);
  const [leftCollapsed, setLeftCollapsed] = useState(false);
  const [addAgentOpen, setAddAgentOpen] = useState(false);
  const [editContent, setEditContent] = useState<string | undefined>(undefined);

  // 智能体实例管理
  const [agentInstances, setAgentInstances] = useState<AgentInstance[]>(getStoredAgents);
  const [activeAgentId, setActiveAgentId] = useState<string>(getStoredActiveAgentId);

  const activeAgent = agentInstances.find((a) => a.id === activeAgentId) || agentInstances[0];

  // 持久化智能体状态
  useEffect(() => {
    localStorage.setItem('agent_instances', JSON.stringify(agentInstances));
  }, [agentInstances]);
  useEffect(() => {
    localStorage.setItem('active_agent_id', activeAgentId);
    localStorage.setItem('selected_agent', activeAgent.agentKey);
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
  }, []);

  // 初始加载数据
  useEffect(() => {
    loadConversations();
    loadTasks();
    loadModifiedFiles();
  }, [loadConversations, loadTasks, loadModifiedFiles]);

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

  // 从 select-agent 返回后自动创建新会话
  useEffect(() => {
    if (!user) return;
    const shouldCreate = localStorage.getItem('should_create_new_session');
    if (shouldCreate !== 'true') return;

    localStorage.removeItem('should_create_new_session');
    const defaultName = localStorage.getItem('default_agent_name');
    if (defaultName) {
      localStorage.removeItem('default_agent_name');
    }

    const timer = setTimeout(async () => {
      const currentAgent = localStorage.getItem('selected_agent') || 'opencode';

      // 如果用户设置了默认名称，更新对应智能体实例
      if (defaultName) {
        const name = defaultName.slice(0, 3);
        const existingInstance = agentInstances.find((a) => a.agentKey === currentAgent);
        if (existingInstance) {
          setAgentInstances((prev) =>
            prev.map((a) => (a.id === existingInstance.id ? { ...a, displayName: name } : a))
          );
        } else {
          const newInstance: AgentInstance = {
            id: Date.now().toString(),
            agentKey: currentAgent,
            displayName: name,
            workspace: localStorage.getItem('default_agent_workspace') || '.',
            modelConfig: { type: 'builtin', modelId: 'gpt-4' },
          };
          setAgentInstances((prev) => [...prev, newInstance]);
          setActiveAgentId(newInstance.id);
        }
      }

      const data = await db.createConversation({
        user_id: user.id,
        title: '新会话',
        agent: currentAgent,
        model: 'gpt-4',
      });

      if (data) {
        setConversations((prev) => [data, ...prev]);
        setActiveConversation(data);
        setMessages([]);
      } else {
        toast.error('创建会话失败');
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
    if (data) {
      setConversations((prev) => [data, ...prev]);
      setActiveConversation(data);
      setMessages([]);
    }
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
    const newInstance: AgentInstance = {
      id: Date.now().toString(),
      agentKey,
      displayName,
      workspace: workspace || '.',
      modelConfig: { type: 'builtin', modelId: 'gpt-4' },
    };
    setAgentInstances((prev) => [...prev, newInstance]);
    setActiveAgentId(newInstance.id);
    toast.success(`已添加智能体 ${displayName}`);

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

  // 激活智能体
  const handleActivateAgent = async (id: string) => {
    setActiveAgentId(id);
    const instance = agentInstances.find((a) => a.id === id);
    if (!instance) return;

    // Ensure agent is managed and started
    let managed = agentManager.getAgent(id);
    if (!managed) {
      try {
        await agentManager.addAgent(instance.agentKey, id, instance.displayName, instance.workspace);
        managed = agentManager.getAgent(id)!;
      } catch (e) {
        toast.error(String(e));
        return;
      }
    }

    if (managed.status.state === 'stopped') {
      await agentManager.startAgent(id);
    }

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

  // 底部栏切换智能体
  const [agentSwitcherOpen, setAgentSwitcherOpen] = useState(false);
  const handleSwitchAgentFromFooter = (id: string) => {
    handleActivateAgent(id);
    setAgentSwitcherOpen(false);
  };

  // 发送消息
  const handleSendMessage = async (content: string) => {
    if (!user || !activeConversation) {
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

    if (userMsg) {
      setMessages((prev) => [...prev, userMsg]);
    }

    // 启动会话日志跟踪
    await sessionLogger.startTrace(activeConversation.id, {
      agent: activeAgent?.agentKey,
      model: currentModel,
      userId: user.id,
    });

    await sessionLogger.logGeneration(
      `gen-${Date.now()}`,
      content,
      undefined,
      { conversationId: activeConversation.id, role: 'user' },
    );

    setIsTyping(true);
    const startTime = Date.now();

    const steps: MessageStep[] = [];
    let finalContent = '';
    let hasError = false;

    const activeAgentInstance = agentInstances.find((a) => a.id === activeAgentId);
    if (!activeAgentInstance) {
      toast.error('未找到活跃智能体');
      setIsTyping(false);
      return;
    }

    try {
      for await (const event of agentManager.sendMessage(activeAgentInstance.id, content)) {
        await sessionLogger.logEvent(event, { conversationId: activeConversation.id });

        switch (event.type) {
          case 'thinking':
            steps.push({ type: 'thinking', content: event.content });
            break;
          case 'tool_use':
            steps.push({
              type: 'tool_use',
              content: `使用工具 ${event.toolName}...`,
              toolName: event.toolName,
              summary: { file: event.args?.file as string, lines: 0, durationMs: 0 },
            });
            break;
          case 'tool_result':
            steps.push({
              type: 'tool_result',
              content: event.result,
              toolName: event.toolName,
              failed: event.failed,
              summary: { file: event.toolName, lines: 0, durationMs: 0 },
            });
            break;
          case 'ask_permission':
            steps.push({
              type: 'ask_permission',
              content: event.message,
              permissionType: event.toolName,
            });
            break;
          case 'ask_user':
            steps.push({
              type: 'ask_user',
              content: '请回答以下问题：',
              questions: event.questions,
            });
            break;
          case 'text_delta':
            finalContent += event.content;
            break;
          case 'error':
            hasError = true;
            toast.error(`智能体错误: ${event.message}`);
            await sessionLogger.logError(event.message, { conversationId: activeConversation.id });
            break;
          case 'done':
            break;
        }
      }
    } catch (error) {
      hasError = true;
      const errMsg = String(error);
      toast.error(`通信错误: ${errMsg}`);
      await sessionLogger.logError(errMsg, { conversationId: activeConversation.id });
    }

    const duration_ms = Date.now() - startTime;

    if (!hasError) {
      const aiMsg = await db.createMessage({
        conversation_id: activeConversation.id,
        role: 'assistant',
        content: finalContent || '（无内容）',
      });

      if (aiMsg) {
        const enriched: Message = {
          ...aiMsg,
          steps,
          is_complete: true,
          token_in: Math.floor(content.length * 0.8),
          token_out: Math.floor(finalContent.length * 0.9),
          duration_ms,
        };
        setMessages((prev) => [...prev, enriched]);
      }

      await sessionLogger.logGeneration(
        `gen-${Date.now()}`,
        content,
        finalContent,
        { conversationId: activeConversation.id, role: 'assistant', durationMs: duration_ms },
      );
    }

    setIsTyping(false);
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
        <div className="flex items-center gap-2">
          <Bot className="w-5 h-5 text-primary" />
          <span className="font-semibold text-sm text-foreground">AI Coding</span>
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

      {/* 底部状态栏 */}
      <div className="h-7 border-t border-border bg-card flex items-center justify-between px-4 shrink-0">
        <div className="flex items-center gap-3">
          <Popover open={agentSwitcherOpen} onOpenChange={setAgentSwitcherOpen}>
            <PopoverTrigger asChild>
              <button
                type="button"
                className="flex items-center gap-1 text-[10px] text-muted-foreground hover:text-foreground transition-colors"
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
                    <span className={`w-5 h-5 rounded text-[10px] font-bold flex items-center justify-center ${config.bg} ${config.color} overflow-hidden`}>
                      <AgentIcon agentKey={instance.agentKey} size={14} />
                    </span>
                    <span className="flex-1 text-left truncate">{instance.displayName}</span>
                    {isActive && <span className="text-[10px] text-primary">当前</span>}
                  </button>
                );
              })}
            </PopoverContent>
          </Popover>
          <span className="text-[10px] text-muted-foreground truncate max-w-[200px]" title={activeAgent.workspace}>
            工作目录：{activeAgent.workspace}
          </span>
        </div>
        <div className="flex items-center gap-3 text-[10px] text-muted-foreground">
          <span>AI Coding v1.0</span>
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
