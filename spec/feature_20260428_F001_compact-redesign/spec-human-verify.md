# Compact 系统重设计 人工验收清单

**生成时间:** 2026-04-28
**关联计划:** spec-plan-1.md / spec-plan-2.md
**关联设计:** spec-design.md

---

## 验收前准备

### 环境要求
- [ ] [AUTO] 检查 Cargo 构建工具可用: `cargo --version`
- [ ] [AUTO] 编译 rust-create-agent: `cargo build -p rust-create-agent 2>&1 | tail -3`
- [ ] [AUTO] 编译 rust-agent-tui: `cargo build -p rust-agent-tui 2>&1 | tail -3`

### 测试数据准备
- 无需额外测试数据，所有验证通过单元测试和 Mock 驱动

---

## 验收项目

### 场景 1: 核心层模块构建与注册

#### - [x] 1.1 rust-create-agent 编译通过
- **来源:** plan-1 Task 1 §检查步骤 / design §整体架构
- **目的:** 确认核心层代码无编译错误
- **操作步骤:**
  1. [A] `cargo build -p rust-create-agent 2>&1 | tail -5` → 期望包含: "Finished"

#### - [x] 1.2 compact 子模块完整注册（5 个子模块）
- **来源:** plan-1 Task 1-4 / plan-2 Task 5 §mod.rs
- **目的:** 确认 config/invariant/micro/full/re_inject 均已注册
- **操作步骤:**
  1. [A] `grep -c 'pub mod' rust-create-agent/src/agent/compact/mod.rs` → 期望包含: "5"

#### - [x] 1.3 compact 模块公共 API 导出完整
- **来源:** plan-1 Task 1-4 / plan-2 Task 5 §mod.rs
- **目的:** 确认核心类型和函数均可通过 crate::agent::compact 访问
- **操作步骤:**
  1. [A] `grep -c 'pub use' rust-create-agent/src/agent/compact/mod.rs` → 期望包含: "5"

---

### 场景 2: CompactConfig 配置体系

#### - [x] 2.1 CompactConfig 结构体 12 个可配置字段完整
- **来源:** plan-1 Task 1 §CompactConfig / design §CompactConfig 结构体
- **目的:** 确认配置字段与设计规格对齐
- **操作步骤:**
  1. [A] `grep -c 'pub ' rust-create-agent/src/agent/compact/config.rs` → 期望包含: "1" 不适用，改用 `grep 'pub auto_compact_enabled\|pub auto_compact_threshold\|pub micro_compact_threshold\|pub micro_compact_stale_steps\|pub micro_compactable_tools\|pub summary_max_tokens\|pub re_inject_max_files\|pub re_inject_max_tokens_per_file\|pub re_inject_file_budget\|pub re_inject_skills_budget\|pub max_consecutive_failures\|pub ptl_max_retries' rust-create-agent/src/agent/compact/config.rs | wc -l` → 期望包含: "12"

#### - [x] 2.2 CompactConfig 单元测试全部通过
- **来源:** plan-1 Task 1 §单元测试 / design §环境变量覆盖
- **目的:** 确认默认值/serde/环境变量覆盖 12 个场景均正确
- **操作步骤:**
  1. [A] `cargo test -p rust-create-agent --lib -- config::tests 2>&1 | tail -20` → 期望包含: "test result: ok"

---

### 场景 3: 工具对完整性保护

#### - [x] 3.1 invariant 模块 3 个公共 API 正确导出
- **来源:** plan-1 Task 2 §mod.rs
- **目的:** 确认分组/调整/结构体 API 可访问
- **操作步骤:**
  1. [A] `grep -c 'adjust_index_to_preserve_invariants\|group_messages_by_round\|MessageRound' rust-create-agent/src/agent/compact/mod.rs` → 期望包含: "3"

#### - [x] 3.2 invariant 单元测试全部通过
- **来源:** plan-1 Task 2 §单元测试
- **目的:** 确认 9 个分组/边界测试场景均正确
- **操作步骤:**
  1. [A] `cargo test -p rust-create-agent --lib -- compact::invariant::tests 2>&1 | tail -20` → 期望包含: "test result: ok"

---

### 场景 4: Micro-compact 增强策略

#### - [x] 4.1 micro 模块注册并导出 micro_compact_enhanced
- **来源:** plan-1 Task 3 §mod.rs
- **目的:** 确认增强版函数可被外部调用
- **操作步骤:**
  1. [A] `grep -n 'pub mod micro' rust-create-agent/src/agent/compact/mod.rs && grep -n 'pub use micro' rust-create-agent/src/agent/compact/mod.rs` → 期望包含: "pub mod micro" 和 "pub use micro::micro_compact_enhanced"

#### - [x] 4.2 旧 micro_compact 标记为 deprecated
- **来源:** plan-1 Task 3 §token.rs / design §向后兼容
- **目的:** 确认旧 API 已标记迁移提示
- **操作步骤:**
  1. [A] `grep -n 'deprecated' rust-create-agent/src/agent/token.rs` → 期望包含: "deprecated" 和 "micro_compact_enhanced"

#### - [x] 4.3 micro_compact_enhanced 函数签名完整
- **来源:** plan-1 Task 3 §函数签名
- **目的:** 确认接受 config 和 messages 参数
- **操作步骤:**
  1. [A] `grep -n 'pub fn micro_compact_enhanced' rust-create-agent/src/agent/compact/micro.rs` → 期望包含: "config: &CompactConfig, messages: &mut [BaseMessage]"

#### - [x] 4.4 Micro-compact 单元测试全部通过
- **来源:** plan-1 Task 3 §单元测试
- **目的:** 确认白名单/时间衰减/图片清除/工具对保护 15+ 场景正确
- **操作步骤:**
  1. [A] `cargo test -p rust-create-agent --lib -- compact::micro::tests 2>&1 | tail -20` → 期望包含: "test result: ok"

#### - [x] 4.5 旧 micro_compact 测试仍兼容（deprecated 不阻断）
- **来源:** plan-1 Task 3 §检查步骤
- **目的:** 确认向后兼容
- **操作步骤:**
  1. [A] `cargo test -p rust-create-agent --lib -- token::tests::test_micro_compact 2>&1 | tail -10` → 期望包含: "test result: ok"

---

### 场景 5: Full Compact 结构化摘要

#### - [x] 5.1 full 模块注册并导出 full_compact + FullCompactResult
- **来源:** plan-1 Task 4 §mod.rs
- **目的:** 确认核心摘要函数可被外部调用
- **操作步骤:**
  1. [A] `grep -n 'pub mod full' rust-create-agent/src/agent/compact/mod.rs && grep -n 'pub use full' rust-create-agent/src/agent/compact/mod.rs` → 期望包含: "pub mod full" 和 "pub use full::{full_compact, FullCompactResult}"

#### - [x] 5.2 full_compact 函数签名完整
- **来源:** plan-1 Task 4 §函数签名
- **目的:** 确认接受 model/config/instructions 参数
- **操作步骤:**
  1. [A] `grep -n 'pub async fn full_compact' rust-create-agent/src/agent/compact/full.rs` → 期望包含: "model: &dyn BaseModel, config: &CompactConfig, instructions: &str"

#### - [x] 5.3 9 段结构化摘要模板存在
- **来源:** plan-1 Task 4 §摘要 Prompt / design §结构化摘要 Prompt
- **目的:** 确认对齐 Claude Code 的摘要质量
- **操作步骤:**
  1. [A] `grep -c 'Primary Request and Intent' rust-create-agent/src/agent/compact/full.rs` → 期望包含: "1"

#### - [x] 5.4 PTL 降级重试逻辑存在
- **来源:** plan-1 Task 4 §PTL 降级 / design §PTL 降级重试
- **目的:** 确认超长对话也能成功压缩
- **操作步骤:**
  1. [A] `grep -c 'is_ptl_error\|truncate_for_ptl' rust-create-agent/src/agent/compact/full.rs` → 期望包含: "4"

#### - [x] 5.5 后处理函数（移除 analysis / 提取 summary）存在
- **来源:** plan-1 Task 4 §后处理 / design §后处理
- **目的:** 确认摘要输出仅保留结构化内容
- **操作步骤:**
  1. [A] `grep -c 'postprocess_summary\|remove_analysis_blocks\|extract_summary_content' rust-create-agent/src/agent/compact/full.rs` → 期望包含: "6"

#### - [x] 5.6 Full Compact 单元测试全部通过
- **来源:** plan-1 Task 4 §单元测试
- **目的:** 确认预处理/后处理/PTL/集成 20+ 场景正确
- **操作步骤:**
  1. [A] `cargo test -p rust-create-agent --lib -- compact::full::tests 2>&1 | tail -20` → 期望包含: "test result: ok"

---

### 场景 6: 重新注入

#### - [x] 6.1 re_inject 模块注册并导出核心函数和类型
- **来源:** plan-2 Task 5 §mod.rs
- **目的:** 确认 re_inject + ReInjectResult 可被外部调用
- **操作步骤:**
  1. [A] `grep -n 'pub mod re_inject' rust-create-agent/src/agent/compact/mod.rs && grep -n 'pub use re_inject' rust-create-agent/src/agent/compact/mod.rs` → 期望包含: "pub mod re_inject" 和 "pub use re_inject::{re_inject, ReInjectResult}"

#### - [x] 6.2 核心函数与辅助函数签名存在
- **来源:** plan-2 Task 5 §函数签名
- **目的:** 确认提取/截断/注入函数完整
- **操作步骤:**
  1. [A] `grep -c 'pub async fn re_inject\|fn extract_recent_files\|fn extract_skills_paths\|fn is_skills_path\|async fn read_file_with_budget\|fn truncate_to_budget' rust-create-agent/src/agent/compact/re_inject.rs` → 期望包含: "6"

#### - [x] 6.3 Re-inject 单元测试全部通过
- **来源:** plan-2 Task 5 §单元测试
- **目的:** 确认路径提取/截断/注入/集成 20+ 场景正确
- **操作步骤:**
  1. [A] `cargo test -p rust-create-agent --lib -- compact::re_inject::tests 2>&1 | tail -20` → 期望包含: "test result: ok"

---

### 场景 7: TUI 层集成

#### - [x] 7.1 rust-agent-tui 编译通过（含 AppConfig compact 字段）
- **来源:** plan-2 Task 6 §AppConfig / design §settings.json 新增字段
- **目的:** 确认 TUI 层集成无编译错误
- **操作步骤:**
  1. [A] `cargo build -p rust-agent-tui 2>&1 | tail -5` → 期望包含: "Finished"

#### - [x] 7.2 compact_task 新签名包含 config + cwd 参数
- **来源:** plan-2 Task 6 §compact_task 重写
- **目的:** 确认核心层三阶段流程正确调用
- **操作步骤:**
  1. [A] `grep -n 'pub async fn compact_task' rust-agent-tui/src/app/agent.rs` → 期望包含: "config:" 和 "cwd:"

#### - [x] 7.3 compact_task 不再包含旧的自由格式摘要逻辑
- **来源:** plan-2 Task 6 §compact_task 重写
- **目的:** 确认旧代码已完全替换
- **操作步骤:**
  1. [A] `grep -c 'truncate_content\|500 字以内' rust-agent-tui/src/app/agent.rs` → 期望精确: "0"

#### - [x] 7.4 compact_task 调用核心层 full_compact + re_inject
- **来源:** plan-2 Task 6 §compact_task 重写
- **目的:** 确认委托核心层执行压缩和注入
- **操作步骤:**
  1. [A] `grep -c 'full_compact\|re_inject(\|RE_INJECT_SEPARATOR' rust-agent-tui/src/app/agent.rs` → 期望包含: "3"

#### - [x] 7.5 start_compact 传递 CompactConfig 和 cwd
- **来源:** plan-2 Task 6 §start_compact 扩展
- **目的:** 确认配置和工作目录正确传入
- **操作步骤:**
  1. [A] `grep -n 'get_compact_config\|cwd.clone' rust-agent-tui/src/app/thread_ops.rs` → 期望包含: "get_compact_config" 和 "cwd.clone"

#### - [x] 7.6 auto-compact 触发使用 CompactConfig 驱动
- **来源:** plan-2 Task 6 §auto-compact 触发 / design §触发机制
- **目的:** 确认阈值和熔断器可配置
- **操作步骤:**
  1. [A] `grep -n 'compact_config\|max_consecutive_failures\|auto_compact_threshold' rust-agent-tui/src/app/agent_ops.rs` → 期望包含: "compact_config" 和 "max_consecutive_failures"

#### - [x] 7.7 CompactDone 处理拆分重新注入内容
- **来源:** plan-2 Task 6 §CompactDone 处理
- **目的:** 确认新 Thread 包含摘要 + 重新注入
- **操作步骤:**
  1. [A] `grep -c 'RE_INJECT_SEPARATOR\|re_inject_messages' rust-agent-tui/src/app/agent_ops.rs` → 期望包含: "2"

#### - [x] 7.8 /compact 命令描述已更新
- **来源:** plan-2 Task 6 §compact 命令
- **目的:** 确认用户可见的帮助信息反映新能力
- **操作步骤:**
  1. [A] `grep 'description' rust-agent-tui/src/command/compact.rs` → 期望包含: "结构化摘要" 和 "重新注入"

#### - [x] 7.9 ContextBudget 新增 builder 方法
- **来源:** plan-2 Task 6 §ContextBudget 扩展
- **目的:** 确认阈值可通过 builder 自定义
- **操作步骤:**
  1. [A] `grep -n 'with_auto_compact_threshold\|with_warning_threshold' rust-create-agent/src/agent/token.rs` → 期望包含: "with_auto_compact_threshold" 和 "with_warning_threshold"

#### - [x] 7.10 AppConfig compact 序列化测试通过
- **来源:** plan-2 Task 6 §序列化测试
- **目的:** 确认 settings.json 读写兼容
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-tui --lib -- config::types::tests::test_app_config_compact 2>&1 | tail -10` → 期望包含: "test result: ok"

#### - [x] 7.11 Headless 集成测试通过
- **来源:** plan-2 Task 6 §Headless 测试
- **目的:** 确认 CompactDone 事件处理含重新注入
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-tui --lib -- ui::headless::tests::compact 2>&1 | tail -15` → 期望包含: "test result: ok"

---

### 场景 8: 全 workspace 端到端回归

#### - [x] 8.1 全 workspace 编译通过
- **来源:** plan-2 Task 7 §端到端验证 / design §验收标准
- **目的:** 确认三个 crate 联合编译无错误
- **操作步骤:**
  1. [A] `cargo build 2>&1 | tail -5` → 期望包含: "Finished"

#### - [x] 8.2 全 workspace 测试通过
- **来源:** plan-2 Task 7 §端到端验证
- **目的:** 确认无回归
- **操作步骤:**
  1. [A] `cargo test 2>&1 | tail -15` → 期望包含: "test result: ok"

#### - [x] 8.3 全 workspace 无编译 error
- **来源:** plan-2 Task 7 §端到端验证
- **目的:** 确认无隐藏编译错误
- **操作步骤:**
  1. [A] `cargo build 2>&1 | grep -i "error" | head -10` → 期望精确: ""（无输出）

---

## 验收后清理

- 无需要清理的后台服务（所有验证均为一次性构建/测试命令）

---

## 验收结果汇总

| 场景 | 序号 | 验收项 | [A] | [H] | 结果 |
|------|------|--------|-----|-----|------|
| 场景 1 | 1.1 | 核心层编译通过 | 1 | 0 | ⬜ |
| 场景 1 | 1.2 | compact 子模块 5 个注册 | 1 | 0 | ⬜ |
| 场景 1 | 1.3 | 公共 API 导出完整 | 1 | 0 | ⬜ |
| 场景 2 | 2.1 | CompactConfig 12 字段完整 | 1 | 0 | ⬜ |
| 场景 2 | 2.2 | 配置单元测试通过 | 1 | 0 | ⬜ |
| 场景 3 | 3.1 | invariant 3 个 API 导出 | 1 | 0 | ⬜ |
| 场景 3 | 3.2 | invariant 单元测试通过 | 1 | 0 | ⬜ |
| 场景 4 | 4.1 | micro 模块注册并导出 | 1 | 0 | ⬜ |
| 场景 4 | 4.2 | 旧函数 deprecated 标记 | 1 | 0 | ⬜ |
| 场景 4 | 4.3 | 函数签名完整 | 1 | 0 | ⬜ |
| 场景 4 | 4.4 | micro 单元测试通过 | 1 | 0 | ⬜ |
| 场景 4 | 4.5 | 旧测试仍兼容 | 1 | 0 | ⬜ |
| 场景 5 | 5.1 | full 模块注册并导出 | 1 | 0 | ⬜ |
| 场景 5 | 5.2 | full_compact 签名完整 | 1 | 0 | ⬜ |
| 场景 5 | 5.3 | 9 段摘要模板存在 | 1 | 0 | ⬜ |
| 场景 5 | 5.4 | PTL 降级逻辑存在 | 1 | 0 | ⬜ |
| 场景 5 | 5.5 | 后处理函数存在 | 1 | 0 | ⬜ |
| 场景 5 | 5.6 | full 单元测试通过 | 1 | 0 | ⬜ |
| 场景 6 | 6.1 | re_inject 模块注册并导出 | 1 | 0 | ⬜ |
| 场景 6 | 6.2 | 核心函数与辅助函数签名 | 1 | 0 | ⬜ |
| 场景 6 | 6.3 | re_inject 单元测试通过 | 1 | 0 | ⬜ |
| 场景 7 | 7.1 | TUI 层编译通过 | 1 | 0 | ⬜ |
| 场景 7 | 7.2 | compact_task 新签名 | 1 | 0 | ⬜ |
| 场景 7 | 7.3 | 旧摘要逻辑已清除 | 1 | 0 | ⬜ |
| 场景 7 | 7.4 | 调用核心层函数 | 1 | 0 | ⬜ |
| 场景 7 | 7.5 | start_compact 传递新参数 | 1 | 0 | ⬜ |
| 场景 7 | 7.6 | auto-compact CompactConfig 驱动 | 1 | 0 | ⬜ |
| 场景 7 | 7.7 | CompactDone 拆分重新注入 | 1 | 0 | ⬜ |
| 场景 7 | 7.8 | /compact 命令描述更新 | 1 | 0 | ⬜ |
| 场景 7 | 7.9 | ContextBudget builder 方法 | 1 | 0 | ⬜ |
| 场景 7 | 7.10 | AppConfig 序列化测试通过 | 1 | 0 | ⬜ |
| 场景 7 | 7.11 | Headless 集成测试通过 | 1 | 0 | ⬜ |
| 场景 8 | 8.1 | 全 workspace 编译通过 | 1 | 0 | ⬜ |
| 场景 8 | 8.2 | 全 workspace 测试通过 | 1 | 0 | ⬜ |
| 场景 8 | 8.3 | 无编译 error | 1 | 0 | ⬜ |

**验收结论:** ✅ 全部通过 / ⬜ 存在问题
