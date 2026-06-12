# DeepHarness LLM Gateway Daemon — 待办事项

## 已完成 ✅

### Phase 1: Monorepo + Gatewayd MVP
- [x] Rust workspace 结构（dh-core, dh-platform, dh-db crates）
- [x] gatewayd: OpenAI/Anthropic 兼容 HTTP API
- [x] gatewayd: Admin API + Health Check
- [x] gatewayd: 异步审计日志（SQLite）
- [x] CLI: `dh exec <agent>` 包装器
- [x] CLI: `dh gatewayd start/stop/status/logs`
- [x] 跨平台数据目录和 lock file 管理

### Phase 2: RTK Token Killer
- [x] RtkEngine 可配置优化引擎
- [x] SlidingWindowSummarizer — 大上下文自动摘要
- [x] RedundancyEliminator — 去重 system prompt
- [x] PromptCompressor — 空白和换行压缩
- [x] 请求处理管道集成（OpenAI/Anthropic）
- [x] 优化指标日志

## 进行中 🚧

*暂无*

## 待办 📋

### Phase 3: 策略引擎 + 审批协调器
- [ ] 策略规则配置（YAML/SQLite）
- [ ] 规则匹配器（model, time, content pattern）
- [ ] 动作执行器（pass / block / transform / require_approval）
- [ ] 审批协调器（阻塞等待队列 + oneshot channel）
- [ ] 系统通知（macOS/Windows/Linux）
- [ ] 终端内联审批 UI
- [ ] IPC 推送通知（Desktop/托盘程序）
- [ ] 审批超时回退策略

### Phase 4: MCP 聚合层
- [x] MCP Server 配置管理（SQLite `mcp_servers` 表 + CLI `dh mcp add/remove`）
- [x] MCP Client 池（启动时初始化，运行时 `is_alive` 检测）
- [x] 工具列表聚合（`namespace:tool_name` 前缀，如 `filesystem:read_file`）
- [x] Tool Call 拦截器（检测 URL，记录 audit_logs，不阻塞）
- [x] Admin API（`/mcp/servers`, `/mcp/tools`, `/mcp/tools/{name}/call`）
- [x] CLI 命令（`dh mcp list/add/remove/call`）

### Phase 5: Config Hub + 云端同步
- [ ] 配置层级（session > agent > project > global）
- [ ] 云端管理控制台轮询（HTTP polling）
- [ ] 配置变更检测与热重载
- [ ] 规范注入（agents.md, design.md, claude.md）
- [ ] 文件监听（notify crate）
- [ ] Skill 注册表与下发
- [ ] MCP Server 配置下发

### Phase 6: HTTP 代理层（CLI Skill 拦截）
- [ ] HTTP 代理服务器（127.0.0.1:2347）
- [ ] 环境变量注入（HTTP_PROXY）
- [ ] 出站请求拦截与策略检查
- [ ] LLM API 流量放行

### Phase 7: 异步上报
- [ ] 上报队列（内存 + SQLite 缓冲）
- [ ] 批量上报（batch + flush interval）
- [ ] 网络失败重传机制
- [ ] 数据脱敏选项

### Phase 8: 系统托盘程序
- [ ] 轻量级 Tauri 托盘应用
- [ ] IPC 连接 gatewayd
- [ ] 审批弹窗/面板
- [ ] 系统通知接收

### Phase 9: Desktop 集成
- [ ] Desktop 检测并复用已有 daemon
- [ ] Desktop 自动启动 daemon（如果不存在）
- [ ] Desktop 与 gatewayd 的 IPC 通信
- [ ] 迁移 Desktop 到 apps/desktop/

### Phase 10: 企业级功能
- [ ] 多租户支持
- [ ] 用户认证（API Key / JWT）
- [ ] 审计日志不可篡改（哈希链）
- [ ] 配额管理（token 限制）
- [ ] 模型自动降级策略

## 已知问题 🔧

- rustc 1.95 存在目录模块 ICE bug（`check_mod_deathness` 阶段崩溃），gatewayd 当前使用内联模块规避。后续可尝试：
  - 升级 rustc 到修复版本
  - 将代码拆分为扁平文件结构（非目录模块）
