import { Label } from '@/components/ui/label';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Button } from '@/components/ui/button';
import { Zap, Save, RefreshCw } from 'lucide-react';

interface SkillsTabProps {
  skillDesign: string;
  skillCode: string;
  skillTest: string;
  skillDeploy: string;
  skillSyncing: boolean;
  onSkillChange: (field: 'design' | 'code' | 'test' | 'deploy', value: string) => void;
  onSave: () => void;
  onSync: () => void;
}

export default function SkillsTab({
  skillDesign,
  skillCode,
  skillTest,
  skillDeploy,
  skillSyncing,
  onSkillChange,
  onSave,
  onSync,
}: SkillsTabProps) {
  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between mb-2">
        <div className="flex items-center gap-2">
          <Zap className="w-4 h-4 text-primary" />
          <span className="text-sm font-medium text-foreground">技能槽配置</span>
        </div>
        <Button
          variant="ghost"
          size="sm"
          onClick={onSync}
          disabled={skillSyncing}
          className="h-7 text-[12px] gap-1 text-muted-foreground hover:text-foreground"
        >
          <RefreshCw className={`w-3 h-3 ${skillSyncing ? 'animate-spin' : ''}`} />
          {skillSyncing ? '同步中...' : '同步云端'}
        </Button>
      </div>

      <div className="space-y-3">
        <div className="space-y-2">
          <Label className="text-sm font-normal">需求设计</Label>
          <Select value={skillDesign} onValueChange={(v) => onSkillChange('design', v)}>
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
          <Select value={skillCode} onValueChange={(v) => onSkillChange('code', v)}>
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
          <Select value={skillTest} onValueChange={(v) => onSkillChange('test', v)}>
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
          <Select value={skillDeploy} onValueChange={(v) => onSkillChange('deploy', v)}>
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
      <Button onClick={onSave} className="w-full bg-primary text-primary-foreground hover:bg-primary/90">
        <Save className="w-3.5 h-3.5 mr-1.5" /> 保存技能配置
      </Button>
    </div>
  );
}
