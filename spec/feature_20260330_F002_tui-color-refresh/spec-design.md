# Feature: 20260330_F002 - tui-color-refresh

## 需求背景

当前 TUI 配色系统以 `#FF6B2B` 橙色为唯一主强调色（ACCENT），设计哲学为"极简锋利，单色制胜"。然而实际使用中出现了以下问题：

- **ACCENT 过度使用**：H1/H2 标题（ACCENT + BOLD + 下划线三重强调）、边框激活、状态栏、工具名（间接通过 SAGE）、行内代码全部用橙/绿，信息密度高时视觉混乱
- **SAGE 泛化**：所有工具名统一 SAGE（#6EB56A），导致绿色失去"成功/正面"的语义信号价值
- **重心不明显**：橙色覆盖面积过大，视觉重心无法聚焦到最重要的交互元素（命令输入）

> 配色问题不是颜色本身不对，而是**使用面积和叠加方式**超出了视觉舒适边界。

## 目标

- **降噪优先，橙歇山顶**：橙色只留给最高优先级交互（命令输入 + 危险操作确认），其余靠 MUTED/WARNING 自然分层
- **恢复语义色信号价值**：SAGE 只表示真正的"成功"，不再用于标记工具名
- **信息层级三档清晰**：TEXT → MUTED → DIM + 语义色点缀，不靠颜色堆叠区分层级

## 方案设计

### 3.1 标题层级降噪

**现状**：H1/H2 = ACCENT + BOLD + 下划线，与边框激活同级别强调，视觉重量过载。

**改动**：

| 元素 | 当前 | 改动后 | 理由 |
|------|------|--------|------|
| H1 | ACCENT + BOLD + 下划线 | WARNING + BOLD（去下划线） | 保留层级区分，去掉过重的三重复合强调 |
| H2 | ACCENT + BOLD | WARNING + BOLD | 同上 |
| H3 | WARNING + BOLD | WARNING + BOLD | 无需改动，已经是次级强调色 |

> ACCENT 橙色的"下划线"强调效果从 Markdown 渲染中移除，仅靠 BOLD + WARNING 色差区分 H1/H2 与普通文字即可。

### 3.2 工具名颜色分级

**现状**：所有工具名统一 SAGE（`pub const TOOL_NAME = SAGE`），绿色失去区分意义，且与"成功状态"颜色重叠。

**改动**：工具名颜色按操作危险度三级分级：

| 工具 | 颜色 | 理由 |
|------|------|------|
| `bash` | `ACCENT` | 最高权限，最需注意 |
| `write_file`, `edit_file`, `folder_operations`, `delete_*`, `rm_*` | `WARNING` | 破坏性/结构性操作，警示但不刺眼 |
| `read_file`, `glob_files`, `search_files_rg`, `launch_agent`, `ask_user_question`, `todo_write` | `MUTED` | 只读/委派/安全操作，无需颜色强调 |
| 工具**执行成功**（结果区） | `SAGE` | 保留，工具**执行成功**才用绿色，语义清晰 |
| 工具**执行出错** | `ERROR` | 无变化 |

> 关键变化：`TOOL_NAME` 常量不再等于 SAGE，改为按工具动态选择颜色。`SAGE` 语义收窄为"成功状态"，不再用于标签。

### 3.3 边框状态分层

**现状**：`BORDER_ACTIVE = ACCENT` 统一应用于所有激活边框（输入框、配置面板、弹窗），导致"满屏橙色"感。

**改动**：边框激活按交互层级分两级：

| 场景 | 边框色 | 说明 |
|------|--------|------|
| 主命令输入框激活 | `ACCENT` | 唯一保留橙色的边框——命令入口 |
| 配置面板激活（ModelPanel, RelayPanel, AgentsPanel） | `MUTED` | 次要交互区，降为灰调，不抢主输入框的戏 |
| HITL/AskUser 弹窗 | `WARNING` | 需要注意但不刺眼，警示感 |
| 子 Agent 组块边框 | `SAGE` | 语义色表示"成功委派"，边框语义化 |

> `BORDER_ACTIVE` 常量保留（= ACCENT），仅在主输入框渲染路径使用；配置面板激活用 `MUTED_ACTIVE: Color = MUTED`（新增常量）。

### 3.4 状态栏信息降噪

**现状**：状态栏中时间显示和 Agent 名称用 ACCENT 橙色，与输入框争抢视觉重心。

**改动**：

| 元素 | 当前 | 改动后 |
|------|------|--------|
| 任务时长（`0.8s`） | `ACCENT` | `MUTED` |
| Agent 名称 | `ACCENT` | `MUTED` |
| 模型名称 | `MODEL_INFO` | 无变化（棕金色，OK） |
| 快捷键说明 | `WARNING` | 无变化 |
| Loading spinner | `WARNING` | 无变化 |
| 错误状态 | `ERROR` | 无变化 |

### 3.5 消息类型颜色总览（对照表）

![配色总览](./images/01-color-overview.png)

| 消息/UI 元素 | 当前颜色 | 改动后颜色 | 变化 |
|-------------|---------|-----------|------|
| H1 标题 | ACCENT+BOLD+下划线 | WARNING+BOLD | 降噪 |
| H2 标题 | ACCENT+BOLD | WARNING+BOLD | 降噪 |
| H3 标题 | WARNING+BOLD | WARNING+BOLD | 无 |
| 普通文字 | TEXT | TEXT | 无 |
| 次要文字 | MUTED | MUTED | 无 |
| 工具成功（结果区） | TEXT | SAGE | 新语义 |
| 工具执行失败 | ERROR | ERROR | 无 |
| `bash` 工具名 | TOOL_NAME=SAGE | ACCENT | 升级为最高危险色 |
| 写操作工具名 | TOOL_NAME=SAGE | WARNING | 升级为警示色 |
| 只读工具名 | TOOL_NAME=SAGE | MUTED | 降为无强调 |
| 子 Agent 组边框 | BORDER_ACTIVE=ACCENT | SAGE | 语义化 |
| 主输入框激活边框 | ACCENT | ACCENT | 无（唯一保留） |
| 配置面板激活边框 | ACCENT | MUTED | 降噪 |
| HITL 弹窗边框 | ACCENT | WARNING | 警示感 |
| 状态栏时间 | ACCENT | MUTED | 降噪 |
| 状态栏 Agent 名 | ACCENT | MUTED | 降噪 |
| Loading spinner | WARNING | WARNING | 无 |
| 错误状态 | ERROR | ERROR | 无 |

## 实现要点

### 关键文件变更

- **`rust-agent-tui/src/ui/theme.rs`**：更新常量注释和 TOOL_NAME 别名逻辑（TOOL_NAME 改为按工具名动态返回，不再是简单别名）
- **`rust-agent-tui/src/ui/markdown/mod.rs`**：`render_heading()` 方法中 H1/H2 的颜色从 ACCENT 改为 WARNING
- **`rust-agent-tui/src/ui/message_render.rs`** 或工具渲染路径：按工具名返回对应颜色（`get_tool_name_color(name)` 函数）
- **`rust-agent-tui/src/ui/main_ui/panels/`**：ModelPanel/RelayPanel/AgentsPanel 的激活边框改用 MUTED_ACTIVE
- **`rust-agent-tui/src/ui/main_ui/status_bar.rs`**：时间/Agent 名颜色改为 MUTED
- **`TUI-STYLE.md`**：同步更新配色表格和指南，版本升为 1.1

### 工具名颜色函数

```rust
// theme.rs 新增
/// 按工具名返回对应的标签色（用于工具调用中的工具名显示）
pub fn tool_name_color(name: &str) -> Color {
    match name {
        "bash" => ACCENT,
        "write_file" | "edit_file" | "folder_operations" | "delete_file"
        | "delete_folder" | "rm" | "rm_rf" => WARNING,
        _ => MUTED,
    }
}
```

### 改动量评估

- 文件级改动：6-7 个文件
- 性质：纯渲染颜色调整，无逻辑/数据模型变化
- 风险：低，纯 UI 层修改，无 API 变更

## 约束一致性

本方案与 `spec/global/constraints.md` 和 `spec/global/architecture.md` 的约束完全一致，无架构偏离：

- 配色仍通过 `theme.rs` 统一管理，符合单点定义原则
- 渲染管道不变（双线程架构不变）
- 无新增依赖
- TUI-STYLE.md 同步更新为 v1.1，保持文档与实现同步

## 验收标准

- [ ] H1/H2 标题渲染为 WARNING + BOLD，无下划线
- [ ] `bash` 工具名显示为 ACCENT 橙色
- [ ] `write_file` / `edit_file` / `folder_operations` 工具名显示为 WARNING 黄色
- [ ] `read_file` / `glob_files` / `search_files_rg` 等只读工具名显示为 MUTED 灰色
- [ ] 工具**执行成功**时结果区用 SAGE 绿色（非工具名）
- [ ] 主输入框激活边框保持 ACCENT 橙色
- [ ] ModelPanel / RelayPanel 激活边框为 MUTED（无橙色）
- [ ] HITL 弹窗边框为 WARNING 黄色
- [ ] 状态栏任务时长和 Agent 名称为 MUTED
- [ ] TUI-STYLE.md 更新为 v1.1，与实现一致
