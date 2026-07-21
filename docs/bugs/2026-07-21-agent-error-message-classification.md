# session.error 映射优化：错误信息提炼与前端分类

## 现象
Agent 运行出错时，用户看到的信息过于笼统：
- `RUN_ERROR` 事件：直接展示原始错误消息 `运行出错：${errorMsg}`，未做分类引导
- `session.error` 的 payload 在 opencode-plugin mapper 中直接以完整 JSON 字符串 (`payload.to_string()`) 作为错误消息传递，导致前端显示丑陋的 JSON 文本

## 根因
1. **opencode-plugin `mapper.rs`**：`session.error` 事件映射时，使用 `payload.to_string()` 将整个原始 JSON 作为错误消息，未提取 `message` 字段
2. **前端 `use-ag-ui-chat.ts`**：`RUN_ERROR` 处理器中直接拼接 `运行出错：${errorMsg}`，未对错误消息按类型进行分类

## 解决方案

### 1. opencode-plugin mapper.rs — 提取真实错误消息
新增 `extract_error_message()` 函数，从 `session.error` payload 中按优先级提取：
1. `error.message` — 嵌套的具体错误消息
2. `message` — 顶层错误消息
3. `error` 字符串值
4. 完整 JSON 原文（兜底）

### 2. 前端 use-ag-ui-chat.ts — 错误分类引导
新增 `classifyAgentError()` 函数，根据关键词匹配进行分类：
- **API Key 类**：`api.key|密钥|unauthorized|401` → 提示检查 API Key 与 Base URL
- **余额/配额类**：`quota|余额|insufficient|billing` → 提示充值
- **限流类**：`rate.limit|429|限流` → 提示稍后重试
- **超时类**：`timeout|超时|ETIMEDOUT` → 提示检查网络/服务
- **连接类**：`connect|网络|refused|unreachable` → 提示检查 Base URL
- **服务繁忙类**：`overloaded|busy|503|502` → 提示稍后重试
- **模型不可用类**：`model.*not.found|404` → 提示检查模型名称

匹配格式：`运行出错：{分类引导}\n\n原始错误：{原始错误信息}`
