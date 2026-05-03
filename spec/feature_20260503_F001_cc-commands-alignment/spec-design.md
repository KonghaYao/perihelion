# Feature: 20260503_F001 - cc-commands-alignment

## 需求背景

Perihelion TUI 当前有 10 个内置命令（`/help`、`/model`、`/login`、`/clear`、`/compact`、`/history`、`/agents`、`/loop`、`/cron`、`/mcp`），而 Claude Code 有 70+ 命令。用户期望对齐 Claude Code 的核心高频命令集，优先覆盖配置/设置类和会话/状态查询类。

## 目标

- 新增 4 个命令：`/config`、`/cost`、`/context`、`/memory`
- 扩展 Command trait 支持 alias 机制
- 为现有命令补充常用别名（如 `/clear` → `/reset`、`/new`）

## 方案设计

### 命令清单总览

新增 4 个命令，总命令数从 10 → 14：

| 命令 | 别名 | 类型 | 说明 |
|------|------|------|------|
| `/config` | `/settings` | 表单面板 | 全局配置（autocompact、语言、system prompt 覆盖） |
| `/cost` | — | 只读面板 | 当前会话费用 + 时长 + token 占用 |
| `/context` | — | 只读面板 | 上下文使用率，与 `/cost` 共用面板组件 |
| `/memory` | — | 列表面板 | 编辑用户/项目级 CLAUDE.md memory 文件 |

### Command trait 扩展

当前 trait：

```rust
pub trait Command: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn execute(&self, app: &mut App, args: &str);
}
```

扩展后：

```rust
pub trait Command: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn aliases(&self) -> Vec<&str> { vec![] }
    fn execute(&self, app: &mut App, args: &str);
}
```

### dispatch 匹配优先级变更

当前：精确匹配 `name` → 前缀唯一匹配

扩展为：
1. 精确匹配 `name`
2. 精确匹配 `aliases` 中任一项
3. 前缀唯一匹配（同时对 `name` 和所有 `aliases`）

### 现有命令更新

- `clear` 增加 aliases: `["reset", "new"]`

### /config 面板设计

**UI 形式：** 表单面板（类似 `/login` 风格），字段上下排列。

| 字段 | 组件类型 | 默认值 | 说明 |
|------|----------|--------|------|
| Autocompact | RadioGroup（开/关） | 开 | 上下文压缩自动触发开关 |
| Compact Threshold | InputField（数字） | 85 | 上下文窗口使用率阈值（%） |
| Language | InputField（字符串） | auto | UI 语言，`auto` 自动探测系统语言 |
| Persona | InputField（文本） | 空 | 系统提示词 persona 覆盖 |
| Tone | InputField（文本） | 空 | 系统提示词 tone 覆盖 |
| Proactiveness | RadioGroup（low/medium/high） | medium | 主动性级别 |

**数据流：**
- 面板打开时从 `AppConfig` 读取当前值
- 保存时调用 `App::save_config()` 写入 `settings.json`（使用 `config_path_override` 保证测试隔离）
- 语言设置影响 TUI 状态栏/提示语的语言选择
- Persona/Tone/Proactiveness 通过 `AgentOverrides` 注入到 `build_system_prompt()`

**操作模式：** Browse 模式 ↑↓ 导航字段，Enter 进入编辑，Esc 取消退出，编辑完成后 Enter 保存。

### /cost & /context 面板设计

**UI 形式：** 单一面板组件 `StatusPanel`，顶部 TabBar 切换两个 Tab。

**Tab 1 — 费用（`/cost` 默认激活）：**

| 信息项 | 来源 |
|--------|------|
| 会话时长 | `Instant::now() - session_start` |
| 总 Token 消耗（input/output/cache） | `TokenTracker` 累积值 |
| 估算费用（USD） | `TokenTracker` × 模型单价 |
| 当前模型名 | `active_model` |

**Tab 2 — 上下文（`/context` 默认激活）：**

| 信息项 | 来源 |
|--------|------|
| 上下文窗口大小 | `ContextBudget::window_size` |
| 已使用 Token | `TokenTracker::total_input()` |
| 使用率百分比 | `used / window × 100%` |
| 消息数 | `messages.len()` |
| 工具调用次数 | 统计 `ToolStart` 事件数 |
| Autocompact 触发阈值 | `ContextBudget::threshold` |

**操作模式：** 只读展示面板，↑↓ 滚动（内容超出时），←→ 切换 Tab，Esc 关闭。

### /memory 面板设计

**UI 形式：** 列表面板，列出可编辑的 memory 文件。

| 条目 | 路径 | 说明 |
|------|------|------|
| 项目说明 | `{cwd}/CLAUDE.md` | 当前项目的 Claude 配置 |
| 用户全局 | `~/.claude/CLAUDE.md` | 用户级全局配置 |

**操作模式：**
- Browse 模式 ↑↓ 选择文件，Enter 用系统编辑器（`$EDITOR` 或 `vi`）打开文件
- 底部提示：文件不存在时显示「按 Enter 创建」
- Esc 关闭面板

**实现要点：**
- 调用 `std::process::Command` 打开外部编辑器，TUI 暂时挂起（恢复 alternate screen）
- 编辑完成后 TUI 恢复，不需要重新加载（memory 文件在下次 agent 调用时自动读取）

## 实现要点

1. **Command trait 向后兼容：** `aliases()` 有默认实现返回空 Vec，现有命令无需修改
2. **状态栏感知：** 新增面板需在 `status_bar.rs` 的 `render_second_row` 中添加快捷键提示
3. **测试隔离：** `/config` 保存必须使用 `App::save_config(cfg, self.config_path_override.as_deref())`
4. **TokenTracker 扩展：** 需暴露 `total_input()` / `total_output()` / `total_cache()` 方法供面板读取
5. **ContextBudget 扩展：** 需暴露 `window_size()` / `threshold()` 方法

## 约束一致性

- **Workspace 分层：** 新命令全部在 `rust-agent-tui` 应用层实现，符合「禁止下层依赖上层」约束
- **Widget 复用：** 面板使用 `perihelion-widgets` 的 `BorderedPanel`、`ScrollableArea`、`RadioGroup`、`InputField`、`TabBar` 组件
- **编码规范：** 使用 `tracing` 日志，不使用 `println!`
- **测试：** 所有面板支持 headless 测试模式

## 验收标准

- [ ] `/config` 打开表单面板，可编辑 autocompact/语言/persona/tone/proactiveness
- [ ] `/config` 保存后配置持久化到 `settings.json`
- [ ] `/cost` 显示费用+时长+token 消耗
- [ ] `/context` 显示上下文使用率+消息数+工具调用数
- [ ] `/cost` 和 `/context` 共用面板，TabBar 切换
- [ ] `/memory` 列出 memory 文件，Enter 打开外部编辑器
- [ ] `/settings` 别名正确路由到 `/config`
- [ ] `/clear` 别名 `reset`、`new` 可用
- [ ] Command trait aliases 默认实现不破坏现有命令
- [ ] 所有新面板在 headless 测试中可测试
- [ ] 状态栏正确显示新面板的快捷键
