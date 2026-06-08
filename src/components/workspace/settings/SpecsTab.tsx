import { useState } from 'react';
import { Button } from '@/components/ui/button';
import { Label } from '@/components/ui/label';
import { Save, BookOpen, RefreshCw } from 'lucide-react';
import { toast } from 'sonner';

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

interface SpecsTabProps {
  onSave: () => void;
}

export default function SpecsTab({ onSave }: SpecsTabProps) {
  const [specTab, setSpecTab] = useState<'global' | 'engineering' | 'visual'>('global');
  const [specs, setSpecs] = useState<Record<string, string>>(() => {
    try {
      const raw = localStorage.getItem('specs_configs');
      if (raw) { return JSON.parse(raw); }
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
