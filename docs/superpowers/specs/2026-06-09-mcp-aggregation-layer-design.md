# MCP 聚合层设计文档

> **日期**: 2026-06-09
> **主题**: gatewayd MCP Aggregation Layer + CLI `dh mcp` 命令
> **状态**: 已批准

---

## 1. 需求概述

实现 TODO.md Phase 4 MCP 聚合层，包含：
- MCP Server 配置管理（SQLite 表 + CLI CRUD）
- MCP Client 池（启动时初始化 + 运行时管理）
- 工具列表聚合（namespace prefix，如 `filesystem:read_file`）
- Tool Call 拦截器（检测远程请求 URL，记录 audit_logs，不阻塞）
- Admin API（`GET /admin/mcp/tools`, `POST /admin/mcp/tools/:name/call`）
- CLI 命令（`dh mcp list/add/remove/call`）

### 1.1 非功能约束

- 单文件 ≤ 600 行有效代码（AGENTS.md 硬性规则）
- gatewayd main.rs 已有 713 行，**所有新增代码必须放在新文件中**
- rustc 1.95 ICE 限制：不能使用目录模块，只能用扁平文件
- `cargo check --workspace` 0 warnings

---

## 2. 架构设计

### 2.1 模块划分

```
apps/gatewayd/src/
├── main.rs              (已有，713 行 — 不改或最小改动)
├── mcp_aggregator.rs    (NEW: Registry + Interceptor + Admin handlers)
apps/cli/src/commands/
├── mcp.rs               (NEW: dh mcp list/add/remove/call)
crates/dh-core/src/mcp/
├── client.rs            (扩展: list_tools())
├── transport.rs         (扩展: is_alive())
crates/dh-db/src/schema.rs  (扩展: mcp_servers 表定义)
```

### 2.2 数据流

```
用户终端
  │ dh mcp add filesystem --cmd npx --args "-y,@mcp/server-filesystem,/home"
  ▼
CLI 写入 SQLite mcp_servers 表
  │
  ▼
gatewayd Admin API
  GET /admin/mcp/tools          → Registry 聚合所有 client 的 tools
  POST /admin/mcp/tools/:name/call → Interceptor 检测 → 转发到 client
  GET /admin/mcp/servers        → 返回 server 状态列表
```

---

## 3. 数据库设计

### 3.1 mcp_servers 表

```sql
CREATE TABLE IF NOT EXISTS mcp_servers (
    name TEXT PRIMARY KEY,
    command TEXT NOT NULL,
    args TEXT NOT NULL DEFAULT '[]',        -- JSON array
    env TEXT NOT NULL DEFAULT '{}',         -- JSON object
    enabled INTEGER NOT NULL DEFAULT 1,     -- 0 or 1
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
```

### 3.2 configs 表元配置

```
key = "mcp_aggregator_enabled", value = "true"   -- 聚合层总开关
```

---

## 4. MCP Client 扩展（dh-core）

### 4.1 McpClient::list_tools()

发送 `tools/list` JSON-RPC 请求，返回 `Vec<Tool>`。

```rust
pub async fn list_tools(&self) -> Result<Vec<Tool>, McpError> {
    let request = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(json!(self.next_id())),
        method: "tools/list".to_string(),
        params: json!({}),
    };
    // send and await response
}
```

### 4.2 StdioTransport::is_alive()

检查子进程是否仍在运行：

```rust
pub fn is_alive(&mut self) -> bool {
    match self._child.try_wait() {
        Ok(None) => true,      // still running
        Ok(Some(_)) => false,  // exited
        Err(_) => false,
    }
}
```

---

## 5. MCP 聚合层（gatewayd）

### 5.1 McpRegistry

```rust
pub struct McpRegistry {
    clients: HashMap<String, McpClientEntry>,
    db_path: PathBuf,
}

pub struct McpClientEntry {
    pub name: String,
    pub client: Arc<McpClient>,
    pub config: McpServerConfig,
}

pub struct McpServerConfig {
    pub command: String,
    pub args: Vec<String>,
    pub env: HashMap<String, String>,
    pub enabled: bool,
}
```

**生命周期：**
1. `Registry::load_from_db(db_path)` → 读取 mcp_servers 表，为 enabled=true 的条目 spawn client
2. `Registry::initialize_all()` → 对每个 client 调用 `initialize()`
3. `Registry::aggregate_tools()` → 调用每个 client 的 `list_tools()`，添加 namespace prefix
4. `Registry::call_tool(full_name, args)` → 解析 `namespace:tool_name`，转发到对应 client

### 5.2 McpInterceptor

```rust
pub struct McpInterceptor;

impl McpInterceptor {
    pub fn inspect(args: &serde_json::Value) -> Option<RemoteRequestDetected> {
        // 递归扫描 JSON value，检测字符串是否匹配 URL 模式
        // 模式: "http://*" | "https://*" | "ftp://*"
    }
}
```

**行为：** 检测到远程请求时，构造 `AuditLogEntry`（direction=ToolCall，metadata 包含检测到的 URLs），异步写入 audit_logs。然后**继续放行**调用。

### 5.3 Admin API 路由

在现有 `admin_router` 上追加：

```rust
.route("/mcp/servers", get(list_mcp_servers))
.route("/mcp/tools", get(list_mcp_tools))
.route("/mcp/tools/:name/call", post(call_mcp_tool))
```

**Handler 签名：**

```rust
async fn list_mcp_servers(State(state): State<ApiState>) -> Result<Json<Value>, StatusCode>
async fn list_mcp_tools(State(state): State<ApiState>) -> Result<Json<Value>, StatusCode>
async fn call_mcp_tool(
    State(state): State<ApiState>,
    Path(name): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, StatusCode>
```

### 5.4 与 gatewayd 启动流程集成

在 `main()` 中，DB 初始化完成后：

```rust
let mcp_registry = McpRegistry::load_from_db(&db_path).await?;
mcp_registry.initialize_all().await;
```

`ApiState` 新增 `mcp_registry: Arc<tokio::sync::Mutex<McpRegistry>>`。

---

## 6. CLI 命令设计

### 6.1 命令结构

```bash
dh mcp list                          # 列出所有 MCP servers 及存活状态
dh mcp add <name> --cmd <command> [--args <args>] [--env KEY=VAL]
dh mcp remove <name>                 # 删除并停止
dh mcp call <tool-name> --args <json># 手动调用 tool（测试用）
```

### 6.2 实现方式

CLI **直接读写 SQLite**（和 `dh config` 命令一样），不通过 HTTP。因为：
- `add/remove` 需要持久化配置到 DB
- `list` 可以查询 DB 的 server 配置 + 通过 Admin API 获取实时 tools 列表
- `call` 通过 Admin API `POST /admin/mcp/tools/:name/call`

**例外：** 如果 gatewayd 未运行，`list` 只显示 DB 中的配置（无实时状态），`call` 报错。

---

## 7. 错误处理

| 场景 | 行为 |
|------|------|
| MCP server 启动失败 | 记录 error 日志，client 不加入 registry，不影响其他 servers |
| list_tools 失败 | 该 server 的工具不加入聚合列表，返回部分结果 |
| call_tool 时 client 已死 | 返回 503，提示 server 不可用 |
| Interceptor 检测失败 | 不阻塞调用，只记录日志失败（不 panic） |
| gatewayd 启动时 mcp_aggregator_enabled=false | 跳过 Registry 初始化，Admin API 返回 503 |

---

## 8. 测试策略

- **单元测试**：`McpInterceptor::inspect()` 的 URL 检测逻辑（多种 JSON 嵌套结构）
- **集成测试**：`dh mcp add` → `dh mcp list` → `dh mcp call` → `dh mcp remove` 完整流程
- **手动测试**：使用真实 MCP server（如 `@modelcontextprotocol/server-filesystem`）验证端到端

---

## 9. 实现顺序

1. **dh-core 扩展**：`list_tools()` + `is_alive()`
2. **dh-db schema**：`mcp_servers` 表 + 迁移
3. **gatewayd 聚合层**：`mcp_aggregator.rs`（Registry + Interceptor + Admin handlers）
4. **gatewayd 集成**：main.rs 启动时加载 Registry，注册 Admin 路由
5. **CLI mcp 命令**：`apps/cli/src/commands/mcp.rs`
6. **编译验证**：`cargo check --workspace` 0 warnings
