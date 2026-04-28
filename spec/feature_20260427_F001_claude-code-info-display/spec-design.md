# Feature: 20260427_F001 - claude-code-info-display

## 需求背景

调研了 Claude Code（TypeScript/React + ink CLI）的信息显示实现，发现多个值得借鉴的 UI 模式：

1. **Spinner 动词体系**：Claude Code 的 Spinner 不仅显示旋转动画，还显示当前动作的动词（如 "Reading… / Writing… / Searching…"），动词自动从 TODO 任务的 `activeForm` 取得，无任务时从随机动词池选取。配合 Token 计数平滑动画和已用时间显示。
2. **工具调用显示**：每个工具调用有独立的状态指示器（闪烁黑圆点 = 进行中，静态 = 完成/失败），参数一行摘要，结果可折叠/展开。Claude Code 使用 `ToolUseLoader` 组件实现闪烁效果。
3. **对话信息流**：Markdown 渲染增强（代码块语法高亮、diff 着色）、SubAgent 调用折叠展示、思考内容可折叠。

当前 perihelion 的 `status_bar.rs` 只显示 `⠿ 运行中`，没有动词提示；`message_render.rs` 的工具调用显示已有基础折叠但缺乏状态指示器；Markdown 渲染无代码高亮和 diff 着色。

## 目标

- 在 `perihelion-widgets` crate 中新增 **SpinnerWidget**、**ToolCallWidget**、**MessageBlockWidget** 三个核心 widget
- 对标 Claude Code 的 Spinner 动词体系，提供动态动作提示 + Token 计数 + 已用时间
- 增强工具调用显示：状态指示器 + 智能折叠策略（只读默认折叠、写操作默认展开）
- 优化对话信息流：Markdown 代码高亮 + diff 着色 + 思考内容折叠 + SubAgent 显示优化

## 方案设计

### 架构设计

在已有 `perihelion-widgets` crate 内新增 3 个 widget 模块：

```
perihelion-widgets/src/
├── spinner/           # SpinnerWidget
│   ├── mod.rs         # SpinnerWidget + SpinnerState + SpinnerMode 枚举
│   ├── verb.rs        # 动词管理：从 TODO activeForm 取 / 随机动词池
│   └── animation.rs   # 动画帧计算：字符帧序列 + shimmer + stalled 检测
├── tool_call/         # ToolCallWidget
│   ├── mod.rs         # ToolCallWidget + ToolCallState
│   ├── display.rs     # 工具名/参数/结果格式化
│   └── collapse.rs    # 折叠/展开状态管理 + 结果截断
└── message_block/     # MessageBlockWidget
    ├── mod.rs         # MessageBlockWidget + MessageBlockState
    ├── markdown.rs    # 从现有 markdown.rs 迁移，增强代码块/diff 高亮
    └── blocks.rs      # 各种 ContentBlock 渲染策略（Text/ToolUse/SubAgent/Thinking）
```

**Widget 依赖关系**：`SpinnerWidget` 和 `ToolCallWidget` 无依赖；`MessageBlockWidget` 依赖 `tool_call` 子模块渲染 ToolUse 块。

**动画驱动**：所有 widget 通过 ratatui 的 `tick` 事件驱动，不引入额外线程。`SpinnerState` 维护当前帧索引和时间戳，`render()` 时计算当前帧。

### SpinnerWidget

#### API 设计

```rust
/// Spinner 模式
pub enum SpinnerMode {
    Thinking,    // 思考中：shimmer 脉冲效果
    ToolUse,     // 工具执行中：闪烁指示器
    Responding,  // 文本生成中：滚动文本效果
    Idle,        // 空闲：静态显示
}

pub struct SpinnerState {
    mode: SpinnerMode,
    verb: String,          // 当前动词（如 "搜索代码中"）
    start_time: Instant,   // 开始时间
    token_count: usize,    // 当前 token 数（平滑动画）
    displayed_tokens: usize, // 显示中的 token 数（平滑递增）
    tick: u64,             // 动画帧计数器
}

pub struct SpinnerWidget<'a> {
    state: &'a mut SpinnerState,
    show_elapsed: bool,    // 是否显示已用时间
    show_tokens: bool,     // 是否显示 token 计数
}
```

#### 动词管理

```rust
const DEFAULT_VERBS: &[&str] = &[
    "处理中", "分析中", "思考中", "生成中", "搜索中",
    "读取中", "编写中", "执行中", "计算中",
];

impl SpinnerState {
    /// 从 TODO 任务取 activeForm，无则随机选一个
    pub fn set_verb_from_todo(&mut self, active_form: Option<&str>) {
        self.verb = active_form
            .map(|s| format!("{}…", s))
            .unwrap_or_else(|| {
                DEFAULT_VERBS[rand::random::<usize>() % DEFAULT_VERBS.len()].to_string()
            });
    }
}
```

#### 渲染效果

- **字符帧**：`⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏` Braille 动画（与 Claude Code 类似）
- **Shimmer**：Thinking 模式下文字颜色脉冲渐变（灰色 → 浅灰 → 灰色）
- **Token 计数**：平滑递增动画（每次 tick 向目标值靠近 3~50 字符，取决于差距）
- **已用时间**：`1:23` 格式，30秒后自动显示

#### 在 TUI 中的集成

替换 `status_bar.rs` 中的 `⠿ 运行中` 为 `SpinnerWidget`，动词从 `app.core.view_messages` 中最后一个 TODO 任务取。

### ToolCallWidget

#### API 设计

```rust
pub enum ToolCallStatus {
    Pending,        // 等待执行（队列中）
    Running,        // 执行中（闪烁指示器）
    Completed,      // 已完成（静态绿圆点）
    Failed,         // 失败（红叉）
}

pub struct ToolCallState {
    tool_name: String,
    args_summary: String,      // 参数摘要（路径缩短、关键参数）
    status: ToolCallStatus,
    collapsed: bool,           // 结果是否折叠
    result_lines: Vec<String>, // 结果内容
    is_error: bool,
    tick: u64,                 // 动画帧（用于闪烁）
}

pub struct ToolCallWidget<'a> {
    state: &'a ToolCallState,
    color: Color,              // 工具名颜色（现有三级分级保留）
}
```

#### 渲染效果

**头行**：`[状态指示器] 工具名 (参数摘要)`

状态指示器：
- Running：闪烁黑圆点 `●`（每 500ms 闪烁）
- Completed：静态圆点 `●`
- Failed：红叉 `✗`
- Pending：暗淡圆点

结果区域：
- 折叠时只显示头行 `▸ 工具名 (参数)`
- 展开时显示结果 `▾ 工具名 (参数)` + 结果内容（缩进 + `│` 前缀）
- **智能默认折叠**：只读工具（read_file、glob_files、search_files_rg）默认折叠；写操作（write_file、edit_file、bash）默认展开
- 结果超过 20 行自动截断，显示 `… 还有 N 行`

#### 与现有代码的关系

现有 `MessageViewModel::ToolBlock` 已有基础折叠/展开，ToolCallWidget 将替换其渲染逻辑，增加状态指示器和智能折叠策略。

### MessageBlockWidget

#### API 设计

```rust
pub enum BlockRenderStrategy {
    /// 文本块：Markdown 渲染 + 代码高亮 + diff 着色
    Text { content: String, streaming: bool },
    /// 工具调用：委托给 ToolCallWidget
    ToolCall(ToolCallState),
    /// SubAgent 调用：折叠展示
    SubAgent {
        agent_id: String,
        task_preview: String,
        total_steps: usize,
        collapsed: bool,
        result: Option<String>,
    },
    /// 思考/推理：可折叠
    Thinking { char_count: usize, expanded: bool },
    /// 系统提示
    SystemNote { content: String },
}

pub struct MessageBlockState {
    blocks: Vec<BlockRenderStrategy>,
    collapsed_set: HashSet<usize>,
}

pub struct MessageBlockWidget<'a> {
    state: &'a MessageBlockState,
    index: Option<usize>,     // 消息序号
    width: usize,             // 可用宽度
}
```

#### Markdown 渲染增强

从现有 `markdown.rs` 迁移，增加：
- **代码块语法高亮**：基于文件扩展名推断语言，用颜色区分关键字/字符串/注释（简单正则匹配，不引入完整 parser）
- **diff 着色**：检测 `+`/`-`/`@@ ` 行前缀，绿色/红色/蓝色着色
- **内联代码**：反引号包裹的内容用不同背景色显示

#### SubAgent 显示优化

参考 Claude Code 的 `GroupedToolUseContent`（同批次工具调用折叠）：
- SubAgent 内部工具调用只在展开时显示摘要（工具名列表）
- 完成后显示结果前 100 字符预览
- 步数 > 4 时默认折叠内部消息，只显示最近 4 步

#### 思考内容折叠

- 默认折叠为 `💭 思考 (1234 chars)`
- 展开/折叠通过快捷键切换
- 折叠时占 1 行，展开时显示完整内容

## 实现要点

### 关键技术决策

1. **动画帧驱动**：ratatui 本身不支持 `requestAnimationFrame`，需要通过 `crossterm::Event::Tick` 或定时器事件驱动帧更新。每帧调用 `SpinnerState::tick()` 递增帧计数器，`render()` 时根据帧计算当前动画状态。
2. **平滑 Token 计数**：参考 Claude Code 的实现，维护 `displayed_tokens` 和实际 `token_count` 两个值，每帧向目标值靠近（差值小时步进 3，差值大时步进 50），实现数字平滑递增效果。
3. **代码高亮策略**：不引入 `tree-sitter` 等重量级 parser，采用简单正则匹配关键字/字符串/注释，够用即可。后续如有需要可以升级。
4. **折叠状态持久化**：`ToolCallState` 和 `MessageBlockState` 的折叠状态需要在 re-render 间保持一致，通过 `collapsed`/`collapsed_set` 字段管理。

### 实现顺序

1. **Phase 1**：SpinnerWidget（最高优先级，用户体验提升最大）
   - 实现基础动画帧和动词管理
   - 集成到 `status_bar.rs` 替换 `⠿ 运行中`
2. **Phase 2**：ToolCallWidget
   - 实现状态指示器和智能折叠
   - 替换 `MessageViewModel::ToolBlock` 渲染
3. **Phase 3**：MessageBlockWidget
   - 迁移 Markdown 渲染到 widget
   - 增加代码高亮和 diff 着色
   - 优化 SubAgent 和 Thinking 折叠

### 依赖

- 无新外部依赖（代码高亮用正则实现，不用 tree-sitter）
- 依赖已有的 `perihelion-widgets` crate（F001_ratatui-widget-lib）
- 需要 TODO 任务列表提供 `activeForm` 字段（已支持）

## 约束一致性

- **Workspace 分层**：新增 widget 在 `perihelion-widgets`（纯通用库），不依赖项目业务逻辑，符合「禁止下层依赖上层」约束
- **异步优先**：Widget 渲染本身是同步的（ratatui 的 render 在主线程），动画帧通过事件驱动，不阻塞异步运行时
- **事件驱动通信**：Widget 不直接访问 Agent 事件 channel，通过 `State` 中间层传递数据，与「禁止共享可变状态」约束一致
- **编码规范**：遵循 Rust 标准 PascalCase/snake_case 命名，widget 通过 `StatefulWidget` trait 实现

## 验收标准

- [ ] SpinnerWidget 能显示动态动词提示（从 TODO activeForm 取得或随机选取）
- [ ] Spinner 支持至少 4 种模式（Thinking/ToolUse/Responding/Idle）的视觉区分
- [ ] Token 计数平滑递增动画工作正常
- [ ] ToolCallWidget 显示状态指示器（闪烁 = 运行中，静态 = 完成/失败）
- [ ] 只读工具默认折叠，写操作默认展开
- [ ] MessageBlockWidget 代码块有基本语法高亮
- [ ] diff 内容（`+`/`-`/`@@ ` 行）有颜色区分
- [ ] 思考内容默认折叠，可展开查看
- [ ] SubAgent 调用步数 > 4 时自动折叠内部消息
- [ ] 所有 widget 通过 ratatui tick 事件驱动动画，无额外线程
- [ ] Headless 测试模式下的截图测试通过
