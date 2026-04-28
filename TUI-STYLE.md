# TUI Style Guide

## 设计哲学

极简锋利，单色制胜。背景透明，信息层级只用亮度和 BOLD 区分，颜色只表达状态。

## 消息流

### 消息类型与视觉

| 类型 | 前缀 | 前景色 | 底色 | 说明 |
|------|------|--------|------|------|
| 用户消息 | `❯` | ACCENT (橙红) | `#4A4642` (暖灰) | 底色与 sticky header 一致，所有行带底色 |
| AI 回复 | `●` | White | — | 正文直接跟在 `● ` 后，支持 markdown 渲染 |
| 思考 (Reasoning) | — | — | — | 不在消息流中渲染，完全隐藏 |
| 工具调用 (非只读) | `●` | 白色工具名 + 状态色指示器 | — | bash/write/edit 等操作型工具 |
| 工具聚合组 (只读) | 无 | MUTED | — | read/glob/search 折叠为单行摘要 |
| SubAgent | `●` + emoji | SAGE | — | 折叠显示摘要，展开显示嵌套消息 |
| 系统消息 | `ℹ` | SAGE | — | 系统/提示信息 |

### 间距规则

- 每条有内容的消息后加 **1 个空行**，由 `render_one` 统一管理
- 空内容消息（如纯思考被隐藏的 AssistantBubble）不渲染、不占位
- 消息内部不插入多余空行

### 工具状态指示器

指示器 `●` 按状态变色，工具名称始终白色：

| 状态 | 指示器颜色 | 说明 |
|------|-----------|------|
| Running | White | 闪烁动画 |
| Completed | Green | 稳定显示 |
| Failed | ERROR (暗红) | 错误标记 |

## 只读工具聚合折叠

read_file、search_files_rg、glob_files 等只读工具自动聚合：

- **相邻的同类型工具**合并为一组（无其他消息穿插时）
- 折叠时显示单行摘要：`  Read 3 files (ctrl+o to expand)`
- 展开时列出每个工具参数：`  │ src/main.rs`
- `ctrl+o` 切换最近的 ToolCallGroup 展开/折叠

摘要格式：

| 工具 | 单数 | 复数 |
|------|------|------|
| read_file | Read 1 file | Read N files |
| search_files_rg | Searched for 1 pattern | Searched for N patterns |
| glob_files | Matched 1 pattern | Matched N patterns |

## 色板

### 强调色

| 名称 | 色值 | 用途 |
|------|------|------|
| ACCENT | `#FF6B2B` | 唯一主交互色：用户消息前缀、激活边框、光标、关键操作 |

### 功能色

| 名称 | 色值 | 用途 |
|------|------|------|
| SAGE | `#6EB56A` | 哑光绿：成功状态、SubAgent |
| WARNING | `#B09878` | 暖米灰：次要强调、标题 |
| ERROR | `#CC463E` | 暗红：错误/拒绝 |
| THINKING | `#A78BFA` | 亮紫罗兰：推理/CoT |
| LOADING | `#22D3EE` | 电光青：Loading spinner |
| White | 终端白色 | AI 回复前缀、工具名、进行中指示器 |
| Green | 终端绿色 | 完成状态指示器 |

### 文字层级（三级亮度）

| 层级 | 色值 | 用途 |
|------|------|------|
| TEXT | `#DACED0` | 主文字：需要立即看到的内容 |
| MUTED | `#8C7D78` | 次要文字：标签、路径、工具参数、聚合摘要 |
| DIM | `#483E3A` | 极弱文字：占位、已完成项、分隔符 |

### 底色

| 名称 | 色值 | 用途 |
|------|------|------|
| USER_BG | `#4A4642` | 用户消息底色（与 sticky header 一致） |
| POPUP_BG | `#0A0806` | 弹窗底色 |
| CURSOR_BG | `#261608` | 弹窗列表选中行底色 |

### 边框

| 名称 | 色值 | 用途 |
|------|------|------|
| BORDER | `#302620` | 空闲边框：极低对比 |
| BORDER_ACTIVE | ACCENT | 激活边框：输入框/panel focus |

## Spinner

位于消息区域底部（loading 状态时显示），通过 `SpinnerState` 管理：

| 模式 | 动词 | 触发时机 |
|------|------|---------|
| Thinking | 思考中… | Agent 开始处理 |
| ToolUse | {工具名} {参数摘要} | 收到 ToolCall 事件 |
| Responding | 正在生成回复… | 收到 AssistantChunk 事件 |
| Idle | (空) | 非加载状态 |

动画帧由 `perihelion-widgets::spinner::animation::tick_to_frame()` 提供，每渲染周期 `advance_tick()` 推进一帧。

## 弹窗面板

所有面板遵循统一按键约定：

| 按键 | 行为 |
|------|------|
| Up / Down | 竖向列表导航 |
| Left / Right | 横向切换（枚举字段） |
| Enter | 确认/进入/保存 |
| Space | 选中/切换 |
| Esc | 关闭/取消 |
| Ctrl+V | 粘贴剪贴板内容 |

## 命令

| 命令 | 说明 |
|------|------|
| `/login` | Provider 配置管理 |
| `/model` | 模型选择面板 |
| `/history` | 历史对话浏览 |
| `/agents` | SubAgent 管理 |
| `/compact` | 触发上下文压缩 |
| `/clear` | 清空消息列表 |
| `/help` | 列出所有命令 |

输入 `#` 前缀触发 Skills 浮层，`Tab` 导航，`Enter` 补全。
