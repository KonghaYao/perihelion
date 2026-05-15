# MCP 中间件与文件系统工具大文件拆分：client.rs 1309 行、grep.rs 1162 行

**状态**：Open
**优先级**：中
**创建日期**：2026-05-14

## 问题描述

`rust-agent-middlewares/src/mcp/client.rs`（1309 行）承载了 MCP 客户端池的全部实现（连接、重连、OAuth、shutdown）。`rust-agent-middlewares/src/tools/filesystem/grep.rs`（1162 行）混合了工具定义、参数解析、ripgrep 集成和结果格式化。两个文件职责过重，应按功能拆分。

## 现状数据

| 文件 | 行数 | 主要职责 |
|------|------|---------|
| `rust-agent-middlewares/src/mcp/client.rs` | 1309 | McpClientPool 全部实现 |
| `rust-agent-middlewares/src/tools/filesystem/grep.rs` | 1162 | GrepTool 全部实现 |

### `client.rs` 内部分布

| 职责 | 约行数 | 说明 |
|------|--------|------|
| 构造器 + 配置 | ~130 | `new()`, 配置加载 |
| 连接初始化 | ~230 | `run_initialize()` 逐服务器连接 |
| OAuth 流程 | ~110 | `start_oauth_flow()` |
| 重连逻辑 | ~180 | `reconnect()` |
| 结果格式化 | ~26 | pub 声明数（26 个） |
| 其他 | ~633 | shutdown、事件处理、状态管理、测试 |

### `grep.rs` 内部分布

| 职责 | 约行数 | 说明 |
|------|--------|------|
| ParsedArgs 参数解析 | ~200 | 复杂的命令行参数解析 |
| invoke 实现 | ~500 | ripgrep 集成 + 结果格式化 + multiline 搜索 |
| 测试 | ~489 | 内联测试（应分离） |

### 其他需要关注的文件

| 文件 | 行数 | 说明 |
|------|------|------|
| `rust-agent-middlewares/src/subagent/tool.rs` | 980 | SubAgentTool + 4 种执行路径 + 中间件链构建 |
| `rust-agent-middlewares/src/middleware/web.rs` | 773 | WebFetchTool + WebSearchTool + SSRF 防护混合 |
| `rust-agent-middlewares/src/hooks/types.rs` | 854 | HookEvent + 7 个 HookInput 工厂方法 |

## 期望改进方向

### client.rs 拆分

```
mcp/
├── client.rs        # McpClientPool struct + new() + 核心方法
├── initialize.rs    # run_initialize() 连接初始化
├── oauth.rs         # start_oauth_flow() + OAuth 流程
└── reconnect.rs     # reconnect() 重连逻辑
```

### grep.rs 拆分

```
tools/filesystem/
├── grep.rs          # GrepTool struct + invoke 骨架
├── grep_args.rs     # ParsedArgs 参数解析
└── grep_format.rs   # 结果格式化逻辑
```

### web.rs 拆分

```
middleware/
├── web_common.rs    # SSRF 防护 + 共享逻辑
├── web_fetch.rs     # WebFetchTool
└── web_search.rs    # WebSearchTool + Bing 结果解析
```

## 涉及文件

- `rust-agent-middlewares/src/mcp/client.rs`（1309 行）
- `rust-agent-middlewares/src/tools/filesystem/grep.rs`（1162 行）
- `rust-agent-middlewares/src/subagent/tool.rs`（980 行）
- `rust-agent-middlewares/src/middleware/web.rs`（773 行）
- `rust-agent-middlewares/src/hooks/types.rs`（854 行）
