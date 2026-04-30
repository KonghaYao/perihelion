# Feature: 20260429_F001 - skill-slash-trigger

## 需求背景

当前 Skills 使用 `#` 前缀触发（`#skill-name`），而命令使用 `/` 前缀触发（`/model`、`/help`）。这导致用户需要记忆两套触发键，且与 Claude Code 原版行为不一致（原版命令和 Skills 均使用 `/` 前缀）。

需要将 Skills 的触发键从 `#` 统一到 `/`，实现命令和 Skills 共用一个命名空间，降低用户认知负担。

## 目标

- Skills 触发键从 `#` 改为 `/`，与命令共用统一命名空间
- 提示浮层合并展示命令和 Skills 候选
- Enter 触发时先命令匹配，未命中则尝试 Skill 预加载并提交消息
- 中间件提示词、消息解析逻辑、TUI 文案全部同步更新

## 方案设计

### 1. 整体交互流程（改后）

```
用户输入 /xxx
  ├─ 提示浮层：实时展示匹配的命令 + Skills（分组显示）
  ├─ Tab 补全：选中当前候选项替换输入框
  └─ Enter 触发：
      ├─ 命令精确匹配 → 执行命令
      ├─ 命令唯一前缀匹配 → 执行命令
      ├─ Skill 名称匹配 → 提交消息 + 预加载 Skill
      └─ 无匹配 → 显示"未知命令"提示
```

### 2. 提示浮层合并（hints.rs）

**当前状态：** 两个独立浮层函数 `render_command_hint` 和 `render_skill_hint`，分别处理 `/` 和 `#` 前缀。

**改为：** 合并为一个统一的 `render_unified_hint` 函数，输入 `/` 时同时搜索命令和 Skills：

- 候选列表分为两组：命令组（标题 "命令"）和 Skills 组（标题 "Skills"）
- 命令组在前，Skills 组在后
- 总候选数上限保持 8 条（命令最多 6 条 + Skills 最多 4 条，超出按组内截断）
- 光标导航跨组连续，`↑`/`↓` 从命令组最后一项自然过渡到 Skills 组第一项
- 选中项高亮样式不变（`theme::CURSOR_BG`）

**分组渲染示例：**

```
┌ 命令 ──────────────────────────┐
│ ▸ /model    切换 LLM 模型      │
│   /mock     ...                │
├ Skills ────────────────────────┤
│   /commit   提交代码           │
│   /review   代码审查           │
└───────────────────────────────┘
```

移除 `render_skill_hint` 函数，在调用点只调用 `render_unified_hint`。

### 3. Tab 补全合并（hint_ops.rs）

**`hint_candidates_count()`：** `/` 前缀时返回命令候选数 + Skills 候选数之和。

**`hint_complete()`：** 统一候选列表中按 `cursor` 索引定位：
- 索引 < 命令数 → 补全为 `/command_name `
- 索引 >= 命令数 → 补全为 `/skill-name `

### 4. Enter 触发逻辑（event.rs）

**当前状态（event.rs:350-361）：** `/` 前缀 → `registry.dispatch()`，未命中显示"未知命令"。

**改为：**

```rust
if text.starts_with('/') {
    // 清空输入框
    app.core.textarea = build_textarea(false);

    // 第 1 步：尝试命令匹配
    let registry = std::mem::take(&mut app.core.command_registry);
    let known = registry.dispatch(app, &text);
    app.core.command_registry = registry;

    if known {
        // 命令命中，结束
    } else {
        // 第 2 步：尝试 Skill 匹配
        let skill_name = text.trim_start_matches('/');
        let skill_name: String = skill_name.chars()
            .take_while(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
            .collect();

        if let Some(skill) = app.core.skills.iter().find(|s| s.name == skill_name) {
            // Skill 命中：将 skill_name 追加到消息并提交
            // 使用 Action::SubmitWithSkill(text, skill_name) 或直接走 Submit
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

关键决策：Skill 命中时走 `Action::Submit`，因为 Skill 预加载逻辑在 `submit_message` → `agent_ops.rs` 中已经存在，只需要消息解析逻辑识别 `/skill-name` 格式即可。

### 5. 消息解析逻辑（agent_ops.rs:81-93）

**当前：** 从消息中提取 `#skill-name` tokens。

**改为：** 从消息中提取 `/skill-name` tokens，但需要排除已知命令名称。

```rust
let preload_skills: Vec<String> = input
    .split_whitespace()
    .filter(|token| token.starts_with('/') && token.len() > 1)
    .map(|token| {
        let name = token.trim_start_matches('/');
        name.chars()
            .take_while(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
            .collect::<String>()
    })
    .filter(|s| !s.is_empty())
    .filter(|s| !is_known_command(s))  // 排除命令名
    .collect();
```

需要一种方式判断是否为已知命令。方案是在 AppCore 中暴露一个 `command_names: Vec<String>` 缓存（从 `command_registry.list()` 初始化），或直接在 `agent_ops` 中做一次简单的已知命令名列表判断。

更简洁的方案：由于 `submit_message` 是在 Enter 事件中被调用的，对于纯命令输入（如 `/model`），event.rs 已经拦截不会走到 Submit。所以实际能到达 `submit_message` 的 `/xxx` 输入，要么是用户混在普通消息中的 `/skill-name` 引用，要么是 Skill 命中后提交的消息。为简化，消息解析中直接提取所有 `/xxx` 格式的 token，不做命令排除——因为普通消息中不太可能出现 `/model` 这种纯命令格式（命令在 event.rs 已拦截），而 Skill 命中提交的消息正好需要预加载。

### 6. 中间件提示词更新（skills/mod.rs:133）

**当前：** `"如需加载某 skill 的完整内容，在消息中提及其 name 即可。用户一般会使用 '#skill_name' 的形式。"`

**改为：** `"如需加载某 skill 的完整内容，在消息中提及其 name 即可。用户一般会使用 '/skill-name' 的形式。"`

同时检查描述中的其他 `#` 引用，确保全部替换。

### 7. TUI 文案更新（tips.rs）

**当前：** `"使用 # 前缀快速搜索可用 Skills"`

**改为：** `"输入 / 前缀搜索可用命令和 Skills"`

Tips 中涉及 Tab 补全的描述也需确认一致性：

**当前：** `"按 Tab 在 Skills 或命令提示中补全"`

**改为：** `"按 Tab 在命令或 Skills 提示中补全"`

### 8. 数据流图（改后）

```
用户输入 /review
  │
  ├─ 提示浮层: 实时显示匹配候选
  │     ├─ 命令: (无匹配)
  │     └─ Skills: /review - 代码审查
  │
  └─ Enter:
        ├─ registry.dispatch("/review") → false (非命令)
        ├─ skills.find("review") → Some
        └─ Action::Submit("/review")
              └─ submit_message()
                    ├─ 解析 /review → preload_skills = ["review"]
                    └─ SkillPreloadMiddleware 注入 skill 全文
```

```
用户输入 /mo
  │
  ├─ 提示浮层:
  │     ├─ 命令: /model - 切换 LLM 模型
  │     └─ Skills: (无匹配)
  │
  └─ Enter:
        ├─ registry.dispatch("/mo") → true (唯一前缀匹配 /model)
        └─ 执行 model 命令
```

### 9. 边界情况处理

| 场景 | 处理 |
|------|------|
| Skill 名与命令名相同（如命令 `/commit` 和 Skill `commit`） | 命令优先（先 dispatch） |
| `/xxx` 不是命令也不是 Skill | 显示"未知命令或 Skill"提示 |
| 消息中混有 `/skill-name` 文本 | 提取并预加载，但不影响消息内容 |
| 多个 `/` 开头的 token | 全部提取，逐一匹配 Skill 预加载 |
| 空输入 `/` | 提示浮层显示所有命令 + 所有 Skills（候选过多时截断） |

## 实现要点

### 关键技术决策

1. **命令优先原则**：Enter 触发时命令匹配优先于 Skill 匹配，保证现有命令行为不受影响
2. **消息解析不排除命令**：`submit_message` 中的 Skill 预加载不做命令名排除，因为纯命令输入已被 event.rs 拦截
3. **浮层合并策略**：命令组在前 Skills 组在后，候选数限制为命令最多 6 条、Skills 最多 4 条

### 修改文件清单

| 文件 | 改动 |
|------|------|
| `rust-agent-tui/src/ui/main_ui/popups/hints.rs` | 合并两个浮层函数为 `render_unified_hint`，移除 `render_skill_hint` |
| `rust-agent-tui/src/app/hint_ops.rs` | 合并候选计数和 Tab 补全逻辑 |
| `rust-agent-tui/src/event.rs` | Enter 处理中增加 Skill 匹配 fallback |
| `rust-agent-tui/src/app/agent_ops.rs` | 消息解析 `#` → `/` |
| `rust-agent-middlewares/src/skills/mod.rs` | 提示词 `#skill_name` → `/skill-name` |
| `rust-agent-tui/src/ui/tips.rs` | 提示文案更新 |
| `rust-agent-tui/src/ui/main_ui/mod.rs` | 浮层调用点更新（移除 `render_skill_hint` 调用） |

### 潜在风险

- **Skill 名与命令前缀冲突**：如用户创建了一个名为 `m` 的 Skill，输入 `/m` 时命令 dispatch 已因歧义返回 false，然后 Skill 匹配会命中 `m`。这是可接受的行为。
- **headless 测试**：需要同步更新 hints 测试中的 `#` → `/` 断言。

## 约束一致性

本方案完全符合 `spec/global/constraints.md` 和 `spec/global/architecture.md`：

- 无新增依赖，纯逻辑重构
- 保持事件驱动 TUI 通信模式（mpsc channel）
- 保持 Middleware Chain 模式不变
- 保持命令注册表 `CommandRegistry` 的 dispatch 语义不变（精确匹配 > 前缀唯一匹配）
- 无违反 Workspace 分层规则（所有改动在 `rust-agent-tui` 和 `rust-agent-middlewares` 内部）

## 验收标准

- [ ] 输入 `/` 前缀时，提示浮层同时显示命令和 Skills 候选
- [ ] 输入 `/skill-name` 后 Enter，Skill 被预加载并提交消息
- [ ] 输入 `/model` 等命令后 Enter，命令正常执行（不受 Skills 影响）
- [ ] 输入 `/unknown` 后 Enter，显示"未知命令或 Skill"提示
- [ ] 命令匹配优先于 Skill 匹配
- [ ] Tab 补全在合并的候选列表中正常工作
- [ ] SkillsMiddleware 提示词中不再出现 `#skill_name`
- [ ] TUI 提示文案已更新
- [ ] 消息中 `/skill-name` token 被正确提取和预加载
- [ ] headless 测试中相关断言更新并通过
