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
- 折叠时显示单行摘要：`  Read 3 files`
- 展开时列出每个工具参数：`  │ src/main.rs`

摘要格式：

| 工具 | 单数 | 复数 |
|------|------|------|
| read_file | Read 1 file | Read N files |
| search_files_rg | Searched for 1 pattern | Searched for N patterns |
| glob_files | Matched 1 pattern | Matched N patterns |

## 快捷键全览

### 快捷键设计规则

- **禁止 Shift + 字母**：`Shift + 字母` 在编辑状态下等同于输入大写字母，二者不可区分，因此快捷键一律不得使用 `Shift + A-Z` 组合。
- 全局操作优先使用 `Ctrl + 字母`（如 `Ctrl+C`）或功能键（如 `Esc`、`PageUp`）。
- 面板内操作优先使用 `↑/↓`、`Space`、`Enter`、`Esc` 等无冲突按键。

### 全局快捷键

| 按键 | 行为 | 说明 |
|------|------|------|
| `Ctrl+C` | 中断 Agent（loading 时）/ 退出（idle 时） | |
| `Esc` | 退出程序（idle 时） | |
| `Enter` | 提交消息（idle）/ 缓冲消息（loading） | loading 时消息排队等待 |
| `Alt+Enter` | 插入换行 | |
| `Shift+Tab` | 循环切换权限模式 | DEFAULT → AUTO-EDIT → AUTO → YOLO → NO-ASK |
| `↑` | 浮层导航 / 历史恢复 | 浮层激活时导航候选，否则恢复上一条输入 |
| `↓` | 浮层导航 / 历史恢复 | 浮层激活时导航候选，否则恢复下一条输入 |
| `Tab` | 命令/Skills 提示浮层导航 | 选中后 Enter 补全 |
| `Ctrl+V` | 粘贴剪贴板（优先图片，回退文字） | |
| `PageUp/PageDown` | 消息区上下翻页（每次 10 行） | |
| `Del` | 删除最后一个待发送附件 | |
| `MouseScrollUp/Down` | 消息区滚动 | |

### HITL 审批弹窗

| 按键 | 行为 |
|------|------|
| `↑/↓` | 移动光标 |
| `Space` `t` | 切换当前项（批准/拒绝） |
| `y` | 全部批准并确认 |
| `n` | 全部拒绝并确认 |
| `Enter` | 按当前选择确认提交 |
| `Ctrl+C` | 退出程序 |

### AskUser 批量问答弹窗

| 按键 | 行为 |
|------|------|
| `Tab` / `Shift+Tab` | 切换问题 Tab |
| `↑/↓` | 当前问题内选项光标移动 |
| `Space` | 切换当前选项选中状态 |
| `Enter` | 提交所有答案 |
| 普通字符 | 自定义文本输入 |
| `Backspace` | 删除字符 |
| `Ctrl+C` | 退出程序 |

### /login 面板

**Browse 模式：**

| 按键 | 行为 |
|------|------|
| `↑/↓` | 上下移动光标 |
| `Enter` `e` | 进入编辑模式 |
| `n` | 新建 Provider |
| `Space` | 选中/激活当前 Provider |
| `Esc` | 关闭面板 |

**Edit / New 模式：**

| 按键 | 行为 |
|------|------|
| `↑/↓` `Tab/Shift+Tab` | 切换字段 |
| `←/→` `Space` | 切换 Type 字段（仅 Type 字段有效） |
| `Enter` | 保存编辑 |
| `Esc` | 取消编辑，回到 Browse |
| `Backspace` | 删除当前字段末字符 |
| `Ctrl+V` | 粘贴到当前字段 |

**ConfirmDelete 模式：**

| 按键 | 行为 |
|------|------|
| `Enter` | 确认删除 |
| `Esc` | 取消删除，回到 Browse |

### /model 面板

| 按键 | 行为 |
|------|------|
| `↑/↓` | 上下移动光标 |
| `Enter` | 确认选择（Opus/Sonnet/Haiku 行切换模型） |
| `Space` | 切换 Thinking 开关 |
| `Esc` | 关闭面板 |
| 普通字符 | 编辑当前行字段 |
| `Backspace` | 删除当前字段末字符 |
| `Ctrl+V` | 粘贴到当前字段 |

### /agents 面板

| 按键 | 行为 |
|------|------|
| `↑/↓` | 上下移动光标 |
| `Enter` | 确认选择当前 Agent |
| `Esc` | 关闭面板（不改变 Agent） |

### /history (Thread Browser) 面板

| 按键 | 行为 |
|------|------|
| `↑/↓` | 上下移动光标 |
| `Enter` | 打开选中对话 |
| `Esc` | 关闭面板 |

### /cron 面板

| 按键 | 行为 |
|------|------|
| `↑/↓` | 上下移动光标 |
| `Enter` | 切换任务启用/暂停 |
| `Esc` | 关闭面板 |

### 通用面板按键约定

所有面板遵循以下统一约定（优先级高于面板特定按键）：

| 按键 | 行为 |
|------|------|
| `↑/↓` | 竖向列表导航 |
| `←/→` | 横向切换（枚举字段） |
| `Enter` | 确认/进入/保存 |
| `Space` | 选中/切换 |
| `Esc` | 关闭/取消 |
| `Ctrl+V` | 粘贴剪贴板内容 |

## 色板

### 强调色

| 名称 | 色值 | 用途 |
|------|------|------|
| ACCENT | `#FF6B2B` | 唯一主交互色：用户消息前缀、激活边框、光标、关键操作 |

### 功能色

| 名称 | 色值 | 用途 |
|------|------|------|
| SAGE | `#6EB56A` | 哑光绿：成功状态、工具名、SubAgent、只读工具摘要 |
| WARNING | `#B09878` | 暖米灰：次要强调、标题、快捷键高亮 |
| ERROR | `#CC463E` | 暗红：错误/拒绝 |
| THINKING | `#A78BFA` | 亮紫罗兰：推理/CoT 思考内容 |
| LOADING | `#22D3EE` | 电光青：Loading spinner、AUTO 权限模式 |
| MODEL_INFO | `#A0825F` | 棕金：状态栏模型名（不抢眼） |
| TOOL_NAME | `= SAGE` | 语义别名：工具名展示色 |
| SUB_AGENT | `= SAGE` | 语义别名：SubAgent 展示色 |
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

## 命令与 Skills

| 命令 | 说明 |
|------|------|
| `/login` | Provider 配置管理（新建/编辑/删除） |
| `/model` | 打开模型选择面板 |
| `/model <alias>` | 直接切换活跃模型（`opus` / `sonnet` / `haiku`） |
| `/history` | 历史对话浏览 |
| `/agents` | SubAgent 定义管理 |
| `/compact` | 触发上下文压缩 |
| `/clear` | 清空当前消息列表 |
| `/cron` | 定时任务管理面板 |
| `/help` | 列出所有命令 |

输入 `#` 前缀触发 Skills 浮层，`Tab` / `↑↓` 导航，`Enter` 补全为 `#skill-name`。
输入 `/` 前缀触发命令提示，支持前缀唯一匹配（如 `/m` 匹配 `/model`），`Tab` / `↑↓` 导航。

## 权限模式

通过 `Shift+Tab` 循环切换，状态栏首列实时显示：

| 模式 | 标签 | 颜色 | 说明 |
|------|------|------|------|
| Default | DEFAULT | TEXT | 默认：所有危险工具需审批 |
| AcceptEdits | AUTO-EDIT | SAGE | 自动批准编辑类工具 |
| Auto | AUTO | LOADING | 自动批准更多工具 |
| YOLO | YOLO | WARNING | 跳过所有 HITL 审批（不影响 ask_user_question） |
| NoAsk | NO-ASK | ERROR | 不主动提问 |
