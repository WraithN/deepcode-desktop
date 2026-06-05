import { invoke } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { Minus, Square, X } from 'lucide-react';
import type { MouseEvent, ReactNode } from 'react';

interface WindowTitleBarProps {
  title?: string;
  children?: ReactNode;
}

const appWindow = getCurrentWindow();

export default function WindowTitleBar({ title = 'DeepHarness Desktop', children }: WindowTitleBarProps) {
  const runWindowAction = async (action: 'minimize' | 'toggle_maximize' | 'close') => {
    console.log('[WindowTitleBar] button clicked:', action);
    try {
      if (action === 'minimize') await appWindow.minimize();
      if (action === 'toggle_maximize') await appWindow.toggleMaximize();
      if (action === 'close') await appWindow.close();
      console.log('[WindowTitleBar] js window api success:', action);
    } catch (error) {
      console.error('[WindowTitleBar] js window api failed:', action, error);
      await invoke('window_control', { action });
      console.log('[WindowTitleBar] rust fallback success:', action);
    }
  };

  const minimize = () => runWindowAction('minimize');

  const toggleMaximize = () => runWindowAction('toggle_maximize');

  const closeWindow = () => runWindowAction('close');

  const startDragging = async (event: MouseEvent<HTMLDivElement>) => {
    if (event.button !== 0) return;
    if (event.target instanceof HTMLElement && event.target.closest('[data-no-drag]')) return;
    console.log('[WindowTitleBar] start dragging');
    try {
      await appWindow.startDragging();
      console.log('[WindowTitleBar] start dragging success');
    } catch (error) {
      console.error('[WindowTitleBar] start dragging failed:', error);
    }
  };

  return (
    <div
      className="h-[38px] border-b border-border bg-card flex items-center justify-between shrink-0 select-none"
      onMouseDown={startDragging}
    >
      <div className="flex items-center min-w-0 flex-1 h-full">
        <div className="px-3 text-xs text-muted-foreground truncate pointer-events-none">{title}</div>
        {children}
      </div>
      <div data-no-drag className="flex h-full [-webkit-app-region:no-drag]">
        <button
          type="button"
          onPointerDown={(event) => {
            event.stopPropagation();
            console.log('[WindowTitleBar] pointer down: minimize');
          }}
          onMouseDown={(event) => event.stopPropagation()}
          onPointerUp={() => console.log('[WindowTitleBar] pointer up: minimize')}
          onClick={minimize}
          className="w-11 h-full flex items-center justify-center text-muted-foreground hover:text-foreground hover:bg-secondary/70 transition-colors [-webkit-app-region:no-drag]"
          aria-label="最小化"
        >
          <Minus className="w-4 h-4" />
        </button>
        <button
          type="button"
          onPointerDown={(event) => {
            event.stopPropagation();
            console.log('[WindowTitleBar] pointer down: toggle_maximize');
          }}
          onMouseDown={(event) => event.stopPropagation()}
          onPointerUp={() => console.log('[WindowTitleBar] pointer up: toggle_maximize')}
          onClick={toggleMaximize}
          className="w-11 h-full flex items-center justify-center text-muted-foreground hover:text-foreground hover:bg-secondary/70 transition-colors [-webkit-app-region:no-drag]"
          aria-label="最大化"
        >
          <Square className="w-3.5 h-3.5" />
        </button>
        <button
          type="button"
          onPointerDown={(event) => {
            event.stopPropagation();
            console.log('[WindowTitleBar] pointer down: close');
          }}
          onMouseDown={(event) => event.stopPropagation()}
          onPointerUp={() => console.log('[WindowTitleBar] pointer up: close')}
          onClick={closeWindow}
          className="w-11 h-full flex items-center justify-center text-muted-foreground hover:text-white hover:bg-red-500 transition-colors [-webkit-app-region:no-drag]"
          aria-label="关闭"
        >
          <X className="w-4 h-4" />
        </button>
      </div>
    </div>
  );
}
