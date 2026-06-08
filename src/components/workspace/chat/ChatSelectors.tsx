import { useState } from 'react';
import {
  Popover, PopoverContent, PopoverTrigger,
} from '@/components/ui/popover';
import { Settings2, ChevronUp, Zap } from 'lucide-react';

// ========================== 工具栏选择器组件 ==========================

export function ModeSelector({ value, onChange }: { value: 'plan' | 'build'; onChange: (v: 'plan' | 'build') => void }) {
  const [open, setOpen] = useState(false);
  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <button type="button" className="flex items-center gap-1 px-2 py-0.5 text-[12px] rounded bg-secondary border border-border text-foreground hover:bg-secondary/80 transition-colors">
          <Settings2 className="w-2.5 h-2.5 text-muted-foreground" />
          {value === 'plan' ? 'Plan' : 'Build'}
          <ChevronUp className={`w-2.5 h-2.5 text-muted-foreground transition-transform ${open ? 'rotate-180' : ''}`} />
        </button>
      </PopoverTrigger>
      <PopoverContent side="top" align="start" className="w-28 p-1">
        <button type="button" onClick={() => { onChange('plan'); setOpen(false); }} className={`w-full text-left px-2.5 py-1.5 text-xs rounded transition-colors ${value === 'plan' ? 'bg-primary/10 text-primary' : 'text-foreground hover:bg-secondary'}`}>Plan</button>
        <button type="button" onClick={() => { onChange('build'); setOpen(false); }} className={`w-full text-left px-2.5 py-1.5 text-xs rounded transition-colors ${value === 'build' ? 'bg-primary/10 text-primary' : 'text-foreground hover:bg-secondary'}`}>Build</button>
      </PopoverContent>
    </Popover>
  );
}

export function ModelSelector({ value, onChange }: { value: string; onChange: (v: string) => void }) {
  const [open, setOpen] = useState(false);
  const models = [
    { value: 'gpt-4', label: 'GPT-4' },
    { value: 'gpt-4-turbo', label: 'GPT-4 Turbo' },
    { value: 'claude-3-opus', label: 'Claude 3 Opus' },
    { value: 'claude-3-sonnet', label: 'Claude 3 Sonnet' },
    { value: 'deepseek-v3', label: 'DeepSeek V3' },
  ];
  const current = models.find((m) => m.value === value);

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <button type="button" className="flex items-center gap-1 px-2 py-0.5 text-[12px] rounded bg-secondary border border-border text-foreground hover:bg-secondary/80 transition-colors">
          {current?.label || value}
          <ChevronUp className={`w-2.5 h-2.5 text-muted-foreground transition-transform ${open ? 'rotate-180' : ''}`} />
        </button>
      </PopoverTrigger>
      <PopoverContent side="top" align="start" className="w-36 p-1">
        {models.map((m) => (
          <button key={m.value} type="button" onClick={() => { onChange(m.value); setOpen(false); }} className={`w-full text-left px-2.5 py-1.5 text-xs rounded transition-colors ${value === m.value ? 'bg-primary/10 text-primary' : 'text-foreground hover:bg-secondary'}`}>{m.label}</button>
        ))}
      </PopoverContent>
    </Popover>
  );
}

// ========================== 技能选择 ==========================

export interface SkillItem {
  name: string;
  desc: string;
}

export const skillCategories = [
  { key: 'all', label: '全部' },
  { key: 'frontend', label: '前端' },
  { key: 'backend', label: '后端' },
  { key: 'fullstack', label: '全栈' },
  { key: 'design', label: '设计' },
  { key: 'other', label: '其他' },
];

export const skillData: Record<string, SkillItem[]> = {
  all: [
    { name: 'React', desc: 'React 组件开发与状态管理' },
    { name: 'Vue', desc: 'Vue 组合式API与响应式系统' },
    { name: 'Tailwind CSS', desc: '原子化CSS样式设计与实现' },
    { name: 'Next.js', desc: '全栈React框架与SSR开发' },
    { name: 'Node.js', desc: '服务端JavaScript运行环境' },
    { name: 'Python', desc: 'Python脚本与数据处理' },
    { name: 'TypeScript', desc: '静态类型检查与高级类型' },
    { name: 'Docker', desc: '容器化部署与镜像管理' },
    { name: 'Git', desc: '版本控制与分支策略' },
    { name: 'Figma', desc: '设计稿转代码与UI还原' },
  ],
  frontend: [
    { name: 'React', desc: 'React 组件开发与状态管理' },
    { name: 'Vue', desc: 'Vue 组合式API与响应式系统' },
    { name: 'Angular', desc: '企业级前端框架开发' },
    { name: 'Tailwind CSS', desc: '原子化CSS样式设计' },
    { name: 'Sass', desc: 'CSS预处理器与变量管理' },
    { name: 'shadcn/ui', desc: '基于Radix的组件库' },
    { name: 'Vite', desc: '前端构建工具与热更新' },
    { name: 'Webpack', desc: '模块打包与资源优化' },
  ],
  backend: [
    { name: 'Node.js', desc: '服务端JavaScript运行环境' },
    { name: 'Python', desc: 'Python脚本与数据处理' },
    { name: 'Go', desc: '高并发服务端开发' },
    { name: 'Rust', desc: '系统级编程与内存安全' },
    { name: 'Java', desc: 'Spring生态与企业级开发' },
    { name: 'PostgreSQL', desc: '关系型数据库设计与优化' },
    { name: 'Redis', desc: '缓存与高性能数据存储' },
    { name: 'GraphQL', desc: '查询语言与API设计' },
  ],
  fullstack: [
    { name: 'Next.js', desc: '全栈React框架与SSR开发' },
    { name: 'Nuxt', desc: '全栈Vue框架与SSR开发' },
    { name: 'Django', desc: 'Python全栈Web框架' },
    { name: 'Spring Boot', desc: 'Java全栈开发框架' },
    { name: 'NestJS', desc: 'Node.js企业级框架' },
    { name: 'Express', desc: '轻量级Node.js服务端' },
  ],
  design: [
    { name: 'UI/UX', desc: '用户界面与体验设计' },
    { name: 'Figma', desc: '设计稿转代码与UI还原' },
    { name: 'Framer Motion', desc: 'React动画库与交互设计' },
    { name: 'CSS Animation', desc: 'CSS3动画与过渡效果' },
    { name: 'Responsive Design', desc: '响应式布局与多端适配' },
  ],
  other: [
    { name: 'Docker', desc: '容器化部署与镜像管理' },
    { name: 'Git', desc: '版本控制与分支策略' },
    { name: 'CI/CD', desc: '持续集成与自动化部署' },
    { name: 'Nginx', desc: '反向代理与负载均衡' },
    { name: 'Linux', desc: 'Linux系统操作与Shell' },
    { name: 'Shell Script', desc: '自动化脚本编写' },
    { name: 'Testing', desc: '单元测试与集成测试' },
  ],
};

export function SkillSelector({ value, onChange, onInsertSkill }: { value: string; onChange: (v: string) => void; onInsertSkill: (skill: string) => void }) {
  const [open, setOpen] = useState(false);
  const [activeCategory, setActiveCategory] = useState('all');

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <button type="button" className="flex items-center gap-1 px-2 py-0.5 text-[12px] rounded bg-secondary border border-border text-foreground hover:bg-secondary/80 transition-colors">
          <Zap className="w-2.5 h-2.5 text-muted-foreground" />
          技能
          <ChevronUp className={`w-2.5 h-2.5 text-muted-foreground transition-transform ${open ? 'rotate-180' : ''}`} />
        </button>
      </PopoverTrigger>
      <PopoverContent side="top" align="start" className="w-72 p-0 overflow-hidden">
        <div className="flex border-b border-border overflow-x-auto">
          {skillCategories.map((cat) => (
            <button
              key={cat.key}
              type="button"
              onClick={() => setActiveCategory(cat.key)}
              className={`px-3 py-2 text-[12px] whitespace-nowrap transition-colors flex-shrink-0 ${
                activeCategory === cat.key
                  ? 'text-primary border-b-2 border-primary'
                  : 'text-muted-foreground hover:text-foreground'
              }`}
            >
              {cat.label}
            </button>
          ))}
        </div>
        <div className="max-h-56 overflow-y-auto overflow-x-auto">
          <div className="min-w-full">
            {skillData[activeCategory]?.map((skill) => (
              <button
                key={skill.name}
                type="button"
                onClick={() => {
                  onChange(skill.name);
                  onInsertSkill(skill.name);
                  setOpen(false);
                }}
                className={`w-full text-left px-3 py-2 text-xs transition-colors border-b border-border last:border-b-0 whitespace-nowrap hover:bg-secondary/50 ${
                  value === skill.name ? 'bg-primary/5 text-primary' : 'text-foreground'
                }`}
              >
                <span className="font-medium">{skill.name}</span>
                <span className="text-muted-foreground ml-2 text-[12px]">{skill.desc}</span>
              </button>
            ))}
          </div>
        </div>
      </PopoverContent>
    </Popover>
  );
}
