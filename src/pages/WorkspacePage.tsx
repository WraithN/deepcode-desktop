import { useState, useEffect, useCallback } from 'react';
import { useNavigate } from 'react-router-dom';
import { useAuth } from '@/contexts/AuthContext';
import { supabase } from '@/db/supabase';
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

const defaultAgents: AgentInstance[] = [
  { id: 'default-1', agentKey: 'opencode', displayName: '小智', workspace: '.', modelConfig: { type: 'builtin', modelId: 'gpt-4' } },
  { id: 'default-2', agentKey: 'claude-code', displayName: '小文', workspace: '.', modelConfig: { type: 'builtin', modelId: 'claude-3-opus' } },
  { id: 'default-3', agentKey: 'cursor-agent', displayName: '小游', workspace: '.', modelConfig: { type: 'builtin', modelId: 'deepseek-v3' } },
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
    const { data, error } = await supabase
      .from('conversations')
      .select('*')
      .eq('user_id', user.id)
      .order('updated_at', { ascending: false })
      .limit(50);
    if (error) {
      console.error('加载会话失败:', error);
      return;
    }
    setConversations(Array.isArray(data) ? data : []);
  }, [user]);

  // 加载任务
  const loadTasks = useCallback(async () => {
    if (!user) return;
    const { data, error } = await supabase
      .from('tasks')
      .select('*')
      .eq('user_id', user.id)
      .order('created_at', { ascending: false })
      .limit(20);
    if (error) {
      console.error('加载任务失败:', error);
      return;
    }
    setTasks(Array.isArray(data) ? data : []);
  }, [user]);

  // 加载修改文件
  const loadModifiedFiles = useCallback(async () => {
    if (!user) return;
    const { data, error } = await supabase
      .from('modified_files')
      .select('*')
      .eq('user_id', user.id)
      .order('created_at', { ascending: false })
      .limit(20);
    if (error) {
      console.error('加载修改文件失败:', error);
      return;
    }
    setModifiedFiles(Array.isArray(data) ? data : []);
  }, [user]);

  // 加载消息
  const loadMessages = useCallback(async (conversationId: string) => {
    const { data, error } = await supabase
      .from('messages')
      .select('*')
      .eq('conversation_id', conversationId)
      .order('created_at', { ascending: true })
      .limit(100);
    if (error) {
      console.error('加载消息失败:', error);
      return;
    }
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
        const { data } = await supabase
          .from('tasks')
          .insert({
            user_id: user.id,
            title: task.title,
            status: task.status,
          })
          .select()
          .maybeSingle();
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

      const { data, error } = await supabase
        .from('conversations')
        .insert({
          user_id: user.id,
          title: '新会话',
          agent: currentAgent,
          model: 'gpt-4',
        })
        .select()
        .maybeSingle();

      if (!error && data) {
        setConversations((prev) => [data, ...prev]);
        setActiveConversation(data);
        setMessages([]);
      } else if (error) {
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
    const { data, error } = await supabase
      .from('conversations')
      .insert({
        user_id: user.id,
        title: '新会话',
        agent: agentKey,
        model: 'gpt-4',
      })
      .select()
      .maybeSingle();
    if (error) {
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
      const { data, error } = await supabase
        .from('conversations')
        .insert({
          user_id: user.id,
          title: '新会话',
          agent: agentKey,
          model: 'gpt-4',
        })
        .select()
        .maybeSingle();
      if (!error && data) {
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

    // 查找该智能体类型的最新会话
    const agentConvs = conversations.filter((c) => c.agent === instance.agentKey);
    if (agentConvs.length > 0) {
      setActiveConversation(agentConvs[0]);
      await loadMessages(agentConvs[0].id);
    } else if (user) {
      // 没有会话则自动创建
      const { data, error } = await supabase
        .from('conversations')
        .insert({
          user_id: user.id,
          title: '新会话',
          agent: instance.agentKey,
          model: 'gpt-4',
        })
        .select()
        .maybeSingle();
      if (!error && data) {
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

    const { data: userMsg, error: msgError } = await supabase
      .from('messages')
      .insert({
        conversation_id: activeConversation.id,
        role: 'user',
        content,
      })
      .select()
      .maybeSingle();

    if (msgError) {
      toast.error('发送消息失败');
      return;
    }

    if (userMsg) {
      setMessages((prev) => [...prev, userMsg]);
    }

    // 模拟AI回复
    setIsTyping(true);
    const startTime = Date.now();
    setTimeout(async () => {
      const { steps, finalContent, token_in, token_out } = generateAIReply(content);
      const duration_ms = Date.now() - startTime;

      // 先插入一个未完成的AI消息占位
      const { data: aiMsg, error: aiError } = await supabase
        .from('messages')
        .insert({
          conversation_id: activeConversation.id,
          role: 'assistant',
          content: finalContent,
        })
        .select()
        .maybeSingle();

      if (aiError) {
        toast.error('AI回复失败');
        setIsTyping(false);
        return;
      }

      if (aiMsg) {
        const enrichedMsg: Message = {
          ...aiMsg,
          steps,
          is_complete: false,
          token_in,
          token_out,
          duration_ms,
        };
        setMessages((prev) => [...prev, enrichedMsg]);

        // 模拟流式完成：先显示进行中，1秒后标记为完成
        setTimeout(() => {
          setMessages((prev) =>
            prev.map((m) =>
              m.id === aiMsg.id ? { ...m, is_complete: true } : m
            )
          );
        }, 1200);
      }

      // 更新会话标题
      if (messages.length === 0) {
        const title = content.length > 20 ? content.substring(0, 20) + '...' : content;
        await supabase
          .from('conversations')
          .update({ title, updated_at: new Date().toISOString() })
          .eq('id', activeConversation.id);
        setActiveConversation((prev) => prev ? { ...prev, title } : null);
        loadConversations();
      }

      // 模拟添加三种状态的任务
      const statuses: Array<'pending' | 'in_progress' | 'completed'> = ['pending', 'in_progress', 'completed'];
      for (const status of statuses) {
        if (Math.random() > 0.3) {
          const { data: newTask } = await supabase
            .from('tasks')
            .insert({
              user_id: user.id,
              conversation_id: activeConversation.id,
              title: status === 'completed' ? '环境初始化' : status === 'in_progress' ? `实现${content.substring(0, 10)}功能` : '代码审查',
              status,
            })
            .select()
            .maybeSingle();
          if (newTask) setTasks((prev) => [newTask, ...prev]);
        }
      }

      if (Math.random() > 0.3) {
        const files = ['src/components/App.tsx', 'src/utils/helpers.ts', 'src/pages/Home.tsx', 'src/styles/main.css'];
        const changeTypes: Array<'created' | 'modified' | 'deleted'> = ['created', 'modified', 'modified'];
        const diffs: Record<string, string> = {
          'src/components/App.tsx': `diff --git a/src/components/App.tsx b/src/components/App.tsx\n--- a/src/components/App.tsx\n+++ b/src/components/App.tsx\n@@ -1,5 +1,7 @@\n import React from "react";\n+import { useState } from "react";\n \n export default function App() {\n+  const [count, setCount] = useState(0);\n   return <div>Hello World</div>;\n }`,
          'src/utils/helpers.ts': `diff --git a/src/utils/helpers.ts b/src/utils/helpers.ts\n--- a/src/utils/helpers.ts\n+++ b/src/utils/helpers.ts\n@@ -1,3 +1,8 @@\n export function formatDate(date: Date) {\n   return date.toISOString();\n }\n+\n+export function debounce(fn: Function, ms: number) {\n+  let timer: ReturnType<typeof setTimeout>;\n+  return (...args: any[]) => { clearTimeout(timer); timer = setTimeout(() => fn(...args), ms); };\n+}`,
          'src/pages/Home.tsx': `diff --git a/src/pages/Home.tsx b/src/pages/Home.tsx\n--- a/src/pages/Home.tsx\n+++ b/src/pages/Home.tsx\n@@ -1,5 +1,9 @@\n import React from "react";\n+import { Hero } from "@/components/Hero";\n \n export default function Home() {\n-  return <div>Home</div>;\n+  return (\n+    <div>\n+      <Hero title="Welcome" />\n+    </div>\n+  );\n }`,
          'src/styles/main.css': `diff --git a/src/styles/main.css b/src/styles/main.css\n--- a/src/styles/main.css\n+++ b/src/styles/main.css\n@@ -1,3 +1,7 @@\n body {\n   margin: 0;\n+  font-family: system-ui, sans-serif;\n+  background: #1e1e1e;\n+  color: #e0e0e0;\n }`,
        };
        const filePath = files[Math.floor(Math.random() * files.length)];
        const { data: newFile } = await supabase
          .from('modified_files')
          .insert({
            user_id: user.id,
            conversation_id: activeConversation.id,
            file_path: filePath,
            change_type: changeTypes[Math.floor(Math.random() * changeTypes.length)],
          })
          .select()
          .maybeSingle();
        if (newFile) {
          const enriched = { ...newFile, diff: diffs[filePath] || diffs['src/components/App.tsx'] };
          setModifiedFiles((prev) => [enriched, ...prev]);
        }
      }

      setContextPercent((prev) => Math.min(prev + Math.floor(Math.random() * 8) + 2, 100));
      setIsTyping(false);
    }, 1500);
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

  // 模拟AI回复
  const generateAIReply = (userMsg: string): { steps: MessageStep[]; finalContent: string; token_in: number; token_out: number } => {
    const toolNames = ['read_file', 'list_dir', 'generate_code', 'apply_diff'];
    const randomTool = toolNames[Math.floor(Math.random() * toolNames.length)];

    const shouldFail = Math.random() > 0.7;
    const shouldCompress = Math.random() > 0.6;

    const steps: MessageStep[] = [
      {
        type: 'thinking',
        content: `正在分析用户请求："${userMsg.substring(0, 60)}${userMsg.length > 60 ? '...' : ''}"\n\n1. 识别需求类型：功能开发\n2. 确定涉及文件范围\n3. 规划实现步骤`,
      },
      {
        type: 'ask_permission',
        content: `AI 需要使用 ${randomTool} 工具来读取和修改项目文件。此操作将访问本地代码文件系统，是否允许？`,
        permissionType: randomTool,
      },
      {
        type: 'tool_use',
        content: `正在读取项目文件结构，分析现有代码...`,
        toolName: 'list_dir',
        summary: { file: 'src/', lines: 12, durationMs: 340 },
      },
      {
        type: 'tool_result',
        content: `已获取项目结构信息：\n- src/components/ 目录存在\n- src/pages/ 目录存在\n- 依赖包已安装`,
        toolName: 'list_dir',
        summary: { file: 'src/', lines: 12, durationMs: 340 },
      },
      {
        type: 'tool_use',
        content: `正在读取目标文件内容...`,
        toolName: 'read_file',
        summary: { file: 'src/components/App.tsx', lines: 45, durationMs: 210 },
      },
      {
        type: 'tool_result',
        content: `读取到文件内容，包含现有组件定义。\n\n\`\`\`typescript\nimport React from "react";\n\nexport default function App() {\n  return <div>Hello World</div>;\n}\n\`\`\`\n\n分析：当前为简单的功能组件，需要扩展以支持新功能。`,
        toolName: 'read_file',
        summary: { file: 'src/components/App.tsx', lines: 45, durationMs: 210 },
      },
      shouldFail ? {
        type: 'tool_use',
        content: `尝试写入文件...`,
        toolName: 'apply_diff',
        summary: { file: 'src/components/App.tsx', lines: 8, durationMs: 560 },
        failed: true,
      } : {
        type: 'tool_use',
        content: `正在生成代码补丁...`,
        toolName: 'apply_diff',
        summary: { file: 'src/components/App.tsx', lines: 8, durationMs: 560 },
      },
      shouldFail ? {
        type: 'tool_result',
        content: `写入失败：权限不足，无法修改 src/components/App.tsx`,
        toolName: 'apply_diff',
        failed: true,
      } : {
        type: 'tool_result',
        content: `文件写入成功。`,
        toolName: 'apply_diff',
        summary: { file: 'src/components/App.tsx', lines: 8, durationMs: 560 },
        diff: `diff --git a/src/components/App.tsx b/src/components/App.tsx\n--- a/src/components/App.tsx\n+++ b/src/components/App.tsx\n@@ -1,5 +1,8 @@\n import React from "react";\n+import { useState } from "react";\n \n-export default function App() {\n+export default function App(): JSX.Element {\n+  const [count, setCount] = useState(0);\n   return <div>Hello World</div>;\n }`,
      },
      {
        type: 'ask_user',
        content: '为了更好地实现功能，请回答以下问题：',
        questions: [
          { id: 'q1', label: '你希望使用哪种状态管理方案？', type: 'choice', options: ['React Context', 'Zustand', 'Redux Toolkit', 'Jotai'] },
          { id: 'q2', label: '样式方案偏好', type: 'choice', options: ['Tailwind CSS', 'CSS Modules', 'Styled Components', 'SCSS'] },
        ],
      },
    ];

    if (shouldCompress) {
      steps.push({
        type: 'compress',
        content: '正在压缩生成的资源文件...',
        compressInfo: { originalSize: 12480, compressedSize: 4320, ratio: 0.346, status: 'compressing' },
      });
      steps.push({
        type: 'compress',
        content: '资源压缩完成',
        compressInfo: { originalSize: 12480, compressedSize: 4320, ratio: 0.346, status: 'done' },
      });
    }

    steps.push({
      type: 'final',
      content: `已完成你的需求！我分析了项目结构并生成了相应的代码。\n\n主要改动：\n1. 创建了新组件\n2. 更新了相关页面\n3. 添加了必要的样式\n\n\`\`\`typescript\n// 示例代码\nfunction implementFeature() {\n  console.log("功能已实现");\n  return true;\n}\n\`\`\`\n\n你可以在右侧「变更」面板查看修改的文件列表，在「任务」面板跟踪进度。`,
    });

    const finalContent = steps[steps.length - 1].content;
    const token_in = Math.floor(userMsg.length * 0.8) + Math.floor(Math.random() * 200);
    const token_out = Math.floor(finalContent.length * 0.9) + Math.floor(Math.random() * 300);
    return { steps, finalContent, token_in, token_out };
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
            {user?.email?.replace('@miaoda.com', '')}
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
                    <span className={`w-5 h-5 rounded text-[10px] font-bold flex items-center justify-center ${config.bg} ${config.color}`}>
                      {config.letter}
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
