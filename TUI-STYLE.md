# TUI Style Guide

## 设计哲学

中性灰层级 + Claude 暖橙品牌色。背景透明（弹窗光标行和用户消息区除外）。信息层级用亮度区分（TEXT/MUTED/DIM），颜色表达状态语义。

## 消息流

### 消息类型与视觉

| 类型 | 前缀 | 前景色 | 底色 | 说明 |
|------|------|--------|------|------|
| 用户消息 | `❯` | ACCENT (暖橙) | `#373737` (中性灰) | 底色与 sticky header 一致，所有行带底色 |
| AI 回复 | `●` | TEXT (纯白) | — | 正文直接跟在 `● ` 后，支持 markdown 渲染 |
| 思考 (Reasoning) | — | — | — | 不在消息流中渲染，完全隐藏 |
| 工具调用 (非只读) | `●` | 白色工具名 + 状态色指示器 | — | bash=粉红/write=琥珀/read=绿结果边框用类别色 |
| 工具聚合组 (只读) | 无 | MUTED | — | read/glob/search 折叠为单行摘要 |
| SubAgent | `●` + emoji | SAGE | — | 折叠显示摘要，展开显示嵌套消息 |
| 系统消息 | `[i]` | SAGE | — | 系统/提示信息 |

### 间距规则

- 每条有内容的消息后加 **1 个空行**，由 `render_one` 统一管理
- 空内容消息（如纯思考被隐藏的 AssistantBubble）不渲染、不占位
- 消息内部不插入多余空行

### 工具状态指示器

指示器 `●` 按状态变色，工具名称统一白色 + BOLD，结果边框使用类别颜色区分：

| 状态 | 指示器颜色 | 工具名颜色 | 结果边框颜色 |
|------|-----------|-----------|------------|
| Running | TEXT (白) | TEXT + BOLD | — |
| Completed | SAGE (绿) | TEXT + BOLD | 类别色 |
| Failed | ERROR (红) | TEXT + BOLD | ERROR |

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
| `Enter` / `Space` | 确认选择（模型行切换模型并保存） |
| `Enter` / `Space` | 切换 effort 等级（Effort 行） |
| `←/→` | 循环切换 effort 等级（Low/Medium/High） |
| `Esc` | 关闭面板 |

#### /model 面板样式

布局（12 行含边框）：

```
───────────── Select model ──────────────────
  Switch between models. Applies to this session.

  ❯ 1. Opus  ✔   claude-opus-4-7
    2. Sonnet      claude-sonnet-4-6
    3. Haiku       claude-haiku-4-5

    ● High effort ← → to adjust

  Enter to confirm · Esc to exit
──────────────────────────────────────────────
```

颜色规则：

| 元素 | 颜色 | 说明 |
|------|------|------|
| 边框 | BORDER (`#505050`) | 中性灰，不用紫色 |
| 标题 "Select model" | THINKING + BOLD | 标准紫 |
| 描述文字 | MUTED | 灰色 |
| 光标箭头 `❯` | THINKING | 标准紫 |
| 选中项标签（✔ 所在行） | SAGE + BOLD | 绿色，优先级高于紫色 |
| 光标行标签（非选中项） | THINKING + BOLD | 标准紫 |
| 普通行标签 | TEXT + BOLD | 白色 |
| 对勾 `✔` | SAGE | 绿色 |
| 模型名称（右侧） | MUTED | 始终灰色，不变色 |
| Effort 圆点 `●` | ACCENT | 暖橙品牌色 |
| Effort 文字 | MUTED + BOLD | 灰色 |
| Effort 箭头提示 | MUTED | 灰色 |
| 底部快捷键 | WARNING (键) + MUTED (说明) | 统一风格 |

列对齐：编号列、标签列、对勾列固定宽度，模型名称左对齐。默认 effort 为 "high"。

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
| `d` | 删除选中对话（需二次确认） |
| `Esc` | 关闭面板 |

选中行样式：`fg(THINKING)` 紫色文字，`❯` 光标指示器。当前对话用 `✓` + `ACCENT` 标记。

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

## 面板样式统一规范

### 选中行样式

所有面板/弹窗的选中行统一使用 `fg(THINKING) #A2A9E4` 标准紫文字，**不使用底色**。光标指示器使用 `❯` 符号（THINKING 色）。/model 面板例外：选中项（✔）标签使用 SAGE 绿色，优先级高于紫色。

### 边框颜色规则

| 类型 | 边框颜色 | 面板 |
|------|----------|------|
| 常规面板 | MUTED | agent, cron, thread_browser, model, hints |
| 警告弹窗 | WARNING | HITL 审批、AskUser 提问 |
| 模式语义 | 动态 | login（Edit=WARNING, New=SAGE, Delete=ERROR, Browse=MUTED） |
| 引导流程 | ACCENT | setup_wizard 步骤 1-2（Done=SAGE） |

### 快捷键提示位置

所有面板的快捷键提示统一放在列表/表单**底部**，格式为 `按键:说明` 横排，按键用 WARNING 色高亮，说明用 MUTED 色。

## 色板

### 强调色

| 名称 | 色值 | 用途 |
|------|------|------|
| ACCENT | `#D77757` | Claude 暖橙品牌色：用户消息前缀、激活边框、光标、关键操作 |

### 功能色

| 名称 | 色值 | 用途 |
|------|------|------|
| SAGE | `#4EBA65` | 明亮绿：成功状态、Read/Glob/Grep 工具名、SubAgent、只读工具摘要 |
| WARNING | `#FFC107` | 明亮琥珀：Write/Edit 工具名、次要强调、快捷键高亮 |
| ERROR | `#FF6B80` | 明亮红：错误/拒绝、失败工具结果边框 |
| THINKING | `#A2A9E4` | 标准紫：推理/CoT 思考内容、面板选中行、/model 面板光标 |
| LOADING | `#93A5FF` | 浅蓝紫：Loading spinner、AUTO 权限模式 |
| BASH_BORDER | `#FD5DB1` | 粉红：Bash 工具名和结果边框（Claude bashBorder） |
| MODEL_INFO | `#A0825F` | 棕金：状态栏模型名（不抢眼） |
| TOOL_NAME | `= SAGE` | 语义别名：工具名展示色 |
| SUB_AGENT | `= SAGE` | 语义别名：SubAgent 展示色 |

### 文字层级（三级亮度）

| 层级 | 色值 | 用途 |
|------|------|------|
| TEXT | `#FFFFFF` | 纯白：主文字、AI 回复前缀、Running 指示器 |
| MUTED | `#999999` | 浅灰：次要文字、标签、路径、工具参数、聚合摘要 |
| DIM | `#505050` | 深灰：极弱文字、占位、已完成项、分隔符 |

### 底色

| 名称 | 色值 | 用途 |
|------|------|------|
| USER_BG | `#373737` | 用户消息底色（与 sticky header 一致） |
| POPUP_BG | `#000000` | 纯黑弹窗底色 |
| CURSOR_BG | `#262626` | 中性暗灰（保留，暂未使用） |

### 边框

| 名称 | 色值 | 用途 |
|------|------|------|
| BORDER | `#505050` | 中性灰空闲边框 |
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
