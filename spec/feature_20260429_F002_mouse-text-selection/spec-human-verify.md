# 鼠标文本选择 人工验收清单

**生成时间:** 2026-04-29
**关联计划:** spec/feature_20260429_F002_mouse-text-selection/spec-plan.md
**关联设计:** spec/feature_20260429_F002_mouse-text-selection/spec-design.md

---

## 验收前准备

### 环境要求
- [ ] [AUTO] 编译项目: `cargo build -p rust-agent-tui 2>&1 | tail -5`
- [ ] [MANUAL] 启动 TUI 应用: `cargo run -p rust-agent-tui`
- [ ] [MANUAL] 在 TUI 中发送至少一条消息，使消息区域有可选中内容

---

## 验收项目

### 场景 1：构建与编译

#### - [x] 1.1 项目编译通过
- **来源:** spec-plan.md Task 0 / Task 1~4 检查步骤
- **目的:** 确认所有改动编译无错误
- **操作步骤:**
  1. [A] `cargo build -p rust-agent-tui 2>&1 | tail -5` → 期望包含: Finished

---

### 场景 2：TextSelection 数据模型单元测试

#### - [x] 2.1 TextSelection 状态管理测试通过
- **来源:** spec-plan.md Task 1 检查步骤 / spec-design.md §数据结构
- **目的:** 确认选区状态转换逻辑正确
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-tui --lib -- app::text_selection::tests 2>&1 | tail -10` → 期望包含: 5 passed

#### - [x] 2.2 AppCore 构造函数不破坏现有测试
- **来源:** spec-plan.md Task 1 检查步骤
- **目的:** 确认新增字段初始化无回归
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-tui --lib -- app::core::tests 2>&1 | tail -5` → 期望包含: test result: ok

---

### 场景 3：WrapMap 计算正确性

#### - [x] 3.1 build_wrap_map 单元测试通过
- **来源:** spec-plan.md Task 2 检查步骤 / spec-design.md §坐标映射与换行计算
- **目的:** 确认换行映射表计算正确
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-tui --lib -- ui::render_thread::tests::test_build_wrap_map 2>&1 | tail -5` → 期望包含: passed

#### - [x] 3.2 渲染线程既有测试无回归
- **来源:** spec-plan.md Task 2 检查步骤
- **目的:** 确认 wrap_map 不影响已有渲染逻辑
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-tui --lib -- ui::render_thread::tests 2>&1 | tail -5` → 期望包含: test result: ok

---

### 场景 4：鼠标事件处理与坐标映射

#### - [x] 4.1 鼠标滚轮事件保留
- **来源:** spec-plan.md Task 3 检查步骤 / spec-design.md §鼠标事件处理
- **目的:** 确认 ScrollUp/ScrollDown 分支未丢失
- **操作步骤:**
  1. [A] `grep -n "ScrollUp\|ScrollDown" rust-agent-tui/src/event.rs` → 期望包含: ScrollUp

#### - [x] 4.2 extract_selected_text 调用包含 usable_width 参数
- **来源:** spec-plan.md Task 3 检查步骤
- **目的:** 确认字符级提取接口调用正确
- **操作步骤:**
  1. [A] `grep -n "extract_selected_text" rust-agent-tui/src/event.rs` → 期望包含: usable_width

#### - [x] 4.3 坐标映射与文本提取测试通过
- **来源:** spec-plan.md Task 3 检查步骤 / spec-design.md §坐标映射与换行计算
- **目的:** 确认字符级坐标映射和文本提取正确
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-tui --lib -- app::text_selection::tests 2>&1 | tail -10` → 期望包含: test result: ok

---

### 场景 5：选区渲染与 Ctrl+C 复制

#### - [x] 5.1 Ctrl+C 选区复制优先级正确
- **来源:** spec-plan.md Task 4 检查步骤 / spec-design.md §Ctrl+C 冲突处理
- **目的:** 确认选区复制在加载中断之前
- **操作步骤:**
  1. [A] `grep -A 10 "key: Key::Char.*c.*ctrl: true" rust-agent-tui/src/event.rs | head -15` → 期望包含: selected_text

#### - [x] 5.2 消息区域 Rect 在渲染时更新
- **来源:** spec-plan.md Task 4 检查步骤 / spec-design.md §鼠标事件处理
- **目的:** 确认 messages_area 用于坐标判定
- **操作步骤:**
  1. [A] `grep -n "messages_area = Some" rust-agent-tui/src/ui/main_ui.rs` → 期望包含: messages_area = Some

#### - [x] 5.3 选区高亮渲染测试通过
- **来源:** spec-plan.md Task 4 检查步骤 / spec-design.md §选区渲染与高亮
- **目的:** 确认字符级 span 拆分高亮正确
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-tui --lib -- ui::main_ui::tests 2>&1 | tail -5` → 期望包含: test result: ok

---

### 场景 6：端到端交互验证

> **前置操作:** 启动 TUI (`cargo run -p rust-agent-tui`)，发送一条包含中文和英文混合内容的消息。

#### - [x] 6.1 鼠标拖拽选中文字有反色高亮
- **来源:** spec-plan.md Task 5.3 / spec-design.md 验收标准 1
- **目的:** 确认拖拽过程有视觉反馈
- **操作步骤:**
  1. [H] 在 TUI 消息区域按住鼠标左键拖拽选中文本 → 观察选中文字是否显示反色高亮 → 是/否

#### - [x] 6.2 松开鼠标后高亮保持
- **来源:** spec-plan.md Task 5.4 / spec-design.md 验收标准 2
- **目的:** 确认选区持久化显示
- **操作步骤:**
  1. [H] 拖拽选中文字后松开鼠标 → 观察选区反色高亮是否仍然保持 → 是/否

#### - [x] 6.3 Ctrl+C 复制选中文字到剪贴板
- **来源:** spec-plan.md Task 5.5 / spec-design.md 验收标准 3
- **目的:** 确认复制功能完整（剪贴板+提示+清除）
- **操作步骤:**
  1. [H] 选中文字后按 Ctrl+C → 观察高亮是否消失、状态栏是否显示"已复制 N 个字符"、在终端粘贴确认剪贴板内容与选中文字一致 → 是/否

#### - [x] 6.4 无选区时 Ctrl+C 保持原有行为
- **来源:** spec-plan.md Task 5.6 / spec-design.md 验收标准 6
- **目的:** 确认 Ctrl+C 优先级链不破坏原有逻辑
- **操作步骤:**
  1. [H] 无选区时按 Ctrl+C（非 loading 状态）→ 观察是否退出应用 → 是/否
  2. [H] 重新启动 TUI，Agent 处理中无选区按 Ctrl+C → 观察是否中断 Agent 而非退出 → 是/否

#### - [x] 6.5 鼠标滚轮滚动不受影响
- **来源:** spec-plan.md Task 5.2 / spec-design.md 验收标准 5
- **目的:** 确认滚轮滚动功能正常
- **操作步骤:**
  1. [H] 在消息区域使用鼠标滚轮上下滚动 → 观察消息列表是否正常滚动、行为与改动前一致 → 是/否

#### - [ ] 6.6 CJK 字符选区计算正确
- **来源:** spec-plan.md Task 5.7 / spec-design.md 验收标准 7
- **目的:** 确认宽字符选区对齐
- **操作步骤:**
  1. [H] 选中包含中文的文本 → 观察选区范围是否与显示对齐、复制出的文字是否完整正确 → 是/否

#### - [ ] 6.7 跨行选区文本提取正确
- **来源:** spec-plan.md Task 5.8 / spec-design.md 验收标准 8
- **目的:** 确认多行选取包含换行符
- **操作步骤:**
  1. [H] 拖拽选中跨越多行的文字，按 Ctrl+C 复制 → 在终端粘贴确认复制的文本包含换行符且多行内容完整 → 是/否

#### - [ ] 6.8 窗口 resize 后选区清除
- **来源:** spec-plan.md Task 5.9 / spec-design.md 验收标准 9
- **目的:** 确认 resize 不残留无效高亮
- **操作步骤:**
  1. [H] 选中文字后调整终端窗口大小 → 观察选区是否自动清除、无残留高亮 → 是/否

---

### 场景 7：全量回归

#### - [ ] 7.1 完整测试套件通过
- **来源:** spec-plan.md Task 5.1 / spec-design.md §约束一致性
- **目的:** 确认无功能回归
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-tui 2>&1 | tail -10` → 期望包含: test result: ok

---

## 验收后清理

无需清理后台服务（TUI 为交互式终端应用，退出即终止）。

---

## 验收结果汇总

| 场景 | 序号 | 验收项 | [A] | [H] | 结果 |
|------|------|--------|-----|-----|------|
| 场景 1 | 1.1 | 项目编译通过 | 1 | 0 | ⬜ |
| 场景 2 | 2.1 | TextSelection 状态管理测试 | 1 | 0 | ⬜ |
| 场景 2 | 2.2 | AppCore 构造无回归 | 1 | 0 | ⬜ |
| 场景 3 | 3.1 | build_wrap_map 测试 | 1 | 0 | ⬜ |
| 场景 3 | 3.2 | 渲染线程无回归 | 1 | 0 | ⬜ |
| 场景 4 | 4.1 | 滚轮事件保留 | 1 | 0 | ⬜ |
| 场景 4 | 4.2 | extract_selected_text 参数 | 1 | 0 | ⬜ |
| 场景 4 | 4.3 | 坐标映射测试 | 1 | 0 | ⬜ |
| 场景 5 | 5.1 | Ctrl+C 优先级 | 1 | 0 | ⬜ |
| 场景 5 | 5.2 | messages_area 更新 | 1 | 0 | ⬜ |
| 场景 5 | 5.3 | 高亮渲染测试 | 1 | 0 | ⬜ |
| 场景 6 | 6.1 | 拖拽反色高亮 | 0 | 1 | ⬜ |
| 场景 6 | 6.2 | 松开后高亮保持 | 0 | 1 | ⬜ |
| 场景 6 | 6.3 | Ctrl+C 复制完整 | 0 | 1 | ⬜ |
| 场景 6 | 6.4 | 无选区 Ctrl+C 行为 | 0 | 2 | ⬜ |
| 场景 6 | 6.5 | 滚轮滚动正常 | 0 | 1 | ⬜ |
| 场景 6 | 6.6 | CJK 字符选区 | 0 | 1 | ⬜ |
| 场景 6 | 6.7 | 跨行选区提取 | 0 | 1 | ⬜ |
| 场景 6 | 6.8 | resize 选区清除 | 0 | 1 | ⬜ |
| 场景 7 | 7.1 | 全量测试通过 | 1 | 0 | ⬜ |

**验收结论:** ⬜ 全部通过 / ⬜ 存在问题
