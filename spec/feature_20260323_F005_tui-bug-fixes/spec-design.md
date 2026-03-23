# Feature: 20260323_F005 - tui-bug-fixes

## 需求背景

TUI 当前存在三个影响用户体验的 bug：

1. **弹窗内容超长溢出**：AskUser 弹窗和其他面板（Model/Agents/Thread）当内容超出屏幕高度时，内容被截断不可见，无法滚动查看
2. **粘贴换行符触发提交**：用户粘贴含换行符的文本时，终端将每行换行视为独立 `Key::Enter` 事件，导致第一行被立即提交，后续行丢失
3. **Loading 状态输入框锁死**：Agent 运行期间输入框完全禁用（`build_textarea(true)` + `!app.loading` guard），用户无法提前编辑下一条消息

## 目标

- 修复弹窗内容超长时截断不可见的问题，增加滚动支持
- 修复粘贴多行文本触发错误提交的问题
- 支持 loading 状态下输入并缓冲消息，完成后自动合并发送

## 方案设计

### Bug 1：弹窗内容超长 — 滚动支持

**影响范围：** AskUser 弹窗、Model 面板、Agents 面板、Thread 浏览面板

**修复方案：**

1. **弹窗高度限制**：所有弹窗的 `popup_height` 增加上限 `min(计算高度, area.height * 4/5)`，确保弹窗不超过屏幕 80%
2. **内容滚动**：当渲染内容行数超过 `inner.height` 时，使用 `Paragraph::scroll((offset, 0))` 进行垂直偏移
3. **滚动状态**：在各弹窗/面板的状态结构中新增 `scroll_offset: u16` 字段
4. **光标跟随**：当用户通过 `↑↓` 移动光标时，自动调整 `scroll_offset` 使光标所在行保持在可视区域内（follow cursor）

**具体修改点：**

| 文件 | 修改内容 |
|------|---------|
| `ui/main_ui.rs::render_ask_user_popup` | popup_height 加上限；content_area 使用 Paragraph::scroll |
| `ui/main_ui.rs::render_model_panel` | 同上 |
| `ui/main_ui.rs::render_agent_panel` | 同上 |
| `ui/main_ui.rs::render_thread_browser` | 同上 |
| `app/mod.rs::AskUserBatchPrompt` | 新增 scroll_offset 字段 |
| `app/model_panel.rs::ModelPanel` | 新增 scroll_offset 字段 |
| `app/agent_panel.rs::AgentPanel` | 新增 scroll_offset 字段（如已有则复用） |

**光标跟随逻辑（伪代码）：**

```
fn ensure_cursor_visible(cursor_row, scroll_offset, visible_height) -> new_offset:
    if cursor_row < scroll_offset:
        return cursor_row
    if cursor_row >= scroll_offset + visible_height:
        return cursor_row - visible_height + 1
    return scroll_offset
```

### Bug 2：粘贴换行符触发 Enter — Bracketed Paste Mode

**根因分析：**

crossterm 默认模式下，粘贴的文本中每个换行符会被终端解释为独立的 `Key::Enter` 按键事件。当第一个 Enter 到达 `event.rs` 的 Enter 分支时，输入框的第一行被提交。

**修复方案：**

1. **启用 Bracketed Paste**：在终端初始化时调用 `crossterm::execute!(stdout, EnableBracketedPaste)`
2. **处理 Paste 事件**：在 `event.rs` 的 `match ev` 中新增 `Event::Paste(text)` 分支，将粘贴文本整体插入 textarea（保留换行）
3. **退出时清理**：在终端恢复时调用 `DisableBracketedPaste`

**具体修改点：**

| 文件 | 修改内容 |
|------|---------|
| `main.rs` | 终端初始化：`execute!(stdout, EnableBracketedPaste)`；退出时 `execute!(stdout, DisableBracketedPaste)` |
| `event.rs::next_event` | 新增 `Event::Paste(text)` 匹配分支 |

**Paste 事件处理逻辑：**

```rust
Event::Paste(text) => {
    // 粘贴文本直接插入 textarea，保留换行（不触发 Submit）
    if !app.loading {
        app.textarea.insert_str(&text);
    } else {
        // loading 状态下也允许输入（见 Bug 3）
        app.textarea.insert_str(&text);
    }
}
```

### Bug 3：Loading 状态输入缓冲 — Pending Messages

**修复方案：**

1. **输入框保持可编辑**：`set_loading(true)` 时不再调用 `build_textarea(true)` 完全禁用输入框，改为仅变更边框颜色标识 loading 状态，但允许输入
2. **消息缓冲区**：`App` 新增 `pending_messages: Vec<String>` 字段
3. **Loading 时 Enter 行为**：将文本加入 `pending_messages`，清空输入框，在输入框标题显示 `"已缓存 N 条"`
4. **完成后自动发送**：在 `AgentEvent::Done` / `AgentEvent::Error` 处理后，检查 `pending_messages`，若非空则合并（`\n\n` 分隔）并调用 `submit_message`

**具体修改点：**

| 文件 | 修改内容 |
|------|---------|
| `app/mod.rs::App` | 新增 `pending_messages: Vec<String>` |
| `app/mod.rs::set_loading` | loading=true 时不完全禁用 textarea，仅改样式 |
| `app/mod.rs::build_textarea` | 新增 `buffered_count` 参数控制标题显示 |
| `event.rs::next_event` | 去掉 Enter/Tab/Esc 等操作的 `!app.loading` guard；loading 时 Enter 将文本加入 pending_messages 而非 Submit |
| `app/mod.rs::handle_agent_event(Done/Error)` | 完成后检查 pending_messages 并合并发送 |

**缓冲流程：**

```
用户 Enter（loading 中）
  → text = textarea.lines().join("\n").trim()
  → pending_messages.push(text)
  → textarea = build_textarea_with_hint(pending_count)
  → 标题显示 "已缓存 N 条"

AgentEvent::Done
  → set_loading(false)
  → if !pending_messages.is_empty():
      combined = pending_messages.join("\n\n")
      pending_messages.clear()
      submit_message(combined)  // 立即触发新一轮 Agent 执行
```

## 实现要点

1. **Bracketed Paste 兼容性**：部分终端（如旧版 macOS Terminal.app）可能不支持 bracketed paste。crossterm 在不支持时会 silent fail，不影响正常功能
2. **滚动状态管理**：滚动 offset 在弹窗关闭时自动重置（弹窗状态 take 后销毁）
3. **缓冲合并策略**：多条 pending messages 用 `\n\n` 合并为一条，避免多轮 Agent 执行串联
4. **输入框样式区分**：loading 时边框颜色变黄、标题显示 "处理中…"，但光标和文字输入仍正常工作
5. **事件优先级**：loading 期间弹窗（HITL/AskUser）仍然优先拦截，不受输入缓冲影响

## 验收标准

- [ ] AskUser 弹窗内容超长时，弹窗高度不超过屏幕 80%，可通过 ↑↓ 滚动查看全部内容
- [ ] Model/Agents/Thread 面板内容超长时同样支持滚动
- [ ] 粘贴含换行符的文本时，文本整体插入输入框而非触发提交
- [ ] Loading 状态下输入框可编辑，按 Enter 将消息存入缓冲区
- [ ] 输入框标题在有缓冲消息时显示 "已缓存 N 条"
- [ ] Agent 完成后自动合并发送所有缓冲消息
- [ ] 无缓冲消息时，Agent 完成后输入框恢复正常（不自动发送空消息）
