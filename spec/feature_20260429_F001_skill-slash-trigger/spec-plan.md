# Skills 触发键统一到 / 命名空间 执行计划

**目标:** 将 Skills 触发键从 `#` 改为 `/`，与命令共用统一命名空间，合并提示浮层、补全逻辑和 Enter 触发

**技术栈:** Rust, ratatui TUI, tokio async

**设计文档:** spec/feature_20260429_F001_skill-slash-trigger/spec-design.md

## 改动总览

本次改动涉及 7 个文件，分布在 `rust-agent-tui`（6 文件）和 `rust-agent-middlewares`（1 文件）中。改动按 4 个 Task 分组：Task 1 合并提示浮层渲染，Task 2 合并 Tab 补全逻辑，Task 3 修改 Enter 触发和消息解析，Task 4 更新文案。Task 1 的 `render_unified_hint` 输出统一的候选列表结构，被 Task 2 的 `hint_candidates_count`/`hint_complete` 直接依赖。Task 3 的 Enter 触发逻辑依赖 Task 1 的合并浮层行为（`hint_cursor` 索引到统一候选列表）。Task 4 独立于前三个 Task。

---

### Task 0: 环境准备

**背景:**
确保构建和测试工具链在当前开发环境中可用，避免后续 Task 因环境问题阻塞。

**执行步骤:**
- [x] 验证构建工具可用
  - `cargo build -p rust-agent-tui -p rust-agent-middlewares 2>&1 | tail -5`
  - 预期: 输出包含 `Finished` 且无 error
- [x] 验证测试工具可用
  - `cargo test -p rust-agent-tui --lib -- test_snapshot_row_count 2>&1 | tail -5`
  - 预期: 输出包含 `test result: ok`

**检查步骤:**
- [x] 构建命令执行成功
  - `cargo build -p rust-agent-tui -p rust-agent-middlewares 2>&1 | tail -5`
  - 预期: 构建成功，无错误
- [x] 测试命令可用
  - `cargo test -p rust-agent-tui --lib -- test_snapshot_row_count 2>&1 | tail -5`
  - 预期: 测试框架可用，无配置错误

---

### Task 1: 提示浮层合并

**背景:**
当前 `hints.rs` 中有两个独立的浮层渲染函数：`render_command_hint`（处理 `/` 前缀）和 `render_skill_hint`（处理 `#` 前缀）。用户输入不同前缀触发不同浮层。合并后，输入 `/` 前缀时在同一个浮层中分组展示命令候选和 Skills 候选，`#` 前缀不再触发浮层。`render_unified_hint` 的输出结构（命令组 + Skills 组的扁平索引）将被 Task 2 的 `hint_candidates_count`/`hint_complete` 直接依赖。

**涉及文件:**
- 修改: `rust-agent-tui/src/ui/main_ui/popups/hints.rs`
- 修改: `rust-agent-tui/src/ui/main_ui.rs`

**执行步骤:**

- [x] 在 `hints.rs` 中新增 `render_unified_hint` 函数 — 替代 `render_command_hint` + `render_skill_hint` 的统一浮层
  - 位置: `rust-agent-tui/src/ui/main_ui/popups/hints.rs`，在 `render_command_hint` 函数（~L15-71）之前插入新函数
  - 签名: `pub(crate) fn render_unified_hint(f: &mut Frame, app: &App, input_area: Rect)`
  - 前缀守卫: 仅当 `first_line.starts_with('/')` 时继续执行，否则 return
  - 候选构建逻辑:
    ```rust
    let prefix = first_line.trim_start_matches('/');
    let cmd_candidates: Vec<(&str, &str)> = app.core.command_registry.match_prefix(prefix);
    let cmd_show: Vec<_> = cmd_candidates.into_iter().take(6).collect();
    let skill_candidates: Vec<_> = app.core.skills.iter()
        .filter(|s| prefix.is_empty() || s.name.contains(prefix))
        .take(4)
        .collect();
    let total_count = cmd_show.len() + skill_candidates.len();
    if total_count == 0 { return; }
    ```
  - 浮层高度计算（含分组标题和分隔线）:
    ```rust
    let has_skills = !skill_candidates.is_empty();
    let hint_height = total_count as u16
        + 2 // 边框
        + 1 // "命令" 组标题
        + if has_skills { 2 } else { 0 }; // "Skills" 组标题 + 分隔线
    let y = input_area.y.saturating_sub(hint_height);
    let hint_area = Rect {
        x: input_area.x + 1,
        y,
        width: input_area.width.saturating_sub(2).min(60),
        height: hint_height,
    };
    ```
  - 外层边框使用 `BorderedPanel::new(Span::styled(" / ", ...))`，与原 `render_command_hint` 一致
  - 组标题渲染: 渲染命令行之前插入一组标题行 `Line::from(Span::styled("命令", ...))`，渲染 Skills 行之前插入一组标题行 `Line::from(Span::styled("Skills", ...))`，两组之间使用 `Line::from("─".repeat(width as usize))` 分隔线。标题行不计入 `hint_cursor` 索引（仅候选项计入），与设计文档的分组渲染示例一致
  - 行渲染逻辑: 遍历 `cmd_show` 渲染命令行（前缀 `▸ /` 或 `  /`，与原 `render_command_hint` 一致），然后遍历 `skill_candidates` 渲染 Skills 行（前缀 `▸ /` 或 `  /`，高亮逻辑与原 `render_skill_hint` 的 `name.find(prefix)` 匹配一致，但前缀从 `#` 改为 `/`）
  - 选中索引: `let selected = app.core.hint_cursor;`，直接用扁平索引对比 `i`（命令组索引 0..cmd_show.len()，Skills 组索引 cmd_show.len()..total_count）。注意：标题行和分隔线不计入 `i` 的递增，`i` 仅在候选项行上递增
  - 原因: 将两个独立浮层的渲染逻辑合并到单一函数中，`hint_cursor` 使用扁平索引覆盖两组候选，分组标题帮助用户区分命令和 Skills

- [x] 删除 `render_skill_hint` 函数 — `/` 前缀已覆盖 Skills 候选
  - 位置: `rust-agent-tui/src/ui/main_ui/popups/hints.rs`，删除 ~L74-141 整个 `render_skill_hint` 函数
  - 原因: Skills 候选已合并到 `render_unified_hint` 中，`#` 前缀不再需要独立浮层

- [x] 保留 `render_command_hint` 函数不变 — Task 2/3 完成前保持向后兼容
  - 位置: `rust-agent-tui/src/ui/main_ui/popups/hints.rs`，~L15-71 不做修改
  - 原因: `render_command_hint` 仍被 `main_ui.rs` 的旧调用点使用。Task 1 只新增 `render_unified_hint` 和删除 `render_skill_hint`，调用点的替换留到本 Task 的下一步

- [x] 在 `main_ui.rs` 中替换浮层调用点 — 用 `render_unified_hint` 替代两个旧调用
  - 位置: `rust-agent-tui/src/ui/main_ui.rs`，~L116-117
  - 将:
    ```rust
    popups::hints::render_command_hint(f, app, chunks[4]);
    popups::hints::render_skill_hint(f, app, chunks[4]);
    ```
    替换为:
    ```rust
    popups::hints::render_unified_hint(f, app, chunks[4]);
    ```
  - 原因: 统一浮层已合并两个函数的功能

- [x] 在 `hints.rs` 中删除 `render_command_hint` 函数 — 已无调用点
  - 位置: `rust-agent-tui/src/ui/main_ui/popups/hints.rs`，删除 `render_command_hint` 函数（原 ~L15-71，但在步骤 1 插入 `render_unified_hint` 后行号已下移，按函数名定位删除）
  - 原因: `main_ui.rs` 已改用 `render_unified_hint`，旧函数不再被引用

- [x] 为 `render_unified_hint` 编写 headless 集成测试
  - 测试文件: `rust-agent-tui/src/ui/headless.rs`（在 `mod tests` 块末尾追加）
  - 测试场景:
    - `test_unified_hint_shows_commands_and_skills`: 设置 `app.core.textarea` 内容为 `/`，注册 2 个命令（通过默认 registry）和 2 个 Skills（通过 `app.core.skills.push(SkillMetadata{...})`），调用 `render`，断言快照包含命令名和 Skill 名，且包含 "命令" 和 "Skills" 分组标题
    - `test_unified_hint_filters_by_prefix`: 设置 textarea 内容为 `/mo`，断言快照包含匹配的命令（如 `/model`）且不包含不匹配的 Skill
    - `test_unified_hint_no_result_for_hash`: 设置 textarea 内容为 `#skill`，断言浮层不渲染（无 `Skills` 标题出现）
  - 运行命令: `cargo test -p rust-agent-tui --lib -- test_unified_hint`
  - 预期: 所有测试通过

**检查步骤:**

- [x] 验证 `render_skill_hint` 函数已移除
  - `grep -c 'render_skill_hint' rust-agent-tui/src/ui/main_ui/popups/hints.rs rust-agent-tui/src/ui/main_ui.rs`
  - 预期: 两个文件中合计出现 0 次

- [x] 验证 `render_command_hint` 函数已移除
  - `grep -c 'render_command_hint' rust-agent-tui/src/ui/main_ui/popups/hints.rs rust-agent-tui/src/ui/main_ui.rs`
  - 预期: 两个文件中合计出现 0 次

- [x] 验证 `render_unified_hint` 已在调用点使用
  - `grep 'render_unified_hint' rust-agent-tui/src/ui/main_ui.rs`
  - 预期: 输出包含 `popups::hints::render_unified_hint(f, app, chunks[4]);`

- [x] 验证编译通过
  - `cargo build -p rust-agent-tui 2>&1 | tail -5`
  - 预期: 输出包含 `Finished` 且无 error

- [x] 验证 headless 测试通过
  - `cargo test -p rust-agent-tui --lib -- test_unified_hint 2>&1 | tail -10`
  - 预期: 输出包含 `test result: ok` 且 3 个测试全部通过

---

### Task 2: Tab 补全合并

**背景:**
当前 `hint_ops.rs` 中 `hint_candidates_count()` 和 `hint_complete()` 按 `/` 和 `#` 两个分支独立处理：`/` 前缀只匹配命令，`#` 前缀只匹配 Skills。合并后，`/` 前缀需要同时覆盖命令和 Skills 的候选，`#` 前缀不再处理。`hint_candidates_count()` 返回的候选总数将被 `event.rs` 中的上下/Tab 导航（~L231-254、~L300-315）以及输入内容变化时的自动选中（~L398）直接消费。`hint_complete()` 被 `event.rs` 的 Enter 确认（~L322）调用，需要在统一的扁平候选列表中按 `hint_cursor` 索引定位并补全为 `/command_name ` 或 `/skill-name `。Task 1 的 `render_unified_hint` 已定义了命令组（最多 6 条）+ Skills 组（最多 4 条）的扁平索引结构，本 Task 的候选计数和补全逻辑必须与之一致。

**涉及文件:**
- 修改: `rust-agent-tui/src/app/hint_ops.rs`

**执行步骤:**

- [x] 重写 `hint_candidates_count()` 方法 — `/` 前缀返回命令候选数 + Skills 候选数之和，移除 `#` 分支
  - 位置: `rust-agent-tui/src/app/hint_ops.rs`，`hint_candidates_count()` 方法（~L6-26），替换整个方法体
  - 新逻辑:
    ```rust
    pub fn hint_candidates_count(&self) -> usize {
        let first_line = self
            .core.textarea
            .lines()
            .first()
            .map(|s| s.as_str())
            .unwrap_or("");
        if first_line.starts_with('/') {
            let prefix = first_line.trim_start_matches('/');
            let cmd_count = self.core.command_registry.match_prefix(prefix)
                .into_iter()
                .take(6)
                .count();
            let skill_count = self.core.skills.iter()
                .filter(|s| prefix.is_empty() || s.name.contains(prefix))
                .take(4)
                .count();
            cmd_count + skill_count
        } else {
            0
        }
    }
    ```
  - 关键变更: 移除 `else if first_line.starts_with('#')` 分支（原 ~L16-25），`#` 前缀不再产生候选。命令候选上限 6 条、Skills 候选上限 4 条，与 Task 1 的 `render_unified_hint` 保持一致
  - 原因: `hint_candidates_count` 的返回值被 `event.rs` 的 Up/Down/Tab 导航和输入变化自动选中直接使用，必须与 `render_unified_hint` 渲染的候选数量一致

- [x] 重写 `hint_complete()` 方法 — 统一候选列表中按 cursor 索引定位，索引 < 命令数补全命令，索引 >= 命令数补全 Skill
  - 位置: `rust-agent-tui/src/app/hint_ops.rs`，`hint_complete()` 方法（~L29-61），替换整个方法体
  - 新逻辑:
    ```rust
    pub fn hint_complete(&mut self) {
        let first_line = self
            .core.textarea
            .lines()
            .first()
            .map(|s| s.as_str())
            .unwrap_or("")
            .to_string();
        let cursor = self.core.hint_cursor.unwrap_or(0);

        if first_line.starts_with('/') {
            let prefix = first_line.trim_start_matches('/');
            let cmd_candidates: Vec<_> = self.core.command_registry
                .match_prefix(prefix)
                .into_iter()
                .take(6)
                .collect();
            let cmd_count = cmd_candidates.len();

            let skill_candidates: Vec<_> = self.core.skills.iter()
                .filter(|s| prefix.is_empty() || s.name.contains(prefix))
                .take(4)
                .collect();

            if cursor < cmd_count {
                // 命令组
                if let Some((name, _)) = cmd_candidates.get(cursor) {
                    self.core.textarea = build_textarea(false);
                    self.core.textarea.insert_str(format!("/{} ", name));
                    self.core.hint_cursor = None;
                }
            } else {
                // Skills 组
                let skill_index = cursor - cmd_count;
                if let Some(skill) = skill_candidates.get(skill_index) {
                    self.core.textarea = build_textarea(false);
                    self.core.textarea.insert_str(format!("/{} ", skill.name));
                    self.core.hint_cursor = None;
                }
            }
        }
    }
    ```
  - 关键变更: 移除 `else if first_line.starts_with('#')` 分支（原 ~L47-59），Skill 补全前缀从 `#` 改为 `/`。两组候选按 `cursor` 索引在扁平列表中定位，与 Task 1 的 `render_unified_hint` 渲染顺序一致
  - 原因: Tab 补全和 Enter 确认必须在同一扁平索引空间中定位候选项，索引 < cmd_count 对应命令，索引 >= cmd_count 对应 Skills

- [x] 为 `hint_candidates_count` 和 `hint_complete` 编写单元测试
  - 测试文件: `rust-agent-tui/src/app/hint_ops.rs`（在文件末尾添加 `#[cfg(test)] mod tests` 块）
  - 测试场景:
    - `test_candidates_count_slash_prefix_returns_cmd_plus_skills`: 设置 textarea 内容为 `/`，验证 `hint_candidates_count()` 返回命令总数 + Skills 总数（使用默认 registry 命令数 + 注入的 Skills 数）
    - `test_candidates_count_slash_prefix_filters_both`: 设置 textarea 内容为 `/mo`，验证返回值只包含前缀匹配的命令和 Skills 之和
    - `test_candidates_count_hash_prefix_returns_zero`: 设置 textarea 内容为 `#skill`，验证返回 0
    - `test_candidates_count_no_prefix_returns_zero`: 设置 textarea 内容为 `hello`，验证返回 0
    - `test_hint_complete_command_at_cursor_0`: 设置 textarea 内容为 `/m`，`hint_cursor = Some(0)`，调用 `hint_complete()`，验证 textarea 内容变为 `/model `（假设 `model` 是 registry 中的第一个匹配命令）
    - `test_hint_complete_skill_after_commands`: 设置 textarea 内容为 `/`，注册 1 个命令（如 `help`），注册 1 个 Skill（如 `commit`），设置 `hint_cursor = Some(1)`（跳过命令组），调用 `hint_complete()`，验证 textarea 内容变为 `/commit `
    - `test_hint_complete_clears_hint_cursor`: 调用 `hint_complete()` 后验证 `hint_cursor` 为 `None`
  - 测试辅助: 使用 `App::new_headless(80, 24)` 创建测试实例，通过 `app.core.textarea.insert_str()` 设置输入内容，通过 `app.core.skills.push(SkillMetadata{ name, description, path })` 注入 Skills 数据
  - 运行命令: `cargo test -p rust-agent-tui --lib -- test_candidates_count -- --nocapture 2>&1 | tail -20; cargo test -p rust-agent-tui --lib -- test_hint_complete -- --nocapture 2>&1 | tail -20`
  - 预期: 所有测试通过

**检查步骤:**
- [x] 验证 `#` 分支已从 `hint_candidates_count` 移除
  - `grep -n 'starts_with.*#' rust-agent-tui/src/app/hint_ops.rs`
  - 预期: 无输出（`#` 前缀判断不再存在）
- [x] 验证 `#` 分支已从 `hint_complete` 移除
  - `grep -n 'starts_with.*#' rust-agent-tui/src/app/hint_ops.rs`
  - 预期: 无输出
- [x] 验证编译通过
  - `cargo build -p rust-agent-tui 2>&1 | tail -5`
  - 预期: 输出包含 `Finished` 且无 error
- [x] 验证单元测试通过
  - `cargo test -p rust-agent-tui --lib -- hint_ops::tests 2>&1 | tail -15`
  - 预期: 输出包含 `test result: ok` 且所有 `test_candidates_count_*` 和 `test_hint_complete_*` 测试通过

---

### Task 3: Enter 触发逻辑 + 消息解析

**背景:**
当前用户在输入框输入 `/xxx` 后按 Enter，event.rs 的 `dispatch()` 仅匹配命令，未命中时显示"未知命令"。需在命令未命中时增加 Skill 名称匹配 fallback：若 `app.core.skills` 中存在同名 Skill，则将输入作为消息提交（`Action::Submit`），由 agent_ops.rs 中已有的 Skill 预加载逻辑处理。同时在 agent_ops.rs 中将 Skill token 提取从 `#skill-name` 改为 `/skill-name`，使整个链路从输入到预加载统一使用 `/` 前缀。本 Task 依赖 Task 1 的 `render_unified_hint`（浮层已展示 Skill 候选）和 Task 2 的 `hint_candidates_count`/`hint_complete`（Tab 补全能定位 Skill）。

**涉及文件:**
- 修改: `rust-agent-tui/src/event.rs`
- 修改: `rust-agent-tui/src/app/agent_ops.rs`

**执行步骤:**

- [x] 在 event.rs 的 Enter 处理中增加 Skill 匹配 fallback — 命令 dispatch 未命中时尝试匹配 Skill 名称
  - 位置: `rust-agent-tui/src/event.rs`，`Input { key: Key::Enter, .. }` 分支中 `if text.starts_with('/')` 块（~L350-361），替换整个 `if text.starts_with('/') { ... }` 块
  - 当前代码（~L350-361）:
    ```rust
    } else if text.starts_with('/') {
        app.core.textarea = crate::app::build_textarea(false);
        let registry = std::mem::take(&mut app.core.command_registry);
        let known = registry.dispatch(app, &text);
        app.core.command_registry = registry;
        if !known {
            app.core.view_messages.push(MessageViewModel::system(format!(
                "未知命令: {}  （输入 /help 查看可用命令）",
                text
            )));
        }
    }
    ```
  - 替换为:
    ```rust
    } else if text.starts_with('/') {
        app.core.textarea = crate::app::build_textarea(false);
        let registry = std::mem::take(&mut app.core.command_registry);
        let known = registry.dispatch(app, &text);
        app.core.command_registry = registry;
        if known {
            // 命令命中，结束
        } else {
            // 命令未命中，尝试 Skill 匹配
            let skill_name: String = text.trim_start_matches('/')
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
                .collect();
            if let Some(_skill) = app.core.skills.iter().find(|s| s.name == skill_name) {
                // Skill 命中：将整条消息提交给 agent，agent_ops.rs 的预加载逻辑处理
                return Ok(Some(Action::Submit(text)));
            } else {
                // 完全无匹配
                app.core.view_messages.push(MessageViewModel::system(format!(
                    "未知命令或 Skill: {}  （输入 /help 查看可用命令）",
                    text
                )));
            }
        }
    }
    ```
  - 关键变更: 在 `!known` 分支内增加 Skill 查找。`skill_name` 提取逻辑取 `/` 后的合法字符序列（字母、数字、连字符、下划线），与 agent_ops.rs 中的 token 解析保持一致。Skill 命中时返回 `Action::Submit(text)`，消息原样传递到 `submit_message`
  - 原因: 命令优先原则不变，Skill 是命令未命中时的 fallback。Skill 命中后走 Submit 是因为 `submit_message` → `agent_ops.rs` 中已有完整的 Skill 预加载逻辑

- [x] 提取 `extract_skill_tokens` 辅助函数并修改 `submit_message` 调用 — 将 `#skill-name` 改为 `/skill-name`
  - 位置（辅助函数）: `rust-agent-tui/src/app/agent_ops.rs`，在 `impl App` 块之前新增模块级函数
  - 新增辅助函数:
    ```rust
    /// 从输入文本中提取 `/skill-name` 格式的 token（字母、数字、连字符、下划线）
    fn extract_skill_tokens(input: &str) -> Vec<String> {
        input
            .split_whitespace()
            .filter(|token| token.starts_with('/') && token.len() > 1)
            .map(|token| {
                let name = token.trim_start_matches('/');
                name.chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
                    .collect::<String>()
            })
            .filter(|s| !s.is_empty())
            .collect()
    }
    ```
  - 位置（调用点替换）: `rust-agent-tui/src/app/agent_ops.rs`，`submit_message()` 方法中 `preload_skills` 构建处（~L81-93），替换整个代码块
  - 当前代码（~L81-93）:
    ```rust
    let preload_skills: Vec<String> = input
        .split_whitespace()
        .filter(|token| token.starts_with('#') && token.len() > 1)
        .map(|token| {
            let name = token.trim_start_matches('#');
            name.chars()
                .take_while(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
                .collect::<String>()
        })
        .filter(|s| !s.is_empty())
        .collect();
    ```
  - 替换为:
    ```rust
    let preload_skills = extract_skill_tokens(&input);
    ```
  - 关键变更: 提取独立函数 `extract_skill_tokens`，`starts_with('#')` → `starts_with('/')`，`trim_start_matches('#')` → `trim_start_matches('/')`。不排除命令名：纯命令输入（如 `/model`）已被 event.rs 的 dispatch 拦截不会走到 submit_message，能到达 submit_message 的 `/xxx` 都是 Skill 引用
  - 原因: 消息解析与 event.rs 的 Enter 触发逻辑配合，`/` 前缀统一用于 Skill 引用。提取为独立函数方便单元测试直接调用

- [x] 为 event.rs 的 Skill fallback 逻辑编写 headless 集成测试
  - 测试文件: `rust-agent-tui/src/ui/headless.rs`（在 `mod tests` 块末尾追加）
  - 测试场景:
    - `test_enter_skill_name_submits_message`: 设置 `app.core.textarea` 内容为 `/review`，注入 Skill `SkillMetadata { name: "review".into(), description: "code review".into(), path: "/tmp/review.md".into() }` 到 `app.core.skills`，模拟 Enter 事件（调用 `next_event` 或直接调用对应逻辑），验证返回 `Action::Submit("/review".to_string())`
    - `test_enter_unknown_command_shows_error`: 设置 `app.core.textarea` 内容为 `/nonexistent`，模拟 Enter 事件，验证 `app.core.view_messages` 最后一条为系统消息且包含 "未知命令或 Skill"
    - `test_enter_known_command_no_skill_fallback`: 设置 `app.core.textarea` 内容为 `/help`（注册的命令），验证命令正常执行（不触发 Submit），即使 `app.core.skills` 中有名为 `help` 的 Skill 也优先走命令
  - 运行命令: `cargo test -p rust-agent-tui --lib -- test_enter_skill`
  - 预期: 所有测试通过

- [x] 为 agent_ops.rs 的 `extract_skill_tokens` 编写单元测试
  - 测试文件: `rust-agent-tui/src/app/agent_ops.rs`（在文件末尾添加 `#[cfg(test)] mod tests` 块）
  - 测试场景:
    - `test_preload_skills_extracts_slash_prefix`: 输入 `"请使用 /commit 提交"` → `extract_skill_tokens(input)` 返回 `["commit"]`
    - `test_preload_skills_extracts_multiple_skills`: 输入 `"/review /refactor"` → 返回 `["review", "refactor"]`
    - `test_preload_skills_ignores_hash_prefix`: 输入 `"#old-skill /new-skill"` → 返回 `["new-skill"]`（`#` 前缀不再匹配）
    - `test_preload_skills_empty_for_no_skills`: 输入 `"普通消息没有 skill 引用"` → 返回 `[]`
    - `test_preload_skills_truncates_on_invalid_char`: 输入 `"/skill-name!suffix"` → 返回 `["skill-name"]`（遇到 `!` 截断）
  - 测试辅助: 直接调用模块级函数 `extract_skill_tokens(input: &str) -> Vec<String>`（已在上一执行步骤中提取）
  - 运行命令: `cargo test -p rust-agent-tui --lib -- agent_ops::tests`
  - 预期: 所有测试通过

**检查步骤:**

- [x] 验证 event.rs 中 Skill fallback 逻辑存在
  - `grep -n 'skills.iter().find' rust-agent-tui/src/event.rs`
  - 预期: 输出包含一行匹配结果，在 Enter 处理的 `!known` 分支中

- [x] 验证 agent_ops.rs 中 `#` 前缀已替换为 `/`
  - `grep -n "starts_with('#')" rust-agent-tui/src/app/agent_ops.rs`
  - 预期: 无输出
  - `grep -n "starts_with('/')" rust-agent-tui/src/app/agent_ops.rs`
  - 预期: 输出包含 1 行

- [x] 验证"未知命令"文案已更新为"未知命令或 Skill"
  - `grep -n '未知命令或 Skill' rust-agent-tui/src/event.rs`
  - 预期: 输出包含 1 行

- [x] 验证编译通过
  - `cargo build -p rust-agent-tui 2>&1 | tail -5`
  - 预期: 输出包含 `Finished` 且无 error

- [x] 验证单元测试通过
  - `cargo test -p rust-agent-tui --lib -- test_enter_skill 2>&1 | tail -10; cargo test -p rust-agent-tui --lib -- agent_ops::tests 2>&1 | tail -10`
  - 预期: 输出包含 `test result: ok` 且所有测试通过

---

### Task 4: 提示词与文案更新

**背景:**
Skills 触发键已从 `#` 统一到 `/`（Task 1-3 完成了浮层、补全、触发逻辑的改动），但中间件提示词和 TUI 提示文案仍引用旧的 `#` 前缀。LLM 通过 SkillsMiddleware 的系统提示词了解 Skill 引用格式，若提示词未同步更新，LLM 会继续指导用户使用 `#skill_name` 格式，导致功能不可用。TUI tips 是用户首次接触 Skills 时的引导文案，也需要准确反映新的交互方式。本 Task 独立于 Task 1-3，无上下游依赖。

**涉及文件:**
- 修改: `rust-agent-middlewares/src/skills/mod.rs`
- 修改: `rust-agent-tui/src/ui/tips.rs`

**执行步骤:**

- [x] 更新 SkillsMiddleware 提示词中的 Skill 引用格式 — 将 `'#skill_name'` 改为 `'/skill-name'`
  - 位置: `rust-agent-middlewares/src/skills/mod.rs`，`build_summary()` 方法中的提示词行（~L133）
  - 当前内容:
    ```rust
    lines.push("如需加载某 skill 的完整内容，在消息中提及其 name 即可。用户一般会使用 '#skill_name' 的形式。".to_string());
    ```
  - 替换为:
    ```rust
    lines.push("如需加载某 skill 的完整内容，在消息中提及其 name 即可。用户一般会使用 '/skill-name' 的形式。".to_string());
    ```
  - 关键变更: `'#skill_name'` → `'/skill-name'`，前缀从 `#` 改为 `/`，分隔符从下划线改为连字符（与 Skill 命名惯例一致）
  - 原因: LLM 依据此提示词指导用户使用正确的 Skill 引用格式，必须与实际触发键 `/` 保持一致

- [x] 更新 TUI tips 中 Skills 搜索提示 — 将 `#` 前缀描述改为 `/` 前缀
  - 位置: `rust-agent-tui/src/ui/tips.rs`，`TIPS` 数组第 1 项（~L3）
  - 当前内容:
    ```rust
    "使用 # 前缀快速搜索可用 Skills",
    ```
  - 替换为:
    ```rust
    "输入 / 前缀搜索可用命令和 Skills",
    ```
  - 关键变更: "使用 # 前缀快速搜索可用 Skills" → "输入 / 前缀搜索可用命令和 Skills"，体现合并后的统一命名空间
  - 原因: tips 是用户首次接触功能时的引导文案，必须准确反映 `/` 前缀同时覆盖命令和 Skills 的新交互

- [x] 更新 TUI tips 中 Tab 补全描述 — 将 "Skills 或命令" 改为 "命令或 Skills"
  - 位置: `rust-agent-tui/src/ui/tips.rs`，`TIPS` 数组（~L6）
  - 当前内容:
    ```rust
    "按 Tab 在 Skills 或命令提示中补全",
    ```
  - 替换为:
    ```rust
    "按 Tab 在命令或 Skills 提示中补全",
    ```
  - 关键变更: "Skills 或命令" → "命令或 Skills"，与合并浮层中命令组在前、Skills 组在后的显示顺序一致
  - 原因: 描述顺序应与实际 UI 展示顺序一致，减少用户认知偏差

- [x] 为 `build_summary` 提示词更新编写单元测试
  - 测试文件: `rust-agent-middlewares/src/skills/mod.rs`（在 `mod tests` 块末尾追加）
  - 测试场景:
    - `test_build_summary_contains_slash_prefix`: 创建包含 1 个 skill 的临时目录，调用 `SkillsMiddleware` 的 `before_agent`，断言注入的系统消息内容包含 `'/skill-name'` 且不包含 `'#skill_name'`
    - `test_build_summary_does_not_contain_hash_prefix`: 同上场景，断言系统消息内容不包含 `#skill_name` 字符串
  - 测试辅助: 复用已有的 `write_skill` 辅助函数（~L177-185）创建测试 skill 目录
  - 运行命令: `cargo test -p rust-agent-middlewares --lib -- skills::tests::test_build_summary`
  - 预期: 所有测试通过

- [x] 为 TUI tips 文案更新编写单元测试
  - 测试文件: `rust-agent-tui/src/ui/tips.rs`（在文件末尾添加 `#[cfg(test)] mod tests` 块）
  - 测试场景:
    - `test_tips_no_hash_prefix_for_skills`: 遍历 `TIPS` 数组所有条目，断言没有任何条目包含 `"# 前缀"` 或 `"#skill"` 或 `"#Skill"` 子串（确认旧的 `#` 引用已全部移除）
    - `test_tips_contains_slash_skills_hint`: 断言 `TIPS` 数组中存在一条包含 `"命令和 Skills"` 的条目（确认新的合并提示文案存在）
    - `test_tips_tab_hint_order`: 断言 `TIPS` 数组中存在一条包含 `"命令或 Skills 提示中补全"` 的条目（确认 Tab 补全提示顺序已更新）
  - 运行命令: `cargo test -p rust-agent-tui --lib -- tips::tests`
  - 预期: 所有测试通过

**检查步骤:**

- [x] 验证 skills/mod.rs 中不再包含 `#skill_name`
  - `grep -n '#skill_name' rust-agent-middlewares/src/skills/mod.rs`
  - 预期: 无输出

- [x] 验证 skills/mod.rs 包含 `/skill-name`
  - `grep -n '/skill-name' rust-agent-middlewares/src/skills/mod.rs`
  - 预期: 输出包含 1 行（~L133 提示词行）

- [x] 验证 tips.rs 中不再包含 `# 前缀` 相关的 Skills 提示
  - `grep -n '#' rust-agent-tui/src/ui/tips.rs | grep -i skill`
  - 预期: 无输出

- [x] 验证 tips.rs 包含更新后的文案
  - `grep -n '命令和 Skills' rust-agent-tui/src/ui/tips.rs`
  - 预期: 输出包含 1 行

- [x] 验证 tips.rs Tab 补全提示顺序已更新
  - `grep -n '命令或 Skills 提示中补全' rust-agent-tui/src/ui/tips.rs`
  - 预期: 输出包含 1 行

- [x] 验证编译通过
  - `cargo build -p rust-agent-middlewares -p rust-agent-tui 2>&1 | tail -5`
  - 预期: 输出包含 `Finished` 且无 error

- [x] 验证 skills 测试通过
  - `cargo test -p rust-agent-middlewares --lib -- skills::tests 2>&1 | tail -10`
  - 酬期: 输出包含 `test result: ok` 且所有测试通过

- [x] 验证 tips 测试通过
  - `cargo test -p rust-agent-tui --lib -- tips::tests 2>&1 | tail -10`
  - 预期: 输出包含 `test result: ok` 且所有测试通过

---

### Task 5: Skills 触发键统一 验收

**前置条件:**
- 构建命令: `cargo build -p rust-agent-tui -p rust-agent-middlewares`
- 无需额外测试数据准备

**端到端验证:**

1. 运行完整测试套件确保无回归
   - `cargo test -p rust-agent-tui -p rust-agent-middlewares 2>&1 | tail -20`
   - 预期: 全部测试通过
   - 失败排查: 检查各 Task 的测试步骤，关注 `test_unified_hint`、`hint_ops::tests`、`test_enter_skill`、`agent_ops::tests`、`tips::tests`
   - [x] 通过: 166 + 4 + 208 + 1 = 379 个测试全部通过

2. 验证 `#` 前缀已从代码中完全移除
   - `grep -rn "starts_with('#')" rust-agent-tui/src/ rust-agent-middlewares/src/`
   - 预期: 无输出（所有 `#` 前缀判断已替换为 `/`）
   - 失败排查: 检查 Task 2（hint_ops.rs）和 Task 3（agent_ops.rs）
   - [x] 通过: 无输出

3. 验证提示浮层合并后命令和 Skills 均可展示（含分组标题）
   - `cargo test -p rust-agent-tui --lib -- test_unified_hint 2>&1 | tail -10`
   - 预期: 3 个测试全部通过（浮层展示命令+Skills 含分组标题、按前缀过滤、`#` 不触发浮层）
   - 失败排查: 检查 Task 1 的 `render_unified_hint` 实现，确认 "命令" 和 "Skills" 分组标题正常渲染
   - [x] 通过: 3 个测试全部通过

4. 验证 Enter 触发链路完整（命令优先 → Skill fallback → 无匹配报错）
   - `cargo test -p rust-agent-tui --lib -- test_enter_skill 2>&1 | tail -10`
   - 预期: 3 个测试全部通过
   - 失败排查: 检查 Task 3 的 event.rs 修改
   - [x] 通过: 3 个测试全部通过

5. 验证消息解析正确提取 `/skill-name` token
   - `cargo test -p rust-agent-tui --lib -- agent_ops::tests 2>&1 | tail -10`
   - 预期: 5 个测试全部通过
   - 失败排查: 检查 Task 3 的 agent_ops.rs 修改
   - [x] 通过: 5 个测试全部通过

6. 验证提示词和文案已全部更新
   - `grep -rn '#skill_name\|# 前缀' rust-agent-middlewares/src/skills/ rust-agent-tui/src/ui/tips.rs`
   - 预期: 无输出（旧文案已全部移除）
   - `grep -rn '/skill-name' rust-agent-middlewares/src/skills/mod.rs`
   - 预期: 输出包含 1 行
   - 失败排查: 检查 Task 4
   - [x] 通过: 旧文案仅出现在测试断言消息中（作为反向检查），功能代码中已全部更新

