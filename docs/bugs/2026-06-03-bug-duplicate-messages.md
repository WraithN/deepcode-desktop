# Bug: 发送一条消息展示多条重复消息

## 发现时间
2026-06-03

## 现象
用户在聊天框输入一条消息（如 "nihao"），界面展示了 4 条相同的用户消息。

## 根因
`WorkspacePage.handleSendMessage` 和 `chatStore.sendMessage` **各自独立添加了一条用户消息**到 messages 状态：

```typescript
// WorkspacePage.tsx handleSendMessage
const userMsg = await db.createMessage({...});
setMessages((prev) => [...prev, userMsg]);        // ← 第1次添加
// ...
await chatSendMessage(content);                    // 调用 chatStore.sendMessage

// chatStore.ts sendMessage
const userMessage: Message = { ... };
set((state) => ({
  messages: [...state.messages, userMessage],     // ← 第2次添加
  isStreaming: true,
  currentConversationId: conversationId,
}));
```

消息被添加了两次，加上 React StrictMode 的 double-render 效果，最终显示 4 条。

## 影响范围
- 所有用户发送的消息都会重复显示
- 阻塞核心聊天功能

## 解决方案
移除 `chatStore.sendMessage` 中的用户消息添加逻辑，让它只负责 WebSocket 通信。UI 消息状态由 `WorkspacePage.handleSendMessage` 统一管理。

## 修复文件
- `src/stores/chatStore.ts`

## 验证方法
1. 启动应用
2. 发送消息
3. 应只显示一条用户消息
