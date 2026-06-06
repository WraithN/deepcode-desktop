# HTTP + SSE 架构回滚方案

## 概述

在实施 HTTP + SSE 架构前，设计完整的回滚方案，确保可以在出现问题时快速恢复到 WebSocket 架构。

## 回滚触发条件

1. **SSE 连接不稳定** - 频繁断线或无法建立连接
2. **性能问题** - HTTP 请求延迟明显高于 WebSocket
3. **兼容性问题** - 浏览器或网络环境不支持 SSE
4. **功能缺失** - 新架构无法实现某些原有功能
5. **用户反馈** - 用户体验明显下降

## 回滚策略

### 策略一：代码分支回滚（推荐）

**实施前准备：**
1. 创建 feature 分支：`git checkout -b feature/http-sse-streaming`
2. 所有 HTTP + SSE 改动在此分支进行
3. 保持 master 分支的 WebSocket 代码不变

**回滚步骤：**
```bash
# 1. 丢弃 feature 分支
git checkout master
git branch -D feature/http-sse-streaming

# 2. 重新构建
pnpm tauri build

# 3. 重启服务
bash run-desktop.sh
```

**优点：**
- 回滚最快（秒级）
- 零数据丢失
- 无需修改代码

**缺点：**
- 需要重新构建
- 丢失 feature 分支上的新功能

### 策略二：功能开关回滚

**实施前准备：**
1. 在代码中添加功能开关
2. 同时保留 WebSocket 和 HTTP + SSE 两套代码

**代码结构：**
```typescript
// src/config/transport.ts
export const TRANSPORT_MODE = 'http-sse' as const; // 或 'websocket'

// src/db/index.ts
import { TRANSPORT_MODE } from '@/config/transport';

export const db = TRANSPORT_MODE === 'http-sse' 
  ? httpDataStore 
  : wsDataStore;
```

**回滚步骤：**
```typescript
// 修改配置文件
export const TRANSPORT_MODE = 'websocket' as const;
```

**优点：**
- 回滚最快（只需改配置）
- 无需重新构建（前端）
- 可以同时测试两套方案

**缺点：**
- 代码复杂度增加
- 需要维护两套代码
- 包体积增大

### 策略三：数据库兼容层

**实施前准备：**
1. 确保数据库 schema 兼容
2. 新旧架构使用相同的数据库表

**检查清单：**
- [ ] 不删除任何表
- [ ] 不修改现有列
- [ ] 只添加新列（可为空）
- [ ] 索引保持不变

**回滚步骤：**
1. 切换代码到 WebSocket 版本
2. 无需数据库操作

## 实施建议

### 推荐方案：策略一 + 策略二结合

1. **开发阶段**：使用 feature 分支（策略一）
2. **测试阶段**：在 feature 分支中使用功能开关（策略二）
3. **生产阶段**：合并到 master 后移除功能开关

### 具体实施步骤

#### Phase 0: 回滚准备（在实施前完成）

1. **创建 feature 分支**
   ```bash
   git checkout -b feature/http-sse-streaming
   git push -u origin feature/http-sse-streaming
   ```

2. **添加功能开关（前端）**
   ```typescript
   // src/config/transport.ts
   export const USE_HTTP_SSE = import.meta.env.VITE_USE_HTTP_SSE === 'true';
   ```

3. **添加功能开关（后端）**
   ```rust
   // src-tauri/src/config.rs
   pub const USE_HTTP_SSE: bool = cfg!(feature = "http-sse");
   ```

4. **保留 WebSocket 代码**
   - 不删除 `src/db/ws-client.ts`
   - 不删除 `src/stores/websocketStore.ts`
   - 不删除 `src-tauri/src/gateway/` 目录

#### Phase 1: 渐进式迁移

1. **先实现 HTTP API（保留 WebSocket）**
   - 新增 HTTP endpoints
   - WebSocket 继续运行
   - 前端通过开关选择使用哪个

2. **测试 HTTP API**
   - 验证所有 DB 操作正常
   - 验证性能达标

3. **实现 SSE（保留 WebSocket）**
   - 新增 SSE endpoint
   - WebSocket 继续运行
   - 前端通过开关选择使用哪个

4. **测试 SSE**
   - 验证流式输出正常
   - 验证 isTyping 行为正确

#### Phase 2: 完全切换

1. **默认使用 HTTP + SSE**
   - 修改功能开关默认值
   - WebSocket 作为 fallback

2. **监控和观察**
   - 观察 1-2 周
   - 收集用户反馈

3. **移除 WebSocket（可选）**
   - 确认稳定后移除
   - 清理无用代码

## 回滚测试

### 测试场景

1. **正常回滚**
   - 从 HTTP + SSE 回滚到 WebSocket
   - 验证所有功能正常

2. **数据一致性**
   - 回滚后数据不丢失
   - 会话和消息完整

3. **性能对比**
   - 回滚前后性能对比
   - 确保 WebSocket 性能正常

### 测试步骤

```bash
# 1. 切换到 feature 分支
git checkout feature/http-sse-streaming

# 2. 构建并测试
pnpm tauri build
bash run-desktop.sh

# 3. 发现问题，回滚
git checkout master
pnpm tauri build
bash run-desktop.sh

# 4. 验证回滚成功
# - 检查 WebSocket 连接正常
# - 检查 DB 操作正常
# - 检查消息展示正常
```

## 监控指标

### 需要监控的指标

1. **连接成功率**
   - SSE 建立成功率 > 95%
   - HTTP 请求成功率 > 99%

2. **延迟**
   - HTTP API 响应时间 < 500ms
   - SSE 首字节时间 < 1s

3. **错误率**
   - HTTP 5xx 错误率 < 1%
   - SSE 断线率 < 5%

4. **用户体验**
   - 流式输出流畅度
   - isTyping 行为正确性

### 告警阈值

- SSE 连接成功率 < 90% → 考虑回滚
- HTTP 5xx 错误率 > 5% → 立即回滚
- 用户投诉增加 > 50% → 立即回滚

## 决策流程

```
发现问题
  │
  ▼
评估影响
  │
  ├── 轻微 → 修复问题，继续观察
  │
  ├── 中等 → 启用功能开关回滚到 WebSocket
  │
  └── 严重 → 代码分支回滚到 master
  │
  ▼
验证回滚
  │
  ▼
问题修复后重新评估
```

## 总结

**推荐方案：**
1. 使用 feature 分支开发
2. 保留功能开关（开发阶段）
3. 渐进式迁移，随时可回滚
4. 充分测试后再合并到 master

**关键原则：**
- 不删除旧代码，直到新架构稳定
- 保持数据库兼容
- 快速回滚能力
- 充分监控和测试
