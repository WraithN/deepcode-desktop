import { Folder, FolderOpen, ChevronRight, RefreshCw, Search, ChevronsDown } from 'lucide-react';
import { useState } from 'react';
import type { WorkspaceFileNode, GitStatusEntry } from '@/types/types';
import { getFileIcon, getNodeGitStatus } from './tree-utils';

interface FileTreeNodeProps {
  items: WorkspaceFileNode[];
  depth?: number;
  hoveredFile: string | null;
  setHoveredFile: (f: string | null) => void;
  onOpenFile: (path: string) => void;
  gitStatus: Map<string, GitStatusEntry['status']>;
  expanded: Record<string, boolean>;
  onToggleFolder: (path: string, open: boolean) => void;
}

function FileTreeNode({
  items,
  depth = 0,
  hoveredFile,
  setHoveredFile,
  onOpenFile,
  gitStatus,
  expanded,
  onToggleFolder,
}: FileTreeNodeProps) {
  const sorted = [...items].sort((a, b) => {
    if (a.is_dir === b.is_dir) { return a.name.localeCompare(b.name); }
    return a.is_dir ? -1 : 1;
  });

  return (
    <>
      {sorted.map((item) => {
        const isFolder = item.is_dir;
        const isHovered = hoveredFile === item.path;
        const nodeStatus = getNodeGitStatus(item, gitStatus);

        if (isFolder) {
          const isOpen = expanded[item.path] === true;
          return (
            <div key={item.path}>
              <button
                type="button"
                onClick={() => onToggleFolder(item.path, !isOpen)}
                className={`w-full flex items-center gap-1 px-3 py-1 text-left hover:bg-secondary/40 transition-colors ${depth > 0 ? 'pl-6' : ''}`}
              >
                <ChevronRight
                  className={`w-3 h-3 shrink-0 text-muted-foreground transition-transform ${isOpen ? 'rotate-90' : ''}`}
                />
                {isOpen ? (
                  <FolderOpen className="w-3.5 h-3.5 shrink-0 text-primary" />
                ) : (
                  <Folder className="w-3.5 h-3.5 shrink-0 text-muted-foreground" />
                )}
                <span
                  className={`text-[12px] truncate flex-1 min-w-0 ${item.ignored ? 'text-muted-foreground/50' : 'text-foreground'}`}
                  title={item.path}
                >
                  {item.name}
                </span>
                {nodeStatus === 'dot' && <span className="w-1.5 h-1.5 rounded-full bg-primary shrink-0" />}
              </button>
              {isOpen && item.children && (
                <FileTreeNode
                  items={item.children}
                  depth={depth + 1}
                  hoveredFile={hoveredFile}
                  setHoveredFile={setHoveredFile}
                  onOpenFile={onOpenFile}
                  gitStatus={gitStatus}
                  expanded={expanded}
                  onToggleFolder={onToggleFolder}
                />
              )}
            </div>
          );
        }

        const Icon = getFileIcon(item.path);
        return (
          <button
            key={item.path}
            type="button"
            onDoubleClick={() => onOpenFile(item.path)}
            onMouseEnter={() => setHoveredFile(item.path)}
            onMouseLeave={() => setHoveredFile(null)}
            className={`w-full flex items-center gap-1 px-3 py-1 text-left hover:bg-secondary/40 transition-colors ${depth > 0 ? 'pl-6' : ''}`}
            title={item.path}
          >
            <span className="w-3 shrink-0" />
            <Icon
              className={`w-3.5 h-3.5 shrink-0 ${item.ignored ? 'text-muted-foreground/40' : isHovered ? 'text-primary' : 'text-muted-foreground'}`}
            />
            <span
              className={`text-[12px] truncate font-mono flex-1 min-w-0 ${item.ignored ? 'text-muted-foreground/50' : isHovered ? 'text-foreground' : 'text-muted-foreground'}`}
            >
              {item.name}
            </span>
            {nodeStatus && nodeStatus !== 'dot' && (
              <span
                className={`text-xs shrink-0 ${nodeStatus === 'U' || nodeStatus === 'A' ? 'text-green-400' : nodeStatus === 'D' ? 'text-red-400' : 'text-orange-400'}`}
              >
                {nodeStatus}
              </span>
            )}
          </button>
        );
      })}
    </>
  );
}

export interface FileTreePanelProps {
  fileTree: WorkspaceFileNode[];
  gitStatus: Map<string, GitStatusEntry['status']>;
  fileSearch: string;
  onFileSearchChange: (value: string) => void;
  fileTreeLoading: boolean;
  fileTreeError: string | null;
  expandedFolders: Record<string, boolean>;
  filteredTree: WorkspaceFileNode[];
  onLoadFileTree: () => void;
  onExpandAll: () => void;
  onOpenFile: (path: string) => void;
  onToggleFolder: (path: string, open: boolean) => void;
}

export default function FileTreePanel({
  fileTree,
  gitStatus,
  fileSearch,
  onFileSearchChange,
  fileTreeLoading,
  fileTreeError,
  expandedFolders,
  filteredTree,
  onLoadFileTree,
  onExpandAll,
  onOpenFile,
  onToggleFolder,
}: FileTreePanelProps) {
  const [hoveredFile, setHoveredFile] = useState<string | null>(null);

  return (
    <div className="flex flex-col h-full" data-workspace-context-menu="false" onContextMenu={(event) => event.preventDefault()}>
      <div className="flex items-center justify-between px-3 py-2 border-b border-border shrink-0">
        <span className="text-xs font-medium text-foreground">文件</span>
        <div className="flex items-center gap-1">
          <button
            type="button"
            onClick={onExpandAll}
            disabled={fileTree.length === 0}
            className="w-5 h-5 flex items-center justify-center rounded hover:bg-secondary text-muted-foreground hover:text-foreground transition-colors disabled:opacity-50"
            title="全部展开"
          >
            <ChevronsDown className="w-3 h-3" />
          </button>
          <button
            type="button"
            onClick={onLoadFileTree}
            disabled={fileTreeLoading}
            className="w-5 h-5 flex items-center justify-center rounded hover:bg-secondary text-muted-foreground hover:text-foreground transition-colors disabled:opacity-50"
            title="刷新"
          >
            <RefreshCw className={`w-3 h-3 ${fileTreeLoading ? 'animate-spin' : ''}`} />
          </button>
        </div>
      </div>
      <div className="px-3 py-2 border-b border-border shrink-0">
        <div className="flex items-center gap-1.5 px-2 py-1 rounded-md border border-border bg-secondary/40">
          <Search className="w-3 h-3 text-muted-foreground shrink-0" />
          <input
            type="text"
            value={fileSearch}
            onChange={(e) => onFileSearchChange(e.target.value)}
            placeholder="搜索文件..."
            className="flex-1 min-w-0 bg-transparent text-xs text-foreground placeholder:text-muted-foreground focus:outline-none"
          />
        </div>
      </div>
      <div className="flex-1 overflow-y-auto py-1">
        {fileTreeError ? (
          <div className="px-3 py-6 text-center text-xs text-red-400">{fileTreeError}</div>
        ) : fileTree.length === 0 ? (
          <div className="px-3 py-6 text-center text-xs text-muted-foreground">暂无文件</div>
        ) : (
          <FileTreeNode
            items={filteredTree}
            hoveredFile={hoveredFile}
            setHoveredFile={setHoveredFile}
            onOpenFile={onOpenFile}
            gitStatus={gitStatus}
            expanded={expandedFolders}
            onToggleFolder={onToggleFolder}
          />
        )}
      </div>
    </div>
  );
}
