# Session Log Drawer Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the standalone Debug Logs window with a bottom drawer in WorkspacePage that displays current session execution logs, triggered by 5 rapid clicks on the settings button.

**Architecture:** A global `SessionLogStore` singleton manages per-conversation log arrays. Components emit logs via `sessionLog.add()` without knowing about the UI. The `SessionLogDrawer` React component subscribes to the store and renders logs for the active conversation. The drawer is toggled by a click counter on the settings button.

**Tech Stack:** React 18, TypeScript, Tailwind CSS, shadcn/ui patterns

---

## File Map

| File | Action | Responsibility |
|------|--------|----------------|
| `src/store/session-log.ts` | Create | Global log store singleton, per-conversation isolation |
| `src/components/workspace/SessionLogDrawer.tsx` | Create | Bottom drawer UI, auto-scroll, clear/close buttons |
| `src/pages/WorkspacePage.tsx` | Modify | Add drawer + 5-click trigger on settings button |
| `src/agents/opencode/adapter.ts` | Modify | Replace `appLog`/`debugLogger` with `sessionLog.add` |
| `src/services/debug-logger.ts` | Modify | Delegate to `sessionLog.add` instead of `appLog` |
| `src-tauri/src/main.rs` | Modify | Remove `logs` window creation |
| `src-tauri/capabilities/default.json` | Modify | Remove `"logs"` from windows array |
| `src-tauri/tauri.conf.json` | Modify | Remove `"devtools": true` |
| `src/pages/LogWindow.tsx` | Delete | Standalone log page no longer needed |
| `src/utils/logEmitter.ts` | Delete | Replaced by session-log store |
| `src/utils/getCurrentWindowLabel.ts` | Delete | No longer needed |
| `src/routes.tsx` | Modify | Remove `/logs` route |
| `src/App.tsx` | Modify | Remove `LogWindowRedirect` component |
| `src/store/session-log.test.ts` | Create | Unit tests for the store |

---

### Task 1: Create SessionLogStore

**Files:**
- Create: `src/store/session-log.ts`
- Test: `src/store/session-log.test.ts`

**Design:**
- Global singleton, no React Context needed
- `Map<string, LogEntry[]>` keyed by `conversationId`
- Max 500 logs per conversation to prevent memory leaks
- Simple pub-sub pattern with `Set<() => void>` listeners

- [ ] **Step 1: Write the store**

```typescript
// src/store/session-log.ts
export type LogLevel = 'info' | 'warn' | 'error' | 'debug';

export interface LogEntry {
  id: string;
  timestamp: string;
  level: LogLevel;
  source: string;
  message: string;
  detail?: Record<string, unknown>;
}

class SessionLogStore {
  private logs = new Map<string, LogEntry[]>();
  private listeners = new Set<() => void>();
  private maxLogsPerSession = 500;

  add(
    conversationId: string,
    level: LogLevel,
    source: string,
    message: string,
    detail?: Record<string, unknown>,
  ): void {
    const entry: LogEntry = {
      id: `${Date.now()}-${Math.random().toString(36).slice(2, 7)}`,
      timestamp: new Date().toLocaleTimeString('en-US', { hour12: false }) + '.' + String(new Date().getMilliseconds()).padStart(3, '0'),
      level,
      source,
      message,
      detail,
    };

    const sessionLogs = this.logs.get(conversationId) || [];
    sessionLogs.push(entry);
    if (sessionLogs.length > this.maxLogsPerSession) {
      sessionLogs.splice(0, sessionLogs.length - this.maxLogsPerSession);
    }
    this.logs.set(conversationId, sessionLogs);
    this.notify();
  }

  getLogs(conversationId: string): LogEntry[] {
    return this.logs.get(conversationId) || [];
  }

  clear(conversationId: string): void {
    this.logs.delete(conversationId);
    this.notify();
  }

  subscribe(listener: () => void): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  private notify(): void {
    for (const listener of this.listeners) {
      listener();
    }
  }
}

export const sessionLog = new SessionLogStore();
```

- [ ] **Step 2: Write the test**

```typescript
// src/store/session-log.test.ts
import { describe, it, expect, vi } from 'vitest';
import { sessionLog } from './session-log';

describe('SessionLogStore', () => {
  it('should add and retrieve logs for a conversation', () => {
    sessionLog.clear('conv-1');
    sessionLog.add('conv-1', 'info', 'TestSource', 'hello');
    const logs = sessionLog.getLogs('conv-1');
    expect(logs).toHaveLength(1);
    expect(logs[0].source).toBe('TestSource');
    expect(logs[0].message).toBe('hello');
    expect(logs[0].level).toBe('info');
  });

  it('should isolate conversations', () => {
    sessionLog.clear('conv-a');
    sessionLog.clear('conv-b');
    sessionLog.add('conv-a', 'info', 'Src', 'msg-a');
    sessionLog.add('conv-b', 'info', 'Src', 'msg-b');
    expect(sessionLog.getLogs('conv-a')).toHaveLength(1);
    expect(sessionLog.getLogs('conv-b')).toHaveLength(1);
    expect(sessionLog.getLogs('conv-a')[0].message).toBe('msg-a');
  });

  it('should notify subscribers', () => {
    sessionLog.clear('conv-sub');
    const listener = vi.fn();
    const unsubscribe = sessionLog.subscribe(listener);
    sessionLog.add('conv-sub', 'info', 'Src', 'test');
    expect(listener).toHaveBeenCalledTimes(1);
    unsubscribe();
    sessionLog.add('conv-sub', 'info', 'Src', 'test2');
    expect(listener).toHaveBeenCalledTimes(1);
  });

  it('should clear logs', () => {
    sessionLog.clear('conv-clear');
    sessionLog.add('conv-clear', 'info', 'Src', 'msg');
    expect(sessionLog.getLogs('conv-clear')).toHaveLength(1);
    sessionLog.clear('conv-clear');
    expect(sessionLog.getLogs('conv-clear')).toHaveLength(0);
  });

  it('should cap logs at 500 per session', () => {
    sessionLog.clear('conv-cap');
    for (let i = 0; i < 510; i++) {
      sessionLog.add('conv-cap', 'info', 'Src', `msg-${i}`);
    }
    expect(sessionLog.getLogs('conv-cap')).toHaveLength(500);
    expect(sessionLog.getLogs('conv-cap')[0].message).toBe('msg-10');
  });
});
```

- [ ] **Step 3: Run tests**

Run: `cd /home/nan/deepcode-desktop && npx vitest run src/store/session-log.test.ts`
Expected: 5 tests PASS

- [ ] **Step 4: Commit**

```bash
git add src/store/session-log.ts src/store/session-log.test.ts
git commit -m "feat: add SessionLogStore for per-conversation log management"
```

---

### Task 2: Create SessionLogDrawer Component

**Files:**
- Create: `src/components/workspace/SessionLogDrawer.tsx`

**Design:**
- Fixed position at bottom of parent container
- Default height 200px, min 100px, max 400px
- Draggable bottom border to resize
- Auto-scroll to bottom on new logs
- Clear button and Close (X) button in toolbar
- Monospace font, color-coded log levels

- [ ] **Step 1: Write the component**

```typescript
// src/components/workspace/SessionLogDrawer.tsx
import React, { useEffect, useRef, useState, useCallback } from 'react';
import { X, Trash2 } from 'lucide-react';
import type { LogEntry } from '@/store/session-log';

interface SessionLogDrawerProps {
  logs: LogEntry[];
  onClose: () => void;
  onClear: () => void;
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

const SessionLogDrawer: React.FC<SessionLogDrawerProps> = ({ logs, onClose, onClear }) => {
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
    if (!isDragging) return;
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

  return (
    <div
      ref={containerRef}
      className="absolute bottom-0 left-0 right-0 bg-gray-950 border-t border-gray-800 flex flex-col z-50"
      style={{ height }}
    >
      {/* Drag handle */}
      <div
        className="h-1 bg-gray-800 cursor-row-resize hover:bg-gray-600 transition-colors"
        onMouseDown={handleMouseDown}
      />

      {/* Toolbar */}
      <div className="flex items-center justify-between px-3 py-1.5 bg-gray-900 border-b border-gray-800">
        <span className="text-xs font-semibold text-gray-400">Session Logs ({logs.length})</span>
        <div className="flex items-center gap-1">
          <button
            onClick={onClear}
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
            className={`flex gap-2 px-3 py-0.5 text-[11px] font-mono border-b border-gray-900/50 hover:bg-gray-800/30 ${levelBg[log.level] || ''}`}
          >
            <span className="text-gray-600 shrink-0 w-[60px]">{log.timestamp}</span>
            <span className={`shrink-0 w-[40px] font-bold ${levelColors[log.level] || 'text-gray-400'}`}>
              {log.level.toUpperCase()}
            </span>
            <span className="text-gray-500 shrink-0 w-[100px] truncate">{log.source}</span>
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
```

- [ ] **Step 2: Commit**

```bash
git add src/components/workspace/SessionLogDrawer.tsx
git commit -m "feat: add SessionLogDrawer component"
```

---

### Task 3: Integrate Drawer into WorkspacePage

**Files:**
- Modify: `src/pages/WorkspacePage.tsx`

**Changes needed:**
1. Import `sessionLog` and `SessionLogDrawer`
2. Add `logDrawerOpen` state and `clickCount`/`clickTimerRef` for 5-click trigger
3. Modify settings button `onClick` to count clicks
4. Subscribe to `sessionLog` store for current conversation
5. Render `SessionLogDrawer` conditionally at bottom of main layout

- [ ] **Step 1: Add imports and state**

At top of `src/pages/WorkspacePage.tsx`, add:
```typescript
import { sessionLog } from '@/store/session-log';
import SessionLogDrawer from '@/components/workspace/SessionLogDrawer';
```

In component body, add after existing state:
```typescript
const [logDrawerOpen, setLogDrawerOpen] = useState(false);
const [sessionLogs, setSessionLogs] = useState<import('@/store/session-log').LogEntry[]>([]);
const clickCountRef = useRef(0);
const clickTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
```

- [ ] **Step 2: Subscribe to log store**

Add useEffect:
```typescript
useEffect(() => {
  if (!activeConversation) {
    setSessionLogs([]);
    return;
  }
  const update = () => setSessionLogs(sessionLog.getLogs(activeConversation.id));
  update();
  const unsubscribe = sessionLog.subscribe(update);
  return unsubscribe;
}, [activeConversation]);
```

- [ ] **Step 3: Add 5-click trigger handler**

Replace settings button `onClick`:
```typescript
const handleSettingsClick = () => {
  clickCountRef.current += 1;
  if (clickTimerRef.current) clearTimeout(clickTimerRef.current);
  clickTimerRef.current = setTimeout(() => {
    clickCountRef.current = 0;
  }, 1000);

  if (clickCountRef.current >= 5) {
    clickCountRef.current = 0;
    if (clickTimerRef.current) clearTimeout(clickTimerRef.current);
    setLogDrawerOpen((v) => !v);
  } else {
    setSettingsOpen(true);
  }
};
```

Update settings button:
```tsx
<Button
  variant="ghost"
  size="icon"
  onClick={handleSettingsClick}
  className="h-7 w-7 text-muted-foreground hover:text-foreground"
>
  <Settings className="w-4 h-4" />
</Button>
```

- [ ] **Step 4: Render drawer**

Wrap the main content div with `relative` and add drawer:
```tsx
{/* 主内容区 */}
<div className="flex flex-1 min-h-0 relative">
  {/* ... existing panels ... */}

  {logDrawerOpen && activeConversation && (
    <SessionLogDrawer
      logs={sessionLogs}
      onClose={() => setLogDrawerOpen(false)}
      onClear={() => sessionLog.clear(activeConversation.id)}
    />
  )}
</div>
```

- [ ] **Step 5: Commit**

```bash
git add src/pages/WorkspacePage.tsx
git commit -m "feat: integrate SessionLogDrawer with 5-click settings trigger"
```

---

### Task 4: Replace Log Calls in Adapter and Page

**Files:**
- Modify: `src/agents/opencode/adapter.ts`
- Modify: `src/pages/WorkspacePage.tsx`
- Modify: `src/services/debug-logger.ts`

**Strategy:**
- `adapter.ts`: Replace `appLog.xxx('OpencodeAdapter', ...)` with `sessionLog.add('current-conv-id', level, 'OpencodeAdapter', ...)`
- But wait — adapter doesn't know the conversation ID. The adapter receives `instanceId` as `_instanceId`.

**Decision:** The adapter should NOT know about conversations. It emits technical logs about CLI execution. We'll pass the conversation ID through `options` or have the caller (WorkspacePage) log on behalf of the adapter.

**Revised approach:**
- Remove `appLog` imports from adapter
- Keep `debugLogger` for file-based logging (it already works)
- WorkspacePage already logs the key events. Just ensure those use `sessionLog.add`.
- The adapter's internal logging can stay as `debugLogger.log` (goes to file) or simple `console.log`.

Actually, looking more carefully: the user wants the **session log** to show the execution flow. The best place to log is in WorkspacePage's `handleSendMessage` and in the adapter's `sendMessage` generator. But the adapter doesn't know the conversation ID.

**Solution:** Pass `conversationId` through `sendMessage` options, or have WorkspacePage log adapter events as they arrive.

Simpler: Keep adapter logs as `debugLogger.log` (file only). Have WorkspacePage log the high-level flow using `sessionLog.add`. The user can see "starting sendMessage", "received event: thinking", "received event: text_delta", etc. That's sufficient for session debugging.

- [ ] **Step 1: Update adapter.ts — remove appLog**

Remove `appLog` import. Replace all `appLog.xxx(...)` calls with `debugLogger.log(...)` (already imported). Keep `console.error` for actual errors.

```typescript
// Remove this import:
// import { appLog } from '@/utils/logEmitter';
```

Replace patterns like:
```typescript
appLog.error('OpencodeAdapter', 'execute failed:', execError);
```
→
```typescript
await debugLogger.log('OpencodeAdapter', 'execute failed', { error: String(execError) });
```

Similarly for all `appLog.log`, `appLog.warn`, `appLog.error` calls.

- [ ] **Step 2: Update WorkspacePage.tsx — replace appLog with sessionLog**

Replace all `appLog.log('WorkspacePage', ...)` with:
```typescript
sessionLog.add(activeConversation.id, 'info', 'WorkspacePage', message, detail);
```

Replace `appLog.error` with `'error'` level.

Note: Need to guard against `!activeConversation` when calling `sessionLog.add`. But these calls are inside `handleSendMessage` which already checks `!user || !activeConversation` at the top, so it's safe.

Example replacements:
```typescript
// Before:
appLog.log('WorkspacePage', 'handleSendMessage called', { content, hasUser: !!user, hasConversation: !!activeConversation });
// After:
sessionLog.add(activeConversation.id, 'info', 'WorkspacePage', 'handleSendMessage called', { content, hasUser: !!user, hasConversation: !!activeConversation });
```

- [ ] **Step 3: Update debug-logger.ts**

Change `debugLogger` to also emit to `sessionLog` if a "current conversation" is set. But that couples it. Better: keep `debugLogger` as file-only logger. The session drawer is fed by explicit `sessionLog.add()` calls.

Remove `appLog` import from `debug-logger.ts` and use `console.log` fallback:
```typescript
// Remove: import { appLog } from '@/utils/logEmitter';
// In catch blocks, use console.error/console.log
```

- [ ] **Step 4: Commit**

```bash
git add src/agents/opencode/adapter.ts src/pages/WorkspacePage.tsx src/services/debug-logger.ts
git commit -m "refactor: replace appLog with sessionLog.add for session-visible logging"
```

---

### Task 5: Remove Old Log Infrastructure

**Files:**
- Delete: `src/pages/LogWindow.tsx`
- Delete: `src/utils/logEmitter.ts`
- Delete: `src/utils/getCurrentWindowLabel.ts`
- Modify: `src/routes.tsx`
- Modify: `src/App.tsx`
- Modify: `src-tauri/src/main.rs`
- Modify: `src-tauri/capabilities/default.json`
- Modify: `src-tauri/tauri.conf.json`

- [ ] **Step 1: Delete frontend files**

```bash
rm src/pages/LogWindow.tsx
rm src/utils/logEmitter.ts
rm src/utils/getCurrentWindowLabel.ts
```

- [ ] **Step 2: Update routes.tsx**

Remove LogWindow import and `/logs` route.

```typescript
// Remove: import LogWindow from './pages/LogWindow';
// Remove the { name: '日志', path: '/logs', ... } route object
```

- [ ] **Step 3: Update App.tsx**

Remove `LogWindowRedirect` component and its usage. Keep only the theme setup and routes.

- [ ] **Step 4: Update main.rs**

Remove the `WebviewWindowBuilder` block that creates the `logs` window:
```rust
// Remove this block from setup():
// let _log_window = tauri::WebviewWindowBuilder::new(...)
```

- [ ] **Step 5: Update capabilities/default.json**

Remove `"logs"` from windows array:
```json
"windows": ["main"]
```

Remove `"core:event:default"` permission (no longer needed for cross-window events).

- [ ] **Step 6: Update tauri.conf.json**

Remove `"devtools": true` from the window config.

- [ ] **Step 7: Commit**

```bash
git add -A
git commit -m "chore: remove standalone Debug Logs window and related infrastructure"
```

---

### Task 6: Run Full Build and Verify

- [ ] **Step 1: Run frontend build**

```bash
cd /home/nan/deepcode-desktop && pnpm build
```
Expected: Success with no TypeScript errors

- [ ] **Step 2: Run tests**

```bash
cd /home/nan/deepcode-desktop && npx vitest run
```
Expected: All tests pass (including new session-log.test.ts)

- [ ] **Step 3: Run Tauri build**

```bash
cd /home/nan/deepcode-desktop/src-tauri && cargo build --release
```
Expected: Compiles successfully

- [ ] **Step 4: Manual smoke test**

Start the app, log in, create a conversation, send a message. Then:
1. Click settings button 5 times rapidly → drawer should appear
2. Drawer should show session logs
3. Click X → drawer should close
4. Click 5 times again → drawer should re-open
5. Click Trash → logs should clear
6. Drag bottom border → height should adjust

- [ ] **Step 5: Final commit**

```bash
git add -A
git commit -m "feat: session log drawer complete - replaces standalone debug window"
```

---

## Self-Review Checklist

**Spec coverage:**
- ✅ Global SessionLogStore with per-conversation isolation → Task 1
- ✅ Bottom drawer UI with auto-scroll → Task 2
- ✅ 5-click settings trigger → Task 3
- ✅ Session log content (current conversation) → Task 3/4
- ✅ Close button (X) and Clear button → Task 2
- ✅ Draggable height adjustment → Task 2
- ✅ Delete old Debug Logs window → Task 5
- ✅ Max 500 logs per session → Task 1

**Placeholder scan:**
- ✅ No TBD/TODO/fill-in-later
- ✅ All code blocks contain complete implementation
- ✅ Exact file paths provided

**Type consistency:**
- ✅ `LogEntry` interface defined once in `session-log.ts`, used by drawer
- ✅ `sessionLog.add()` signature consistent across all call sites
