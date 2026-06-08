import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import FileTreePanel from './FileTreePanel';
import type { WorkspaceFileNode, GitStatusEntry } from '@/types/types';

describe('FileTreePanel', () => {
  const mockTree: WorkspaceFileNode[] = [
    {
      name: 'src',
      path: 'src',
      is_dir: true,
      ignored: false,
      children: [
        { name: 'index.ts', path: 'src/index.ts', is_dir: false, ignored: false },
        { name: 'App.tsx', path: 'src/App.tsx', is_dir: false, ignored: false },
      ],
    },
    { name: 'package.json', path: 'package.json', is_dir: false, ignored: false },
  ];

  const defaultProps = {
    fileTree: mockTree,
    gitStatus: new Map<string, GitStatusEntry['status']>(),
    fileSearch: '',
    onFileSearchChange: vi.fn(),
    fileTreeLoading: false,
    fileTreeError: null,
    expandedFolders: { src: true },
    filteredTree: mockTree,
    onLoadFileTree: vi.fn(),
    onExpandAll: vi.fn(),
    onOpenFile: vi.fn(),
    onToggleFolder: vi.fn(),
  };

  it('renders file tree with folders and files', () => {
    render(<FileTreePanel {...defaultProps} />);
    expect(screen.getByText('src')).toBeInTheDocument();
    expect(screen.getByText('package.json')).toBeInTheDocument();
  });

  it('renders children of expanded folder', () => {
    render(<FileTreePanel {...defaultProps} />);
    expect(screen.getByText('index.ts')).toBeInTheDocument();
    expect(screen.getByText('App.tsx')).toBeInTheDocument();
  });

  it('shows empty state when no files', () => {
    render(<FileTreePanel {...defaultProps} fileTree={[]} filteredTree={[]} />);
    expect(screen.getByText('暂无文件')).toBeInTheDocument();
  });

  it('shows error state', () => {
    render(<FileTreePanel {...defaultProps} fileTreeError="Failed to load" />);
    expect(screen.getByText('Failed to load')).toBeInTheDocument();
  });

  it('calls onLoadFileTree when refresh button clicked', () => {
    const onLoad = vi.fn();
    render(<FileTreePanel {...defaultProps} onLoadFileTree={onLoad} />);
    const refreshButton = screen.getByTitle('刷新');
    fireEvent.click(refreshButton);
    expect(onLoad).toHaveBeenCalled();
  });

  it('calls onExpandAll when expand button clicked', () => {
    const onExpand = vi.fn();
    render(<FileTreePanel {...defaultProps} onExpandAll={onExpand} />);
    const expandButton = screen.getByTitle('全部展开');
    fireEvent.click(expandButton);
    expect(onExpand).toHaveBeenCalled();
  });

  it('calls onFileSearchChange when typing in search', () => {
    const onSearch = vi.fn();
    render(<FileTreePanel {...defaultProps} onFileSearchChange={onSearch} />);
    const input = screen.getByPlaceholderText('搜索文件...');
    fireEvent.change(input, { target: { value: 'index' } });
    expect(onSearch).toHaveBeenCalledWith('index');
  });

  it('calls onToggleFolder when folder is clicked', () => {
    const onToggle = vi.fn();
    render(<FileTreePanel {...defaultProps} onToggleFolder={onToggle} />);
    const folder = screen.getByText('src');
    fireEvent.click(folder);
    expect(onToggle).toHaveBeenCalledWith('src', false);
  });

  it('calls onOpenFile when file is double clicked', () => {
    const onOpen = vi.fn();
    render(<FileTreePanel {...defaultProps} onOpenFile={onOpen} />);
    const file = screen.getByText('package.json');
    fireEvent.doubleClick(file);
    expect(onOpen).toHaveBeenCalledWith('package.json');
  });

  it('disables buttons when loading', () => {
    render(<FileTreePanel {...defaultProps} fileTreeLoading={true} />);
    const refreshButton = screen.getByTitle('刷新');
    expect(refreshButton).toBeDisabled();
  });
});
