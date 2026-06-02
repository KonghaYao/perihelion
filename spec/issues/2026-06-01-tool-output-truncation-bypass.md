---
id: 2026-06-01-tool-output-truncation-bypass
title: 工具输出截断机制被绕过（5月16日后仍有 17 条 >100KB 输出）
status: open
priority: high
created: 2026-06-01
---

## 问题

`output_persist.rs` 的截断机制在 2026-05-16 修复后，仍有 17 条超过 100KB 的工具输出（最新一条 2026-06-01）。

## 数据

| 工具 | 最大输出 | 日期 |
|------|---------|------|
| Bash | 203.3KB | 2026-05-25 |
| Grep | 155.6KB | 2026-05-27 |
| Bash | 114.4KB | 2026-05-27 |
| Glob | 109.4KB | 2026-05-29 |
| Bash | 109.6KB | 2026-06-01 |

## 排查方向

1. Bash: `truncate_output` 是否覆盖 stderr？某些命令大量输出到 stderr
2. Grep: `head_limit` 在 `output_mode=files_with_matches` 模式下是否生效？
3. JSON 序列化：content 字段含 JSON 转义后可能膨胀（`\n` → `\\n`）
4. `output_persist.rs` 的阈值是否被某些工具调用路径绕过

## 相关

- 已修复 issue: `spec/archive-issues/2026-05-15-tool-output-truncation-with-disk-persist.md`
- 分析报告: `side-projects/agent-defect-analyzer/docs/2026-06-01-defect-analysis.md` SIZE-001
