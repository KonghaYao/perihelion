# syntect 代码块语法高亮 人工验收清单

**生成时间:** 2026-04-29 18:00
**关联计划:** spec/feature_20260429_F001_syntect-codeblock-highlight/spec-plan.md
**关联设计:** spec/feature_20260429_F001_syntect-codeblock-highlight/spec-design.md

---

所有验收项均可自动化验证，无需人类参与。仍将生成清单用于自动执行。

---

## 验收前准备

### 环境要求
- [ ] [AUTO] 编译项目（启用 markdown-highlight）: `cargo build -p perihelion-widgets --features markdown-highlight 2>&1 | tail -3`
- [ ] [AUTO] 编译项目（不启用 markdown-highlight）: `cargo build -p perihelion-widgets --features markdown 2>&1 | tail -3`
- [ ] [AUTO] 编译 rust-agent-tui: `cargo build -p rust-agent-tui 2>&1 | tail -3`

---

## 验收项目

### 场景 1：Feature Flag 和依赖配置

#### - [ ] 1.1 验证 feature 定义正确
- **来源:** spec-plan.md Task 1 检查步骤 / spec-design.md §Feature Flag 结构
- **目的:** 确认 feature 声明完整正确
- **操作步骤:**
  1. [A] `grep -A3 '\[features\]' perihelion-widgets/Cargo.toml` → 期望包含: `markdown-highlight = ["markdown", "dep:syntect"]`

#### - [ ] 1.2 验证 syntect 可选依赖声明
- **来源:** spec-plan.md Task 1 检查步骤 / spec-design.md §依赖声明
- **目的:** 确认 syntect 为可选依赖
- **操作步骤:**
  1. [A] `grep 'syntect' perihelion-widgets/Cargo.toml` → 期望包含: `optional = true`

#### - [ ] 1.3 验证 rust-agent-tui 启用 markdown-highlight
- **来源:** spec-plan.md Task 1 检查步骤
- **目的:** 确认 TUI 应用启用高亮 feature
- **操作步骤:**
  1. [A] `grep 'perihelion-widgets' rust-agent-tui/Cargo.toml` → 期望包含: `features = ["markdown-highlight"]`

#### - [ ] 1.4 验证默认构建不受影响
- **来源:** spec-plan.md Task 1 检查步骤 / spec-design.md §Feature Flag 结构
- **目的:** 确认无 syntect 时仍可正常编译
- **操作步骤:**
  1. [A] `cargo check -p perihelion-widgets 2>&1 | tail -3` → 期望包含: `Finished`

---

### 场景 2：高亮引擎模块

#### - [ ] 2.1 验证 highlight 模块文件存在
- **来源:** spec-plan.md Task 2 检查步骤
- **目的:** 确认高亮引擎模块已创建
- **操作步骤:**
  1. [A] `test -f perihelion-widgets/src/markdown/highlight.rs && echo "exists"` → 期望包含: `exists`

#### - [ ] 2.2 验证 mod.rs 中 cfg-gated 注册
- **来源:** spec-plan.md Task 2 检查步骤
- **目的:** 确认模块仅在 feature 启用时编译
- **操作步骤:**
  1. [A] `grep -B1 'mod highlight' perihelion-widgets/src/markdown/mod.rs` → 期望包含: `#[cfg(feature = "markdown-highlight")]`

#### - [ ] 2.3 验证 highlight_code_block 函数签名
- **来源:** spec-plan.md Task 2 检查步骤
- **目的:** 确认公开 API 签名正确
- **操作步骤:**
  1. [A] `grep 'pub fn highlight_code_block' perihelion-widgets/src/markdown/highlight.rs` → 期望包含: `Option<Vec<Line<'static>>>`

#### - [ ] 2.4 验证 Rust 代码高亮单元测试通过
- **来源:** spec-plan.md Task 2 执行步骤 / spec-design.md §高亮函数
- **目的:** 确认 Rust 语法高亮产生多色输出
- **操作步骤:**
  1. [A] `cargo test -p perihelion-widgets --features markdown-highlight -- highlight_rust_code 2>&1` → 期望包含: `test markdown::highlight::tests::highlight_rust_code ... ok`

#### - [ ] 2.5 验证未识别语言返回 None
- **来源:** spec-plan.md Task 2 执行步骤 / spec-design.md §高亮函数
- **目的:** 确认 fallback 机制正常
- **操作步骤:**
  1. [A] `cargo test -p perihelion-widgets --features markdown-highlight -- highlight_unknown_lang 2>&1` → 期望包含: `test markdown::highlight::tests::highlight_unknown_lang ... ok`

#### - [ ] 2.6 验证空语言标签返回 None
- **来源:** spec-plan.md Task 2 执行步骤
- **目的:** 确认省略语言标签时触发 fallback
- **操作步骤:**
  1. [A] `cargo test -p perihelion-widgets --features markdown-highlight -- highlight_empty_lang 2>&1` → 期望包含: `test markdown::highlight::tests::highlight_empty_lang ... ok`

#### - [ ] 2.7 验证多行代码跨行高亮正确
- **来源:** spec-plan.md Task 2 执行步骤 / spec-design.md §实现要点
- **目的:** 确认 HighlightLines 跨行状态正确
- **操作步骤:**
  1. [A] `cargo test -p perihelion-widgets --features markdown-highlight -- highlight_multiline 2>&1` → 期望包含: `test markdown::highlight::tests::highlight_multiline ... ok`

---

### 场景 3：渲染集成

#### - [ ] 3.1 验证 render_state.rs 条件导入
- **来源:** spec-plan.md Task 3 检查步骤
- **目的:** 确认 highlight 函数仅在 feature 启用时导入
- **操作步骤:**
  1. [A] `grep -B1 'use super::highlight::highlight_code_block' perihelion-widgets/src/markdown/render_state.rs` → 期望包含: `#[cfg(feature = "markdown-highlight")]`

#### - [ ] 3.2 验证 cfg-gated 分支数量
- **来源:** spec-plan.md Task 3 检查步骤
- **目的:** 确认双 cfg 分支（启用/未启用）均已实现
- **操作步骤:**
  1. [A] `grep -c 'markdown-highlight' perihelion-widgets/src/markdown/render_state.rs` → 期望包含: `3`

#### - [ ] 3.3 验证多行 Rust 代码块显示多色语法高亮
- **来源:** spec-plan.md Task 3 执行步骤 / spec-design.md §验收标准
- **目的:** 确认集成后 Rust 代码产生多色输出
- **操作步骤:**
  1. [A] `cargo test -p perihelion-widgets --features markdown-highlight -- parse_multiline_code_block_rust_highlight 2>&1` → 期望包含: `test markdown::tests::parse_multiline_code_block_rust_highlight ... ok`

#### - [ ] 3.4 验证未识别语言标签回退到统一颜色
- **来源:** spec-plan.md Task 3 执行步骤 / spec-design.md §验收标准
- **目的:** 确认 fallback 使用 theme.text() 颜色
- **操作步骤:**
  1. [A] `cargo test -p perihelion-widgets --features markdown-highlight -- parse_multiline_code_block_unknown_lang_fallback 2>&1` → 期望包含: `test markdown::tests::parse_multiline_code_block_unknown_lang_fallback ... ok`

#### - [ ] 3.5 验证省略语言标签回退到统一颜色
- **来源:** spec-plan.md Task 3 执行步骤 / spec-design.md §验收标准
- **目的:** 确认无语言标签时走 fallback 路径
- **操作步骤:**
  1. [A] `cargo test -p perihelion-widgets --features markdown-highlight -- parse_multiline_code_block_no_lang_fallback 2>&1` → 期望包含: `test markdown::tests::parse_multiline_code_block_no_lang_fallback ... ok`

---

### 场景 4：回归与兼容性

#### - [ ] 4.1 验证单行代码块行为不变
- **来源:** spec-plan.md Task 4 验收 / spec-design.md §单行代码块
- **目的:** 确认单行代码块仍使用 code() 颜色
- **操作步骤:**
  1. [A] `cargo test -p perihelion-widgets --features markdown-highlight -- parse_code_block 2>&1` → 期望包含: `test markdown::tests::parse_code_block ... ok`

#### - [ ] 4.2 验证不启用 markdown-highlight 时全部测试通过
- **来源:** spec-plan.md Task 4 验收 / spec-design.md §验收标准
- **目的:** 确认 feature flag 隔离无回归
- **操作步骤:**
  1. [A] `cargo test -p perihelion-widgets --features markdown 2>&1` → 期望包含: `test result: ok`

#### - [ ] 4.3 验证启用 markdown-highlight 后全量测试通过
- **来源:** spec-plan.md Task 4 验收
- **目的:** 确认无回归且新增测试通过
- **操作步骤:**
  1. [A] `cargo test -p perihelion-widgets --features markdown-highlight 2>&1` → 期望包含: `test result: ok`

#### - [ ] 4.4 验证 rust-agent-tui 完整构建
- **来源:** spec-plan.md Task 4 验收
- **目的:** 确认 TUI 应用集成无误
- **操作步骤:**
  1. [A] `cargo build -p rust-agent-tui 2>&1 | tail -3` → 期望包含: `Finished`

---

## 验收后清理

无需清理（本次验收不涉及后台服务启动）。

---

## 验收结果汇总

| 场景 | 序号 | 验收项 | [A] | [H] | 结果 |
|------|------|--------|-----|-----|------|
| 场景 1 | 1.1 | feature 定义正确 | 1 | 0 | ⬜ |
| 场景 1 | 1.2 | syntect 可选依赖声明 | 1 | 0 | ⬜ |
| 场景 1 | 1.3 | TUI 启用 markdown-highlight | 1 | 0 | ⬜ |
| 场景 1 | 1.4 | 默认构建不受影响 | 1 | 0 | ⬜ |
| 场景 2 | 2.1 | highlight 模块文件存在 | 1 | 0 | ⬜ |
| 场景 2 | 2.2 | cfg-gated 注册 | 1 | 0 | ⬜ |
| 场景 2 | 2.3 | 函数签名正确 | 1 | 0 | ⬜ |
| 场景 2 | 2.4 | Rust 代码高亮测试 | 1 | 0 | ⬜ |
| 场景 2 | 2.5 | 未识别语言返回 None | 1 | 0 | ⬜ |
| 场景 2 | 2.6 | 空语言标签返回 None | 1 | 0 | ⬜ |
| 场景 2 | 2.7 | 多行跨行高亮正确 | 1 | 0 | ⬜ |
| 场景 3 | 3.1 | 条件导入正确 | 1 | 0 | ⬜ |
| 场景 3 | 3.2 | cfg 分支数量正确 | 1 | 0 | ⬜ |
| 场景 3 | 3.3 | Rust 多色高亮集成 | 1 | 0 | ⬜ |
| 场景 3 | 3.4 | 未识别语言 fallback | 1 | 0 | ⬜ |
| 场景 3 | 3.5 | 省略语言标签 fallback | 1 | 0 | ⬜ |
| 场景 4 | 4.1 | 单行代码块不变 | 1 | 0 | ⬜ |
| 场景 4 | 4.2 | 不启用 feature 测试通过 | 1 | 0 | ⬜ |
| 场景 4 | 4.3 | 全量测试通过 | 1 | 0 | ⬜ |
| 场景 4 | 4.4 | TUI 完整构建 | 1 | 0 | ⬜ |

**验收结论:** ⬜ 全部通过 / ⬜ 存在问题
