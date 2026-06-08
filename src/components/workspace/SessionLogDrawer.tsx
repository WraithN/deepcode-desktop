import React, { useEffect, useRef, useState, useCallback } from 'react';
import { Copy, X, Trash2 } from 'lucide-react';
import { toast } from 'sonner';
import { formatIdShort } from '@/lib/id';
import { useLogStore } from '@/stores';
import type { LogEntry } from '@/store/session-log';

interface SessionLogDrawerProps {
  logs?: LogEntry[];
  onClose: () => void;
  onClear?: () => void;
}

const levelColors: Record<string, string> = {
  info: 'text-blue-400',
  warn: 'text-yellow-400',
  error: 'text-red-400',
  debug: 'text-gray-400',
};

const levelBg: Record<string, string> = {
  info: 'bg-blue-950/20',
  warn: 'bg-yellow-950/20',
  error: 'bg-red-950/20',
  debug: '',
};

const SessionLogDrawer: React.FC<SessionLogDrawerProps> = ({ onClose, onClear }) => {
  const logs = useLogStore((s) => s.logs);
  const [height, setHeight] = useState(200);
  const [isDragging, setIsDragging] = useState(false);
  const containerRef = useRef<HTMLDivElement>(null);
  const logsRef = useRef<HTMLDivElement>(null);
  const startYRef = useRef(0);
  const startHeightRef = useRef(200);

  const handleMouseDown = useCallback((e: React.MouseEvent) => {
    setIsDragging(true);
    startYRef.current = e.clientY;
    startHeightRef.current = height;
    e.preventDefault();
  }, [height]);

  useEffect(() => {
    if (!isDragging) { return; }
    const handleMouseMove = (e: MouseEvent) => {
      const delta = startYRef.current - e.clientY;
      const newHeight = Math.min(Math.max(startHeightRef.current + delta, 100), 400);
      setHeight(newHeight);
    };
    const handleMouseUp = () => setIsDragging(false);
    document.addEventListener('mousemove', handleMouseMove);
    document.addEventListener('mouseup', handleMouseUp);
    return () => {
      document.removeEventListener('mousemove', handleMouseMove);
      document.removeEventListener('mouseup', handleMouseUp);
    };
  }, [isDragging]);

  useEffect(() => {
    if (logsRef.current) {
      logsRef.current.scrollTop = logsRef.current.scrollHeight;
    }
  }, [logs]);

  const handleClear = () => {
    useLogStore.setState({ logs: [], filteredLogs: [] });
    onClear?.();
  };

  const handleCopy = async () => {
    if (logs.length === 0) {
      toast.info('暂无日志可复制');
      return;
    }

    const text = logs.map((log) => {
      const detail = log.detail ? ` ${JSON.stringify(log.detail)}` : '';
      const instance = log.instanceId ? `${formatIdShort(log.instanceId)} · ${log.source}` : log.source;
      return `[${log.timestamp}] [${log.level.toUpperCase()}] [${instance}] ${log.message}${detail}`;
    }).join('\n');

    await navigator.clipboard.writeText(text);
    toast.success('日志已复制');
  };

  return (
    <div
      ref={containerRef}
      className="bg-gray-950 border-t border-gray-800 flex flex-col shrink-0"
      style={{ height }}
    >
      {/* Drag handle */}
      <div
        className="h-1 bg-gray-800 cursor-row-resize hover:bg-gray-600 transition-colors"
        onMouseDown={handleMouseDown}
      />

      {/* Toolbar */}
      <div className="flex items-center justify-between px-3 py-1.5 bg-gray-900 border-b border-gray-800">
        <span className="text-xs font-normal text-gray-400">Session Logs ({logs.length})</span>
        <div className="flex items-center gap-1">
          <button
            onClick={handleCopy}
            className="p-1 text-gray-500 hover:text-gray-300 transition-colors"
            title="复制日志"
          >
            <Copy className="w-3.5 h-3.5" />
          </button>
          <button
            onClick={handleClear}
            className="p-1 text-gray-500 hover:text-red-400 transition-colors"
            title="Clear logs"
          >
            <Trash2 className="w-3.5 h-3.5" />
          </button>
          <button
            onClick={onClose}
            className="p-1 text-gray-500 hover:text-gray-300 transition-colors"
            title="Close drawer"
          >
            <X className="w-3.5 h-3.5" />
          </button>
        </div>
      </div>

      {/* Log list */}
      <div ref={logsRef} className="flex-1 overflow-y-auto p-0">
        {logs.length === 0 && (
          <div className="flex items-center justify-center h-full text-gray-600 text-xs">
            No logs for this session yet...
          </div>
        )}
        {logs.map((log) => (
          <div
            key={log.id}
            className={`flex gap-2 px-3 py-0.5 text-[12px] font-mono border-b border-gray-900/50 hover:bg-gray-800/30 ${levelBg[log.level] || ''}`}
          >
            <span className="text-gray-600 shrink-0 w-[60px]">{log.timestamp}</span>
            <span className={`shrink-0 w-[40px] font-semibold ${levelColors[log.level] || 'text-gray-400'}`}>
              {log.level.toUpperCase()}
            </span>
            <span className="text-gray-500 shrink-0 w-[100px] truncate">
              {log.instanceId ? `${formatIdShort(log.instanceId)} · ${log.source}` : log.source}
            </span>
            <span className="text-gray-300 break-all whitespace-pre-wrap">
              {log.message}
              {log.detail && (
                <span className="text-gray-500 ml-1">
                  {JSON.stringify(log.detail).substring(0, 200)}
                </span>
              )}
            </span>
          </div>
        ))}
      </div>
    </div>
  );
};

export default SessionLogDrawer;
