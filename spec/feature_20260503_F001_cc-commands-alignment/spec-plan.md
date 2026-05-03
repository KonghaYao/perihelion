# cc-commands-alignment 执行计划

**目标:** 新增 /config、/cost、/context、/memory 四个命令，扩展 Command trait 支持 alias 机制，为现有命令补充别名

**技术栈:** Rust 2021, ratatui, perihelion-widgets (TabBar/BorderedPanel/ScrollableArea), tokio, serde

**设计文档:** spec/feature_20260503_F001_cc-commands-alignment/spec-design.md

## 改动总览

- 本次改动涉及 16 个文件：3 个新建命令（config/cost/context_cmd/memory）+ 3 个新建面板状态（config_panel/status_panel/memory_panel）+ 3 个新建渲染模块（panels/config/status/memory）+ 7 个修改文件（command/mod.rs、app/mod.rs、config/types.rs、event.rs、main_ui.rs、status_bar.rs、panel_ops.rs）+ 2 个框架文件（agent_comm.rs、agent_ops.rs）
- Task 1 是基础（Command trait 别名），Task 2/3/4 各自独立创建面板文件（状态+渲染+命令），Task 5 负责将三个新面板接入事件处理链、渲染管线和状态栏
- 关键决策：ConfigPanel 挂在 AppCore（与 LoginPanel 同级），StatusPanel/MemoryPanel 挂在 App 层级；Task 2/3/4 仅创建新文件，Task 5 统一集成到主应用生命周期

---

### Task 0: 环境准备

**背景:**
确保构建和测试工具链在当前开发环境中可用，避免后续 Task 因环境问题阻塞。

**执行步骤:**
- [x] 验证构建工具可用
  - `cargo build -p rust-agent-tui 2>&1 | tail -3`
- [x] 验证测试工具可用
  - `cargo test -p rust-agent-tui --lib -- command::tests 2>&1 | tail -3`

**检查步骤:**
- [x] 构建命令执行成功
  - `cargo build -p rust-agent-tui 2>&1 | tail -3`
  - 预期: 输出包含 `Finished` 且无 error
- [x] 测试命令可用
  - `cargo test -p rust-agent-tui --lib 2>&1 | tail -3`
  - 预期: 输出包含 `test result: ok`

---

### Task 1: Command trait 别名扩展

**背景:**
当前 Command trait 只有 `name`/`description`/`execute` 三个方法，dispatch 仅按 `name` 匹配，无法支持 `/reset` → `/clear` 等别名路由。本 Task 在 trait 中新增 `aliases()` 默认方法（向后兼容，现有命令无需修改），并在 dispatch 管线中插入别名精确匹配和别名前缀匹配逻辑。Task 2-5 的所有新命令（如 `/config` 别名 `/settings`）将依赖此机制。

**涉及文件:**
- 修改: `rust-agent-tui/src/command/mod.rs`
- 修改: `rust-agent-tui/src/command/clear.rs`

**执行步骤:**

- [x] 在 Command trait 中新增 `aliases()` 默认方法
  - 位置: `rust-agent-tui/src/command/mod.rs` trait 定义块，`description()` 之后、`execute()` 之前（~L36-37 之间）
  - 内容:
    ```rust
    /// 命令别名列表（不含 /），默认为空
    fn aliases(&self) -> Vec<&str> { vec![] }
    ```
  - 原因: 默认返回空 Vec，所有现有命令（agents/login/model/clear/compact/help/history/loop/cron/mcp）无需任何修改即可编译通过

- [x] 在 `dispatch` 方法中插入别名精确匹配（优先级第 2 级）
  - 位置: `rust-agent-tui/src/command/mod.rs` `dispatch()` 方法，精确匹配 `name` 块（~L68-72）之后、前缀唯一匹配块（~L75-83）之前
  - 内容: 在 L72 `}` 之后、L74 注释之前插入:
    ```rust
    // 2. 别名精确匹配
    if let Some(cmd) = self.commands.iter().find(|c| c.aliases().iter().any(|a| *a == name)) {
        cmd.execute(app, args);
        return true;
    }
    ```
  - 原因: 别名精确匹配优先级高于前缀匹配，确保 `/reset` 精确路由到 clear 而不会被前缀逻辑干扰

- [x] 扩展 `dispatch` 前缀匹配以覆盖别名
  - 位置: `rust-agent-tui/src/command/mod.rs` `dispatch()` 方法，前缀匹配块（~L75-83）
  - 内容: 将 `filter` 条件从仅匹配 `name` 扩展为同时匹配 `name` 和 `aliases`:
    ```rust
    // 3. 前缀唯一匹配（同时对 name 和 aliases）
    let matches: Vec<_> = self
        .commands
        .iter()
        .filter(|c| {
            c.name().starts_with(name) || c.aliases().iter().any(|a| a.starts_with(name))
        })
        .collect();
    ```
  - 原因: 输入 `/re` 应能前缀匹配到 `clear` 的别名 `reset`；更新注释编号为 "3."

- [x] 扩展 `list()` 方法返回别名信息
  - 位置: `rust-agent-tui/src/command/mod.rs` `list()` 方法（~L89-94）
  - 内容: 将返回类型从 `Vec<(&str, &str)>` 改为 `Vec<(&str, &str, Vec<&str>)>`，返回 `(name, description, aliases)`:
    ```rust
    /// 返回所有已注册命令的 (name, description, aliases) 列表
    pub fn list(&self) -> Vec<(&str, &str, Vec<&str>)> {
        self.commands
            .iter()
            .map(|c| (c.name(), c.description(), c.aliases()))
            .collect()
    }
    ```
  - 原因: help 命令和 command_help_list 需要别名信息用于展示

- [x] 同步更新 `core.rs` 中 `command_help_list` 的构建逻辑
  - 位置: `rust-agent-tui/src/app/core.rs` ~L80-83
  - 内容: 将元组解构从 `(n, d)` 改为 `(n, d, aliases)`，存储类型改为 `Vec<(String, String, Vec<String>)>`:
    ```rust
    let command_help_list: Vec<(String, String, Vec<String>)> = command_registry
        .list()
        .into_iter()
        .map(|(n, d, a)| (n.to_string(), d.to_string(), a.into_iter().map(String::from).collect()))
        .collect();
    ```
  - 同步更新 `AppCore` 结构体中 `command_help_list` 字段类型（~L36）:
    ```rust
    pub command_help_list: Vec<(String, String, Vec<String>)>,
    ```

- [x] 同步更新 `help.rs` 显示逻辑以展示别名
  - 位置: `rust-agent-tui/src/command/help.rs` ~L18-19
  - 内容: 修改循环，解构三个字段，非空别名追加显示:
    ```rust
    for (name, desc, aliases) in &app.core.command_help_list {
        let alias_str = if aliases.is_empty() {
            String::new()
        } else {
            format!(" (别名: /{})", aliases.join(", /"))
        };
        lines.push(format!("  /{:<10} {}{}", name, desc, alias_str));
    }
    ```
  - 原因: 用户执行 `/help` 时应能看到每个命令的别名

- [x] 扩展 `match_prefix()` 方法以覆盖别名
  - 位置: `rust-agent-tui/src/command/mod.rs` `match_prefix()` 方法（~L98-104）
  - 内容: 将 `filter` 条件扩展为同时匹配 `name` 和 `aliases`:
    ```rust
    pub fn match_prefix(&self, prefix: &str) -> Vec<(&str, &str)> {
        self.commands
            .iter()
            .filter(|c| {
                c.name().starts_with(prefix) || c.aliases().iter().any(|a| a.starts_with(prefix))
            })
            .map(|c| (c.name(), c.description()))
            .collect()
    }
    ```
  - 原因: 浮层输入 `/re` 时应能匹配到 `clear` 命令（别名 `reset`），浮层统一显示主名 `clear`

- [x] 为 ClearCommand 添加 aliases
  - 位置: `rust-agent-tui/src/command/clear.rs` ~L6-18，在 `description()` 和 `execute()` 之间
  - 内容: 添加 `aliases()` 实现:
    ```rust
    fn aliases(&self) -> Vec<&str> {
        vec!["reset", "new"]
    }
    ```
  - 原因: `/reset` 和 `/new` 是 Claude Code 中 clear 的常见别名

- [x] 为 Command trait 别名匹配逻辑编写单元测试
  - 测试文件: `rust-agent-tui/src/command/mod.rs`（现有 `#[cfg(test)] mod tests` 块末尾，~L276 之前）
  - 测试场景:
    - **别名精确匹配**: 注册带别名 `["reset", "new"]` 的 StubCommand `name="clear"`，dispatch `/reset` → 返回 true 且 called=true
    - **别名不匹配**: 注册 StubCommand `name="model"` 无别名，dispatch `/reset` → 返回 false
    - **name 优先于别名**: 注册 cmd_a `name="reset"` + cmd_b `name="clear"` 别名 `["reset"]`，dispatch `/reset` → 匹配 cmd_a（name 精确匹配优先于别名匹配）
    - **别名前缀匹配**: 注册带别名 `["reset"]` 的 StubCommand `name="clear"`，dispatch `/res` → 返回 true（前缀匹配别名）
    - **别名前缀歧义**: 注册 cmd_a `name="clear"` 别名 `["reset"]` + cmd_b `name="real"`，dispatch `/re` → 返回 false（两个命令都匹配前缀 `re`）
    - **match_prefix 覆盖别名**: 注册 StubCommand `name="clear"` 别名 `["reset"]`，`match_prefix("res")` → 返回包含 `("clear", "stub")` 的列表
    - **list 包含别名信息**: 注册带别名的 StubCommand，`list()` 返回的元组第三项包含别名
    - **无别名命令向后兼容**: 注册无别名覆盖的 StubCommand，`aliases()` 返回空 Vec，dispatch/list/match_prefix 行为不变
  - StubCommand 需扩展支持别名: 新增 `aliases_vec: Vec<&'static str>` 字段，实现 `aliases()` 返回该字段；为保持现有测试简洁，提供 `make_stub_with_aliases()` 辅助函数
  - 运行命令: `cargo test -p rust-agent-tui --lib -- command::tests`
  - 预期: 所有测试通过（含原有 8 个 + 新增 8 个）

**检查步骤:**

- [x] 验证 Command trait 包含 aliases 默认方法
  - `grep -n 'fn aliases' rust-agent-tui/src/command/mod.rs`
  - 预期: 输出包含 `fn aliases(&self) -> Vec<&str>` 行

- [x] 验证 ClearCommand 实现了 aliases
  - `grep -n 'aliases\|reset\|new' rust-agent-tui/src/command/clear.rs`
  - 预期: 输出包含 `vec!["reset", "new"]`

- [x] 验证 dispatch 包含别名精确匹配逻辑
  - `grep -n '别名精确匹配' rust-agent-tui/src/command/mod.rs`
  - 预期: 输出匹配到对应注释行

- [x] 验证 help 命令显示别名
  - `grep -n 'alias_str\|别名' rust-agent-tui/src/command/help.rs`
  - 预期: 输出包含别名拼接逻辑

- [x] 编译通过
  - `cargo build -p rust-agent-tui 2>&1 | tail -5`
  - 预期: 输出包含 `Compiling rust-agent-tui` 且无 error

- [x] 全部测试通过
  - `cargo test -p rust-agent-tui --lib -- command::tests 2>&1 | tail -10`
  - 预期: 输出包含 `test result: ok`，测试数量 ≥ 16

- [x] 已有测试无回归
  - `cargo test -p rust-agent-tui 2>&1 | tail -5`
  - 预期: 输出包含 `test result: ok`，无 failed

**认知变更:**
- [x] [CLAUDE.md] Command trait 的 `aliases()` 方法有默认实现返回空 Vec，新增命令时按需覆盖即可；dispatch 匹配优先级为：name 精确 → aliases 精确 → name+aliases 前缀唯一
---

### Task 2: /config 配置面板

**背景:**
用户需要一个统一的配置面板来管理 autocompact 开关/阈值、语言、系统提示词覆盖（persona/tone/proactiveness），替代手动编辑 settings.json。当前 AppConfig 已有 `compact`（CompactConfig）和 `thinking` 字段，但缺少 language/persona/tone/proactiveness 字段。面板设计复用 LoginPanel 的 Browse/Edit 双模式交互模式。

**涉及文件:**
- 新建: `rust-agent-tui/src/app/config_panel.rs`（核心）
- 新建: `rust-agent-tui/src/ui/main_ui/panels/config.rs`（核心）
- 新建: `rust-agent-tui/src/command/config.rs`（核心）
- 修改: `rust-agent-tui/src/config/types.rs`（AppConfig 增加 4 个字段）
- 注: app/mod.rs/core.rs/panel_ops.rs/command/mod.rs 的集成修改由 Task 5 统一执行

**执行步骤:**

- [x] 在 AppConfig 中新增 4 个配置字段
  - 位置: `rust-agent-tui/src/config/types.rs` AppConfig 结构体（~L96-120），在 `compact` 字段之后、`extra` 字段之前插入
  - 新增字段：
    ```rust
    /// UI 语言，"auto" 自动探测系统语言
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    /// 系统提示词 persona 覆盖
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub persona: Option<String>,
    /// 系统提示词 tone 覆盖
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tone: Option<String>,
    /// 主动性级别（low/medium/high）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proactiveness: Option<String>,
    ```
  - 原因: 这 4 个字段通过 `extra` 的 `#[serde(flatten)]` 兜底，旧配置文件无需迁移；新增的 `skip_serializing_if` 保证 None 时不序列化

- [x] 创建 ConfigPanel 状态结构体
  - 新建文件: `rust-agent-tui/src/app/config_panel.rs`
  - 参考 LoginPanel（`app/login_panel.rs`）的 Browse/Edit 模式设计
  - 内容要点：
    ```rust
    #[derive(Debug, Clone, PartialEq)]
    pub enum ConfigPanelMode { Browse, Edit }

    #[derive(Debug, Clone, PartialEq)]
    pub enum ConfigEditField {
        Autocompact,       // RadioGroup: 开/关
        CompactThreshold,  // InputField: 数字字符串（"85"）
        Language,          // InputField
        Persona,           // InputField
        Tone,              // InputField
        Proactiveness,     // RadioGroup: low/medium/high
    }

    impl ConfigEditField {
        // next()/prev() 循环导航链
        // label() 返回字段显示标签
    }

    pub struct ConfigPanel {
        pub mode: ConfigPanelMode,
        pub cursor: usize,        // Browse 模式当前选中字段索引（0-5）
        pub edit_field: ConfigEditField,
        // 编辑缓冲区
        pub buf_autocompact: bool,
        pub buf_threshold: String,    // "85" 形式的字符串
        pub cur_threshold: usize,
        pub buf_language: String,
        pub cur_language: usize,
        pub buf_persona: String,
        pub cur_persona: usize,
        pub buf_tone: String,
        pub cur_tone: usize,
        pub buf_proactiveness: String, // "low" / "medium" / "high"
        pub scroll_offset: u16,
    }
    ```
  - 实现 `from_config(cfg: &ZenConfig) -> Self`：从 AppConfig + CompactConfig 读取当前值填充缓冲区
  - 实现 `enter_edit(&mut self)`：Browse → Edit，保持当前值
  - 实现 `field_next()` / `field_prev()`：循环导航
  - 实现 `active_field() -> Option<(&mut String, &mut usize)>`：返回当前可编辑字段的 buf/cursor（Autocompact 和 Proactiveness 返回 None，用 Space 循环切换）
  - 实现 `cycle_autocompact(&mut self)`：切换 buf_autocompact 布尔值
  - 实现 `cycle_proactiveness(&mut self)`：在 "low"/"medium"/"high" 间循环
  - 实现 `paste_text(&mut self, text: &str)`：过滤换行符粘贴到当前活动字段
  - 实现 `apply_edit(&mut self, cfg: &mut ZenConfig)`：将缓冲区值写回 ZenConfig
    - `buf_autocompact` → `cfg.config.compact.get_or_insert_with(Default::default).auto_compact_enabled`
    - `buf_threshold` → 解析为 u8，clamp 50-99，转 f64/100 → `cfg.config.compact.auto_compact_threshold`
    - `buf_language` → `cfg.config.language = Some(v)` 或 None（空/auto）
    - `buf_persona` → `cfg.config.persona = Some(v)` 或 None（空）
    - `buf_tone` → `cfg.config.tone = Some(v)` 或 None（空）
    - `buf_proactiveness` → `cfg.config.proactiveness = Some(v)` 或 None（空/medium）
  - 实现 `field_count() -> usize` 返回 6（Browse 模式下用于光标循环边界）
  - 实现 `field_label(index: usize) -> &'static str` 和 `field_display_value(index: usize) -> String`（Browse 模式下显示当前值）
  - 原因: 参考 LoginPanel 的 active_field/field_next 模式，保证交互一致性

- [x] 创建 ConfigCommand
  - 新建文件: `rust-agent-tui/src/command/config.rs`
  - 参考 `command/login.rs` 的结构：
    ```rust
    use super::Command;
    use crate::app::App;

    pub struct ConfigCommand;

    impl Command for ConfigCommand {
        fn name(&self) -> &str { "config" }

        fn aliases(&self) -> Vec<&str> { vec!["settings"] }

        fn description(&self) -> &str {
            "全局配置（autocompact、语言、系统提示词覆盖）"
        }

        fn execute(&self, app: &mut App, _args: &str) {
            app.open_config_panel();
        }
    }
    ```
  - 原因: aliases vec 依赖 Task 1 的 trait 扩展，但 aliases 默认实现返回空 Vec，Task 1 未完成时不影响编译

- [x] 创建 ConfigPanel 渲染函数
  - 新建文件: `rust-agent-tui/src/ui/main_ui/panels/config.rs`
  - 在 `panels/mod.rs` 添加 `pub mod config;`
  - 参考 `panels/login.rs` 的渲染模式
  - 实现 `pub(crate) fn render_config_panel(f: &mut Frame, app: &App, area: Rect)`
  - Browse 模式渲染：
    - 标题 " /config — 配置 "，border_color = theme::BORDER
    - 6 行字段，每行格式：`  {label}  {value}`，cursor 处的字段加 ❯ 前缀和高亮
    - Autocompact 显示 "开"/"关"
    - Threshold 显示百分比字符串
    - Proactiveness 显示当前值
  - Edit 模式渲染：
    - 标题 " /config — 编辑配置 "，border_color = theme::WARNING
    - 6 行字段表单，活跃字段高亮
    - Autocompact 字段：显示 `[开]  关` 或 `开  [关]`，Space 切换
    - Proactiveness 字段：显示 `[low]  medium  high` 格式，Space 循环
    - InputField 字段：显示 `{before}█{after}` 光标块，参考 login.rs 的 edit_display_parts
  - 原因: 渲染逻辑与 LoginPanel 一致，区分 Browse 展示和 Edit 编辑两种视觉状态

- [x] 为 AppConfig 新增字段编写序列化/反序列化单元测试
  - 测试文件: `rust-agent-tui/src/config/types.rs` 的 `#[cfg(test)] mod tests` 块
  - 测试场景:
    - `test_app_config_new_fields_optional`: 缺失字段时 4 个新字段为 None
    - `test_app_config_language_serde_roundtrip`: language Some("zh-CN") 的序列化/反序列化往返
    - `test_app_config_proactiveness_serde_roundtrip`: proactiveness "low" 的往返
    - `test_app_config_persona_tone_skip_when_none`: persona/tone 为 None 时不序列化
  - 运行命令: `cargo test -p rust-agent-tui --lib -- config::types::tests::test_app_config_new`
  - 预期: 所有测试通过

- [x] 为 ConfigPanel 状态逻辑编写单元测试
  - 测试文件: `rust-agent-tui/src/app/config_panel.rs` 的 `#[cfg(test)] mod tests` 块
  - 测试场景:
    - `test_config_panel_from_config_defaults`: 从默认 ZenConfig 初始化，autocompact=true, threshold="85", language="", proactiveness="medium"
    - `test_config_panel_field_navigation`: field_next/prev 循环 6 个字段
    - `test_config_panel_cycle_autocompact`: true → false → true
    - `test_config_panel_cycle_proactiveness`: low → medium → high → low
    - `test_config_panel_apply_edit_saves_to_config`: 编辑后 apply_edit 写入 ZenConfig，验证 cfg.config.language/persona/tone/proactiveness 正确
    - `test_config_panel_apply_edit_compact_threshold`: 输入 "90" 后 apply_edit，验证 compact.auto_compact_threshold == 0.90
    - `test_config_panel_apply_edit_invalid_threshold_clamps`: 输入 "30" 后 apply_edit，阈值 clamp 到 0.50
    - `test_config_panel_active_field_text_editable`: Language/Persona/Tone 字段 active_field 返回 Some，Autocompact/Proactiveness 返回 None
  - 运行命令: `cargo test -p rust-agent-tui --lib -- config_panel::tests`
  - 预期: 所有测试通过

**检查步骤:**

- [x] 验证 AppConfig 新增字段不影响现有序列化兼容性
  - `cargo test -p rust-agent-tui --lib -- config::types::tests`
  - 预期: 所有现有测试 + 新测试通过，无回归

- [x] 验证 ConfigPanel 状态逻辑正确
  - `cargo test -p rust-agent-tui --lib -- config_panel::tests`
  - 预期: 所有 8 个测试通过

- [x] 验证编译通过
  - `cargo build -p rust-agent-tui 2>&1 | tail -5`
  - 预期: 构建成功，无编译错误

- [x] 验证 ConfigCommand 已注册
  - `grep -n 'config::ConfigCommand' rust-agent-tui/src/command/mod.rs`
  - 预期: 输出包含注册行

- [x] 验证 config_panel 模块和渲染模块存在
  - `test -f rust-agent-tui/src/app/config_panel.rs && echo "OK"`
  - `test -f rust-agent-tui/src/ui/main_ui/panels/config.rs && echo "OK"`
  - `test -f rust-agent-tui/src/command/config.rs && echo "OK"`
  - 预期: 三个文件均存在

- [x] 验证 AppCore 包含 config_panel 字段
  - `grep -n 'config_panel' rust-agent-tui/src/app/core.rs`
  - 预期: 包含 `pub config_panel: Option<...>` 和初始化行
---

### Task 3: /cost + /context 状态面板

**背景:**
用户需要查看当前会话的 token 消耗、估算费用和上下文使用率。当前 `AgentComm` 缺少会话开始时间和工具调用次数统计，`TokenTracker` 已暴露所有 pub 字段可直接使用。本 Task 的 StatusPanel 数据来源于 `AgentComm` 和 `App` 的现有字段，无需核心框架改动。

**涉及文件:**
- 新建: `rust-agent-tui/src/app/status_panel.rs`（核心）
- 新建: `rust-agent-tui/src/ui/main_ui/panels/status.rs`（核心）
- 新建: `rust-agent-tui/src/command/cost.rs`（核心）
- 新建: `rust-agent-tui/src/command/context_cmd.rs`（核心）
- 修改: `rust-agent-tui/src/app/agent_comm.rs`（增加 session_start_time 和 tool_call_count 字段）
- 修改: `rust-agent-tui/src/app/agent_ops.rs`（session_start_time 赋值和 tool_call_count 递增）
- 注: app/mod.rs/command/mod.rs 的模块注册由 Task 5 统一执行

**执行步骤:**

- [x] 在 `AgentComm` 中新增会话计时和工具调用计数字段 — 为面板提供数据来源
  - 位置: `rust-agent-tui/src/app/agent_comm.rs:21-55` (AgentComm 结构体)
  - 在 `subagent_depth: u32,` (L54) 之后追加:
    ```rust
    /// 会话开始时间（首次 submit_message 时记录）
    pub session_start_time: Option<std::time::Instant>,
    /// 会话级工具调用次数（统计 ToolStart 事件数）
    pub tool_call_count: u32,
    ```
  - 位置: `rust-agent-tui/src/app/agent_comm.rs:57-79` (Default impl)
  - 在 `subagent_depth: 0,` (L76) 之后追加:
    ```rust
    session_start_time: None,
    tool_call_count: 0,
    ```

- [x] 在 `submit_message` 中记录会话开始时间 — 首次提交时启动计时
  - 位置: `rust-agent-tui/src/app/agent_ops.rs`，找到 `submit_message` 方法
  - 在方法体开头（创建 AgentInput 之前）追加:
    ```rust
    if self.agent.session_start_time.is_none() {
        self.agent.session_start_time = Some(std::time::Instant::now());
    }
    ```

- [x] 在 `handle_agent_event` 的 ToolStart 分支递增 `tool_call_count` — 每次工具调用计数
  - 位置: `rust-agent-tui/src/app/agent_ops.rs:376-405` (ToolStart match 分支)
  - 在 `self.agent.retry_status = None;` (L383) 之后追加:
    ```rust
    self.agent.tool_call_count += 1;
    ```

- [x] 创建 `StatusPanel` 状态结构 — 管理面板的 Tab 状态和滚动偏移
  - 新建: `rust-agent-tui/src/app/status_panel.rs`
  - 内容:
    ```rust
    use perihelion_widgets::tab_bar::TabState;

    /// Status 面板 Tab 索引
    pub const STATUS_TAB_COST: usize = 0;
    pub const STATUS_TAB_CONTEXT: usize = 1;

    /// /cost & /context 共用的只读状态面板
    pub struct StatusPanel {
        pub tab: TabState,
        pub scroll_offset: u16,
    }

    impl StatusPanel {
        /// 创建面板并激活指定 Tab
        pub fn new(active_tab: usize) -> Self {
            let mut tab = TabState::new(vec!["Cost".to_string(), "Context".to_string()]);
            tab.set_active(active_tab);
            Self {
                tab,
                scroll_offset: 0,
            }
        }
    }
    ```

- [x] 在 App 结构体中添加 `status_panel` 字段 — 挂载面板状态
  - 位置: `rust-agent-tui/src/app/mod.rs:75-103` (App 结构体定义)
  - 在 `pub mcp_ready_shown_until: std::cell::Cell<Option<std::time::Instant>>,` (L102) 之后追加:
    ```rust
    pub status_panel: Option<status_panel::StatusPanel>,
    ```
  - 位置: `rust-agent-tui/src/app/mod.rs` 顶部模块声明区（L1-11），在 `pub mod agent_panel;` 之后追加:
    ```rust
    pub mod status_panel;
    ```
  - 位置: `rust-agent-tui/src/app/mod.rs` re-export 区（L66-71），追加:
    ```rust
    pub use status_panel::StatusPanel;
    ```
  - 位置: `App::new()` 返回值初始化处（~L164-196），追加:
    ```rust
    status_panel: None,
    ```
  - 位置: `panel_ops.rs` 中 `new_headless` 的 App 构造处（~L300-325），追加:
    ```rust
    status_panel: None,
    ```

- [x] 创建 `/cost` 命令 — 打开 StatusPanel 并激活 Cost Tab
  - 新建: `rust-agent-tui/src/command/cost.rs`
  - 内容:
    ```rust
    use super::Command;
    use crate::app::status_panel::STATUS_TAB_COST;
    use crate::app::App;

    pub struct CostCommand;

    impl Command for CostCommand {
        fn name(&self) -> &str {
            "cost"
        }

        fn description(&self) -> &str {
            "查看当前会话费用和 token 消耗"
        }

        fn execute(&self, app: &mut App, _args: &str) {
            app.status_panel = Some(crate::app::status_panel::StatusPanel::new(STATUS_TAB_COST));
        }
    }
    ```

- [x] 创建 `/context` 命令 — 打开 StatusPanel 并激活 Context Tab
  - 新建: `rust-agent-tui/src/command/context_cmd.rs`
  - 内容:
    ```rust
    use super::Command;
    use crate::app::status_panel::STATUS_TAB_CONTEXT;
    use crate::app::App;

    pub struct ContextCommand;

    impl Command for ContextCommand {
        fn name(&self) -> &str {
            "context"
        }

        fn description(&self) -> &str {
            "查看上下文使用率和会话统计"
        }

        fn execute(&self, app: &mut App, _args: &str) {
            app.status_panel = Some(crate::app::status_panel::StatusPanel::new(STATUS_TAB_CONTEXT));
        }
    }
    ```

- [x] 注册 cost 和 context 命令到默认注册表
  - 位置: `rust-agent-tui/src/command/mod.rs`（L1-10 模块声明区）
  - 在 `pub mod model;` 之后追加:
    ```rust
    pub mod context_cmd;
    pub mod cost;
    ```
  - 位置: `rust-agent-tui/src/command/mod.rs:14-25` (`default_registry` 函数)
  - 在 `r.register(Box::new(mcp::McpCommand));` 之后追加:
    ```rust
    r.register(Box::new(cost::CostCommand));
    r.register(Box::new(context_cmd::ContextCommand));
    ```

- [x] 创建 StatusPanel 渲染模块 — 实现 Cost 和 Context 两个 Tab 的只读展示
  - 新建: `rust-agent-tui/src/ui/main_ui/panels/status.rs`
  - 内容:
    ```rust
    use ratatui::{
        layout::Rect,
        style::{Modifier, Style},
        text::{Line, Span},
        widgets::Paragraph,
        Frame,
    };
    use perihelion_widgets::{BorderedPanel, tab_bar::TabBar};
    use crate::app::status_panel::{STATUS_TAB_COST, STATUS_TAB_CONTEXT};
    use crate::app::App;
    use crate::ui::theme;

    pub(crate) fn render_status_panel(f: &mut Frame, app: &App, area: Rect) {
        let Some(panel) = &app.status_panel else {
            return;
        };

        let inner = BorderedPanel::new(Span::styled(
            " Status ",
            Style::default()
                .fg(theme::THINKING)
                .add_modifier(Modifier::BOLD),
        ))
        .border_style(Style::default().fg(theme::BORDER))
        .render(f, area);

        // Tab 栏（1 行）
        let tab_height = 1u16;
        let tab_area = Rect {
            x: inner.x,
            y: inner.y,
            width: inner.width,
            height: tab_height,
        };
        let content_area = Rect {
            x: inner.x,
            y: inner.y + tab_height + 1, // tab + 空行
            width: inner.width,
            height: inner.height.saturating_sub(tab_height + 1),
        };

        let mut tab_state = panel.tab.clone();
        f.render_stateful_widget(TabBar::new(), tab_area, &mut tab_state);

        let lines = match panel.tab.active() {
            STATUS_TAB_COST => build_cost_lines(app),
            STATUS_TAB_CONTEXT => build_context_lines(app),
            _ => vec![],
        };

        let text = lines.into_iter().collect::<Text>();
        f.render_widget(Paragraph::new(text), content_area);
    }

    fn build_cost_lines(app: &App) -> Vec<Line<'static>> {
        let tracker = &app.agent.session_token_tracker;
        let mut lines: Vec<Line<'static>> = Vec::new();

        // 会话时长
        let duration_str = match app.agent.session_start_time {
            Some(start) => {
                let s = start.elapsed().as_secs();
                if s >= 3600 {
                    format!("{}h{}m{}s", s / 3600, (s % 3600) / 60, s % 60)
                } else if s >= 60 {
                    format!("{}m{}s", s / 60, s % 60)
                } else {
                    format!("{}s", s)
                }
            }
            None => "未开始".to_string(),
        };
        lines.push(label_value("会话时长", &duration_str));
        lines.push(Line::from(""));

        // Token 消耗
        lines.push(label_value("输入 Tokens", &format_number(tracker.total_input_tokens)));
        lines.push(label_value("输出 Tokens", &format_number(tracker.total_output_tokens)));
        lines.push(label_value("Cache 创建", &format_number(tracker.total_cache_creation_tokens)));
        lines.push(label_value("Cache 读取", &format_number(tracker.total_cache_read_tokens)));
        lines.push(Line::from(""));

        // LLM 调用次数
        lines.push(label_value("LLM 调用次数", &tracker.llm_call_count.to_string()));
        lines.push(Line::from(""));

        // 估算费用
        let cost = estimate_cost(app);
        lines.push(label_value("估算费用", &format!("${:.4}", cost)));
        lines.push(Line::from(""));

        // 当前模型
        lines.push(label_value("当前模型", &app.model_name));

        lines
    }

    fn build_context_lines(app: &App) -> Vec<Line<'static>> {
        let tracker = &app.agent.session_token_tracker;
        let context_window = app.agent.context_window;
        let mut lines: Vec<Line<'static>> = Vec::new();

        // 上下文窗口大小
        lines.push(label_value("上下文窗口", &format_number(context_window as u64)));
        lines.push(Line::from(""));

        // 已使用 Token
        let used = tracker.estimated_context_tokens().unwrap_or(0);
        lines.push(label_value("已使用 Token", &format_number(used)));

        // 使用率百分比
        let pct = tracker.context_usage_percent(context_window)
            .map(|p| format!("{:.1}%", p))
            .unwrap_or_else(|| "N/A".to_string());
        lines.push(label_value("使用率", &pct));
        lines.push(Line::from(""));

        // 消息数
        let msg_count = app.agent.agent_state_messages.len();
        lines.push(label_value("消息数", &msg_count.to_string()));

        // 工具调用次数
        lines.push(label_value("工具调用次数", &app.agent.tool_call_count.to_string()));
        lines.push(Line::from(""));

        // Autocompact 阈值
        let compact_config = app.get_compact_config();
        let threshold_pct = (compact_config.auto_compact_threshold * 100.0) as u32;
        lines.push(label_value("Autocompact 阈值", &format!("{}%", threshold_pct)));

        lines
    }

    fn label_value(label: &str, value: &str) -> Line<'static> {
        Line::from(vec![
            Span::styled(
                format!("  {:<16}", label),
                Style::default().fg(theme::MUTED),
            ),
            Span::styled(
                value.to_string(),
                Style::default().fg(theme::TEXT).add_modifier(Modifier::BOLD),
            ),
        ])
    }

    fn format_number(n: u64) -> String {
        if n >= 1_000_000 {
            format!("{:.1}M", n as f64 / 1_000_000.0)
        } else if n >= 1_000 {
            format!("{:.1}K", n as f64 / 1_000.0)
        } else {
            n.to_string()
        }
    }

    /// 基于模型 alias 的简化费用估算
    fn estimate_cost(app: &App) -> f64 {
        let tracker = &app.agent.session_token_tracker;
        let alias = app.zen_config
            .as_ref()
            .map(|c| c.config.active_alias.as_str())
            .unwrap_or("sonnet");

        // 价格表: (input_per_mtok, output_per_mtok) in USD
        let (input_price, output_price) = match alias {
            "opus" => (15.0, 75.0),
            "haiku" => (0.80, 4.0),
            _ => (3.0, 15.0), // sonnet default
        };

        let input_cost = (tracker.total_input_tokens as f64 / 1_000_000.0) * input_price;
        let output_cost = (tracker.total_output_tokens as f64 / 1_000_000.0) * output_price;
        input_cost + output_cost
    }
    ```

- [x] 注册面板渲染模块 — 在 panels/mod.rs 中声明 status 模块
  - 位置: `rust-agent-tui/src/ui/main_ui/panels/mod.rs:1-7`
  - 在文件末尾追加:
    ```rust
    pub mod status;
    ```

- [x] 在主渲染函数中分发 StatusPanel 渲染 — 添加面板到渲染管线
  - 位置: `rust-agent-tui/src/ui/main_ui.rs:94-99`（cron 和 mcp 面板渲染之后）
  - 在 `panels::mcp::render_mcp_panel(f, app, panel_area);` 之后追加:
    ```rust
    if app.status_panel.is_some() {
        panels::status::render_status_panel(f, app, panel_area);
    }
    ```

- [x] 在 `active_panel_height` 中添加 StatusPanel 高度计算 — 固定 14 行（标签栏 1 + 空行 1 + 内容 10 + 边框 2）
  - 位置: `rust-agent-tui/src/ui/main_ui.rs:126-208` (`active_panel_height` 函数)
  - 在 `else if let Some(panel) = &app.mcp_panel { ... }` 分支之后（~L168），`else if let Some(InteractionPrompt::Approval(p))` 分支之前，追加:
    ```rust
    } else if app.status_panel.is_some() {
        14
    ```

- [x] 添加 StatusPanel 事件处理函数 — 处理 ←→ Tab 切换、Esc 关闭
  - 位置: `rust-agent-tui/src/event.rs`
  - 在 `handle_model_panel` 函数之后（~L1134）追加:
    ```rust
    fn handle_status_panel(app: &mut App, input: Input) {
        match input {
            Input { key: Key::Esc, .. } => {
                app.status_panel = None;
            }
            Input { key: Key::Left, .. } => {
                if let Some(panel) = &mut app.status_panel {
                    panel.tab.prev();
                }
            }
            Input { key: Key::Right, .. } => {
                if let Some(panel) = &mut app.status_panel {
                    panel.tab.next();
                }
            }
            _ => {}
        }
    }
    ```

- [x] 在事件分发链中添加 StatusPanel 优先处理 — 面板激活时拦截按键
  - 位置: `rust-agent-tui/src/event.rs` 面板分发区域（~L184-206，model_panel 检查之后）
  - 在 `if app.core.model_panel.is_some() { handle_model_panel(app, input); return ... }` 之后追加:
    ```rust
    // /cost & /context 状态面板优先处理
    if app.status_panel.is_some() {
        handle_status_panel(app, input);
        return Ok(Some(Action::Redraw));
    }
    ```

- [x] 在状态栏快捷键提示中添加 StatusPanel 分支 — 显示 ←→:切换Tab Esc:关闭
  - 位置: `rust-agent-tui/src/ui/main_ui/status_bar.rs:225-273` (`render_second_row` 的 `None` 分支)
  - 在 `app.core.model_panel.is_some()` 分支之后，`app.core.thread_browser` 分支之前，追加:
    ```rust
    } else if app.status_panel.is_some() {
        key!["←→" => ":切换Tab  ", "Esc" => ":关闭"]
    ```

- [x] 为 StatusPanel 编写单元测试
  - 测试文件: `rust-agent-tui/src/ui/headless.rs`（追加测试函数）
  - 测试场景:
    - `/cost` 命令打开面板，默认激活 Cost Tab: `dispatch("/cost")` → `app.status_panel.is_some()` 为 true，`tab.active()` == 0
    - `/context` 命令打开面板，默认激活 Context Tab: `dispatch("/context")` → `tab.active()` == 1
    - Tab 切换: 面板激活时发送 `Key::Right` → `tab.active()` 从 0 变为 1
    - Esc 关闭: 面板激活时发送 `Key::Esc` → `app.status_panel` 为 None
    - `tool_call_count` 递增: 注入 `ToolStart` 事件后 `app.agent.tool_call_count` == 1
    - `session_start_time` 设置: 调用 `submit_message` 后 `app.agent.session_start_time.is_some()` 为 true
  - 运行命令: `cargo test -p rust-agent-tui --lib -- status_panel`
  - 预期: 所有测试通过

**检查步骤:**
- [x] 验证新增文件存在且结构正确
  - `ls rust-agent-tui/src/app/status_panel.rs rust-agent-tui/src/ui/main_ui/panels/status.rs rust-agent-tui/src/command/cost.rs rust-agent-tui/src/command/context_cmd.rs`
  - 预期: 4 个文件均存在
- [x] 验证编译通过
  - `cargo build -p rust-agent-tui 2>&1 | tail -5`
  - 预期: 输出包含 "Finished" 且无 error
- [x] 验证命令注册正确
  - `grep -n 'cost::CostCommand\|context_cmd::ContextCommand' rust-agent-tui/src/command/mod.rs`
  - 预期: 两行注册语句存在
- [x] 验证 AgentComm 新字段存在
  - `grep -n 'session_start_time\|tool_call_count' rust-agent-tui/src/app/agent_comm.rs`
  - 预期: 结构体定义和 Default 初始化各出现 2 次
- [x] 验证面板渲染分发存在
  - `grep -n 'status_panel\|render_status_panel' rust-agent-tui/src/ui/main_ui.rs`
  - 预期: 渲染分支和高度计算分支均存在
- [x] 验证事件处理链存在
  - `grep -n 'handle_status_panel' rust-agent-tui/src/event.rs`
  - 预期: 函数定义和调用各 1 处
- [x] 验证测试通过
  - `cargo test -p rust-agent-tui --lib -- status_panel 2>&1 | tail -10`
  - 预期: 所有测试通过

**认知变更:**
- [x] [CLAUDE.md] [TECH-DEBT] 费用估算使用硬编码价格表（opus/sonnet/haiku 三档），用户使用自定义模型名称时费用显示为 sonnet 价格。未来应支持从配置文件读取自定义价格或从 API 获取实时定价。

---

### Task 4: /memory 记忆文件面板

**背景:**
用户需要通过 TUI 编辑项目级 `{cwd}/CLAUDE.md` 和用户级 `~/.claude/CLAUDE.md` memory 文件，替代手动寻找路径并用编辑器打开。当前无任何面板提供此功能。MemoryPanel 为简单的 2 条目列表面板，核心交互是调用 `$EDITOR` 打开文件（文件不存在时先创建空文件），TUI 暂时挂起，编辑完成后恢复。本 Task 的输出被 Task 5 集成到事件处理链和状态栏快捷键。

**涉及文件:**
- 新建: `rust-agent-tui/src/app/memory_panel.rs`（核心）
- 新建: `rust-agent-tui/src/ui/main_ui/panels/memory.rs`（核心）
- 新建: `rust-agent-tui/src/command/memory.rs`（核心）
- 注: app/mod.rs/command/mod.rs/panel_ops.rs 的集成修改由 Task 5 统一执行

**执行步骤:**

- [x] 创建 MemoryPanel 状态结构体 — 管理面板的条目列表、光标和滚动偏移
  - 新建文件: `rust-agent-tui/src/app/memory_panel.rs`
  - 参考 `app/cron_state.rs`（CronPanel 结构）的 cursor + scroll_offset 模式
  - 内容:
    ```rust
    use std::path::PathBuf;

    /// Memory 文件条目
    #[derive(Debug, Clone)]
    pub struct MemoryEntry {
        pub label: String,
        pub path: PathBuf,
        pub exists: bool,
    }

    /// /memory 面板状态
    #[derive(Debug, Clone)]
    pub struct MemoryPanel {
        pub entries: Vec<MemoryEntry>,
        pub cursor: usize,
        pub scroll_offset: u16,
    }

    impl MemoryPanel {
        /// 根据 cwd 和 home 目录创建面板，自动检测文件是否存在
        pub fn new(cwd: &str, home_dir: Option<PathBuf>) -> Self {
            let project_path = PathBuf::from(cwd).join("CLAUDE.md");
            let global_path = home_dir
                .unwrap_or_else(|| PathBuf::from("/"))
                .join(".claude")
                .join("CLAUDE.md");

            let entries = vec![
                MemoryEntry {
                    label: "项目说明".to_string(),
                    path: project_path,
                    exists: false, // 延迟到渲染时检查
                },
                MemoryEntry {
                    label: "用户全局".to_string(),
                    path: global_path,
                    exists: false,
                },
            ];

            Self {
                entries,
                cursor: 0,
                scroll_offset: 0,
            }
        }

        /// 刷新所有条目的 exists 状态
        pub fn refresh_exists(&mut self) {
            for entry in &mut self.entries {
                entry.exists = entry.path.exists();
            }
        }

        /// 光标上移
        pub fn move_cursor_up(&mut self) {
            if self.cursor > 0 {
                self.cursor -= 1;
            }
        }

        /// 光标下移
        pub fn move_cursor_down(&mut self) {
            if self.cursor < self.entries.len() - 1 {
                self.cursor += 1;
            }
        }
    }
    ```
  - 原因: 固定 2 条目列表，cursor 仅在 0-1 间切换，refresh_exists 在面板打开和 Enter 之前调用确保文件存在状态最新

- [x] 在 app/mod.rs 注册 memory_panel 模块并添加 App 字段
  - 位置: `rust-agent-tui/src/app/mod.rs` L1-10 的 pub mod 区域，在 `pub mod login_panel;` 之后添加 `pub mod memory_panel;`
  - 位置: `rust-agent-tui/src/app/mod.rs` re-export 区（~L66-71），追加:
    ```rust
    pub use memory_panel::MemoryPanel;
    ```
  - 位置: `rust-agent-tui/src/app/mod.rs:75-103` (App 结构体定义)，在 `pub status_panel: Option<status_panel::StatusPanel>,` 之后追加:
    ```rust
    /// /memory 记忆文件面板状态
    pub memory_panel: Option<MemoryPanel>,
    ```
  - 位置: `App::new()` 返回值初始化处（~L164-196），追加:
    ```rust
    memory_panel: None,
    ```
  - 位置: `panel_ops.rs` 中 `new_headless` 的 App 构造处（~L300-325），在 `mcp_ready_shown_until: ...` 之后追加:
    ```rust
    memory_panel: None,
    ```
  - 原因: 与 status_panel 保持一致的 Option<T> 模式，MemoryPanel 挂在 App 层级（需要 cwd 和 home_dir 信息）

- [x] 创建 MemoryCommand — `/memory` 命令实现
  - 新建文件: `rust-agent-tui/src/command/memory.rs`
  - 内容:
    ```rust
    use super::Command;
    use crate::app::{App, MemoryPanel};

    pub struct MemoryCommand;

    impl Command for MemoryCommand {
        fn name(&self) -> &str {
            "memory"
        }

        fn description(&self) -> &str {
            "编辑用户/项目级 CLAUDE.md 记忆文件"
        }

        fn execute(&self, app: &mut App, _args: &str) {
            let home_dir = dirs_next::home_dir();
            let mut panel = MemoryPanel::new(&app.cwd, home_dir);
            panel.refresh_exists();
            app.memory_panel = Some(panel);
        }
    }
    ```
  - 原因: 参考 CronCommand 的结构，execute 直接创建面板并设置到 App 字段；使用 `dirs_next::home_dir()`（已在 `app/mod.rs:148` 中使用）

- [x] 注册 MemoryCommand 到 default_registry
  - 位置: `rust-agent-tui/src/command/mod.rs` L1-10 的 pub mod 区域，在 `pub mod mcp;` 之后添加:
    ```rust
    pub mod memory;
    ```
  - 位置: `rust-agent-tui/src/command/mod.rs:14-25` (`default_registry` 函数)，在 `r.register(Box::new(mcp::McpCommand));` 之后追加:
    ```rust
    r.register(Box::new(memory::MemoryCommand));
    ```

- [x] 在 App 中添加 MemoryPanel 操作方法 — 光标移动、打开编辑器、关闭
  - 位置: `rust-agent-tui/src/app/panel_ops.rs`，在 CronPanel 操作区域之后添加 MemoryPanel 操作区域
  - 实现 4 个方法（均在 `impl App` 中）：
    1. `memory_panel_move_up(&mut self)`: cursor 上移
       ```rust
       pub fn memory_panel_move_up(&mut self) {
           if let Some(ref mut panel) = self.memory_panel {
               panel.move_cursor_up();
           }
       }
       ```
    2. `memory_panel_move_down(&mut self)`: cursor 下移
       ```rust
       pub fn memory_panel_move_down(&mut self) {
           if let Some(ref mut panel) = self.memory_panel {
               panel.move_cursor_down();
           }
       }
       ```
    3. `memory_panel_open_editor(&mut self)`: 打开外部编辑器
       ```rust
       pub fn memory_panel_open_editor(&mut self) -> anyhow::Result<()> {
           let entry = self.memory_panel.as_ref()
               .and_then(|p| p.entries.get(p.cursor))
               .cloned();
           let Some(entry) = entry else {
               return Ok(());
           };

           // 文件不存在时创建空文件
           if !entry.path.exists() {
               if let Some(parent) = entry.path.parent() {
                   std::fs::create_dir_all(parent)?;
               }
               std::fs::File::create(&entry.path)?;
               // 刷新面板中的 exists 状态
               if let Some(ref mut panel) = self.memory_panel {
                   panel.refresh_exists();
               }
           }

           let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());
           tracing::info!("Opening memory file with {}: {:?}", editor, entry.path);

           // 挂起 TUI: 离开 alternate screen + 恢复 raw mode
           crossterm::execute!(
               std::io::stdout(),
               crossterm::terminal::LeaveAlternateScreen
           )?;
           crossterm::terminal::disable_raw_mode()?;

           // 启动编辑器
           let status = std::process::Command::new(&editor)
               .arg(&entry.path)
               .status();

           // 恢复 TUI: 重新进入 alternate screen + raw mode
           crossterm::terminal::enable_raw_mode()?;
           crossterm::execute!(
               std::io::stdout(),
               crossterm::terminal::EnterAlternateScreen
           )?;

           match status {
               Ok(s) if s.success() => {
                   tracing::info!("Editor exited successfully");
               }
               Ok(s) => {
                   tracing::warn!("Editor exited with status: {}", s);
               }
               Err(e) => {
                   tracing::error!("Failed to launch editor: {}", e);
               }
           }

           Ok(())
       }
       ```
    4. `memory_panel_close(&mut self)`: 关闭面板
       ```rust
       pub fn memory_panel_close(&mut self) {
           self.memory_panel = None;
       }
       ```
  - 原因: `memory_panel_open_editor` 使用 crossterm 的 `LeaveAlternateScreen` / `EnterAlternateScreen` 挂起/恢复 TUI，编辑器进程同步阻塞当前事件循环，编辑完成后 TUI 自动恢复；参考 CLAUDE.md 中 "TUI 暂时挂起（需恢复 alternate screen）" 的要求

- [x] 创建 MemoryPanel 渲染模块 — 实现 2 条目列表展示
  - 新建文件: `rust-agent-tui/src/ui/main_ui/panels/memory.rs`
  - 在 `panels/mod.rs` 添加 `pub mod memory;`
  - 参考 `panels/cron.rs` 的 BorderedPanel + ScrollableArea 模式
  - 实现 `pub(crate) fn render_memory_panel(f: &mut Frame, app: &mut App, area: Rect)`:
    ```rust
    use ratatui::{
        layout::Rect,
        style::{Modifier, Style},
        text::{Line, Span, Text},
        Frame,
    };
    use perihelion_widgets::{BorderedPanel, ScrollState, ScrollableArea};
    use crate::app::App;
    use crate::ui::theme;

    pub(crate) fn render_memory_panel(f: &mut Frame, app: &mut App, area: Rect) {
        let Some(panel) = &app.memory_panel else {
            return;
        };

        let title = " Memory 文件 ";
        let inner = BorderedPanel::new(Span::styled(
            title,
            Style::default()
                .fg(theme::THINKING)
                .add_modifier(Modifier::BOLD),
        ))
        .border_style(Style::default().fg(theme::BORDER))
        .render(f, area);

        let mut lines: Vec<Line> = Vec::new();

        for (i, entry) in panel.entries.iter().enumerate() {
            let is_cursor = i == panel.cursor;
            let cursor_char = if is_cursor { "❯ " } else { "  " };

            let style = if is_cursor {
                Style::default()
                    .fg(theme::THINKING)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme::TEXT)
            };

            let exist_icon = if entry.exists {
                ("✓", Style::default().fg(theme::SAGE))
            } else {
                ("✗", Style::default().fg(theme::MUTED))
            };

            let path_str = entry.path.to_string_lossy();
            let path_display = if path_str.len() > 40 {
                format!("...{}", &path_str[path_str.len() - 37..])
            } else {
                path_str.to_string()
            };

            lines.push(Line::from(vec![
                Span::styled(cursor_char.to_string(), Style::default().fg(theme::THINKING)),
                Span::styled(format!("[{}] ", exist_icon.0), exist_icon.1),
                Span::styled(format!("{:<8} ", entry.label), style),
                Span::styled(path_display, Style::default().fg(theme::MUTED)),
            ]));

            // 文件不存在时显示创建提示
            if !entry.exists && is_cursor {
                lines.push(Line::from(Span::styled(
                    "    按 Enter 创建并编辑",
                    Style::default().fg(theme::MUTED),
                )));
            }
        }

        // 存储面板元数据
        app.core.panel_area = Some(inner);
        app.core.panel_scroll_offset = panel.scroll_offset;
        app.core.panel_plain_lines = lines
            .iter()
            .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect())
            .collect();

        let mut scroll_state = ScrollState::with_offset(panel.scroll_offset);
        ScrollableArea::new(Text::from(lines))
            .scrollbar_style(Style::default().fg(theme::MUTED))
            .render(f, inner, &mut scroll_state);
    }
    ```
  - 原因: 参考 cron.rs 的渲染结构：BorderedPanel 包裹、cursor 高亮、ScrollableArea 滚动、panel_area 元数据存储；文件不存在且 cursor 在该行时额外显示创建提示行

- [x] 在面板互斥逻辑中纳入 memory_panel — 打开 memory_panel 时关闭其他面板
  - 位置: `rust-agent-tui/src/app/panel_ops.rs`
  - 在 `open_config_panel()`（如有）、`open_model_panel()`、`open_login_panel()` 的互斥关闭列表中追加 `self.memory_panel = None;`
  - MemoryCommand 的 execute 已直接设置 `app.memory_panel = Some(...)`，无需额外 open 方法中的互斥关闭（各面板打开时需关闭 memory_panel）
  - 原因: 面板互斥，同一时刻只允许一个面板激活

- [x] 为 MemoryPanel 状态逻辑编写单元测试
  - 测试文件: `rust-agent-tui/src/app/memory_panel.rs` 的 `#[cfg(test)] mod tests` 块
  - 测试场景:
    - `test_memory_panel_new_entries`: `MemoryPanel::new("/test/project", Some(PathBuf::from("/home/user")))` → entries.len() == 2，label 分别为 "项目说明"/"用户全局"，path 正确拼接
    - `test_memory_panel_cursor_navigation`: 初始 cursor == 0，move_cursor_down → cursor == 1，move_cursor_down → cursor == 1（不再下移），move_cursor_up → cursor == 0，move_cursor_up → cursor == 0（不再上移）
    - `test_memory_panel_refresh_exists`: 创建临时文件，refresh_exists 后对应 entry.exists == true
  - 运行命令: `cargo test -p rust-agent-tui --lib -- memory_panel::tests`
  - 预期: 所有测试通过

- [x] 为 MemoryPanel 渲染和命令注册编写 headless 集成测试
  - 测试文件: `rust-agent-tui/src/ui/headless.rs`（追加测试函数）
  - 测试场景:
    - `/memory` 命令打开面板: `dispatch("/memory")` → `app.memory_panel.is_some()` 为 true，entries.len() == 2
    - 面板渲染显示两个条目: 设置 `app.memory_panel = Some(panel)`，draw 后 snapshot 包含 "项目说明" 和 "用户全局"
  - 运行命令: `cargo test -p rust-agent-tui --lib -- memory`
  - 预期: 所有测试通过

**检查步骤:**

- [x] 验证新增文件存在且结构正确
  - `ls rust-agent-tui/src/app/memory_panel.rs rust-agent-tui/src/ui/main_ui/panels/memory.rs rust-agent-tui/src/command/memory.rs`
  - 预期: 3 个文件均存在

- [x] 验证编译通过
  - `cargo build -p rust-agent-tui 2>&1 | tail -5`
  - 预期: 输出包含 "Finished" 且无 error

- [x] 验证命令注册正确
  - `grep -n 'memory::MemoryCommand' rust-agent-tui/src/command/mod.rs`
  - 预期: 注册行存在

- [x] 验证 App 结构体包含 memory_panel 字段
  - `grep -n 'memory_panel' rust-agent-tui/src/app/mod.rs`
  - 预期: 模块声明、re-export、App 字段、new() 初始化共 4 处

- [x] 验证 panel_ops 包含 memory_panel 操作方法
  - `grep -n 'memory_panel_' rust-agent-tui/src/app/panel_ops.rs`
  - 预期: move_up/move_down/open_editor/close 共 4 个方法

- [x] 验证测试通过
  - `cargo test -p rust-agent-tui --lib -- memory_panel::tests 2>&1 | tail -10`
  - 预期: 所有 3 个单元测试通过

- [x] 验证已有测试无回归
  - `cargo test -p rust-agent-tui 2>&1 | tail -5`
  - 预期: 输出包含 `test result: ok`，无 failed

**认知变更:**
- [x] [CLAUDE.md] MemoryPanel 的 `memory_panel_open_editor()` 使用 crossterm 的 `LeaveAlternateScreen` / `EnterAlternateScreen` 挂起/恢复 TUI，编辑器进程同步阻塞事件循环。新增面板需要面板挂起/恢复场景时参考此模式。
---

### Task 5: 集成——事件处理、状态栏、面板渲染

**背景:**
Task 2（ConfigPanel）、Task 3（StatusPanel）、Task 4（MemoryPanel）各自创建了面板状态结构体、渲染函数和命令，但尚未接入主事件循环和渲染管线。当前 `event.rs` 的按键分发链按 thread_browser → cron → mcp → agent → login → model → ask_user → hitl → 主输入区 的优先级处理，缺少新面板的拦截。`main_ui.rs` 的渲染分发和 `active_panel_height` 同样缺少新面板分支。`status_bar.rs` 的快捷键提示未覆盖新面板。本 Task 将三个新面板完整接入这些管线，确保面板互斥、事件优先级正确、渲染高度正确、状态栏提示完整。

**涉及文件:**
- 修改: `rust-agent-tui/src/event.rs` — 添加 config/status/memory 面板事件处理函数和分发拦截
- 修改: `rust-agent-tui/src/ui/main_ui/status_bar.rs` — 添加新面板快捷键提示分支
- 修改: `rust-agent-tui/src/ui/main_ui.rs` — 添加面板渲染分发和高度计算
- 修改: `rust-agent-tui/src/app/panel_ops.rs` — 添加新面板 open/close/apply 操作方法
- 修改: `rust-agent-tui/src/app/mod.rs` — 添加模块声明和 re-export
- 修改: `rust-agent-tui/src/ui/main_ui/panels/mod.rs` — 添加模块声明
- 修改: `rust-agent-tui/src/app/core.rs` — 添加 config_panel 字段（如 Task 2 未在此文件操作）
- 修改: `rust-agent-tui/src/app/agent_comm.rs` — 添加 session_start_time 和 tool_call_count 字段（如 Task 3 未在此文件操作）
- 修改: `rust-agent-tui/src/app/agent_ops.rs` — session_start_time 赋值和 tool_call_count 递增（如 Task 3 未在此文件操作）

**执行步骤:**

- [x] 在 `app/mod.rs` 中声明新面板模块 — 注册 Task 2/3/4 创建的模块
  - 位置: `rust-agent-tui/src/app/mod.rs` L1-10 的 pub mod 区域
  - 在 `pub mod model_panel;` 之后追加:
    ```rust
    pub mod config_panel;
    pub mod status_panel;
    pub mod memory_panel;
    ```
  - 原因: Task 2/3/4 创建了这三个模块文件，需在 mod.rs 中声明才能被其他模块引用

- [x] 在 `app/mod.rs` 中添加 re-export — 供外部模块直接使用
  - 位置: `rust-agent-tui/src/app/mod.rs` L66-71 的 re-export 区域
  - 在 `pub use mcp_panel::{McpPanel, McpPanelView};` 之后追加:
    ```rust
    pub use config_panel::ConfigPanel;
    pub use status_panel::StatusPanel;
    pub use memory_panel::MemoryPanel;
    ```
  - 原因: 保持与 AgentPanel/ModelPanel/CronPanel 一致的 re-export 模式

- [x] 在 `App` 结构体中添加新面板字段 — 挂载面板状态
  - 位置: `rust-agent-tui/src/app/mod.rs` L75-103 (App 结构体)
  - 在 `pub mcp_panel: Option<McpPanel>,` (L100) 之后、`pub mcp_ready_shown_until` (L102) 之前追加:
    ```rust
    pub config_panel: Option<ConfigPanel>,
    pub status_panel: Option<StatusPanel>,
    pub memory_panel: Option<MemoryPanel>,
    ```
  - 原因: ConfigPanel 和 MemoryPanel 挂在 App 层级（与 McpPanel 一致），StatusPanel 同理

- [x] 在 `App::new()` 和 `new_headless()` 初始化中添加新字段
  - 位置: `rust-agent-tui/src/app/mod.rs` `App::new()` 返回值初始化（~L164-196）
  - 在 `mcp_panel: None,` 之后追加:
    ```rust
    config_panel: None,
    status_panel: None,
    memory_panel: None,
    ```
  - 位置: `rust-agent-tui/src/app/panel_ops.rs` `new_headless()` 的 App 构造处（~L300-325）
  - 在 `mcp_panel: None,` 之后追加同样三行
  - 原因: 所有面板字段初始化为 None（未激活状态），headless 测试需要完整字段

- [x] 在 `AppCore` 中添加 `config_panel` 字段（如 Task 2 未完成此操作）
  - 位置: `rust-agent-tui/src/app/core.rs` AppCore 结构体（~L21-68）
  - 在 `pub agent_panel: Option<AgentPanel>,` (L43) 之后追加:
    ```rust
    pub config_panel: Option<crate::app::config_panel::ConfigPanel>,
    ```
  - 在 `AppCore::new()` 初始化列表（~L105-121）的 `agent_panel: None,` 之后追加:
    ```rust
    config_panel: None,
    ```
  - 原因: ConfigPanel 与 LoginPanel/ModelPanel/AgentPanel 同属 AppCore 层级

- [x] 在 `AgentComm` 中添加 `session_start_time` 和 `tool_call_count` 字段（如 Task 3 未完成此操作）
  - 位置: `rust-agent-tui/src/app/agent_comm.rs:21-55` (AgentComm 结构体)
  - 在 `pub subagent_depth: u32,` (L54) 之后追加:
    ```rust
    pub session_start_time: Option<std::time::Instant>,
    pub tool_call_count: u32,
    ```
  - 位置: `Default` impl（~L57-79），在 `subagent_depth: 0,` 之后追加:
    ```rust
    session_start_time: None,
    tool_call_count: 0,
    ```
  - 原因: session_start_time 用于 /cost 面板显示会话时长，tool_call_count 用于 /context 面板显示工具调用次数

- [x] 在 `submit_message` 中初始化 `session_start_time` — 首次提交时记录时间
  - 位置: `rust-agent-tui/src/app/agent_ops.rs` `submit_message` 方法（~L21-48）
  - 在 `self.agent.task_start_time = Some(std::time::Instant::now());` (L47) 之后追加:
    ```rust
    if self.agent.session_start_time.is_none() {
        self.agent.session_start_time = Some(std::time::Instant::now());
    }
    ```
  - 原因: session_start_time 跨任务保留（整个会话生命周期），与 task_start_time（单次任务）不同

- [x] 在 `handle_agent_event` 的 `ToolStart` 分支递增 `tool_call_count`
  - 位置: `rust-agent-tui/src/app/agent_ops.rs:376-405` (ToolStart match 分支)
  - 在 `self.agent.retry_status = None;` (L383) 之后追加:
    ```rust
    self.agent.tool_call_count += 1;
    ```
  - 原因: 每次 ToolStart 事件对应一次工具调用，用于 /context 面板统计

- [x] 在 `panels/mod.rs` 中声明新渲染模块
  - 位置: `rust-agent-tui/src/ui/main_ui/panels/mod.rs:1-7`
  - 在文件末尾追加:
    ```rust
    pub mod config;
    pub mod status;
    pub mod memory;
    ```
  - 原因: Task 2/3/4 创建的渲染模块需在此声明才能被 main_ui.rs 引用

- [x] 在 `panel_ops.rs` 中添加 ConfigPanel 操作方法
  - 位置: `rust-agent-tui/src/app/panel_ops.rs`，在 Login 面板操作区域之后（~L173）
  - 添加 Config 面板操作区域:
    ```rust
    // ─── Config 面板操作 ───────────────────────────────────────────────────────

    /// 打开 /config 面板
    pub fn open_config_panel(&mut self) {
        let cfg = self.zen_config.get_or_insert_with(ZenConfig::default);
        self.core.config_panel = Some(config_panel::ConfigPanel::from_config(cfg));
        // 互斥：关闭其他面板
        self.core.login_panel = None;
        self.core.model_panel = None;
        self.status_panel = None;
        self.memory_panel = None;
    }

    /// 关闭 /config 面板
    pub fn close_config_panel(&mut self) {
        self.core.config_panel = None;
    }

    /// 保存 Config 面板编辑并关闭
    pub fn config_panel_apply(&mut self) {
        let Some(panel) = self.core.config_panel.as_mut() else {
            return;
        };
        let Some(cfg) = self.zen_config.as_mut() else {
            return;
        };
        panel.apply_edit(cfg);
        if let Err(e) = Self::save_config(cfg, self.config_path_override.as_deref()) {
            self.core.view_messages.push(MessageViewModel::system(
                format!("配置保存失败: {}", e),
            ));
        } else {
            self.core.view_messages.push(MessageViewModel::system(
                "配置已保存".to_string(),
            ));
        }
        self.core.config_panel = None;
    }
    ```
  - 原因: 参考 `open_model_panel()` 和 `login_panel_apply_edit()` 的互斥+save_config 模式

- [x] 在 `panel_ops.rs` 中添加 StatusPanel 操作方法
  - 位置: `rust-agent-tui/src/app/panel_ops.rs`，在 Config 面板操作之后
  - 添加 Status 面板操作区域:
    ```rust
    // ─── Status 面板操作 ───────────────────────────────────────────────────────

    /// 打开状态面板并激活指定 Tab
    pub fn open_status_panel(&mut self, tab: usize) {
        self.status_panel = Some(status_panel::StatusPanel::new(tab));
        // 互斥
        self.core.config_panel = None;
        self.core.login_panel = None;
        self.core.model_panel = None;
        self.memory_panel = None;
    }

    /// 关闭状态面板
    pub fn close_status_panel(&mut self) {
        self.status_panel = None;
    }
    ```
  - 原因: /cost 和 /context 命令调用 `open_status_panel(STATUS_TAB_COST)` 或 `open_status_panel(STATUS_TAB_CONTEXT)`

- [x] 在 `panel_ops.rs` 中添加 MemoryPanel 操作方法
  - 位置: `rust-agent-tui/src/app/panel_ops.rs`，在 Status 面板操作之后
  - 添加 Memory 面板操作区域:
    ```rust
    // ─── Memory 面板操作 ───────────────────────────────────────────────────────

    /// 打开 /memory 面板
    pub fn open_memory_panel(&mut self) {
        self.memory_panel = Some(memory_panel::MemoryPanel::new(&self.cwd));
        // 互斥
        self.core.config_panel = None;
        self.core.login_panel = None;
        self.core.model_panel = None;
        self.status_panel = None;
    }

    /// 关闭 /memory 面板
    pub fn close_memory_panel(&mut self) {
        self.memory_panel = None;
    }
    ```
  - 原因: MemoryPanel 从 cwd 和 home 目录扫描 memory 文件

- [x] 更新现有面板的互斥逻辑 — 在 open 方法中关闭新面板
  - 位置: `rust-agent-tui/src/app/panel_ops.rs`
  - 在 `open_model_panel()` (~L7-12) 方法体中，在 `self.core.login_panel = None;` 之后追加:
    ```rust
    self.core.config_panel = None;
    self.status_panel = None;
    self.memory_panel = None;
    ```
  - 在 `open_login_panel()` (~L56-61) 方法体中，在 `self.core.model_panel = None;` 之后追加同样三行
  - 原因: 互斥扩展——打开任何面板时关闭所有新面板，与已有互斥逻辑一致

- [x] 在 `event.rs` 中添加 ConfigPanel 事件处理函数
  - 位置: `rust-agent-tui/src/event.rs`，在 `handle_model_panel` 函数之后（~L1134）
  - 追加:
    ```rust
    fn handle_config_panel(app: &mut App, input: Input) {
        use crate::app::config_panel::{ConfigEditField, ConfigPanelMode};
        let Some(panel) = app.core.config_panel.as_mut() else {
            return;
        };
        match panel.mode {
            ConfigPanelMode::Browse => match input {
                Input { key: Key::Up, .. } => {
                    if panel.cursor > 0 {
                        panel.cursor -= 1;
                    } else {
                        panel.cursor = panel.field_count() - 1;
                    }
                }
                Input { key: Key::Down, .. } => {
                    panel.cursor = (panel.cursor + 1) % panel.field_count();
                }
                Input { key: Key::Enter, .. } => {
                    panel.enter_edit();
                }
                Input { key: Key::Esc, .. } => {
                    app.core.config_panel = None;
                }
                _ => {}
            },
            ConfigPanelMode::Edit => match input {
                Input { key: Key::Esc, .. } => {
                    panel.mode = ConfigPanelMode::Browse;
                }
                Input { key: Key::Enter, .. } => {
                    app.config_panel_apply();
                }
                Input { key: Key::Up, .. } => {
                    panel.field_prev();
                }
                Input { key: Key::Down, .. } => {
                    panel.field_next();
                }
                Input { key: Key::Char(' '), ctrl: false, .. } => {
                    match panel.edit_field {
                        ConfigEditField::Autocompact => panel.cycle_autocompact(),
                        ConfigEditField::Proactiveness => panel.cycle_proactiveness(),
                        _ => {}
                    }
                }
                Input { key: Key::Left, ctrl: false, .. }
                | Input { key: Key::Right, ctrl: false, .. } => {
                    match panel.edit_field {
                        ConfigEditField::Autocompact => panel.cycle_autocompact(),
                        ConfigEditField::Proactiveness => panel.cycle_proactiveness(),
                        _ => {
                            let Some((buf, cursor)) = panel.active_field() else { return };
                            crate::app::handle_edit_key(buf, cursor, input);
                        }
                    }
                }
                _ => {
                    if let Some((buf, cursor)) = panel.active_field() {
                        crate::app::handle_edit_key(buf, cursor, input);
                    }
                }
            },
        }
    }
    ```
  - 原因: Browse 模式 ↑↓ 导航、Enter 进入编辑、Esc 关闭；Edit 模式 ↑↓ 切换字段、Space 切换 RadioGroup、Enter 保存、Esc 取消回 Browse

- [x] 在 `event.rs` 中添加 MemoryPanel 事件处理函数
  - 位置: `rust-agent-tui/src/event.rs`，在 `handle_config_panel` 函数之后
  - 追加:
    ```rust
    fn handle_memory_panel(app: &mut App, input: Input) {
        let Some(panel) = app.memory_panel.as_mut() else {
            return;
        };
        match input {
            Input { key: Key::Up, .. } => {
                if panel.cursor > 0 {
                    panel.cursor -= 1;
                }
            }
            Input { key: Key::Down, .. } => {
                if panel.cursor + 1 < panel.items.len() {
                    panel.cursor += 1;
                }
            }
            Input { key: Key::Enter, .. } => {
                // 标记需要打开编辑器，由 main loop 处理
                panel.pending_edit = true;
            }
            Input { key: Key::Esc, .. } => {
                app.memory_panel = None;
            }
            _ => {}
        }
    }
    ```
  - 原因: MemoryPanel 为简单列表，↑↓ 选择、Enter 打开编辑器、Esc 关闭

- [x] 在 `event.rs` 中添加 StatusPanel 事件处理函数（如 Task 3 未完成此操作）
  - 位置: `rust-agent-tui/src/event.rs`，在 `handle_memory_panel` 函数之后
  - 追加:
    ```rust
    fn handle_status_panel(app: &mut App, input: Input) {
        match input {
            Input { key: Key::Esc, .. } => {
                app.status_panel = None;
            }
            Input { key: Key::Left, .. } => {
                if let Some(panel) = &mut app.status_panel {
                    panel.tab.prev();
                }
            }
            Input { key: Key::Right, .. } => {
                if let Some(panel) = &mut app.status_panel {
                    panel.tab.next();
                }
            }
            _ => {}
        }
    }
    ```
  - 原因: StatusPanel 为只读面板，仅支持 ←→ 切换 Tab 和 Esc 关闭

- [x] 在 `event.rs` 的事件分发链中插入新面板拦截 — 按优先级顺序
  - 位置: `rust-agent-tui/src/event.rs` `next_event` 函数，面板分发区域（~L172-206）
  - 在 `if app.core.model_panel.is_some() { handle_model_panel(app, input); return ... }` (~L203-205) 之后、AskUser 批量弹窗之前（~L208），追加:
    ```rust
    // /config 配置面板优先处理
    if app.core.config_panel.is_some() {
        handle_config_panel(app, input);
        return Ok(Some(Action::Redraw));
    }

    // /cost & /context 状态面板优先处理
    if app.status_panel.is_some() {
        handle_status_panel(app, input);
        return Ok(Some(Action::Redraw));
    }

    // /memory 面板优先处理
    if app.memory_panel.is_some() {
        handle_memory_panel(app, input);
        return Ok(Some(Action::Redraw));
    }
    ```
  - 原因: 新面板在 model 面板之后、AskUser 之前拦截，保证面板互斥时的按键优先级正确

- [x] 在 `Event::Paste` 分支中添加新面板拦截 — 防止粘贴文本穿透到 textarea
  - 位置: `rust-agent-tui/src/event.rs` `Event::Paste` 分支（~L529-564）
  - 在 `// model_panel 打开时拦截粘贴` 块（~L547-550）之后、thread_browser 等面板拦截块（~L552-560）之前，追加:
    ```rust
    // config_panel 打开时粘贴到当前编辑字段
    if app.core.config_panel.is_some() {
        if let Some(panel) = app.core.config_panel.as_mut() {
            panel.paste_text(&text);
        }
        return Ok(Some(Action::Redraw));
    }
    ```
  - 在 thread_browser 等面板拦截块的条件列表中（~L554-557）追加 `app.status_panel.is_some()` 和 `app.memory_panel.is_some()`:
    ```rust
    if app.core.thread_browser.is_some()
        || app.core.agent_panel.is_some()
        || app.cron.cron_panel.is_some()
        || app.mcp_panel.is_some()
        || app.status_panel.is_some()
        || app.memory_panel.is_some()
    ```
  - 原因: config_panel 有文本编辑字段需支持粘贴；status_panel 和 memory_panel 无文本字段，拦截即可

- [x] 在 `main_ui.rs` 渲染分发中添加新面板渲染 — model 面板之后
  - 位置: `rust-agent-tui/src/ui/main_ui.rs:70-100`，面板渲染条件链
  - 在 `panels::model::render_model_panel(f, app, panel_area);` (~L86-87) 之后追加:
    ```rust
    if app.core.config_panel.is_some() {
        panels::config::render_config_panel(f, app, panel_area);
    }
    if app.status_panel.is_some() {
        panels::status::render_status_panel(f, app, panel_area);
    }
    if app.memory_panel.is_some() {
        panels::memory::render_memory_panel(f, app, panel_area);
    }
    ```
  - 原因: 三个面板按 config → status → memory 顺序渲染，与事件处理优先级一致

- [x] 在 `active_panel_height` 中添加新面板高度计算
  - 位置: `rust-agent-tui/src/ui/main_ui.rs:126-208` (`active_panel_height` 函数)
  - 在 `app.core.model_panel.is_some()` 分支（~L143-144）之后追加:
    ```rust
    } else if app.core.config_panel.is_some() {
        // 6 fields * 2 lines + 2 borders = 14
        14
    } else if app.status_panel.is_some() {
        // tab 1 + blank 1 + content 10 + borders 2 = 14
        14
    } else if app.memory_panel.is_some() {
        let items = app.memory_panel.as_ref().map(|p| p.items.len()).unwrap_or(0);
        (items as u16 * 2 + 4).max(6)
    ```
  - 原因: ConfigPanel 6 个字段每行占 2 行（标签+值），StatusPanel 固定 14 行，MemoryPanel 按条目数自适应

- [x] 在 `status_bar.rs` 的 `render_second_row` 中添加新面板快捷键提示
  - 位置: `rust-agent-tui/src/ui/main_ui/status_bar.rs:225-273` (`render_second_row` 的 `None` 分支内部条件链)
  - 在 `app.core.model_panel.is_some()` 分支（~L261-262）之后、`app.core.thread_browser` 分支（~L263）之前，插入三个分支:
    ```rust
    } else if app.core.config_panel.is_some() {
        let panel = app.core.config_panel.as_ref().unwrap();
        match panel.mode {
            crate::app::config_panel::ConfigPanelMode::Browse => {
                key!["↑↓" => ":导航  ", "Enter" => ":编辑  ", "Esc" => ":关闭"]
            }
            crate::app::config_panel::ConfigPanelMode::Edit => {
                key!["↑↓" => ":切换字段  ", "←→/Space" => ":切换  ", "Enter" => ":保存  ", "Ctrl+V" => ":粘贴  ", "Esc" => ":取消"]
            }
        }
    } else if app.status_panel.is_some() {
        key!["←→" => ":切换Tab  ", "↑↓" => ":滚动  ", "Esc" => ":关闭"]
    } else if app.memory_panel.is_some() {
        key!["↑↓" => ":选择  ", "Enter" => ":编辑  ", "Esc" => ":关闭"]
    ```
  - 原因: 遵循面板快捷键显示规范——由状态栏第二行统一负责，面板内部不渲染快捷键

- [x] 更新 `/cost` 和 `/context` 命令使用 panel_ops 方法 — 确保互斥逻辑一致
  - 位置: `rust-agent-tui/src/command/cost.rs`（如 Task 3 创建时直接赋值 `app.status_panel`）
  - 将 `execute` 方法中的直接赋值:
    ```rust
    app.status_panel = Some(crate::app::status_panel::StatusPanel::new(STATUS_TAB_COST));
    ```
    替换为调用 panel_ops 方法:
    ```rust
    app.open_status_panel(STATUS_TAB_COST);
    ```
  - 位置: `rust-agent-tui/src/command/context_cmd.rs`，同样替换为 `app.open_status_panel(STATUS_TAB_CONTEXT);`
  - 原因: `open_status_panel()` 包含互斥关闭其他面板的逻辑，直接赋值会绕过互斥

- [x] 为集成编写 headless 端到端测试 — 验证事件分发 + 渲染 + 互斥
  - 测试文件: `rust-agent-tui/src/ui/headless.rs`（追加测试函数）
  - 测试场景:
    - **ConfigPanel 事件分发**: 打开 config 面板后发送 `Key::Down` → cursor 从 0 变为 1；发送 `Key::Esc` → 面板关闭
    - **ConfigPanel 互斥**: 打开 config 面板后打开 model 面板 → `config_panel` 为 None，`model_panel` 为 Some
    - **StatusPanel Tab 切换**: 打开 /cost 面板后发送 `Key::Right` → `tab.active()` 从 0 变为 1
    - **MemoryPanel 基本交互**: 打开 memory 面板后发送 `Key::Down` → cursor 递增
    - **面板渲染无崩溃**: 打开每个面板后执行 `handle.draw()` → 无 panic
    - **Paste 拦截**: config 面板打开时发送 `Event::Paste` → 粘贴到面板字段而非 textarea
    - **tool_call_count 聚合**: 注入 3 个 ToolStart 事件后 `app.agent.tool_call_count` == 3
    - **session_start_time 生命周期**: 注入 ToolStart + Done 事件后 `session_start_time` 仍为 Some
  - 运行命令: `cargo test -p rust-agent-tui --lib -- headless::integration`
  - 预期: 所有测试通过

**检查步骤:**

- [x] 验证模块声明完整
  - `grep -n 'config_panel\|status_panel\|memory_panel' rust-agent-tui/src/app/mod.rs`
  - 预期: 模块声明、re-export、App 字段各出现对应条目

- [x] 验证事件分发链包含新面板
  - `grep -n 'handle_config_panel\|handle_status_panel\|handle_memory_panel' rust-agent-tui/src/event.rs`
  - 预期: 每个函数有定义和调用各 1 处

- [x] 验证渲染分发包含新面板
  - `grep -n 'config_panel\|status_panel\|memory_panel\|render_config\|render_status\|render_memory' rust-agent-tui/src/ui/main_ui.rs`
  - 预期: 渲染分支和高度计算分支均存在

- [x] 验证状态栏快捷键提示完整
  - `grep -n 'config_panel\|status_panel\|memory_panel' rust-agent-tui/src/ui/main_ui/status_bar.rs`
  - 预期: 三个面板各有快捷键提示分支

- [x] 验证 panel_ops 包含所有新方法
  - `grep -n 'open_config_panel\|open_status_panel\|open_memory_panel\|close_config_panel\|close_status_panel\|close_memory_panel\|config_panel_apply' rust-agent-tui/src/app/panel_ops.rs`
  - 预期: 所有方法均存在

- [x] 验证 AgentComm 新字段存在
  - `grep -n 'session_start_time\|tool_call_count' rust-agent-tui/src/app/agent_comm.rs`
  - 预期: 结构体定义和 Default 初始化各出现 2 次

- [x] 验证 agent_ops 正确更新新字段
  - `grep -n 'session_start_time\|tool_call_count' rust-agent-tui/src/app/agent_ops.rs`
  - 预期: session_start_time 在 submit_message 中赋值，tool_call_count 在 ToolStart 中递增

- [x] 编译通过
  - `cargo build -p rust-agent-tui 2>&1 | tail -5`
  - 预期: 输出包含 "Finished" 且无 error

- [x] 全部测试通过
  - `cargo test -p rust-agent-tui 2>&1 | tail -10`
  - 预期: 输出包含 "test result: ok"，无 failed

**认知变更:**
- [x] [CLAUDE.md] 新增面板（config/status/memory）的事件处理优先级位于 model 面板之后、AskUser 之前。打开任何面板时均需关闭所有其他面板（互斥）。ConfigPanel 挂在 AppCore，StatusPanel 和 MemoryPanel 挂在 App 层级。
- [x] [CLAUDE.md] `session_start_time` 在首次 `submit_message` 时记录，跨任务保留（与 `task_start_time` 不同）；`tool_call_count` 在 `AgentEvent::ToolStart` 时递增，会话期间单调递增不重置。

---

### Task 6: cc-commands-alignment 验收

**前置条件:**
- 启动命令: `cargo run -p rust-agent-tui`
- 确保 `ANTHROPIC_API_KEY` 或 `OPENAI_API_KEY` 已配置

**端到端验证:**

1. 运行完整测试套件确保无回归
   - `cargo test -p rust-agent-tui 2>&1 | tail -10`
   - 预期: 全部测试通过，无 failed
   - 失败排查: 检查各 Task 的测试步骤

2. 验证 Command trait 别名机制
   - 启动 TUI，输入 `/reset` → 消息列表被清空（等效 `/clear`）
   - 输入 `/new` → 同上
   - 输入 `/settings` → 打开配置面板（等效 `/config`）
   - 失败排查: 检查 Task 1 dispatch 逻辑

3. 验证 /config 面板完整流程
   - 输入 `/config` → 打开配置面板，Browse 模式显示 6 个字段当前值
   - ↑↓ 导航，Enter 进入编辑
   - 修改 Persona 为 "Rust 专家"，Enter 保存
   - 检查 `~/.zen-code/settings.json` 中 `persona` 字段已更新
   - 失败排查: 检查 Task 2 ConfigPanel.apply_edit + Task 5 panel_ops

4. 验证 /cost 面板
   - 发送一条消息等待回复后，输入 `/cost`
   - 面板显示会话时长 > 0、Token 消耗各字段有值、估算费用有值、模型名正确
   - ←→ 切换到 Context Tab，显示上下文使用率、消息数、工具调用次数
   - Esc 关闭面板
   - 失败排查: 检查 Task 3 StatusPanel 渲染 + Task 5 集成

5. 验证 /context 面板
   - 输入 `/context` → 直接打开 Context Tab（非 Cost Tab）
   - 显示上下文窗口 200K、已使用 Token、使用率百分比
   - 失败排查: 检查 Task 3 ContextCommand

6. 验证 /memory 面板
   - 输入 `/memory` → 打开面板，显示 "项目说明" 和 "用户全局" 两个条目
   - ↑↓ 导航，Enter 打开编辑器（文件不存在时先创建）
   - 编辑器关闭后 TUI 恢复正常显示
   - 失败排查: 检查 Task 4 memory_panel_open_editor

7. 验证状态栏快捷键提示
   - 打开各面板时状态栏第二行显示对应快捷键
   - Config Browse: `↑↓:导航 Enter:编辑 Esc:关闭`
   - Config Edit: `↑↓:切换字段 ←→/Space:切换 Enter:保存 Ctrl+V:粘贴 Esc:取消`
   - Status: `←→:切换Tab ↑↓:滚动 Esc:关闭`
   - Memory: `↑↓:选择 Enter:编辑 Esc:关闭`
   - 失败排查: 检查 Task 5 status_bar.rs

8. 验证面板互斥
   - 打开 /config 后输入 /model → config 面板关闭，model 面板打开
   - 失败排查: 检查 Task 5 panel_ops 互斥逻辑

9. 验证 /help 显示别名
   - 输入 `/help` → clear 命令行显示 `(别名: /reset, /new)`
   - config 命令行显示 `(别名: /settings)`
   - 失败排查: 检查 Task 1 help.rs 更新
