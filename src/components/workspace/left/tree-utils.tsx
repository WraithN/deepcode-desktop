import type { ReactNode } from 'react';
import {
  Braces, FileCode2, FileJson, FileText, FileImage,
  Settings, Hash, Globe, Coffee,
} from 'lucide-react';
import type { WorkspaceFileNode, GitStatusEntry } from '@/types/types';

/** Return the appropriate icon component for a file path. */
export function getFileIcon(path: string) {
  if (path.endsWith('.tsx') || path.endsWith('.ts') || path.endsWith('.jsx') || path.endsWith('.js')) { return Braces; }
  if (path.endsWith('.css') || path.endsWith('.scss') || path.endsWith('.less')) { return Hash; }
  if (path.endsWith('.json')) { return FileJson; }
  if (path.endsWith('.html') || path.endsWith('.htm')) { return Globe; }
  if (path.endsWith('.md')) { return FileText; }
  if (path.endsWith('.png') || path.endsWith('.jpg') || path.endsWith('.svg') || path.endsWith('.jpeg')) { return FileImage; }
  if (path.endsWith('.java')) { return Coffee; }
  if (path.endsWith('.config.ts') || path.endsWith('.config.js')) { return Settings; }
  return FileCode2;
}

/** Collect all file paths from a tree recursively. */
export function collectFilePaths(nodes: WorkspaceFileNode[]): string[] {
  return nodes.flatMap((node) => {
    if (node.is_dir) { return collectFilePaths(node.children || []); }
    return [node.path];
  });
}

/** Collect all folder paths from a tree recursively. */
export function collectFolderPaths(nodes: WorkspaceFileNode[]): string[] {
  return nodes.flatMap((node) => {
    if (!node.is_dir) { return []; }
    return [node.path, ...collectFolderPaths(node.children || [])];
  });
}

/** Determine git status for a single node (file or folder dot indicator). */
export function getNodeGitStatus(
  node: WorkspaceFileNode,
  gitStatus: Map<string, GitStatusEntry['status']>
): GitStatusEntry['status'] | 'dot' | null {
  if (!node.is_dir) { return gitStatus.get(node.path) || null; }
  const hasChangedChild = collectFilePaths(node.children || []).some((path) => gitStatus.has(path));
  return hasChangedChild ? 'dot' : null;
}

/** Filter workspace tree by keyword (case-insensitive). */
export function filterWorkspaceTree(nodes: WorkspaceFileNode[], query: string): WorkspaceFileNode[] {
  const keyword = query.trim().toLowerCase();
  if (!keyword) { return nodes; }

  return nodes.flatMap((node) => {
    const children = node.children ? filterWorkspaceTree(node.children, keyword) : [];
    if (
      node.name.toLowerCase().includes(keyword) ||
      node.path.toLowerCase().includes(keyword) ||
      children.length > 0
    ) {
      return [{ ...node, children }];
    }
    return [];
  });
}

/** Simple syntax highlighting for a single line of code. */
export function highlightCodeLine(line: string, path: string): ReactNode {
  const text = line || ' ';
  const lowerPath = path.toLowerCase();
  const isCode =
    /\.(ts|tsx|js|jsx|json|rs|css|scss|html|md|py|go|java|c|cpp|h|hpp|sh|bash|zsh|ya?ml|toml|xml|env|dockerfile)$/.test(
      lowerPath
    ) || lowerPath.endsWith('dockerfile');
  if (!isCode) { return text; }

  const isYaml = /\.(ya?ml|toml)$/.test(lowerPath);
  const isShell = /\.(sh|bash|zsh|env)$/.test(lowerPath) || lowerPath.endsWith('dockerfile');
  const pattern = isYaml
    ? /(#[^\n]*$)|("(?:\\.|[^"\\])*"|'(?:\\.|[^'\\])*')|\b(true|false|null|yes|no|on|off)\b|\b([0-9]+(?:\.[0-9]+)?)\b|^(\s*)([A-Za-z0-9_.-]+)(:)/g
    : /("(?:\\.|[^"\\])*"|'(?:\\.|[^'\\])*'|`(?:\\.|[^`\\])*`)|(\/\/[^\n]*$|#[^\n]*$)|(\$[A-Za-z_][A-Za-z0-9_]*|\$\{[^}]+\})|\b(import|export|from|const|let|var|function|return|if|else|elif|fi|then|for|while|do|done|case|esac|class|interface|type|async|await|match|pub|struct|enum|impl|use|fn|def|package|func|public|private|protected|static|new|try|catch|finally|throw|throws|echo|cd|pwd|ls|cat|grep|rg|find|mkdir|rm|cp|mv|chmod|chown|source|alias|local|readonly|printf|test)\b|\b(true|false|null|undefined|None|Some|Ok|Err|nil)\b|\b([0-9]+(?:\.[0-9]+)?)\b/g;

  const nodes: ReactNode[] = [];
  let lastIndex = 0;
  for (const match of text.matchAll(pattern)) {
    const index = match.index ?? 0;
    if (index > lastIndex) { nodes.push(text.slice(lastIndex, index)); }
    const value = match[0];
    let className = 'text-foreground';
    if (isYaml) {
      if (match[7]) {
        nodes.push(match[5] || '');
        nodes.push(<span key={`${index}-yaml-key`} className="text-blue-300">{match[6]}</span>);
        nodes.push(match[7]);
        lastIndex = index + value.length;
        continue;
      }
      if (match[1]) { className = 'text-muted-foreground'; }
      else if (match[2]) { className = 'text-green-300'; }
      else if (match[3]) { className = 'text-orange-300'; }
      else if (match[4]) { className = 'text-cyan-300'; }
    } else {
      if (match[2]) { className = 'text-muted-foreground'; }
      else if (match[1]) { className = 'text-green-300'; }
      else if (match[3] && isShell) { className = 'text-yellow-300'; }
      else if (match[4]) { className = 'text-purple-300'; }
      else if (match[5]) { className = 'text-orange-300'; }
      else if (match[6]) { className = 'text-cyan-300'; }
    }
    nodes.push(<span key={`${index}-${value}`} className={className}>{value}</span>);
    lastIndex = index + value.length;
  }
  if (lastIndex < text.length) { nodes.push(text.slice(lastIndex)); }
  return nodes;
}

/** Check if a file path is a markdown file. */
export function isMarkdownFile(path: string): boolean {
  return /\.(md|markdown|mdx)$/i.test(path);
}

/** Render simple markdown content into React nodes. */
export function renderMarkdown(content: string) {
  const lines = content.split('\n');
  const blocks: ReactNode[] = [];
  const listItems: string[] = [];
  const codeLines: string[] = [];
  let inCode = false;

  const flushList = () => {
    if (listItems.length > 0) {
      blocks.push(
        <ul key={`list-${blocks.length}`} className="list-disc pl-5 my-2 space-y-1">
          {listItems.map((item, index) => (
            <li key={`${item}-${index}`}>{item}</li>
          ))}
        </ul>
      );
      listItems.length = 0;
    }
  };

  const flushCode = () => {
    if (codeLines.length > 0) {
      blocks.push(
        <pre key={`code-${blocks.length}`} className="my-2 p-3 rounded bg-secondary/40 overflow-auto text-[12px]">
          {codeLines.join('\n')}
        </pre>
      );
      codeLines.length = 0;
    }
  };

  for (const line of lines) {
    if (line.startsWith('```')) {
      if (inCode) {
        flushCode();
        inCode = false;
      } else {
        flushList();
        inCode = true;
      }
      continue;
    }

    if (inCode) {
      codeLines.push(line);
      continue;
    }

    const heading = line.match(/^(#{1,6})\s+(.*)$/);
    if (heading) {
      flushList();
      const size = heading[1].length <= 2 ? 'text-base' : 'text-sm';
      blocks.push(
        <div key={`h-${blocks.length}`} className={`${size} font-medium mt-3 mb-1 text-foreground`}>
          {heading[2]}
        </div>
      );
      continue;
    }

    const list = line.match(/^\s*[-*+]\s+(.*)$/);
    if (list) {
      listItems.push(list[1]);
      continue;
    }

    flushList();
    if (line.trim()) {
      blocks.push(<p key={`p-${blocks.length}`} className="my-1 text-foreground leading-relaxed">{line}</p>);
    } else {
      blocks.push(<div key={`br-${blocks.length}`} className="h-2" />);
    }
  }

  flushList();
  flushCode();
  return blocks;
}
