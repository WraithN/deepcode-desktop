import React from 'react';

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
  }

  render() {
    if (this.state.error) {
      return (
        <div className="min-h-screen bg-background text-foreground p-6 font-mono">
          <h1 className="text-lg font-semibold text-destructive mb-3">页面渲染失败</h1>
          <pre className="whitespace-pre-wrap text-sm text-muted-foreground">
            {this.state.error.stack || this.state.error.message}
          </pre>
        </div>
      );
    }

    return this.props.children;
  }
}
