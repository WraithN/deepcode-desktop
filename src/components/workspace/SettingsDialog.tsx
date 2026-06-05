import { useState, useEffect } from 'react';
import {
  Sheet, SheetContent, SheetHeader, SheetTitle,
} from '@/components/ui/sheet';
import { Label } from '@/components/ui/label';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Input } from '@/components/ui/input';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import {
  Settings, Zap, FileText, BookOpen, Search, X, Copy, Check, Plug, Plus, Trash2,
  Bot, Eye, EyeOff, Globe, Palette, Save, RefreshCw, Edit2, Tag, Layers,
} from 'lucide-react';
import { toast } from 'sonner';
import type { AgentInstance } from '@/stores';

interface SettingsDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  agents: AgentInstance[];
  onUpdateAgents: (agents: AgentInstance[]) => void;
}

/* ───────── 智能体类型配置 ───────── */
const AGENT_TYPES = [
  { key: 'opencode', name: 'OpenCode', desc: '开源编码智能体，支持多种编程语言和框架' },
  { key: 'claude-code', name: 'Claude Code', desc: 'Anthropic推出的编码助手，擅长复杂逻辑推理' },
  { key: 'cursor-agent', name: 'Cursor Agent', desc: '基于GPT-4的智能编码代理' },
  { key: 'codex', name: 'Codex', desc: 'OpenAI Codex，专为软件工程优化的AI模型' },
  { key: 'custom', name: '自定义智能体', desc: '创建属于你的AI编码助手，自由配置模型和能力参数' },
];

interface AgentTypeConfig {
  type: 'builtin' | 'custom';
  modelId?: string;
  name?: string;
  url?: string;
  apiKey?: string;
}

function getStoredAgentTypeConfigs(): Record<string, AgentTypeConfig> {
  try {
    const raw = localStorage.getItem('agent_type_configs');
    if (raw) return JSON.parse(raw);
  } catch { /* ignore */ }
  return {
    opencode: { type: 'builtin', modelId: 'gpt-4' },
    'claude-code': { type: 'builtin', modelId: 'claude-3-opus' },
    'cursor-agent': { type: 'builtin', modelId: 'deepseek-v3' },
    codex: { type: 'builtin', modelId: 'gpt-4' },
    custom: { type: 'builtin', modelId: 'gpt-4' },
  };
}

function saveAgentTypeConfigs(configs: Record<string, AgentTypeConfig>) {
  localStorage.setItem('agent_type_configs', JSON.stringify(configs));
}

/* ───────── 提示词 ───────── */
interface PromptCard {
  id: string;
  title: string;
  content: string;
  tags: string[];
}

const DEFAULT_PROMPTS: PromptCard[] = [
  { id: '1', title: '需求文档生成', content: '根据用户描述生成完整的需求文档，包含功能说明、验收标准等。', tags: ['产品设计', '需求'] },
  { id: '2', title: '代码审查', content: '对提供的代码进行审查，指出潜在问题和优化建议。', tags: ['编码', '质量'] },
  { id: '3', title: 'API设计', content: '根据业务需求设计RESTful API接口文档。', tags: ['产品设计', '编码'] },
  { id: '4', title: '单元测试生成', content: '为指定函数或模块生成完整的单元测试用例。', tags: ['编码', '测试'] },
  { id: '5', title: '数据库设计', content: '根据业务需求设计数据库表结构和关系。', tags: ['产品设计', '数据库'] },
  { id: '6', title: '性能优化', content: '分析代码性能瓶颈并提供优化方案。', tags: ['编码', '优化'] },
  { id: '7', title: '错误排查', content: '分析错误日志，定位问题根因并提供修复方案。', tags: ['编码', '调试'] },
  { id: '8', title: 'UI设计规范', content: '生成符合设计规范的UI组件样式和交互说明。', tags: ['产品设计', '设计'] },
];

const DEFAULT_TAGS = ['全部', '产品设计', '编码', '需求', '质量', '测试', '数据库', '优化', '调试', '设计'];

/* ───────── 工程规范 ───────── */
const DEFAULT_SPECS = {
  global: `# 全局约束

## 架构约束
- 微服务优先，单体备选
- 无状态服务设计
- API 版本化：/v1/, /v2/

## 安全约束
- 所有 API 需认证鉴权
- 敏感数据加密传输
- 输入输出严格校验

## 性能约束
- API 响应 < 200ms (P95)
- 页面首屏 < 1.5s
- 数据库查询 < 100ms

## 可用性约束
- 服务可用性 >= 99.9%
- 支持滚动发布
- 关键路径有降级方案`,
  engineering: `# 工程约束

## 编码规范
- 使用 TypeScript 进行开发
- 遵循 ESLint 规则
- 组件使用函数式组件 + Hooks

## 命名规范
- 组件: PascalCase
- 函数/变量: camelCase
- 常量: UPPER_SNAKE_CASE
- 文件: kebab-case

## Git 规范
- feat: 新功能
- fix: 修复
- docs: 文档
- style: 样式
- refactor: 重构

## 项目结构
- src/components/ - 组件
- src/pages/ - 页面
- src/hooks/ - 自定义Hooks
- src/lib/ - 工具函数
- src/types/ - 类型定义`,
  visual: `# 视觉约束

## 色彩系统
- 主色: 科技蓝 #3794FF
- 背景: 深灰 #1E1E1E
- 成功: 绿色 #4ADE80
- 警告: 橙色 #FBBF24
- 错误: 红色 #F87171

## 字体规范
- 正文: 系统默认 sans-serif
- 代码: 等宽字体 monospace
- 标题层级: 24px / 20px / 16px / 14px

## 间距规范
- 基础单位: 4px
- 卡片内边距: 16px
- 组件间距: 8px / 12px / 16px / 24px

## 圆角规范
- 按钮: 4px
- 卡片: 8px
- 标签: 9999px
- 输入框: 4px`,
};

/* ───────── 公共常量 ───────── */
const builtinModels = [
  { id: 'gpt-4', name: 'GPT-4' },
  { id: 'gpt-4-turbo', name: 'GPT-4 Turbo' },
  { id: 'claude-3-opus', name: 'Claude 3 Opus' },
  { id: 'claude-3-sonnet', name: 'Claude 3 Sonnet' },
  { id: 'deepseek-v3', name: 'DeepSeek V3' },
];

const themeColorOptions = [
  { key: 'blue', label: '科技蓝', hsl: '213 94% 68%', dot: 'bg-blue-400' },
  { key: 'green', label: '翡翠绿', hsl: '142 71% 45%', dot: 'bg-green-400' },
  { key: 'orange', label: '珊瑚橙', hsl: '25 95% 53%', dot: 'bg-orange-400' },
  { key: 'purple', label: '紫罗兰', hsl: '270 60% 55%', dot: 'bg-purple-400' },
  { key: 'pink', label: '玫红', hsl: '340 75% 55%', dot: 'bg-pink-400' },
];

function applyThemeColor(colorKey: string) {
  const option = themeColorOptions.find((o) => o.key === colorKey);
  if (!option) return;
  const root = document.documentElement;
  root.style.setProperty('--primary', option.hsl);
  root.style.setProperty('--ring', option.hsl);
  root.style.setProperty('--chart-1', option.hsl);
  root.style.setProperty('--sidebar-primary', option.hsl);
  root.style.setProperty('--sidebar-ring', option.hsl);
  root.style.setProperty('--info', option.hsl);
  localStorage.setItem('theme_color', colorKey);
}

/* ───────── 智能体类型配置卡片 ───────── */
function AgentTypeConfigCard({
  agentType,
  config,
  onChange,
}: {
  agentType: { key: string; name: string; desc: string };
  config: AgentTypeConfig;
  onChange: (c: AgentTypeConfig) => void;
}) {
  const [showKey, setShowKey] = useState(false);

  return (
    <div className="rounded-lg border border-border bg-secondary/20 p-3 space-y-3">
      <div className="flex items-center gap-2">
        <span className="text-xs font-medium text-foreground">{agentType.name}</span>
        <Badge variant="secondary" className="text-xs px-1.5 h-4">{agentType.key}</Badge>
      </div>
      <p className="text-[12px] text-muted-foreground">{agentType.desc}</p>

      <div className="flex items-center gap-2">
        {[
          { key: 'builtin', label: '内置模型' },
          { key: 'custom', label: '自定义' },
        ].map((opt) => (
          <button
            key={opt.key}
            type="button"
            onClick={() => onChange({ ...config, type: opt.key as 'builtin' | 'custom' })}
            className={`px-2.5 py-1 text-[12px] rounded border transition-colors ${
              config.type === opt.key
                ? 'border-primary bg-primary/10 text-primary'
                : 'border-border bg-secondary text-foreground hover:bg-secondary/80'
            }`}
          >
            {opt.label}
          </button>
        ))}
      </div>

      {config.type === 'builtin' ? (
        <div className="space-y-1">
          <Label className="text-xs text-muted-foreground">选择模型</Label>
          <Select
            value={config.modelId || 'gpt-4'}
            onValueChange={(v) => onChange({ ...config, modelId: v })}
          >
            <SelectTrigger className="bg-secondary border-border text-xs h-8">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {builtinModels.map((m) => (
                <SelectItem key={m.id} value={m.id} className="text-xs">{m.name}</SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>
      ) : (
        <div className="space-y-2">
          <div className="space-y-1">
            <Label className="text-xs text-muted-foreground">模型名称</Label>
            <Input value={config.name || ''} onChange={(e) => onChange({ ...config, name: e.target.value })} placeholder="例如：自定义 GPT" className="bg-secondary border-border text-xs h-8" />
          </div>
          <div className="space-y-1">
            <Label className="text-xs text-muted-foreground">API URL</Label>
            <Input value={config.url || ''} onChange={(e) => onChange({ ...config, url: e.target.value })} placeholder="https://api.example.com/v1/chat/completions" className="bg-secondary border-border text-xs h-8" />
          </div>
          <div className="space-y-1">
            <Label className="text-xs text-muted-foreground">API KEY</Label>
            <div className="flex items-center gap-1.5">
              <Input type={showKey ? 'text' : 'password'} value={config.apiKey || ''} onChange={(e) => onChange({ ...config, apiKey: e.target.value })} placeholder="sk-..." className="bg-secondary border-border text-xs h-8" />
              <button type="button" onClick={() => setShowKey(!showKey)} className="w-8 h-8 flex items-center justify-center rounded border border-border bg-secondary text-muted-foreground hover:text-foreground transition-colors shrink-0" title={showKey ? '隐藏' : '显示'}>
                {showKey ? <EyeOff className="w-3.5 h-3.5" /> : <Eye className="w-3.5 h-3.5" />}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

/* ───────── 可编辑工程规范面板 ───────── */
function SpecsPanel({ onSave }: { onSave: () => void }) {
  const [specTab, setSpecTab] = useState<'global' | 'engineering' | 'visual'>('global');
  const [specs, setSpecs] = useState<Record<string, string>>(() => {
    try {
      const raw = localStorage.getItem('specs_configs');
      if (raw) return JSON.parse(raw);
    } catch { /* ignore */ }
    return DEFAULT_SPECS;
  });
  const [syncing, setSyncing] = useState(false);

  const specTabs = [
    { id: 'global' as const, label: '全局约束' },
    { id: 'engineering' as const, label: '工程约束' },
    { id: 'visual' as const, label: '视觉约束' },
  ];

  const handleSave = () => {
    localStorage.setItem('specs_configs', JSON.stringify(specs));
    toast.success('工程规范已保存');
    onSave();
  };

  const handleSync = async () => {
    setSyncing(true);
    // 模拟云端同步
    await new Promise((r) => setTimeout(r, 1200));
    setSyncing(false);
    toast.success('已从云端同步工程规范');
  };

  return (
    <div className="space-y-3 h-full flex flex-col">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2 mb-1">
          <BookOpen className="w-4 h-4 text-primary" />
          <span className="text-sm font-medium text-foreground">工程规范</span>
        </div>
        <div className="flex items-center gap-1.5">
          <Button variant="ghost" size="sm" onClick={handleSync} disabled={syncing} className="h-7 text-[12px] gap-1 text-muted-foreground hover:text-foreground">
            <RefreshCw className={`w-3 h-3 ${syncing ? 'animate-spin' : ''}`} />
            {syncing ? '同步中...' : '同步'}
          </Button>
        </div>
      </div>

      {/* 子标签 */}
      <div className="flex gap-1 border-b border-border pb-0.5">
        {specTabs.map((tab) => (
          <button
            key={tab.id}
            type="button"
            onClick={() => setSpecTab(tab.id)}
            className={`px-3 py-1.5 text-[12px] font-medium rounded-t transition-colors ${
              specTab === tab.id
                ? 'text-primary border-b-2 border-primary'
                : 'text-muted-foreground hover:text-foreground'
            }`}
          >
            {tab.label}
          </button>
        ))}
      </div>

      <div className="flex-1 overflow-y-auto space-y-2">
        <Label className="text-sm font-normal">{specTabs.find((t) => t.id === specTab)?.label}</Label>
        <textarea
          value={specs[specTab] || ''}
          onChange={(e) => setSpecs((prev) => ({ ...prev, [specTab]: e.target.value }))}
          className="w-full h-[300px] p-3 text-xs font-mono bg-secondary border border-border rounded resize-none text-foreground placeholder:text-muted-foreground focus:outline-none focus:border-primary focus:ring-1 focus:ring-primary/20"
          spellCheck={false}
        />
        <Button onClick={handleSave} className="w-full bg-primary text-primary-foreground hover:bg-primary/90">
          <Save className="w-3.5 h-3.5 mr-1.5" /> 保存规范
        </Button>
      </div>
    </div>
  );
}

export default function SettingsDialog({ open, onOpenChange, agents = [], onUpdateAgents }: SettingsDialogProps) {
  const [model] = useState('gpt-4');
  const [theme, setTheme] = useState('dark');
  const [themeColor, setThemeColor] = useState(localStorage.getItem('theme_color') || 'blue');
  const [language, setLanguage] = useState(localStorage.getItem('app_language') || 'zh');

  // 智能体类型配置（按类型，不按实例）
  const [agentTypeConfigs, setAgentTypeConfigs] = useState<Record<string, AgentTypeConfig>>(getStoredAgentTypeConfigs);

  // 技能
  const [skillDesign, setSkillDesign] = useState('auto');
  const [skillCode, setSkillCode] = useState('auto');
  const [skillTest, setSkillTest] = useState('auto');
  const [skillDeploy, setSkillDeploy] = useState('auto');
  const [skillSyncing, setSkillSyncing] = useState(false);

  // 提示词
  const [prompts, setPrompts] = useState<PromptCard[]>(() => {
    try { const raw = localStorage.getItem('prompt_cards'); if (raw) return JSON.parse(raw); } catch { /* ignore */ }
    return DEFAULT_PROMPTS;
  });
  const [promptTags, setPromptTags] = useState<string[]>(() => {
    try { const raw = localStorage.getItem('prompt_tags'); if (raw) return JSON.parse(raw); } catch { /* ignore */ }
    return DEFAULT_TAGS;
  });
  const [promptSearch, setPromptSearch] = useState('');
  const [activeTag, setActiveTag] = useState('全部');
  const [showAddPrompt, setShowAddPrompt] = useState(false);
  const [showAddTag, setShowAddTag] = useState(false);
  const [newTagName, setNewTagName] = useState('');
  const [newPromptTitle, setNewPromptTitle] = useState('');
  const [newPromptContent, setNewPromptContent] = useState('');
  const [newPromptTags, setNewPromptTags] = useState<string[]>([]);
  const [editingPrompt, setEditingPrompt] = useState<PromptCard | null>(null);
  const [promptSyncing, setPromptSyncing] = useState(false);

  const [copiedId, setCopiedId] = useState<string | null>(null);

  const [activeSetting, setActiveSetting] = useState<'basic' | 'agents' | 'skills' | 'prompts' | 'specs' | 'mcp'>('basic');

  // MCP
  interface MCPServer {
    id: string;
    name: string;
    command: string;
    args: string;
    env: string;
    enabled: boolean;
  }
  const [mcpServers, setMcpServers] = useState<MCPServer[]>([
    { id: '1', name: 'filesystem', command: 'npx', args: '-y @modelcontextprotocol/server-filesystem /path/to/project', env: '', enabled: true },
    { id: '2', name: 'github', command: 'npx', args: '-y @modelcontextprotocol/server-github', env: 'GITHUB_TOKEN=your_token', enabled: false },
  ]);

  useEffect(() => {
    if (open) {
      const saved = localStorage.getItem('theme_color') || 'blue';
      setThemeColor(saved);
    }
  }, [open]);

  /* ─────── 基础 ─────── */
  const handleSaveBasic = () => {
    localStorage.setItem('app_language', language);
    toast.success('基础设置已保存');
    onOpenChange(false);
  };
  const handleThemeColorChange = (colorKey: string) => {
    setThemeColor(colorKey);
    applyThemeColor(colorKey);
  };
  const handleLanguageChange = (lang: string) => {
    setLanguage(lang);
    localStorage.setItem('app_language', lang);
  };

  /* ─────── 智能体 ─────── */
  const handleSaveAgentConfigs = () => {
    saveAgentTypeConfigs(agentTypeConfigs);
    // 同步到当前所有 agent instances
    const updated = agents.map((a) => {
      const typeCfg = agentTypeConfigs[a.agentKey];
      if (typeCfg) return { ...a, modelConfig: typeCfg };
      return a;
    });
    onUpdateAgents(updated);
    toast.success('智能体配置已保存');
  };

  /* ─────── 技能 ─────── */
  const handleSaveSkills = () => {
    const data = { skillDesign, skillCode, skillTest, skillDeploy };
    localStorage.setItem('skill_configs', JSON.stringify(data));
    toast.success('技能配置已保存');
  };
  const handleSyncSkills = async () => {
    setSkillSyncing(true);
    await new Promise((r) => setTimeout(r, 1200));
    setSkillSyncing(false);
    toast.success('技能配置已同步云端');
  };

  /* ─────── 提示词 ─────── */
  const handleSavePrompts = () => {
    localStorage.setItem('prompt_cards', JSON.stringify(prompts));
    localStorage.setItem('prompt_tags', JSON.stringify(promptTags));
    toast.success('提示词已保存');
  };
  const handleSyncPrompts = async () => {
    setPromptSyncing(true);
    await new Promise((r) => setTimeout(r, 1500));
    setPromptSyncing(false);
    toast.success('提示词已同步远端');
  };
  const handleAddPrompt = () => {
    if (!newPromptTitle.trim()) { toast.error('请输入提示词标题'); return; }
    const newCard: PromptCard = {
      id: Date.now().toString(),
      title: newPromptTitle.trim(),
      content: newPromptContent.trim(),
      tags: newPromptTags.length ? newPromptTags : ['通用'],
    };
    setPrompts((prev) => [newCard, ...prev]);
    setShowAddPrompt(false);
    setNewPromptTitle('');
    setNewPromptContent('');
    setNewPromptTags([]);
    toast.success('提示词已添加');
  };
  const handleDeletePrompt = (id: string) => {
    setPrompts((prev) => prev.filter((p) => p.id !== id));
    toast.success('提示词已删除');
  };
  const handleEditPrompt = () => {
    if (!editingPrompt) return;
    if (!editingPrompt.title.trim()) { toast.error('标题不能为空'); return; }
    setPrompts((prev) => prev.map((p) => (p.id === editingPrompt.id ? editingPrompt : p)));
    setEditingPrompt(null);
    toast.success('提示词已更新');
  };
  const handleAddTag = () => {
    const trimmed = newTagName.trim();
    if (!trimmed) { toast.error('请输入标签名称'); return; }
    if (promptTags.includes(trimmed) || trimmed === '全部') { toast.error('标签已存在'); return; }
    setPromptTags((prev) => [...prev, trimmed]);
    setNewTagName('');
    setShowAddTag(false);
    toast.success(`标签 "${trimmed}" 已添加`);
  };
  const handleDeleteTag = (tag: string) => {
    if (tag === '全部') return;
    setPromptTags((prev) => prev.filter((t) => t !== tag));
    setPrompts((prev) => prev.map((p) => ({ ...p, tags: p.tags.filter((t) => t !== tag) })));
    if (activeTag === tag) setActiveTag('全部');
  };

  /* ─────── MCP ─────── */
  const handleAddMcpServer = () => {
    setMcpServers((prev) => [...prev, { id: Date.now().toString(), name: '', command: '', args: '', env: '', enabled: true }]);
  };
  const handleRemoveMcpServer = (id: string) => setMcpServers((prev) => prev.filter((s) => s.id !== id));
  const handleUpdateMcpServer = (id: string, field: keyof MCPServer, value: string | boolean) => {
    setMcpServers((prev) => prev.map((s) => (s.id === id ? { ...s, [field]: value } : s)));
  };

  const filteredPrompts = prompts.filter((card) => {
    const matchSearch = !promptSearch || card.title.includes(promptSearch) || card.content.includes(promptSearch);
    const matchTag = activeTag === '全部' || card.tags.includes(activeTag);
    return matchSearch && matchTag;
  });

  const handleCopyPrompt = (card: PromptCard) => {
    navigator.clipboard.writeText(`${card.title}\n${card.content}`);
    setCopiedId(card.id);
    toast.success('提示词已复制');
    setTimeout(() => setCopiedId(null), 2000);
  };

  const menuItems = [
    { id: 'basic' as const, label: '基础设置', icon: Settings },
    { id: 'agents' as const, label: '智能体设置', icon: Bot },
    { id: 'skills' as const, label: '技能设置', icon: Zap },
    { id: 'prompts' as const, label: '提示词设置', icon: FileText },
    { id: 'specs' as const, label: '工程规范设置', icon: BookOpen },
    { id: 'mcp' as const, label: 'MCP设置', icon: Plug },
  ];

  return (
    <Sheet open={open} onOpenChange={onOpenChange}>
      <SheetContent side="right" className="w-screen sm:w-screen p-0 bg-card border-l border-border overflow-hidden">
        <SheetHeader className="px-5 pt-5 pb-2 shrink-0 border-b border-border">
          <SheetTitle className="flex items-center gap-2 text-foreground">
            <Settings className="w-5 h-5 text-primary" />
            设置
          </SheetTitle>
        </SheetHeader>

        <div className="flex h-[calc(100vh-64px)]">
          {/* 左侧菜单 */}
          <div className="w-40 shrink-0 border-r border-border bg-secondary/30">
            {menuItems.map((item) => {
              const Icon = item.icon;
              const isActive = activeSetting === item.id;
              return (
                <button
                  key={item.id}
                  type="button"
                  onClick={() => setActiveSetting(item.id)}
                  className={`w-full flex items-center gap-2.5 px-4 py-2.5 text-xs transition-colors text-left ${
                    isActive
                      ? 'bg-primary/10 text-primary border-l-2 border-l-primary'
                      : 'text-muted-foreground hover:text-foreground hover:bg-secondary/50 border-l-2 border-l-transparent'
                  }`}
                >
                  <Icon className="w-4 h-4 shrink-0" />
                  {item.label}
                </button>
              );
            })}
          </div>

          {/* 右侧内容 */}
          <div className="flex-1 overflow-y-auto p-5">
            {activeSetting === 'basic' && (
              <div className="space-y-5">
                {/* 语言设置 */}
                <div className="space-y-2">
                  <Label className="text-sm font-normal flex items-center gap-1.5">
                    <Globe className="w-3.5 h-3.5 text-muted-foreground" />
                    界面语言
                  </Label>
                  <div className="flex items-center gap-2">
                    {[{ key: 'zh', label: '中文' }, { key: 'en', label: 'English' }].map((lang) => (
                      <button
                        key={lang.key}
                        type="button"
                        onClick={() => handleLanguageChange(lang.key)}
                        className={`px-3 py-1.5 text-xs rounded-md border transition-colors ${
                          language === lang.key
                            ? 'border-primary bg-primary/10 text-primary'
                            : 'border-border bg-secondary text-foreground hover:bg-secondary/80'
                        }`}
                      >
                        {lang.label}
                      </button>
                    ))}
                  </div>
                </div>

                {/* 主题色值 */}
                <div className="space-y-2">
                  <Label className="text-sm font-normal flex items-center gap-1.5">
                    <Palette className="w-3.5 h-3.5 text-muted-foreground" />
                    主题色值
                  </Label>
                  <div className="flex flex-wrap gap-2">
                    {themeColorOptions.map((opt) => (
                      <button
                        key={opt.key}
                        type="button"
                        onClick={() => handleThemeColorChange(opt.key)}
                        className={`flex items-center gap-1.5 px-3 py-1.5 text-xs rounded-md border transition-colors ${
                          themeColor === opt.key
                            ? 'border-primary bg-primary/10 text-primary'
                            : 'border-border bg-secondary text-foreground hover:bg-secondary/80'
                        }`}
                      >
                        <span className={`w-2.5 h-2.5 rounded-full ${opt.dot}`} />
                        {opt.label}
                      </button>
                    ))}
                  </div>
                </div>

                {/* 主题模式 */}
                <div className="space-y-2">
                  <Label className="text-sm font-normal">主题模式</Label>
                  <Select value={theme} onValueChange={(v) => { setTheme(v); document.documentElement.className = v === 'light' ? 'light' : 'dark'; }}>
                    <SelectTrigger className="bg-secondary border-border">
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem value="dark">深色</SelectItem>
                      <SelectItem value="light">浅色</SelectItem>
                    </SelectContent>
                  </Select>
                </div>

                <Button onClick={handleSaveBasic} className="w-full bg-primary text-primary-foreground hover:bg-primary/90">
                  <Save className="w-3.5 h-3.5 mr-1.5" /> 保存设置
                </Button>
              </div>
            )}

            {/* ── 智能体设置 ── */}
            {activeSetting === 'agents' && (
              <div className="space-y-4">
                <div className="flex items-center gap-2 mb-2">
                  <Bot className="w-4 h-4 text-primary" />
                  <span className="text-sm font-medium text-foreground">智能体模型设置</span>
                </div>
                <div className="space-y-3 max-h-[500px] overflow-y-auto">
                  {AGENT_TYPES.map((at) => (
                    <AgentTypeConfigCard
                      key={at.key}
                      agentType={at}
                      config={agentTypeConfigs[at.key] || { type: 'builtin', modelId: 'gpt-4' }}
                      onChange={(c) => setAgentTypeConfigs((prev) => ({ ...prev, [at.key]: c }))}
                    />
                  ))}
                </div>
                <Button onClick={handleSaveAgentConfigs} className="w-full bg-primary text-primary-foreground hover:bg-primary/90">
                  <Save className="w-3.5 h-3.5 mr-1.5" /> 保存智能体配置
                </Button>
              </div>
            )}

            {/* ── 技能设置 ── */}
            {activeSetting === 'skills' && (
              <div className="space-y-4">
                <div className="flex items-center justify-between mb-2">
                  <div className="flex items-center gap-2">
                    <Zap className="w-4 h-4 text-primary" />
                    <span className="text-sm font-medium text-foreground">技能槽配置</span>
                  </div>
                  <Button variant="ghost" size="sm" onClick={handleSyncSkills} disabled={skillSyncing} className="h-7 text-[12px] gap-1 text-muted-foreground hover:text-foreground">
                    <RefreshCw className={`w-3 h-3 ${skillSyncing ? 'animate-spin' : ''}`} />
                    {skillSyncing ? '同步中...' : '同步云端'}
                  </Button>
                </div>

                <div className="space-y-3">
                  <div className="space-y-2">
                    <Label className="text-sm font-normal">需求设计</Label>
                    <Select value={skillDesign} onValueChange={setSkillDesign}>
                      <SelectTrigger className="bg-secondary border-border"><SelectValue /></SelectTrigger>
                      <SelectContent>
                        <SelectItem value="auto">自动选择</SelectItem>
                        <SelectItem value="prd">PRD生成</SelectItem>
                        <SelectItem value="user-research">用户研究</SelectItem>
                        <SelectItem value="competitor">竞品分析</SelectItem>
                      </SelectContent>
                    </Select>
                  </div>
                  <div className="space-y-2">
                    <Label className="text-sm font-normal">开发编码</Label>
                    <Select value={skillCode} onValueChange={setSkillCode}>
                      <SelectTrigger className="bg-secondary border-border"><SelectValue /></SelectTrigger>
                      <SelectContent>
                        <SelectItem value="auto">自动选择</SelectItem>
                        <SelectItem value="frontend">前端开发</SelectItem>
                        <SelectItem value="backend">后端开发</SelectItem>
                        <SelectItem value="fullstack">全栈开发</SelectItem>
                        <SelectItem value="mobile">移动端开发</SelectItem>
                      </SelectContent>
                    </Select>
                  </div>
                  <div className="space-y-2">
                    <Label className="text-sm font-normal">测试验证</Label>
                    <Select value={skillTest} onValueChange={setSkillTest}>
                      <SelectTrigger className="bg-secondary border-border"><SelectValue /></SelectTrigger>
                      <SelectContent>
                        <SelectItem value="auto">自动选择</SelectItem>
                        <SelectItem value="unit-test">单元测试</SelectItem>
                        <SelectItem value="integration">集成测试</SelectItem>
                        <SelectItem value="e2e">端到端测试</SelectItem>
                      </SelectContent>
                    </Select>
                  </div>
                  <div className="space-y-2">
                    <Label className="text-sm font-normal">部署发布</Label>
                    <Select value={skillDeploy} onValueChange={setSkillDeploy}>
                      <SelectTrigger className="bg-secondary border-border"><SelectValue /></SelectTrigger>
                      <SelectContent>
                        <SelectItem value="auto">自动选择</SelectItem>
                        <SelectItem value="docker">Docker部署</SelectItem>
                        <SelectItem value="cloud">云服务部署</SelectItem>
                        <SelectItem value="static">静态站点</SelectItem>
                      </SelectContent>
                    </Select>
                  </div>
                </div>
                <Button onClick={handleSaveSkills} className="w-full bg-primary text-primary-foreground hover:bg-primary/90">
                  <Save className="w-3.5 h-3.5 mr-1.5" /> 保存技能配置
                </Button>
              </div>
            )}

            {/* ── 提示词设置 ── */}
            {activeSetting === 'prompts' && (
              <div className="space-y-3">
                <div className="flex items-center justify-between mb-2">
                  <div className="flex items-center gap-2">
                    <FileText className="w-4 h-4 text-primary" />
                    <span className="text-sm font-medium text-foreground">提示词库</span>
                  </div>
                  <div className="flex items-center gap-1.5">
                    <Button variant="ghost" size="sm" onClick={() => setShowAddPrompt(true)} className="h-7 text-[12px] gap-1 text-muted-foreground hover:text-foreground">
                      <Plus className="w-3 h-3" /> 新增
                    </Button>
                    <Button variant="ghost" size="sm" onClick={handleSyncPrompts} disabled={promptSyncing} className="h-7 text-[12px] gap-1 text-muted-foreground hover:text-foreground">
                      <RefreshCw className={`w-3 h-3 ${promptSyncing ? 'animate-spin' : ''}`} />
                      {promptSyncing ? '同步中...' : '同步'}
                    </Button>
                  </div>
                </div>

                {/* 搜索 */}
                <div className="relative">
                  <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-3.5 h-3.5 text-muted-foreground" />
                  <Input value={promptSearch} onChange={(e) => setPromptSearch(e.target.value)} placeholder="搜索提示词..." className="pl-9 bg-secondary border-border text-sm h-8" />
                  {promptSearch && (
                    <button type="button" onClick={() => setPromptSearch('')} className="absolute right-2 top-1/2 -translate-y-1/2">
                      <X className="w-3 h-3 text-muted-foreground hover:text-foreground" />
                    </button>
                  )}
                </div>

                {/* 标签筛选 + 管理 */}
                <div className="flex flex-wrap gap-1.5 items-center">
                  {promptTags.map((tag) => (
                    <div key={tag} className="relative group">
                      <button
                        type="button"
                        onClick={() => setActiveTag(tag)}
                        className={`px-2 py-0.5 text-xs rounded-full transition-colors inline-flex items-center gap-1 ${
                          activeTag === tag ? 'bg-primary text-primary-foreground' : 'bg-secondary text-muted-foreground hover:text-foreground'
                        }`}
                      >
                        {tag}
                        {tag !== '全部' && (
                          <span
                            role="button"
                            tabIndex={0}
                            onClick={(e) => { e.stopPropagation(); handleDeleteTag(tag); }}
                            onKeyDown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.stopPropagation(); handleDeleteTag(tag); } }}
                            className="ml-0.5 text-xs opacity-0 group-hover:opacity-100 hover:text-destructive transition-opacity cursor-pointer"
                          >
                            ×
                          </span>
                        )}
                      </button>
                    </div>
                  ))}
                  <button
                    type="button"
                    onClick={() => setShowAddTag(true)}
                    className="px-2 py-0.5 text-xs rounded-full border border-dashed border-border text-muted-foreground hover:text-foreground hover:border-primary/50 transition-colors"
                  >
                    + 类型
                  </button>
                </div>

                {/* 新增标签弹窗 */}
                {showAddTag && (
                  <div className="flex items-center gap-2 p-2 rounded border border-border bg-secondary/30">
                    <Input value={newTagName} onChange={(e) => setNewTagName(e.target.value)} placeholder="新标签名称..." className="bg-secondary border-border text-xs h-7" />
                    <Button size="sm" onClick={handleAddTag} className="h-7 text-[12px] bg-primary text-primary-foreground">添加</Button>
                    <Button size="sm" variant="ghost" onClick={() => setShowAddTag(false)} className="h-7 text-[12px]">取消</Button>
                  </div>
                )}

                {/* 新增/编辑提示词 */}
                {(showAddPrompt || editingPrompt) && (
                  <div className="p-3 rounded border border-border bg-secondary/30 space-y-2">
                    <div className="flex items-center justify-between">
                      <span className="text-xs font-medium text-foreground">{editingPrompt ? '编辑提示词' : '新增提示词'}</span>
                      <button type="button" onClick={() => { setShowAddPrompt(false); setEditingPrompt(null); }}>
                        <X className="w-3 h-3 text-muted-foreground hover:text-foreground" />
                      </button>
                    </div>
                    <Input
                      value={editingPrompt ? editingPrompt.title : newPromptTitle}
                      onChange={(e) => editingPrompt ? setEditingPrompt({ ...editingPrompt, title: e.target.value }) : setNewPromptTitle(e.target.value)}
                      placeholder="提示词标题..."
                      className="bg-secondary border-border text-xs h-8"
                    />
                    <textarea
                      value={editingPrompt ? editingPrompt.content : newPromptContent}
                      onChange={(e) => editingPrompt ? setEditingPrompt({ ...editingPrompt, content: e.target.value }) : setNewPromptContent(e.target.value)}
                      placeholder="提示词内容..."
                      rows={3}
                      className="w-full p-2 text-xs bg-secondary border border-border rounded resize-none text-foreground placeholder:text-muted-foreground focus:outline-none focus:border-primary focus:ring-1 focus:ring-primary/20"
                    />
                    <div className="flex flex-wrap gap-1">
                      {promptTags.filter((t) => t !== '全部').map((tag) => {
                        const selected = editingPrompt ? editingPrompt.tags.includes(tag) : newPromptTags.includes(tag);
                        return (
                          <button
                            key={tag}
                            type="button"
                            onClick={() => {
                              if (editingPrompt) {
                                setEditingPrompt({ ...editingPrompt, tags: selected ? editingPrompt.tags.filter((t) => t !== tag) : [...editingPrompt.tags, tag] });
                              } else {
                                setNewPromptTags(selected ? newPromptTags.filter((t) => t !== tag) : [...newPromptTags, tag]);
                              }
                            }}
                            className={`px-2 py-0.5 text-xs rounded-full border transition-colors ${
                              selected ? 'border-primary bg-primary/10 text-primary' : 'border-border bg-secondary text-muted-foreground'
                            }`}
                          >
                            {tag}
                          </button>
                        );
                      })}
                    </div>
                    <div className="flex gap-2">
                      <Button size="sm" onClick={editingPrompt ? handleEditPrompt : handleAddPrompt} className="h-7 text-[12px] bg-primary text-primary-foreground">
                        {editingPrompt ? '保存' : '添加'}
                      </Button>
                      <Button size="sm" variant="ghost" onClick={() => { setShowAddPrompt(false); setEditingPrompt(null); }} className="h-7 text-[12px]">
                        取消
                      </Button>
                    </div>
                  </div>
                )}

                {/* 提示词卡片 */}
                <div className="space-y-2 max-h-[260px] overflow-y-auto">
                  {filteredPrompts.length === 0 ? (
                    <div className="text-center py-6 text-xs text-muted-foreground">未找到相关提示词</div>
                  ) : (
                    filteredPrompts.map((card) => (
                      <div key={card.id} className="p-3 rounded border border-border bg-secondary/30 hover:bg-secondary/50 transition-colors group">
                        <div className="flex items-start justify-between gap-2">
                          <h4 className="text-xs font-medium text-foreground">{card.title}</h4>
                          <div className="flex gap-1 shrink-0 items-center opacity-0 group-hover:opacity-100 transition-opacity">
                            {card.tags.map((tag) => (
                              <Badge key={tag} variant="secondary" className="text-xs px-1.5 py-0 h-4">{tag}</Badge>
                            ))}
                            <button type="button" onClick={() => handleCopyPrompt(card)} className="ml-1 text-muted-foreground hover:text-foreground transition-colors">
                              {copiedId === card.id ? <Check className="w-3 h-3 text-green-400" /> : <Copy className="w-3 h-3" />}
                            </button>
                            <button type="button" onClick={() => setEditingPrompt(card)} className="text-muted-foreground hover:text-primary transition-colors">
                              <Edit2 className="w-3 h-3" />
                            </button>
                            <button type="button" onClick={() => handleDeletePrompt(card.id)} className="text-muted-foreground hover:text-destructive transition-colors">
                              <Trash2 className="w-3 h-3" />
                            </button>
                          </div>
                        </div>
                        <p className="text-[12px] text-muted-foreground mt-1.5 line-clamp-2">{card.content}</p>
                      </div>
                    ))
                  )}
                </div>

                <Button onClick={handleSavePrompts} className="w-full bg-primary text-primary-foreground hover:bg-primary/90">
                  <Save className="w-3.5 h-3.5 mr-1.5" /> 保存提示词
                </Button>
              </div>
            )}

            {activeSetting === 'specs' && <SpecsPanel onSave={() => {}} />}

            {activeSetting === 'mcp' && (
              <div className="space-y-4">
                <div className="flex items-center justify-between">
                  <div className="flex items-center gap-2">
                    <Plug className="w-4 h-4 text-primary" />
                    <span className="text-sm font-medium text-foreground">MCP 服务器配置</span>
                  </div>
                  <button type="button" onClick={handleAddMcpServer} className="flex items-center gap-1 px-2 py-1 text-[12px] rounded bg-primary text-primary-foreground hover:bg-primary/90 transition-colors">
                    <Plus className="w-3 h-3" /> 添加
                  </button>
                </div>
                <p className="text-[12px] text-muted-foreground leading-relaxed">Model Context Protocol (MCP) 允许智能体通过标准化接口与外部工具和数据源交互。</p>
                <div className="space-y-3 max-h-[400px] overflow-y-auto">
                  {mcpServers.map((server) => (
                    <div key={server.id} className="rounded border border-border bg-secondary/30 p-3 space-y-2">
                      <div className="flex items-center justify-between">
                        <div className="flex items-center gap-2">
                          <input type="checkbox" checked={server.enabled} onChange={(e) => handleUpdateMcpServer(server.id, 'enabled', e.target.checked)} className="w-3.5 h-3.5 rounded border-border accent-primary" />
                          <span className="text-xs font-medium text-foreground">{server.name || '未命名'}</span>
                        </div>
                        <button type="button" onClick={() => handleRemoveMcpServer(server.id)} className="text-muted-foreground hover:text-destructive transition-colors">
                          <Trash2 className="w-3.5 h-3.5" />
                        </button>
                      </div>
                      <div className="grid grid-cols-2 gap-2">
                        <div className="space-y-1">
                          <Label className="text-xs text-muted-foreground">名称</Label>
                          <Input value={server.name} onChange={(e) => handleUpdateMcpServer(server.id, 'name', e.target.value)} placeholder="server-name" className="bg-secondary border-border text-xs h-7" />
                        </div>
                        <div className="space-y-1">
                          <Label className="text-xs text-muted-foreground">命令</Label>
                          <Input value={server.command} onChange={(e) => handleUpdateMcpServer(server.id, 'command', e.target.value)} placeholder="npx, uvx, python..." className="bg-secondary border-border text-xs h-7" />
                        </div>
                      </div>
                      <div className="space-y-1">
                        <Label className="text-xs text-muted-foreground">参数</Label>
                        <Input value={server.args} onChange={(e) => handleUpdateMcpServer(server.id, 'args', e.target.value)} placeholder="-y @modelcontextprotocol/server-filesystem" className="bg-secondary border-border text-xs h-7" />
                      </div>
                      <div className="space-y-1">
                        <Label className="text-xs text-muted-foreground">环境变量</Label>
                        <Input value={server.env} onChange={(e) => handleUpdateMcpServer(server.id, 'env', e.target.value)} placeholder="KEY=VALUE;KEY2=VALUE2" className="bg-secondary border-border text-xs h-7" />
                      </div>
                    </div>
                  ))}
                  {mcpServers.length === 0 && <div className="text-center py-8 text-xs text-muted-foreground">暂无MCP服务器，点击上方「添加」按钮创建</div>}
                </div>
              </div>
            )}
          </div>
        </div>
      </SheetContent>
    </Sheet>
  );
}