import { useState, useEffect } from 'react';
import {
  Sheet, SheetContent, SheetHeader, SheetTitle,
} from '@/components/ui/sheet';
import {
  Settings, Zap, FileText, BookOpen, Plug, Bot,
} from 'lucide-react';
import { toast } from 'sonner';
import type { AgentInstance } from '@/stores';
import BasicTab from './settings/BasicTab';
import AgentsTab from './settings/AgentsTab';
import SkillsTab from './settings/SkillsTab';
import PromptsTab from './settings/PromptsTab';
import SpecsTab from './settings/SpecsTab';
import McpTab from './settings/McpTab';
import {
  AgentTypeConfig, getStoredAgentTypeConfigs, saveAgentTypeConfigs,
  PromptCard, DEFAULT_PROMPTS, DEFAULT_TAGS, MCPServer,
} from './settings/settings-utils';

interface SettingsDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  agents: AgentInstance[];
  onUpdateAgents: (agents: AgentInstance[]) => void;
}

export default function SettingsDialog({ open, onOpenChange, agents = [], onUpdateAgents }: SettingsDialogProps) {
  const [_model] = useState('gpt-4');
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
    try { const raw = localStorage.getItem('prompt_cards'); if (raw) { return JSON.parse(raw);  }} catch { /* ignore */ }
    return DEFAULT_PROMPTS;
  });
  const [promptTags, setPromptTags] = useState<string[]>(() => {
    try { const raw = localStorage.getItem('prompt_tags'); if (raw) { return JSON.parse(raw);  }} catch { /* ignore */ }
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
      if (typeCfg) { return { ...a, modelConfig: typeCfg }; }
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
    if (!editingPrompt) { return; }
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
    if (tag === '全部') { return; }
    setPromptTags((prev) => prev.filter((t) => t !== tag));
    setPrompts((prev) => prev.map((p) => ({ ...p, tags: p.tags.filter((t) => t !== tag) })));
    if (activeTag === tag) { setActiveTag('全部'); }
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
              <BasicTab
                theme={theme}
                themeColor={themeColor}
                language={language}
                onThemeChange={setTheme}
                onThemeColorChange={handleThemeColorChange}
                onLanguageChange={handleLanguageChange}
                onSave={handleSaveBasic}
              />
            )}

            {activeSetting === 'agents' && (
              <AgentsTab
                agents={agents}
                agentTypeConfigs={agentTypeConfigs}
                onAgentTypeConfigChange={(key, config) => setAgentTypeConfigs((prev) => ({ ...prev, [key]: config }))}
                onSave={handleSaveAgentConfigs}
              />
            )}

            {activeSetting === 'skills' && (
              <SkillsTab
                skillDesign={skillDesign}
                skillCode={skillCode}
                skillTest={skillTest}
                skillDeploy={skillDeploy}
                skillSyncing={skillSyncing}
                onSkillChange={(field, value) => {
                  if (field === 'design') { setSkillDesign(value); }
                  if (field === 'code') { setSkillCode(value); }
                  if (field === 'test') { setSkillTest(value); }
                  if (field === 'deploy') { setSkillDeploy(value); }
                }}
                onSave={handleSaveSkills}
                onSync={handleSyncSkills}
              />
            )}

            {activeSetting === 'prompts' && (
              <PromptsTab
                prompts={prompts}
                promptTags={promptTags}
                promptSearch={promptSearch}
                activeTag={activeTag}
                showAddPrompt={showAddPrompt}
                showAddTag={showAddTag}
                newTagName={newTagName}
                newPromptTitle={newPromptTitle}
                newPromptContent={newPromptContent}
                newPromptTags={newPromptTags}
                editingPrompt={editingPrompt}
                promptSyncing={promptSyncing}
                copiedId={copiedId}
                filteredPrompts={filteredPrompts}
                onPromptSearchChange={setPromptSearch}
                onActiveTagChange={setActiveTag}
                onShowAddPromptChange={setShowAddPrompt}
                onShowAddTagChange={setShowAddTag}
                onNewTagNameChange={setNewTagName}
                onNewPromptTitleChange={setNewPromptTitle}
                onNewPromptContentChange={setNewPromptContent}
                onNewPromptTagsChange={setNewPromptTags}
                onEditingPromptChange={setEditingPrompt}
                onSavePrompts={handleSavePrompts}
                onSyncPrompts={handleSyncPrompts}
                onAddPrompt={handleAddPrompt}
                onDeletePrompt={handleDeletePrompt}
                onEditPrompt={handleEditPrompt}
                onAddTag={handleAddTag}
                onDeleteTag={handleDeleteTag}
                onCopyPrompt={handleCopyPrompt}
              />
            )}

            {activeSetting === 'specs' && <SpecsTab onSave={() => {}} />}

            {activeSetting === 'mcp' && (
              <McpTab
                mcpServers={mcpServers}
                onMcpServerChange={setMcpServers}
              />
            )}
          </div>
        </div>
      </SheetContent>
    </Sheet>
  );
}
