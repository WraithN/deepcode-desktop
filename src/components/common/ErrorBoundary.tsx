import React from 'react';
import { invoke } from '@tauri-apps/api/core';

interface ErrorBoundaryState {
  error: Error | null;
}

export class ErrorBoundary extends React.Component<React.PropsWithChildren, ErrorBoundaryState> {
  state: ErrorBoundaryState = { error: null };

  static getDerivedStateFromError(error: Error): ErrorBoundaryState {
    return { error };
  }

  componentDidCatch(error: Error, errorInfo: React.ErrorInfo) {
    console.error('[ErrorBoundary]', error, errorInfo);
    try {
      void invoke('console_logs', {
        logs: [
          { type: 'error', message: `[ErrorBoundary] ${error.message}` },
          { type: 'error', message: `[ErrorBoundary] stack: ${(error.stack || '').slice(0, 1000)}` },
          { type: 'error', message: `[ErrorBoundary] componentStack: ${(errorInfo.componentStack || '').slice(0, 500)}` },
        ]
      });
    } catch (_) { /* ignore */ }
  }

  render() {
    if (this.state.error) {
      return (
        <div className="min-h-screen bg-background text-foreground p-6 font-mono">
          <h1 className="text-lg font-semibold text-destructive mb-3">页面渲染失败</h1>
          <div className="mb-2 text-xs text-muted-foreground">
            <span>Error: {this.state.error.message}</span>
          </div>
          <pre className="whitespace-pre-wrap text-sm text-muted-foreground">
            {this.state.error.stack || this.state.error.message}
          </pre>
        </div>
      );
    }

    return this.props.children;
  }
}
