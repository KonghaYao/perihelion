# LLM 适配器模块化：anthropic.rs 1983 行、openai.rs 1065 行

**状态**：Open
**优先级**：中
**创建日期**：2026-05-14

## 问题描述

`rust-create-agent/src/llm/anthropic.rs`（1983 行）和 `openai.rs`（1065 行）各自承载了完整的 LLM 适配器实现：构造器、序列化/反序列化、缓存策略、API invoke、流式处理、响应解析。两个文件结构高度对称，但职责过重，修改任一环节需要阅读整个文件。

## 现状数据

| 文件 | 行数 | 主要职责 |
|------|------|---------|
| `rust-create-agent/src/llm/anthropic.rs` | 1983 | ChatAnthropic 全部实现 |
| `rust-create-agent/src/llm/openai.rs` | 1065 | ChatOpenAI 全部实现 |

### `anthropic.rs` 内部分布

| 职责 | 约行数 | 说明 |
|------|--------|------|
| 构造器 + 配置 | ~200 | `new()`, `with_*()` 链式构建器 |
| 序列化 | ~100 | 请求/响应类型的 serde |
| 缓存策略 | ~120 | `split_system_blocks`, `apply_cache_to_messages`, `ensure_thinking_blocks` |
| 消息转换 | ~120 | `messages_to_anthropic()` + system prompt 边界处理 |
| API invoke | ~300 | `invoke()` 请求构建 + HTTP + 响应解析 + 错误处理 |
| 流式处理 | ~200 | SSE 解析 + 流式事件生成 |
| 测试 | ~729 | 内联测试（应分离，见测试分离 issue） |

### `openai.rs` 内部分布

| 职责 | 约行数 | 说明 |
|------|--------|------|
| 构造器 + 配置 | ~100 | `new()`, `with_*()` |
| 序列化/反序列化 | ~100 | 消息不变量校验 |
| 消息转换 | ~84 | `messages_to_json()` |
| API invoke | ~250 | `invoke()` 请求构建 + HTTP + 响应解析 |
| 流式处理 | ~200 | SSE 解析 + 流式事件 |
| 响应解析 | ~110 | `parse_assistant_message()` |
| 测试 | ~0 | 已分离到 `openai_test.rs` |

### 两个文件共享的问题模式

- `invoke()` 函数过长（anthropic ~300 行，openai ~250 行）
- 缓存/序列化/invoke/流式职责混杂在同一文件
- `messages_to_*()` 消息转换逻辑内嵌在适配器中

## 期望改进方向

为两个适配器建立统一的子模块结构：

```
llm/
├── anthropic/
│   ├── mod.rs          # ChatAnthropic struct + new()
│   ├── cache.rs        # split_system_blocks, apply_cache_to_messages
│   ├── invoke.rs       # invoke(), build_request_body(), parse_response()
│   └── stream.rs       # 流式处理
├── openai/
│   ├── mod.rs          # ChatOpenAI struct + new()
│   ├── invoke.rs       # invoke(), messages_to_json(), parse_assistant_message()
│   └── stream.rs       # 流式处理 + reasoning 处理
├── anthropic.rs        # re-export（向后兼容）
└── openai.rs           # re-export（向后兼容）
```

保留原文件路径的 re-export 以避免破坏外部引用。

## 涉及文件

- `rust-create-agent/src/llm/anthropic.rs`（1983 行）
- `rust-create-agent/src/llm/openai.rs`（1065 行）
- `rust-create-agent/src/llm/mod.rs`（模块入口）
