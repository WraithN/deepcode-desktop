/* ───────── 智能体类型配置 ───────── */
export const AGENT_TYPES = [
  { key: 'opencode', name: 'OpenCode', desc: '开源编码智能体，支持多种编程语言和框架' },
  { key: 'claude-code', name: 'Claude Code', desc: 'Anthropic推出的编码助手，擅长复杂逻辑推理' },
  { key: 'cursor-agent', name: 'Cursor Agent', desc: '基于GPT-4的智能编码代理' },
  { key: 'codex', name: 'Codex', desc: 'OpenAI Codex，专为软件工程优化的AI模型' },
  { key: 'custom', name: '自定义智能体', desc: '创建属于你的AI编码助手，自由配置模型和能力参数' },
];

export interface AgentTypeConfig {
  type: 'builtin' | 'custom';
  modelId?: string;
  name?: string;
  url?: string;
  apiKey?: string;
  showThinking?: boolean;
}

export function getStoredAgentTypeConfigs(): Record<string, AgentTypeConfig> {
  try {
    const raw = localStorage.getItem('agent_type_configs');
    if (raw) { return JSON.parse(raw); }
  } catch { /* ignore */ }
  return {
    opencode: { type: 'builtin', modelId: 'gpt-4', showThinking: false },
    'claude-code': { type: 'builtin', modelId: 'claude-3-opus', showThinking: true },
    'cursor-agent': { type: 'builtin', modelId: 'deepseek-v3', showThinking: true },
    codex: { type: 'builtin', modelId: 'gpt-4', showThinking: true },
    custom: { type: 'builtin', modelId: 'gpt-4', showThinking: true },
  };
}

export function saveAgentTypeConfigs(configs: Record<string, AgentTypeConfig>) {
  localStorage.setItem('agent_type_configs', JSON.stringify(configs));
}

/* ───────── 提示词 ───────── */
export interface PromptCard {
  id: string;
  title: string;
  content: string;
  tags: string[];
}

export const DEFAULT_PROMPTS: PromptCard[] = [
  { id: '1', title: '需求文档生成', content: '根据用户描述生成完整的需求文档，包含功能说明、验收标准等。', tags: ['产品设计', '需求'] },
  { id: '2', title: '代码审查', content: '对提供的代码进行审查，指出潜在问题和优化建议。', tags: ['编码', '质量'] },
  { id: '3', title: 'API设计', content: '根据业务需求设计RESTful API接口文档。', tags: ['产品设计', '编码'] },
  { id: '4', title: '单元测试生成', content: '为指定函数或模块生成完整的单元测试用例。', tags: ['编码', '测试'] },
  { id: '5', title: '数据库设计', content: '根据业务需求设计数据库表结构和关系。', tags: ['产品设计', '数据库'] },
  { id: '6', title: '性能优化', content: '分析代码性能瓶颈并提供优化方案。', tags: ['编码', '优化'] },
  { id: '7', title: '错误排查', content: '分析错误日志，定位问题根因并提供修复方案。', tags: ['编码', '调试'] },
  { id: '8', title: 'UI设计规范', content: '生成符合设计规范的UI组件样式和交互说明。', tags: ['产品设计', '设计'] },
];

export const DEFAULT_TAGS = ['全部', '产品设计', '编码', '需求', '质量', '测试', '数据库', '优化', '调试', '设计'];

/* ───────── 工程规范 ───────── */
export const DEFAULT_SPECS = {
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
export const builtinModels = [
  { id: 'gpt-4', name: 'GPT-4' },
  { id: 'gpt-4-turbo', name: 'GPT-4 Turbo' },
  { id: 'claude-3-opus', name: 'Claude 3 Opus' },
  { id: 'claude-3-sonnet', name: 'Claude 3 Sonnet' },
  { id: 'deepseek-v3', name: 'DeepSeek V3' },
];

export const themeColorOptions = [
  { key: 'blue', label: '科技蓝', hsl: '213 94% 68%', dot: 'bg-blue-400' },
  { key: 'green', label: '翡翠绿', hsl: '142 71% 45%', dot: 'bg-green-400' },
  { key: 'orange', label: '珊瑚橙', hsl: '25 95% 53%', dot: 'bg-orange-400' },
  { key: 'purple', label: '紫罗兰', hsl: '270 60% 55%', dot: 'bg-purple-400' },
  { key: 'pink', label: '玫红', hsl: '340 75% 55%', dot: 'bg-pink-400' },
];

export function applyThemeColor(colorKey: string) {
  const option = themeColorOptions.find((o) => o.key === colorKey);
  if (!option) { return; }
  const root = document.documentElement;
  root.style.setProperty('--primary', option.hsl);
  root.style.setProperty('--ring', option.hsl);
  root.style.setProperty('--chart-1', option.hsl);
  root.style.setProperty('--sidebar-primary', option.hsl);
  root.style.setProperty('--sidebar-ring', option.hsl);
  root.style.setProperty('--info', option.hsl);
  localStorage.setItem('theme_color', colorKey);
}

/* ───────── MCP 服务器 ───────── */
export interface MCPServer {
  id: string;
  name: string;
  command: string;
  args: string;
  env: string;
  enabled: boolean;
}
