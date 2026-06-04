# Bug: 通信错误 "No active agent instance"

## 发现时间
2026-06-03

## 现象
发送消息时弹出错误提示：
> 通信错误: Error: No active agent instance

## 根因
`chatStore`（Zustand store）和 `agentStore` 各自维护 `activeInstanceId`：
- `agentStore.activeInstanceId`：控制 UI 中哪个 agent 被选中
- `chatStore.activeInstanceId`：发送消息时用于 WebSocket 请求

**WorkspacePage 初始化 useEffect 只同步了 `agentStore.activeInstanceId`，漏掉了 `chatStore.activeInstanceId`**。

```typescript
// 修复前（只设置了 agentStore）
setAgentInstances(stored);
setActiveAgentId(getStoredActiveAgentId());  // agentStore
// ❌ 缺少：setChatActiveInstanceId(activeId)
```

## 影响范围
- 用户选择 agent 后无法发送消息
- 阻塞核心聊天功能

## 解决方案
在 WorkspacePage 初始化时同时同步两个 store：

```typescript
const activeId = getStoredActiveAgentId();
setActiveAgentId(activeId);           // agentStore
setChatActiveInstanceId(activeId);    // chatStore ← 新增
```

## 修复文件
- `src/pages/WorkspacePage.tsx`

## 验证方法
1. 启动应用
2. 选择/添加 agent
3. 发送消息
4. 不应再出现 "No active agent instance" 错误
