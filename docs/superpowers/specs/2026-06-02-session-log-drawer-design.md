# Session Log Drawer 设计文档

## 背景

当前应用的日志通过独立的 "Debug Logs" 窗口显示，该窗口在启动时自动创建。实际使用中发现：
- 独立窗口容易被其他应用遮挡
- 用户需要频繁切换窗口查看日志
- 日志和当前会话的上下文脱节

## 目标

在 WorkspacePage 底部添加一个可开合的日志抽屉，用于显示**当前会话的完整执行日志**。通过连续点击设置按钮 5 次触发显示/隐藏。

## 设计概述

采用 **全局 SessionLogStore + 事件驱动** 架构，实现日志生产与消费完全解耦。

## 架构

```
┌─────────────────────────────────────────┐
│           WorkspacePage                  │
│  ┌───────────────────────────────────┐  │
│  │         ChatPanel / etc            │  │
│  │   sessionLog.add('thinking', ...)  │  │
│  └───────────────────────────────────┘  │
│  ┌───────────────────────────────────┐  │
│  │      SessionLogDrawer              │  │
│  │   (subscribes to SessionLogStore)  │  │
│  └───────────────────────────────────┘  │
└─────────────────────────────────────────┘
              │
              ▼
      ┌───────────────┐
      │ SessionLogStore│  <-- 全局单例，按会话隔离
      │  - logs: Map<conversationId, LogEntry[]>
      │  - listeners: Set<() => void>
      └───────────────┘
```

## 数据模型

```typescript
interface LogEntry {
  id: string;              // 唯一标识
  timestamp: string;       // HH:MM:SS.mmm
  level: 'info' | 'warn' | 'error' | 'debug';
  source: string;          // 来源组件，如 "OpencodeAdapter"
  message: string;         // 日志消息
  detail?: Record<string, unknown>; // 可选详情
}

interface SessionLogStore {
  // 添加日志到当前会话
  add(level: LogLevel, source: string, message: string, detail?: Record<string, unknown>): void;
  // 获取指定会话的日志
  getLogs(conversationId: string): LogEntry[];
  // 清空指定会话日志
  clear(conversationId: string): void;
  // 订阅变化
  subscribe(listener: () => void): () => void;
}
```

## 组件设计

### SessionLogStore (`src/store/session-log.ts`)

- 全局单例，无需 React Context
- 按 `conversationId` 隔离日志，切换会话时自动显示对应日志
- 每个会话最多保留 500 条日志，防止内存泄漏

### SessionLogDrawer (`src/components/workspace/SessionLogDrawer.tsx`)

- **位置**：WorkspacePage 底部，绝对定位覆盖在内容区之上
- **默认高度**：200px
- **最小高度**：100px
- **最大高度**：400px
- **拖拽调整**：底部边框拖拽改变高度
- **内容**：按时间倒序或正序排列的日志条目（建议正序，最新的在最下面）
- **自动滚动**：新增日志时自动滚动到底部
- **清空按钮**：右上角提供清空当前会话日志按钮
- **关闭按钮**：右上角 X 按钮
- **样式**：深色背景 `bg-gray-900`，等宽字体，带颜色区分级别

### 触发逻辑（WorkspacePage 设置按钮）

```typescript
const [clickCount, setClickCount] = useState(0);
const clickTimerRef = useRef<NodeJS.Timeout | null>(null);

const handleSettingsClick = () => {
  const newCount = clickCount + 1;
  setClickCount(newCount);
  
  if (clickTimerRef.current) clearTimeout(clickTimerRef.current);
  clickTimerRef.current = setTimeout(() => setClickCount(0), 1000);
  
  if (newCount >= 5) {
    setClickCount(0);
    setLogDrawerOpen((v) => !v); // 切换显示/隐藏
  }
};
```

- 1 秒内连续点击 5 次触发
- 超过 1 秒未点击则计数器重置
- 打开设置对话框的单击逻辑不受影响（通过双击或长按区分？）

> **交互细节问题**：设置按钮原本单击打开 SettingsDialog。需要改为：
> - 普通单击 → 打开 SettingsDialog
> - 快速连续 5 次单击 → 切换日志抽屉
> 
> 实现方式：计数器达到 5 时不打开 SettingsDialog，只切换抽屉。

## 数据流

1. **用户发送消息** → WorkspacePage `handleSendMessage`
2. **产生日志** → `sessionLog.add('info', 'WorkspacePage', 'handleSendMessage called', {...})`
3. **Store 更新** → 通知所有订阅者
4. **Drawer 刷新** → 如果当前会话匹配，追加新日志并自动滚动

## 需要删除的内容

1. **独立日志窗口**（Rust 侧）
   - `src-tauri/src/main.rs` 中创建 `logs` 窗口的代码
   - `src-tauri/capabilities/default.json` 中的 `"logs"` 窗口引用
   - `src-tauri/tauri.conf.json` 中的 `"devtools": true`

2. **独立日志页面**
   - `src/pages/LogWindow.tsx`
   - `src/routes.tsx` 中的 `/logs` 路由

3. **旧日志工具**
   - `src/utils/logEmitter.ts`
   - `src/utils/getCurrentWindowLabel.ts`
   - `src/services/debug-logger.ts`（或整合到新 store）

4. **App.tsx 中的日志窗口重定向逻辑**

## 集成点

需要替换日志调用的文件：
- `src/agents/opencode/adapter.ts` — 将 `appLog`/`debugLogger` 改为 `sessionLog.add`
- `src/pages/WorkspacePage.tsx` — 将 `appLog` 改为 `sessionLog.add`
- 其他组件中的 `console.log` 逐步迁移

## 错误处理

- Store 初始化失败不应阻塞应用启动
- Drawer 渲染错误不应影响主界面
- 日志过多时自动截断（保留最近 500 条）

## 测试计划

1. **单元测试**：SessionLogStore 的 add/get/clear/subscribe
2. **组件测试**：Drawer 渲染、滚动、清空、关闭
3. **交互测试**：5 次点击触发、1 秒内重置、普通单击不受影响

## 未来扩展

- 日志导出（复制到剪贴板 / 保存为文件）
- 日志过滤（按级别、按来源）
- 全局日志模式（查看所有会话的日志）
