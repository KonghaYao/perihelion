# Feature: 20260326_F008 - statusbar-msgcount-relay-flag

## 需求背景

当前 TUI 存在两个行为问题：

1. **Status bar 缺少消息计数**：状态栏只展示工作目录、Agent 状态、运行时长、模型信息，用户无法直观感知当前对话积累了多少条消息，不便于判断是否需要 `/clear` 或上下文是否即将达到限制。

2. **远程连接自动触发**：`try_connect_relay(None)` 在没有 `--remote-control` 参数时，仍会读取配置文件中的 relay URL 并自动建立连接。用户在只想本地使用 TUI 时，会意外建立远程连接，引发困惑或不必要的网络行为。

## 目标

- Status bar 左侧实时显示当前会话的 `view_messages` 条数，与消息窗口完全同步
- 远程连接仅当用户明确传入 `--remote-control` 参数时才触发；无参数则完全不尝试连接

## 方案设计

### 1. Status Bar 消息数显示

在 `rust-agent-tui/src/ui/main_ui/status_bar.rs` 的 `render_status_bar` 函数中，模型信息 Span 之后、Agent 面板信息之前，追加一组 Span：

```
│ 🗨 N 条
```

- **数据来源**：`app.view_messages.len()`，直接读取，无需新增状态字段
- **颜色**：分隔符 `│` 和标签文字使用 `Color::DarkGray`，数字使用 `Color::White`（或与分隔符同色保持简洁）
- **更新时机**：ratatui 每帧渲染时自动读取最新值，与消息窗口天然同步，无需额外事件通知

Status bar 布局示意：

![status bar 消息数布局](./images/01-wireframe.png)

### 2. 远程连接仅显式触发

修改 `rust-agent-tui/src/app/mod.rs` 中 `try_connect_relay` 函数的 `else` 分支：

**修改前：**
```rust
} else {
    // 无 CLI 参数：从配置读取（新字段优先，fallback 到 extra）
    let config = self.zen_config
        .as_ref()
        .and_then(|cfg| cfg.config.remote_control.as_ref())
        ...
}
```

**修改后：**
```rust
} else {
    // 无 --remote-control 参数：不尝试连接
    return;
}
```

三种调用场景的行为对比：

| 调用方式 | 修改前 | 修改后 |
|---------|--------|--------|
| 无 `--remote-control` 参数 | 从配置读取并连接 | 直接返回，不连接 |
| `--remote-control`（无 URL） | 从配置读取并连接 | 从配置读取并连接（不变） |
| `--remote-control <url>` | 使用指定 URL 连接 | 使用指定 URL 连接（不变） |

## 实现要点

- **消息数**：无需新增字段或事件，`app.view_messages.len()` 是已有公开字段，直接引用
- **relay 修改极小**：仅删除 `else` 分支中的读取配置逻辑，替换为 `return`，改动行数 < 30 行
- **无破坏性变更**：`--remote-control` 的已有行为（无参数和有参数两种形式）完全保持不变；只有"不传参数"这一场景的隐式行为被消除

## 约束一致性

- 符合「事件驱动 TUI 通信」约束：status bar 消息数从 `App` 状态直接读取，不引入新通道
- 符合「禁止下层依赖上层」架构：修改均在 `rust-agent-tui`（应用层）内
- 符合编码规范：无新的 `println!`，无跨 crate 公开接口变动

## 验收标准

- [ ] status bar 左侧在模型信息之后显示 `│ 🗨 N 条`，N 与消息窗口 `view_messages` 条数一致
- [ ] 每次发送消息或收到 AI 回复后，消息数立即在下一帧更新
- [ ] 不带 `--remote-control` 参数启动时，即使配置文件中有 relay URL，不发起连接、不打印连接尝试日志
- [ ] `--remote-control`（无参数）依然从配置读取 URL 并连接（原有行为不变）
- [ ] `--remote-control <url>` 依然正常连接指定 URL（原有行为不变）
