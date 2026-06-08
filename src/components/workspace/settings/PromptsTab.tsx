import { Input } from '@/components/ui/input';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import {
  FileText, Search, X, Copy, Check, Plus, RefreshCw, Edit2, Trash2, Save,
} from 'lucide-react';
import type { PromptCard } from './settings-utils';

interface PromptsTabProps {
  prompts: PromptCard[];
  promptTags: string[];
  promptSearch: string;
  activeTag: string;
  showAddPrompt: boolean;
  showAddTag: boolean;
  newTagName: string;
  newPromptTitle: string;
  newPromptContent: string;
  newPromptTags: string[];
  editingPrompt: PromptCard | null;
  promptSyncing: boolean;
  copiedId: string | null;
  filteredPrompts: PromptCard[];
  onPromptSearchChange: (value: string) => void;
  onActiveTagChange: (tag: string) => void;
  onShowAddPromptChange: (show: boolean) => void;
  onShowAddTagChange: (show: boolean) => void;
  onNewTagNameChange: (value: string) => void;
  onNewPromptTitleChange: (value: string) => void;
  onNewPromptContentChange: (value: string) => void;
  onNewPromptTagsChange: (tags: string[]) => void;
  onEditingPromptChange: (prompt: PromptCard | null) => void;
  onSavePrompts: () => void;
  onSyncPrompts: () => void;
  onAddPrompt: () => void;
  onDeletePrompt: (id: string) => void;
  onEditPrompt: () => void;
  onAddTag: () => void;
  onDeleteTag: (tag: string) => void;
  onCopyPrompt: (card: PromptCard) => void;
}

export default function PromptsTab({
  promptTags,
  promptSearch,
  activeTag,
  showAddPrompt,
  showAddTag,
  newTagName,
  newPromptTitle,
  newPromptContent,
  newPromptTags,
  editingPrompt,
  promptSyncing,
  copiedId,
  filteredPrompts,
  onPromptSearchChange,
  onActiveTagChange,
  onShowAddPromptChange,
  onShowAddTagChange,
  onNewTagNameChange,
  onNewPromptTitleChange,
  onNewPromptContentChange,
  onNewPromptTagsChange,
  onEditingPromptChange,
  onSavePrompts,
  onSyncPrompts,
  onAddPrompt,
  onDeletePrompt,
  onEditPrompt,
  onAddTag,
  onDeleteTag,
  onCopyPrompt,
}: PromptsTabProps) {
  return (
    <div className="space-y-3">
      <div className="flex items-center justify-between mb-2">
        <div className="flex items-center gap-2">
          <FileText className="w-4 h-4 text-primary" />
          <span className="text-sm font-medium text-foreground">提示词库</span>
        </div>
        <div className="flex items-center gap-1.5">
          <Button
            variant="ghost"
            size="sm"
            onClick={() => onShowAddPromptChange(true)}
            className="h-7 text-[12px] gap-1 text-muted-foreground hover:text-foreground"
          >
            <Plus className="w-3 h-3" /> 新增
          </Button>
          <Button
            variant="ghost"
            size="sm"
            onClick={onSyncPrompts}
            disabled={promptSyncing}
            className="h-7 text-[12px] gap-1 text-muted-foreground hover:text-foreground"
          >
            <RefreshCw className={`w-3 h-3 ${promptSyncing ? 'animate-spin' : ''}`} />
            {promptSyncing ? '同步中...' : '同步'}
          </Button>
        </div>
      </div>

      {/* 搜索 */}
      <div className="relative">
        <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-3.5 h-3.5 text-muted-foreground" />
        <Input
          value={promptSearch}
          onChange={(e) => onPromptSearchChange(e.target.value)}
          placeholder="搜索提示词..."
          className="pl-9 bg-secondary border-border text-sm h-8"
        />
        {promptSearch && (
          <button type="button" onClick={() => onPromptSearchChange('')} className="absolute right-2 top-1/2 -translate-y-1/2">
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
              onClick={() => onActiveTagChange(tag)}
              className={`px-2 py-0.5 text-xs rounded-full transition-colors inline-flex items-center gap-1 ${
                activeTag === tag ? 'bg-primary text-primary-foreground' : 'bg-secondary text-muted-foreground hover:text-foreground'
              }`}
            >
              {tag}
              {tag !== '全部' && (
                <span
                  role="button"
                  tabIndex={0}
                  onClick={(e) => { e.stopPropagation(); onDeleteTag(tag); }}
                  onKeyDown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.stopPropagation(); onDeleteTag(tag); } }}
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
          onClick={() => onShowAddTagChange(true)}
          className="px-2 py-0.5 text-xs rounded-full border border-dashed border-border text-muted-foreground hover:text-foreground hover:border-primary/50 transition-colors"
        >
          + 类型
        </button>
      </div>

      {/* 新增标签弹窗 */}
      {showAddTag && (
        <div className="flex items-center gap-2 p-2 rounded border border-border bg-secondary/30">
          <Input value={newTagName} onChange={(e) => onNewTagNameChange(e.target.value)} placeholder="新标签名称..." className="bg-secondary border-border text-xs h-7" />
          <Button size="sm" onClick={onAddTag} className="h-7 text-[12px] bg-primary text-primary-foreground">添加</Button>
          <Button size="sm" variant="ghost" onClick={() => onShowAddTagChange(false)} className="h-7 text-[12px]">取消</Button>
        </div>
      )}

      {/* 新增/编辑提示词 */}
      {(showAddPrompt || editingPrompt) && (
        <div className="p-3 rounded border border-border bg-secondary/30 space-y-2">
          <div className="flex items-center justify-between">
            <span className="text-xs font-medium text-foreground">{editingPrompt ? '编辑提示词' : '新增提示词'}</span>
            <button
              type="button"
              onClick={() => { onShowAddPromptChange(false); onEditingPromptChange(null); }}
            >
              <X className="w-3 h-3 text-muted-foreground hover:text-foreground" />
            </button>
          </div>
          <Input
            value={editingPrompt ? editingPrompt.title : newPromptTitle}
            onChange={(e) => editingPrompt ? onEditingPromptChange({ ...editingPrompt, title: e.target.value }) : onNewPromptTitleChange(e.target.value)}
            placeholder="提示词标题..."
            className="bg-secondary border-border text-xs h-8"
          />
          <textarea
            value={editingPrompt ? editingPrompt.content : newPromptContent}
            onChange={(e) => editingPrompt ? onEditingPromptChange({ ...editingPrompt, content: e.target.value }) : onNewPromptContentChange(e.target.value)}
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
                      onEditingPromptChange({ ...editingPrompt, tags: selected ? editingPrompt.tags.filter((t) => t !== tag) : [...editingPrompt.tags, tag] });
                    } else {
                      onNewPromptTagsChange(selected ? newPromptTags.filter((t) => t !== tag) : [...newPromptTags, tag]);
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
            <Button
              size="sm"
              onClick={editingPrompt ? onEditPrompt : onAddPrompt}
              className="h-7 text-[12px] bg-primary text-primary-foreground"
            >
              {editingPrompt ? '保存' : '添加'}
            </Button>
            <Button
              size="sm"
              variant="ghost"
              onClick={() => { onShowAddPromptChange(false); onEditingPromptChange(null); }}
              className="h-7 text-[12px]"
            >
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
                  <button type="button" onClick={() => onCopyPrompt(card)} className="ml-1 text-muted-foreground hover:text-foreground transition-colors">
                    {copiedId === card.id ? <Check className="w-3 h-3 text-green-400" /> : <Copy className="w-3 h-3" />}
                  </button>
                  <button type="button" onClick={() => onEditingPromptChange(card)} className="text-muted-foreground hover:text-primary transition-colors">
                    <Edit2 className="w-3 h-3" />
                  </button>
                  <button type="button" onClick={() => onDeletePrompt(card.id)} className="text-muted-foreground hover:text-destructive transition-colors">
                    <Trash2 className="w-3 h-3" />
                  </button>
                </div>
              </div>
              <p className="text-[12px] text-muted-foreground mt-1.5 line-clamp-2">{card.content}</p>
            </div>
          ))
        )}
      </div>

      <Button onClick={onSavePrompts} className="w-full bg-primary text-primary-foreground hover:bg-primary/90">
        <Save className="w-3.5 h-3.5 mr-1.5" /> 保存提示词
      </Button>
    </div>
  );
}
