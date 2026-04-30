# 系统提示词重构 人工验收清单

**生成时间:** 2026-04-30
**关联计划:** spec/feature_20260430_F001_system-prompt-restructure/spec-plan.md
**关联设计:** spec/feature_20260430_F001_system-prompt-restructure/spec-design.md

---

所有验收项均可自动化验证，无需人类参与。仍将生成清单用于自动执行。

---

## 验收前准备

### 环境要求
- [x] [AUTO] 检查 Rust 工具链: `rustc --version && cargo --version`
- [x] [AUTO] 编译全 workspace: `cargo build 2>&1 | tail -3`
- [x] [AUTO] 运行全量测试确认基线: `cargo test 2>&1 | tail -5`

---

## 验收项目

### 场景 1: 环境准备

#### - [x] 1.1 Rust 工具链可用
- **来源:** spec-plan.md Task 0
- **目的:** 确认构建环境就绪
- [A] `rustc --version && cargo --version` → 期望包含: `rustc`

#### - [x] 1.2 项目可编译
- **来源:** spec-plan.md Task 0
- **目的:** 确认无编译错误
- [A] `cargo build 2>&1 | grep -c 'error'` → 期望精确: `0`

#### - [x] 1.3 现有测试通过
- **来源:** spec-plan.md Task 0
- **目的:** 确认基线无回归
- [A] `cargo test 2>&1 | tail -5` → 期望包含: `test result: ok`

---

### 场景 2: 静态提示词段落迁移

#### - [x] 2.1 sections 目录下有 8 个静态段落文件
- **来源:** spec-plan.md Task 1 检查步骤
- **目的:** 确认段落文件全部创建
- [A] `ls rust-agent-tui/prompts/sections/*.md | wc -l` → 期望包含: `8`

#### - [x] 2.2 每个静态段落文件内容非空
- **来源:** spec-plan.md Task 1 检查步骤
- **目的:** 确认段落内容完整
- [A] `for f in rust-agent-tui/prompts/sections/*.md; do test -s "$f" && echo "$f OK"; done` → 期望包含: `OK`

#### - [x] 2.3 08_env.md 包含 5 个占位符
- **来源:** spec-plan.md Task 1 检查步骤
- **目的:** 确认环境模板完整
- [A] `grep -c '{{' rust-agent-tui/prompts/sections/08_env.md` → 期望精确: `5`

#### - [x] 2.4 prompt.rs 不再引用旧常量
- **来源:** spec-plan.md Task 1 检查步骤
- **目的:** 确认旧常量已删除
- [A] `grep -c 'SYSTEM_PROMPT_TEMPLATE\|SYSTEM_PROMPT_DEFAULT_AGENT' rust-agent-tui/src/prompt.rs` → 期望精确: `0`

#### - [x] 2.5 rust-agent-tui 编译通过
- **来源:** spec-plan.md Task 1 检查步骤
- **目的:** 确认 include_str! 加载正确
- [A] `cargo build -p rust-agent-tui 2>&1 | tail -3` → 期望包含: `Finished`

#### - [x] 2.6 静态段落单元测试通过
- **来源:** spec-plan.md Task 1 检查步骤
- **目的:** 确认 build_system_prompt 基础行为正确
- [A] `cargo test -p rust-agent-tui --lib -- prompt::tests 2>&1 | tail -5` → 期望包含: `test result: ok`

---

### 场景 3: Feature-gated 提示词机制

#### - [x] 3.1 sections 目录下共 12 个段落文件
- **来源:** spec-plan.md Task 2 检查步骤
- **目的:** 确认 feature-gated 段落已创建
- [A] `ls rust-agent-tui/prompts/sections/ | wc -l` → 期望精确: `12`

#### - [x] 3.2 新增 4 个 feature 段落文件非空
- **来源:** spec-plan.md Task 2 检查步骤
- **目的:** 确认 feature 段落内容完整
- [A] `for f in rust-agent-tui/prompts/sections/1{0,1,2,3}_*.md; do test -s "$f" && echo "$f OK"; done` → 期望包含: `OK`

#### - [x] 3.3 PromptFeatures 结构体已导出
- **来源:** spec-plan.md Task 2 检查步骤
- **目的:** 确认 PromptFeatures 公开可用
- [A] `grep -c 'pub struct PromptFeatures' rust-agent-tui/src/prompt.rs` → 期望精确: `1`

#### - [x] 3.4 build_system_prompt 签名包含 features 参数
- **来源:** spec-plan.md Task 2 检查步骤
- **目的:** 确认函数签名已更新
- [A] `grep 'fn build_system_prompt' rust-agent-tui/src/prompt.rs` → 期望包含: `features: PromptFeatures`

#### - [x] 3.5 agent.rs 两处调用均已更新
- **来源:** spec-plan.md Task 2 检查步骤
- **目的:** 确认调用方同步修改
- [A] `grep -c 'PromptFeatures::detect()' rust-agent-tui/src/app/agent.rs` → 期望精确: `2`

#### - [x] 3.6 Feature-gated 机制编译通过
- **来源:** spec-plan.md Task 2 检查步骤
- **目的:** 确认条件注入无编译错误
- [A] `cargo build -p rust-agent-tui 2>&1 | tail -3` → 期望包含: `Finished`

#### - [x] 3.7 Feature-gated 单元测试通过
- **来源:** spec-plan.md Task 2 检查步骤
- **目的:** 确认条件注入逻辑正确
- [A] `cargo test -p rust-agent-tui --lib -- prompt::tests 2>&1 | tail -5` → 期望包含: `test result: ok`

---

### 场景 4: 工具提示词扩展

#### - [x] 4.1 所有 9 个工具文件包含 description 常量
- **来源:** spec-plan.md Task 3 检查步骤
- **目的:** 确认 description 已提取为常量
- [A] `for f in rust-agent-middlewares/src/tools/filesystem/read.rs rust-agent-middlewares/src/tools/filesystem/write.rs rust-agent-middlewares/src/tools/filesystem/edit.rs rust-agent-middlewares/src/tools/filesystem/glob.rs rust-agent-middlewares/src/tools/filesystem/grep.rs rust-agent-middlewares/src/middleware/terminal.rs rust-agent-middlewares/src/tools/filesystem/folder.rs rust-agent-middlewares/src/tools/todo.rs rust-agent-middlewares/src/subagent/tool.rs; do echo "$f: $(grep -c '_DESCRIPTION' "$f")"; done` → 期望包含: `1`（每个文件至少 1 个）

#### - [x] 4.2 description() 方法引用常量而非行内字符串
- **来源:** spec-plan.md Task 3 检查步骤
- **目的:** 确认引用方式正确
- [A] `grep -A1 'fn description' rust-agent-middlewares/src/tools/filesystem/read.rs` → 期望包含: `READ_FILE_DESCRIPTION`

#### - [x] 4.3 无 claude-code PascalCase 工具名残留
- **来源:** spec-plan.md Task 3 检查步骤
- **目的:** 确认工具名已替换为 snake_case
- [A] `grep -rn 'use the Read tool\|use the Write tool\|use the Edit tool\|use the Glob tool\|use the Grep tool\|use the Bash tool' rust-agent-middlewares/src/ || echo "clean"` → 期望精确: `clean`

#### - [x] 4.4 rust-agent-middlewares 编译通过
- **来源:** spec-plan.md Task 3 检查步骤
- **目的:** 确认 description 扩展无编译错误
- [A] `cargo build -p rust-agent-middlewares 2>&1 | tail -3` → 期望包含: `Finished`

#### - [x] 4.5 description 扩展测试通过
- **来源:** spec-plan.md Task 3 检查步骤
- **目的:** 确认所有 9 个工具 description 扩展正确
- [A] `cargo test -p rust-agent-middlewares --lib -- test_description_extended 2>&1 | tail -10` → 期望包含: `test result: ok`

#### - [x] 4.6 所有中间件现有测试无回归
- **来源:** spec-plan.md Task 3 检查步骤
- **目的:** 确认工具扩展未破坏现有功能
- [A] `cargo test -p rust-agent-middlewares 2>&1 | tail -5` → 期望包含: `test result: ok`

---

### 场景 5: 清理与验收

#### - [x] 5.1 旧提示词文件已删除
- **来源:** spec-plan.md Task 4 检查步骤
- **目的:** 确认旧文件清理完毕
- [A] `ls rust-agent-tui/prompts/system.md rust-agent-tui/prompts/default.md 2>&1` → 期望包含: `No such file or directory`

#### - [x] 5.2 prompt.rs 不再包含旧文件引用
- **来源:** spec-plan.md Task 4 检查步骤
- **目的:** 确认无残留 include_str! 旧文件
- [A] `grep -c 'prompts/system.md\|prompts/default.md' rust-agent-tui/src/prompt.rs` → 期望精确: `0`

#### - [x] 5.3 unwrap_or_default() 替换已生效
- **来源:** spec-plan.md Task 4 检查步骤
- **目的:** 确认 None 分支简化完成
- [A] `grep 'unwrap_or_default' rust-agent-tui/src/prompt.rs` → 期望包含: `unwrap_or_default()`

#### - [x] 5.4 全 workspace 编译通过
- **来源:** spec-plan.md Task 4 检查步骤
- **目的:** 确认清理后无编译错误
- [A] `cargo build 2>&1 | tail -5` → 期望包含: `Finished`

#### - [x] 5.5 全量测试通过
- **来源:** spec-plan.md Task 4 检查步骤
- **目的:** 确认清理后无功能回归
- [A] `cargo test 2>&1 | tail -10` → 期望包含: `test result: ok`

#### - [x] 5.6 CLAUDE.md 已更新含 PromptFeatures
- **来源:** spec-plan.md Task 4 检查步骤
- **目的:** 确认文档同步更新
- [A] `grep -c 'PromptFeatures' CLAUDE.md` → 期望包含: `1`

#### - [x] 5.7 CLAUDE.md 不再引用 PrependSystemMiddleware
- **来源:** spec-plan.md Task 4 检查步骤
- **目的:** 确认废弃引用已清除
- [A] `grep -c 'PrependSystemMiddleware' CLAUDE.md` → 期望精确: `0`

---

### 场景 6: 端到端功能验收

#### - [x] 6.1 完整测试套件无回归
- **来源:** spec-plan.md Task 5 端到端验证
- **目的:** 确认所有 crate 测试通过
- [A] `cargo test 2>&1 | tail -10` → 期望包含: `test result: ok`

#### - [x] 6.2 TUI 启动正常
- **来源:** spec-plan.md Task 5 端到端验证
- **目的:** 确认 include_str! 引用文件全部存在
- [A] `cargo run -p rust-agent-tui -- --help 2>&1 || echo "exit: $?"` → 期望包含: `--approve` 或 `rust-agent-tui`

#### - [x] 6.3 HITL 审批模式下 feature 段落注入
- **来源:** spec-plan.md Task 5 端到端验证
- **目的:** 确认 YOLO_MODE=false 时 hitl_enabled 为 true
- [A] `YOLO_MODE=false cargo test -p rust-agent-tui --lib -- prompt::tests 2>&1 | tail -5` → 期望包含: `test result: ok`

#### - [x] 6.4 sections 目录结构完整（12 个文件）
- **来源:** spec-plan.md Task 5 端到端验证
- **目的:** 确认 8 静态 + 4 feature-gated 文件齐全
- [A] `ls rust-agent-tui/prompts/sections/ | wc -l` → 期望精确: `12`

#### - [x] 6.5 旧文件完全清除
- **来源:** spec-plan.md Task 5 端到端验证
- **目的:** 确认 prompts 顶层无 .md 文件
- [A] `find rust-agent-tui/prompts/ -maxdepth 1 -name '*.md' 2>/dev/null | wc -l` → 期望精确: `0`

#### - [x] 6.6 工具 description 扩展端到端验证
- **来源:** spec-plan.md Task 5 端到端验证
- **目的:** 确认 9 个工具 description 扩展测试通过
- [A] `cargo test -p rust-agent-middlewares --lib -- test_description_extended 2>&1 | tail -10` → 期望包含: `test result: ok`

---

### 场景 7: 边界与回归

#### - [x] 7.1 PromptFeatures::detect() 默认值合理
- **来源:** spec-plan.md Task 2 + spec-design.md §3
- **目的:** 确认默认环境下 hitl_enabled 为 false
- [A] `cargo test -p rust-agent-tui --lib -- prompt::tests::test_detect 2>&1 | tail -3` → 期望包含: `ok`

#### - [x] 7.2 无 overrides 时 Tone/Proactiveness 不重复注入
- **来源:** spec-plan.md Task 4 + spec-design.md §3
- **目的:** 确认默认内容仅在静态段落中出现一次
- [A] `cargo test -p rust-agent-tui --lib -- prompt::tests 2>&1 | tail -5` → 期望包含: `test result: ok`

#### - [x] 7.3 有 overrides 时覆盖块正确注入
- **来源:** spec-plan.md Task 4
- **目的:** 确认自定义 persona 正确置顶
- [A] `cargo test -p rust-agent-tui --lib -- prompt::tests 2>&1 | tail -5` → 期望包含: `test result: ok`

---

## 验收后清理

（本特性无后台服务需要终止，无测试数据需要清理。）

---

## 验收结果汇总

| 场景 | 序号 | 验收项 | [A] | [H] | 结果 |
|------|------|--------|-----|-----|------|
| 场景 1 | 1.1 | Rust 工具链可用 | Y | - | ✓ |
| 场景 1 | 1.2 | 项目可编译 | Y | - | ✓ |
| 场景 1 | 1.3 | 现有测试通过 | Y | - | ✓ |
| 场景 2 | 2.1 | 8 个静态段落文件 | Y | - | ✓ |
| 场景 2 | 2.2 | 段落文件内容非空 | Y | - | ✓ |
| 场景 2 | 2.3 | 08_env.md 含 5 个占位符 | Y | - | ✓ |
| 场景 2 | 2.4 | 旧常量已删除 | Y | - | ✓ |
| 场景 2 | 2.5 | tui 编译通过 | Y | - | ✓ |
| 场景 2 | 2.6 | 静态段落单元测试通过 | Y | - | ✓ |
| 场景 3 | 3.1 | 共 12 个段落文件 | Y | - | ✓ |
| 场景 3 | 3.2 | feature 段落文件非空 | Y | - | ✓ |
| 场景 3 | 3.3 | PromptFeatures 已导出 | Y | - | ✓ |
| 场景 3 | 3.4 | 签名含 features 参数 | Y | - | ✓ |
| 场景 3 | 3.5 | agent.rs 两处调用更新 | Y | - | ✓ |
| 场景 3 | 3.6 | Feature-gated 编译通过 | Y | - | ✓ |
| 场景 3 | 3.7 | Feature-gated 测试通过 | Y | - | ✓ |
| 场景 4 | 4.1 | 9 个工具含 description 常量 | Y | - | ✓ |
| 场景 4 | 4.2 | description() 引用常量 | Y | - | ✓ |
| 场景 4 | 4.3 | 无 PascalCase 工具名残留 | Y | - | ✓ |
| 场景 4 | 4.4 | middlewares 编译通过 | Y | - | ✓ |
| 场景 4 | 4.5 | description 扩展测试通过 | Y | - | ✓ |
| 场景 4 | 4.6 | 中间件现有测试无回归 | Y | - | ✓ |
| 场景 5 | 5.1 | 旧提示词文件已删除 | Y | - | ✓ |
| 场景 5 | 5.2 | 无旧文件引用残留 | Y | - | ✓ |
| 场景 5 | 5.3 | unwrap_or_default 生效 | Y | - | ✓ |
| 场景 5 | 5.4 | 全 workspace 编译通过 | Y | - | ✓ |
| 场景 5 | 5.5 | 全量测试通过 | Y | - | ✓ |
| 场景 5 | 5.6 | CLAUDE.md 含 PromptFeatures | Y | - | ✓ |
| 场景 5 | 5.7 | CLAUDE.md 无 PrependSystemMiddleware | Y | - | ✓ |
| 场景 6 | 6.1 | 完整测试套件无回归 | Y | - | ✓ |
| 场景 6 | 6.2 | TUI 启动正常 | Y | - | ✓ |
| 场景 6 | 6.3 | HITL 模式 feature 注入 | Y | - | ✓ |
| 场景 6 | 6.4 | 目录结构完整 12 文件 | Y | - | ✓ |
| 场景 6 | 6.5 | 旧文件完全清除 | Y | - | ✓ |
| 场景 6 | 6.6 | 工具 description 端到端验证 | Y | - | ✓ |
| 场景 7 | 7.1 | detect() 默认值合理 | Y | - | ✓ |
| 场景 7 | 7.2 | Tone/Proactiveness 不重复 | Y | - | ✓ |
| 场景 7 | 7.3 | overrides 覆盖块正确注入 | Y | - | ✓ |

**验收结论:** ✓ 全部通过
