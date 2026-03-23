# Feature: 20260323_F001 - TUI 渲染性能优化（双线程架构）

## 需求背景

当前 TUI 在消息较多（10-30 条）时出现明显卡顿，表现为：
- 滚动浏览历史消息时卡顿
- Agent 流式输出时 UI 响应变慢
- 打字输入时延迟

根本原因分析：
1. **每帧全量渲染**：`render_messages()` 遍历全部 `view_messages`，将所有 Line 收集到一个 Vec，无虚拟滚动
2. **无条件重绘**：主循环每帧都调用 `terminal.draw()`，即使没有任何变化
3. **Markdown 解析和行包装在 UI 线程**：虽然有 dirty 缓存，但 Line 收集仍是全量 O(n)

虽然 Agent 执行已通过 `tokio::spawn` 在独立线程运行，但渲染计算本身仍在 UI 主线程，导致消息增多后帧率下降。

## 目标

- 将渲染计算（Markdown 解析、行包装、Line 生成）移到独立线程
- UI 线程只负责从缓存读取可见区域的 lines 并输出
- 实现按需重绘，无变化时不调用 `terminal.draw()`
- 10-30 条消息时滚动和输入流畅无卡顿

## 方案设计

### 整体架构

![双线程渲染架构](./images/01-architecture.png)

引入独立的**渲染线程（Render Task）**，与 UI 主线程通过共享缓存和事件 channel 协作：

```
                    RenderEvent channel (mpsc)
App (UI Thread) ─────────────────────────────────→ Render Task
      │                                                │
      │  Arc<RwLock<RenderCache>>                      │
      │←──────────────────────────────────────────────→│
      │                                                │
      │  Arc<Notify> (render_notify)                   │
      │←───────────────────────────────────────────────│
      ▼
terminal.draw() ← 仅在 version 变化时调用
```

### 数据结构

```rust
/// 渲染缓存，由渲染线程写入、UI 线程读取
struct RenderCache {
    /// 所有消息渲染后的行
    lines: Vec<Line<'static>>,
    /// 每条消息在 lines 中的起始行索引（用于定位）
    message_offsets: Vec<usize>,
    /// 总行数（用于滚动范围计算）
    total_lines: usize,
    /// 版本号，UI 线程比较是否有变化以决定是否重绘
    version: u64,
}

/// 渲染线程接收的事件
enum RenderEvent {
    /// 新增一条完整消息（用户消息/工具结果等）
    AddMessage(MessageViewModel),
    /// 追加流式 chunk 到最后一条 assistant 消息
    AppendChunk(String),
    /// 终端宽度变化，需要全量重新计算行包装
    Resize(u16),
    /// 清空所有消息
    Clear,
}
```

### 渲染线程工作流

渲染线程作为独立 Tokio task 运行，持有消息数据的私有副本：

1. 等待 `RenderEvent` 到达
2. 根据事件类型处理：
   - **AddMessage**：渲染新消息生成 lines，追加到缓存
   - **AppendChunk**：只重新渲染最后一条 assistant 消息，替换对应区间的 lines
   - **Resize**：全量重新渲染所有消息
   - **Clear**：清空所有数据
3. 写入 `RenderCache`（短暂加写锁），递增 version
4. 通过 `Notify` 通知 UI 线程

### UI 线程工作流

```
loop {
    // 1. 检查 RenderCache.version 是否变化
    //    或有键盘/鼠标事件
    // 2. 若无变化且无事件 → 跳过 draw
    // 3. 若有变化：
    //    a. 读取 RenderCache（读锁）
    //    b. 根据 scroll_offset 取 lines[offset..offset+viewport] 切片
    //    c. terminal.draw() 只绘制这些行
    // 4. 处理键盘事件（滚动/输入/命令）
}
```

关键优化：
- `terminal.draw()` 只在有变化时调用（version 变化或用户交互）
- 从 RenderCache 读取时只取可见区域的行切片，不复制全部 lines

### 流式输出的增量渲染

`AppendChunk` 是高频事件（每个 token 一次），需要高效处理：

1. 渲染线程持有最后一条 assistant 消息的 raw text
2. 收到 chunk 后追加到 raw text，重新 parse markdown
3. 计算新 lines，替换 `RenderCache.lines` 中对应区间
4. 更新 `message_offsets` 中后续消息偏移（通常 chunk 是最后一条，无后续）
5. 递增 version，通知 UI

### 滚动机制

- `scroll_follow = true` 时：offset = max(0, total_lines - viewport_height)
- `scroll_follow = false` 时：用户手动控制 offset，不受新消息影响
- UI 线程直接操作 scroll_offset，不需要通知渲染线程

### 兼容性

以下组件不受影响，仍由 UI 线程直接渲染：
- HITL 审批弹窗
- AskUser 弹窗
- Todo 面板
- 历史面板
- Model 配置面板
- 输入框

## 实现要点

1. **新增模块**：在 `rust-agent-tui/src/ui/` 下新增 `render_thread.rs`，包含 `RenderCache`、`RenderEvent` 和渲染线程逻辑
2. **App 改造**：`App::new()` 时启动渲染线程，持有 `render_tx: mpsc::Sender<RenderEvent>` 和 `render_cache: Arc<RwLock<RenderCache>>`
3. **消息写入路径改造**：`poll_agent()` 中收到事件后不再直接操作 `view_messages`，改为发送 `RenderEvent`
4. **main_ui.rs 改造**：`render_messages()` 从 RenderCache 读取可见行，不再遍历 view_messages
5. **按需重绘**：主循环增加 `last_version` 跟踪，version 未变且无用户事件时跳过 draw
6. **生命周期**：`render_tx` drop 时渲染线程自动退出（channel 关闭）
7. **依赖**：使用已有的 `parking_lot` crate 的 `RwLock`，`tokio::sync::Notify` 通知

## 验收标准

- [ ] 渲染线程独立运行，Markdown 解析和行包装不在 UI 线程
- [ ] 100 条消息时滚动流畅，输入无延迟
- [ ] 流式输出时 UI 正常跟踪显示
- [ ] HITL/AskUser 弹窗功能正常
- [ ] 清空对话、加载历史对话正常工作
- [ ] 终端 resize 后渲染正确
- [ ] 无死锁、无 panic
