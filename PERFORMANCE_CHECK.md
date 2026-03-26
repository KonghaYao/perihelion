# 性能走查清单

- [ ] 检查输入卡顿问题
- [ ] 测试时间非常长

## 待修复问题

| 优先级 | 问题 | 位置 |
|--------|------|------|
| 高 | AppendChunk 每个 chunk 都全量重解析 markdown，O(n²) | `render_thread.rs` AppendChunk 分支 |
| 中 | poll 超时返回 `Some(Redraw)`，空闲时每 50ms 无条件重绘 | `event.rs:34`、`main.rs` 主循环 |
| 中 | SubAgent 每步全量 clone SubAgentGroup（含已渲染 spans）再重渲染 | `agent_ops.rs` SubAgent 分支 |
| 低 | `chars().count()` 全量扫描，改 `chars().nth(N).is_some()` | `message_render.rs:171,239` |
