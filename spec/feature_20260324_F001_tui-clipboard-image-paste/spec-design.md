# Feature: 20260324_F001 - tui-clipboard-image-paste

## 需求背景

TUI 输入框目前只能发送纯文本消息。用户在进行多模态交互（如询问 AI 分析截图、代码截图等）时，需要手动将图片转换为文件路径再通过 Agent 工具读取，流程繁琐。

本功能允许用户直接使用 `Ctrl+V` 从系统剪贴板粘贴图片，图片将以 base64 形式附加在下一条 Human 消息中，发送给 LLM 进行多模态分析。

## 目标

- 用户可以通过 `Ctrl+V` 将剪贴板中的图片附加到待发送消息
- 输入框上方显示独立附件栏，展示已附加的图片及操作提示
- `Del` 键删除最后一张附加图片
- 提交时，将文字 + 图片 blocks 组合为 `BaseMessage::Human { content: Blocks([...]) }` 发送给 LLM

## 方案设计

### 技术选型

| 功能 | 选用方案 | 理由 |
|------|---------|------|
| 剪贴板访问 | `arboard` crate | 跨平台（Mac/Linux/Windows），支持读取图片 RGBA 像素数据 |
| 图片编码 | `png` crate | 将 RGBA bytes 编码为 PNG 二进制 |
| base64 编码 | `base64` crate | 将 PNG 二进制转为 base64 字符串 |

`arboard` 的 `get_image()` 返回 `ImageData { width, height, bytes: Vec<u8> }`（RGBA 格式）。
`png` crate 将其编码为标准 PNG，再用 `base64` 编码为字符串，存入 `ContentBlock::Image { source: ImageSource::Base64 { media_type: "image/png", data } }`。

### 数据结构

```rust
// rust-agent-tui/src/app/mod.rs 新增

/// 待发送的图片附件
pub struct PendingAttachment {
    /// 显示名称，如 "clipboard_1.png"
    pub label: String,
    /// MIME 类型，固定为 "image/png"
    pub media_type: String,
    /// base64 编码的 PNG 数据
    pub base64_data: String,
    /// 文件大小（字节，用于显示）
    pub size_bytes: usize,
}

// App 结构体新增字段
pub pending_attachments: Vec<PendingAttachment>,
```

### 事件处理（Ctrl+V 拦截）

在 `event.rs` 的 `Event::Key` 分支中，在通用 textarea 输入之前拦截 `Ctrl+V`：

```
Ctrl+V 触发
├─ arboard::Clipboard::new().get_image() 成功
│   ├─ RGBA bytes → PNG 编码（png crate）
│   ├─ PNG 二进制 → base64 字符串
│   ├─ 生成 label = "clipboard_{n}.png"（n 从 1 递增）
│   ├─ app.pending_attachments.push(PendingAttachment { ... })
│   └─ 不插入 textarea，返回 Action::Redraw
│
└─ get_image() 失败（剪贴板无图片）
    └─ 走原有文字粘贴路径：尝试 get_text() 插入 textarea
       （bracketed paste 的 Event::Paste 仍然独立处理文字，保持兼容）
```

`Del` 键（在非 loading 状态且 `pending_attachments` 非空时）删除最后一个附件。

### UI 渲染 —— 附件栏

在 `main_ui.rs` 中，当 `pending_attachments` 非空时，在输入框上方渲染固定高度（3行）的附件栏：

![附件栏与输入框布局](./images/01-wireframe.png)

```
┌─ 待发送附件 ──────────────────────────────────────────────┐
│  [🖼 clipboard_1.png 24KB]  [🖼 clipboard_2.png 16KB]      │
│  Del: 删除最后一张                                          │
└────────────────────────────────────────────────────────────┘
┌─ 输入 ─────────────────────────────────────────────────────┐
│  请描述图片中的内容 |                                        │
└────────────────────────────────────────────────────────────┘
```

- 附件栏仅在 `pending_attachments.len() > 0` 时渲染，高度固定 3 行
- 无附件时布局与现有完全相同，不占空间
- 附件以 `[🖼 name size]` 格式在同一行展示（emoji 可根据终端兼容性降级为 `[img]`）

### 提交流程

`submit_message` 扩展为同时消费 `pending_attachments`：

```rust
pub fn submit_message(&mut self, input: String) {
    let attachments = std::mem::take(&mut self.pending_attachments);

    let agent_input = if attachments.is_empty() {
        // 原有路径
        AgentInput::text(input.clone())
    } else {
        // 多模态路径：文字 + 图片 blocks
        let mut blocks = vec![ContentBlock::text(input.clone())];
        for att in &attachments {
            blocks.push(ContentBlock::image_base64(&att.media_type, &att.base64_data));
        }
        AgentInput::blocks(MessageContent::blocks(blocks.clone()))
    };

    // 持久化：同样用多模态 BaseMessage
    let user_msg = if attachments.is_empty() {
        BaseMessage::human(input.clone())
    } else {
        let mut blocks = vec![ContentBlock::text(input.clone())];
        for att in &attachments {
            blocks.push(ContentBlock::image_base64(&att.media_type, &att.base64_data));
        }
        BaseMessage::human(MessageContent::blocks(blocks))
    };

    // ... 原有 agent 启动逻辑，将 agent_input 传给 run_universal_agent
}
```

`run_universal_agent` 签名从 `input: String` 改为 `input: AgentInput`，移除函数内部的 `AgentInput::text(input)` 构建，直接使用传入值。

### 数据流

```
用户 Ctrl+V（剪贴板有图片）
  └─ arboard 读取 RGBA 像素
  └─ png 编码 → Vec<u8>
  └─ base64 编码 → String
  └─ App.pending_attachments.push(...)
  └─ UI 重绘：附件栏出现

用户输入文字 + 回车
  └─ submit_message(text, attachments)
  └─ 构建 AgentInput::blocks([Text, Image, Image, ...])
  └─ 构建 BaseMessage::human(Blocks([...]))（持久化）
  └─ run_universal_agent(provider, agent_input, ...)
  └─ ReActAgent executor → LLM 多模态请求
  └─ pending_attachments 清空
```

## 实现要点

1. **依赖新增**（`rust-agent-tui/Cargo.toml`）：
   - `arboard = "3"`（Linux 额外需要 `x11-clipboard` feature 或 `wayland-clipboard`）
   - `png = "0.17"`
   - `base64 = "0.22"`

2. **arboard 线程限制**：`arboard::Clipboard` 不是 `Send`，必须在同步上下文中使用。在 `event.rs`（主线程同步事件循环）中直接调用，无需 `spawn_blocking`。

3. **PNG 编码**：`png::Encoder` 需要宽高和颜色类型（`ColorType::Rgba`），输出到 `Vec<u8>` cursor。

4. **run_universal_agent 签名变更**：`input: String` → `input: AgentInput`，同时 `BaseMessage::human(input.clone())` 的持久化逻辑需移到调用方（`submit_message`），因为 `run_universal_agent` 不再拥有原始文字。

5. **MessageViewModel 用户气泡**：当前 `user(input: String)` 只显示文字。有附件时，气泡需附加 `[🖼 N 张图片]` 摘要（不展示图片内容，避免 TUI 宽度溢出）。

## 约束一致性

- 本方案仅修改 `rust-agent-tui`（TUI 层）和少量 `run_universal_agent` 签名，不修改 `rust-create-agent` 核心逻辑
- `AgentInput::blocks()` 和 `ContentBlock::image_base64()` 均已在框架中存在，无需扩展核心 API
- Thread 持久化（`BaseMessage` → SQLite）本身已支持 `MessageContent::Blocks`，无需修改 `ThreadStore`

## 验收标准

- [ ] 剪贴板有图片时，`Ctrl+V` 不插入文字，而是在附件栏中显示 `[🖼 clipboard_1.png ...]`
- [ ] 剪贴板无图片时，`Ctrl+V` 走原有文字粘贴逻辑，行为不变
- [ ] `Del` 键在有附件时删除最后一张图片
- [ ] 无附件时 UI 布局与现有完全一致
- [ ] 发送含图片的消息后，LLM 能接收到多模态内容（可通过 Anthropic Claude 的 vision 验证）
- [ ] `pending_attachments` 在消息发送后自动清空
- [ ] 图片数据以 base64 PNG 格式正确编码（可通过解码验证像素数据）
