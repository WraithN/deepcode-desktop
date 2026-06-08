# SSE 流式与模拟流式重复发送 token 导致消息内容翻倍

## 现象

发送消息后，AI 回复的内容显示为用户输入的重复文本。例如用户发送 "create a python file for quick sort"，AI 回复区域显示 "create a python file for quick sortcreate a python file for quick sort"。同时出现多个重复的"思考中"步骤。

## 根因（第一轮分析不完整）

最初认为是 SSE 流式和模拟流式重复发送 token 导致。但根本原因是 opencode SSE 事件推送了**用户消息**作为 `message.part.updated`，而代码未区分消息角色（user vs assistant），将用户输入也当作 AI token 发送给前端。

具体原因：
1. opencode SSE 中 `message.part.updated` 事件不仅推送 AI 回复，还推送用户输入（echo）
2. `stream_opencode_output` 未检查 `part.messageID` 对应的消息 role
3. 同时 `message.part.delta` 增量事件未被处理，导致只能依赖 `message.part.updated` 获取文本

## 解决方案

1. **跟踪 assistant 消息 ID**：处理 `message.updated` 事件，当 `info.role == "assistant"` 时记录 `info.id`
2. **过滤用户消息 part**：在 `message.part.updated` 中检查 `messageID` 是否在已记录的 assistant IDs 中，不在则跳过
3. **处理增量事件**：新增 `message.part.delta` 事件处理，直接提取 `delta` 字段作为 token 推送，实现真正的流式效果

修改点：
- 添加 `assistant_message_ids: HashSet<String>` 跟踪 assistant 消息
- 新增 `message.updated` 分支处理
- 新增 `message.part.delta` 分支处理
- `message.part.updated` 中添加 `messageID` 角色检查

## 验证

- `cargo check --bin dh` 编译通过 0 错误
- `pnpm tauri build` release 编译成功
- 需用户在真实环境中测试验证
