# 替换外部 rg 进程为 ripgrep crate 进程内搜索 人工验收清单

**生成时间:** 2026-04-30 20:32
**关联计划:** spec/feature_20260430_F003_replace-grep-with-ripgrep/spec-plan.md
**关联设计:** spec/feature_20260430_F003_replace-grep-with-ripgrep/spec-design.md

---

## 验收前准备

### 环境要求
- [ ] [AUTO] 检查 Rust 工具链版本: `rustc --version`
- [ ] [AUTO] 编译 middlewares crate: `cargo build -p rust-agent-middlewares 2>&1 | tail -3`

### 测试数据准备
- [ ] [AUTO] 运行现有 grep 测试确认基线: `cargo test -p rust-agent-middlewares --lib -- tools::filesystem::grep 2>&1 | tail -10`

---

## 验收项目

### 场景 1：依赖与旧代码清理

#### - [x] 1.1 grep crate 依赖已添加
- **来源:** spec-plan.md Task 1 执行步骤
- **目的:** 确认新依赖正确引入
- **操作步骤:**
  1. [A] `grep "grep = " rust-agent-middlewares/Cargo.toml` → 期望包含: `grep = "0.4"`

#### - [x] 1.2 旧外部进程调用代码已完全移除
- **来源:** spec-plan.md Task 1 检查步骤
- **目的:** 确认无残留外部进程依赖
- **操作步骤:**
  1. [A] `grep -c "which_rg\|tokio::process::Command\|OnceLock\|Stdio" rust-agent-middlewares/src/tools/filesystem/grep.rs` → 期望精确: `0`

#### - [x] 1.3 新 crate API 关键调用已引入
- **来源:** spec-plan.md Task 1 检查步骤
- **目的:** 确认进程内搜索核心组件到位
- **操作步骤:**
  1. [A] `grep -c "RegexMatcher\|WalkBuilder\|SearcherBuilder\|SearchSink\|spawn_blocking" rust-agent-middlewares/src/tools/filesystem/grep.rs` → 期望包含: 输出为正整数

---

### 场景 2：编译与构建

#### - [x] 2.1 middlewares crate 编译成功
- **来源:** spec-plan.md Task 1 检查步骤
- **目的:** 确认重写后编译无错误
- **操作步骤:**
  1. [A] `cargo build -p rust-agent-middlewares 2>&1 | tail -3` → 期望包含: `Finished` 且无 `error`

#### - [x] 2.2 全 workspace 构建无破坏
- **来源:** spec-plan.md Task 2 端到端验证
- **目的:** 确认公开接口未变，下游 crate 正常编译
- **操作步骤:**
  1. [A] `cargo build 2>&1 | tail -5` → 期望包含: `Finished` 且无 `error`

#### - [x] 2.3 TUI crate 编译成功
- **来源:** spec-plan.md Task 2 端到端验证
- **目的:** 确认 TUI 层无编译问题
- **操作步骤:**
  1. [A] `cargo build -p rust-agent-tui 2>&1 | tail -5` → 期望包含: `Finished` 且无 `error`

---

### 场景 3：单元测试

#### - [x] 3.1 grep 模块全部测试通过（无跳过）
- **来源:** spec-plan.md Task 1 执行步骤（更新测试用例）
- **目的:** 确认重写后所有现有+新增测试通过
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-middlewares --lib -- tools::filesystem::grep 2>&1 | tail -5` → 期望包含: `test result` 且无 `FAILED` 且无 `skipped`

#### - [x] 3.2 新增 -l (FilesOnly) 模式测试
- **来源:** spec-plan.md Task 1 执行步骤（新增测试）
- **目的:** 确认仅输出匹配文件路径功能
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-middlewares --lib -- tools::filesystem::grep::tests::test_search_files_rg_files_only 2>&1` → 期望包含: `test result` 且无 `FAILED`

#### - [x] 3.3 新增 -c (CountOnly) 模式测试
- **来源:** spec-plan.md Task 1 执行步骤（新增测试）
- **目的:** 确认匹配行数计数功能
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-middlewares --lib -- tools::filesystem::grep::tests::test_search_files_rg_count 2>&1` → 期望包含: `test result` 且无 `FAILED`

#### - [x] 3.4 新增 -i (case insensitive) 测试
- **来源:** spec-plan.md Task 1 执行步骤（新增测试）
- **目的:** 确认大小写不敏感搜索
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-middlewares --lib -- tools::filesystem::grep::tests::test_search_files_rg_case_insensitive 2>&1` → 期望包含: `test result` 且无 `FAILED`

#### - [x] 3.5 新增 -g (glob filter) 测试
- **来源:** spec-plan.md Task 1 执行步骤（新增测试）
- **目的:** 确认 glob 文件过滤功能
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-middlewares --lib -- tools::filesystem::grep::tests::test_search_files_rg_glob_filter 2>&1` → 期望包含: `test result` 且无 `FAILED`

---

### 场景 4：全量回归

#### - [x] 4.1 middlewares crate 全量测试通过
- **来源:** spec-plan.md Task 2 端到端验证
- **目的:** 确认 grep 重写未引入其他模块回归
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-middlewares --lib 2>&1 | tail -10` → 期望包含: `test result` 且无 `FAILED`

#### - [x] 4.2 全 workspace 测试通过
- **来源:** spec-plan.md Task 2 端到端验证
- **目的:** 确认整体项目无回归
- **操作步骤:**
  1. [A] `cargo test 2>&1 | tail -15` → 期望包含: `test result` 且无 `FAILED`

---

### 场景 5：边界与回归（补充自 spec-design.md）

#### - [x] 5.1 外部 rg 二进制不再被调用
- **来源:** spec-design.md 目标（消除外部依赖）
- **目的:** 确认运行时确实不依赖系统 rg
- **操作步骤:**
  1. [A] `grep -rn "which_rg\|\"rg\"\|rg_binary\|ripgrep.*binary" rust-agent-middlewares/src/tools/filesystem/grep.rs` → 期望精确: 无输出（返回码非0）

#### - [x] 5.2 公开接口未变（name/description/parameters）
- **来源:** spec-design.md 范围（LLM 侧无感知）
- **目的:** 确认工具对外契约保持不变
- **操作步骤:**
  1. [A] `grep -n "fn name\|fn description\|fn parameters" rust-agent-middlewares/src/tools/filesystem/grep.rs` → 期望包含: 三个函数签名均存在

#### - [x] 5.3 超时机制仍为 15 秒
- **来源:** spec-design.md 超时控制
- **目的:** 确认搜索超时行为不变
- **操作步骤:**
  1. [A] `grep "15\|Duration::from_secs" rust-agent-middlewares/src/tools/filesystem/grep.rs` → 期望包含: `15` 或 `from_secs`

#### - [x] 5.4 head_limit 默认值仍为 500
- **来源:** spec-design.md 结果收集（500 行上限）
- **目的:** 确认输出行数限制行为不变
- **操作步骤:**
  1. [A] `grep "500\|head_limit" rust-agent-middlewares/src/tools/filesystem/grep.rs` → 期望包含: `500`

---

## 验收后清理

无需清理，本次验收无后台服务启动。

---

## 验收结果汇总

| 场景 | 序号 | 验收项 | [A] | [H] | 结果 |
|------|------|--------|-----|-----|------|
| 场景 1 | 1.1 | grep crate 依赖已添加 | A | - | ⬜ |
| 场景 1 | 1.2 | 旧外部进程调用代码已完全移除 | A | - | ⬜ |
| 场景 1 | 1.3 | 新 crate API 关键调用已引入 | A | - | ⬜ |
| 场景 2 | 2.1 | middlewares crate 编译成功 | A | - | ⬜ |
| 场景 2 | 2.2 | 全 workspace 构建无破坏 | A | - | ⬜ |
| 场景 2 | 2.3 | TUI crate 编译成功 | A | - | ⬜ |
| 场景 3 | 3.1 | grep 模块全部测试通过 | A | - | ⬜ |
| 场景 3 | 3.2 | -l FilesOnly 模式测试 | A | - | ⬜ |
| 场景 3 | 3.3 | -c CountOnly 模式测试 | A | - | ⬜ |
| 场景 3 | 3.4 | -i case insensitive 测试 | A | - | ⬜ |
| 场景 3 | 3.5 | -g glob filter 测试 | A | - | ⬜ |
| 场景 4 | 4.1 | middlewares 全量测试通过 | A | - | ⬜ |
| 场景 4 | 4.2 | 全 workspace 测试通过 | A | - | ⬜ |
| 场景 5 | 5.1 | 外部 rg 二进制不再被调用 | A | - | ⬜ |
| 场景 5 | 5.2 | 公开接口未变 | A | - | ⬜ |
| 场景 5 | 5.3 | 超时机制仍为 15 秒 | A | - | ⬜ |
| 场景 5 | 5.4 | head_limit 默认值仍为 500 | A | - | ⬜ |

**验收结论:** ⬜ 全部通过 / ⬜ 存在问题
