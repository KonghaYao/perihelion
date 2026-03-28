# Feature: 20260328_F001 - ask-user-question-align

## 需求背景

当前框架的 `ask_user` 工具与 Claude Code 内置的 `AskUserQuestion` 工具在工具名称、入参结构和展示方式上存在较大差异，导致使用者在两个系统之间切换时体验割裂，也不利于 LLM 准确理解和使用该工具。本 feature 旨在将 `ask_user` 全面对齐 Claude 的 `AskUserQuestion`，实现工具名称、入参、数据结构、TUI/前端展示的一致性。

## 目标

- 将工具名称从 `ask_user` 重命名为 `ask_user_question`
- 入参 Schema 对齐 Claude `AskUserQuestion`：`questions` 数组、`header` 字段、选项 `description` 字段
- 移除 `allow_custom_input` / `placeholder`，始终允许自定义输入
- TUI 弹窗使用 `header` 作为 tab 标签，展示选项 `description`
- 前端 `AskUserDialog.js` 同步更新展示

## 方案设计

### 工具 Schema 对比

**变更前（`ask_user`）：**
```json
{
  "description": "问题文本",
  "type": "single_select",
  "options": [{ "label": "选项A" }],
  "allow_custom_input": true,
  "placeholder": "占位符"
}
```

**变更后（`ask_user_question`）：**
```json
{
  "questions": [
    {
      "question": "问题文本，清晰具体",
      "header": "短标签(≤12字)",
      "multi_select": false,
      "options": [
        { "label": "选项A", "description": "选项说明（可选）" }
      ]
    }
  ]
}
```

关键差异：
| 字段 | 变更前 | 变更后 |
|------|--------|--------|
| 工具名 | `ask_user` | `ask_user_question` |
| 顶层结构 | 单问题扁平 | `questions` 数组（1–4 个） |
| 问题字段 | `description` | `question` |
| 选择类型 | `type: single_select/multi_select` | `multi_select: bool` |
| 问题短标签 | 无 | `header`（≤12字） |
| 选项说明 | 无 | `description: Option<String>` |
| 自定义输入 | `allow_custom_input: bool` | 始终允许（移除字段） |
| 占位符 | `placeholder: Option<String>` | 移除 |

### 数据结构变更

**`QuestionItem`**（`rust-create-agent/src/interaction/mod.rs`）：
```rust
pub struct QuestionItem {
    pub id: String,
    pub question: String,
    pub header: String,             // 新增：短标签
    pub options: Vec<QuestionOption>,
    pub multi_select: bool,
    // 移除: allow_custom_input, placeholder
}

pub struct QuestionOption {
    pub label: String,
    pub description: Option<String>, // 新增：选项说明
}
```

**`AskUserQuestionData`**（`rust-create-agent/src/ask_user/mod.rs`，TUI 桥接用）：
```rust
pub struct AskUserQuestionData {
    pub tool_call_id: String,
    pub question: String,           // 原 description
    pub header: String,             // 新增
    pub multi_select: bool,
    pub options: Vec<AskUserOption>,
    // 移除: allow_custom_input, placeholder
}

pub struct AskUserOption {
    pub label: String,
    pub description: Option<String>, // 新增
}
```

### 工具实现变更

**`rust-agent-middlewares/src/ask_user/mod.rs`**：
- `ask_user_tool_definition()` 重写 Schema（见上文）
- `parse_ask_user()` 更新：工具名检查改为 `ask_user_question`，解析 `questions` 数组，为每个 `QuestionItem` 填充 `header`

**`rust-agent-middlewares/src/tools/ask_user_tool.rs`**：
- `name()` 返回 `"ask_user_question"`
- `parse_question()` 改为解析 `questions` 数组（单次调用支持多问题）
- `invoke()` 对多问题返回可读格式：
  ```
  [问: 短标签1]
  回答: answer1

  [问: 短标签2]
  回答: answer2
  ```

### TUI 展示变更

**`rust-agent-tui/src/ui/main_ui/popups/ask_user.rs`**：

![TUI 弹窗展示设计](./images/01-tui-popup.png)

1. **Tab 标签行**：使用 `header` 字段（替换原来取 `description` 前8字符的逻辑）
2. **选项 `description`**：在每个选项 `label` 下方以 `DarkGray` 缩进展示
   ```
    ▶ ○  红色
         温暖、活力感       ← description（DarkGray）
      ○  蓝色
         冷静、专业感
   ```
3. **自定义输入行**：移除 `allow_custom_input` 条件，始终渲染

**`rust-agent-tui/src/app/ask_user_prompt.rs`**：
- `QuestionState::total_rows()` 始终 = 选项数 + 1（自定义输入固定存在）
- 移除 `allow_custom_input` 字段引用，`in_custom_input` 逻辑不变

### 前端展示变更

**`rust-relay-server/web/components/AskUserDialog.js`**：

1. 每个问题上方展示 `header` 芯片标签
   ```html
   <span class="ask-user-header-chip">{q.header}</span>
   <div class="ask-user-question">{q.question}</div>
   ```
2. 选项 `description` 在 label 下方展示
   ```html
   <label class="ask-user-option">
     <input type="radio" ... />
     <div>
       <span>{opt.label}</span>
       {opt.description && <small class="ask-user-opt-desc">{opt.description}</small>}
     </div>
   </label>
   ```
3. 自定义输入框始终显示（移除 `q.allow_custom_input` 条件）
4. `key` 统一使用 `q.tool_call_id`

## 实现要点

- **向后兼容**：`ask_user` 工具名若出现在旧 skill 提示词中，需在迁移后统一更新
- **桥接层**（`agent_ops.rs`）：`QuestionItem` → `AskUserQuestionData` 的转换需增加 `header`、`option.description` 映射，移除 `allow_custom_input`/`placeholder` 映射
- **前端静态文件**：修改 `web/` 后需 `touch rust-relay-server/src/static_files.rs` 重新编译（`include_bytes!` 打包）
- **`questions` 数组解析**：`parse_ask_user()` 返回 `Vec<AskUserQuestionData>`（原返回 `Option<AskUserQuestionData>`），调用方需适配

## 约束一致性

- 遵循 Workspace 依赖方向：数据结构变更在 `rust-create-agent`，工具实现在 `rust-agent-middlewares`，展示在 `rust-agent-tui` / `rust-relay-server`
- 使用 `thiserror` 定义库层错误，`anyhow` 传播应用层错误
- 命名遵循 Rust 标准（`snake_case` 字段名）

## 验收标准

- [ ] `ask_user_question` 工具 Schema 与 Claude `AskUserQuestion` 对齐（`questions` 数组、`header`、选项 `description`、`multi_select`）
- [ ] 旧字段 `allow_custom_input`、`placeholder`、`type` 已从工具定义中移除
- [ ] `QuestionItem` 和 `AskUserQuestionData` 新增 `header` 字段
- [ ] `QuestionOption` 和 `AskUserOption` 新增 `description: Option<String>` 字段
- [ ] TUI 弹窗 Tab 行使用 `header` 展示
- [ ] TUI 弹窗选项列表在 label 下方展示 `description`
- [ ] TUI 弹窗始终显示自定义输入行（不受 `allow_custom_input` 控制）
- [ ] 前端 `AskUserDialog.js` 展示 `header` 芯片和选项 `description`
- [ ] `cargo build` 无编译错误
