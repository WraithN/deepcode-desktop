import { Label } from '@/components/ui/label';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Button } from '@/components/ui/button';
import { Globe, Palette, Save } from 'lucide-react';
import { themeColorOptions, applyThemeColor } from './settings-utils';

interface BasicTabProps {
  theme: string;
  themeColor: string;
  language: string;
  onThemeChange: (theme: string) => void;
  onThemeColorChange: (color: string) => void;
  onLanguageChange: (lang: string) => void;
  onSave: () => void;
}

export default function BasicTab({
  theme,
  themeColor,
  language,
  onThemeChange,
  onThemeColorChange,
  onLanguageChange,
  onSave,
}: BasicTabProps) {
  const handleThemeColorChange = (colorKey: string) => {
    onThemeColorChange(colorKey);
    applyThemeColor(colorKey);
  };

  return (
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
              onClick={() => onLanguageChange(lang.key)}
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
        <Select
          value={theme}
          onValueChange={(v) => {
            onThemeChange(v);
            document.documentElement.className = v === 'light' ? 'light' : 'dark';
          }}
        >
          <SelectTrigger className="bg-secondary border-border">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="dark">深色</SelectItem>
            <SelectItem value="light">浅色</SelectItem>
          </SelectContent>
        </Select>
      </div>

      <Button onClick={onSave} className="w-full bg-primary text-primary-foreground hover:bg-primary/90">
        <Save className="w-3.5 h-3.5 mr-1.5" /> 保存设置
      </Button>
    </div>
  );
}
