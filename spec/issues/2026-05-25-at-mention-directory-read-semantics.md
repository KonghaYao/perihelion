# @ mention 目录引用被注入为 Read 工具调用

**状态**：Open
**优先级**：低
**创建日期**：2026-05-25

## 问题描述

当用户使用 `@` 引用一个目录时（如 `@langfuse-client/`），AtMentionMiddleware 将其作为 Read 工具调用注入到消息历史中。虽然 `file_reader.rs` 正确处理了目录（列出子项），但语义上不恰当——Read 工具的语义是读取文件内容，不是列出目录。

## 当前行为

```text
用户输入: "@langfuse-client/"
    ↓ AtMentionMiddleware
注入: Ai[ToolUse{Read, path: "langfuse-client/"}] → Tool[ToolResult: "file1\nfile2\n..."]
```

## 期望行为

选项（待决定）：

1. **目录不注入**：@ 目录时不做任何注入，仅保留原文本让 LLM 自行处理
2. **用不同方式注入**：注入为自定义消息（非 Read 工具），明确标注是目录列表
3. **替换为 Bash ls**：注入为 Bash `ls` 工具调用，语义更准确

## 复现条件

- **复现频率**：必现
- **触发步骤**：输入 `@` + 选择一个目录 + 提交

## 涉及文件

- `peri-middlewares/src/at_mention/file_reader.rs` — `read_file_content()` 目录处理逻辑
- `peri-middlewares/src/at_mention/mod.rs` — AtMentionMiddleware 注入逻辑
