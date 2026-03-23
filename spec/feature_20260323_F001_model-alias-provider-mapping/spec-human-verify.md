# 模型别名 Provider 映射 人工验收清单

**生成时间:** 2026-03-23 16:00
**关联计划:** ./spec-plan.md
**关联设计:** ./spec-design.md

---

## 验收前准备

### 环境要求
- [ ] [AUTO] 检查 Rust 工具链可用: `cargo --version`
- [ ] [AUTO] 编译 rust-agent-tui: `cargo build -p rust-agent-tui 2>&1 | grep -c "^error"`
  → 期望: 输出 `0`（零编译错误）
- [ ] [AUTO] 备份当前 settings.json（如存在）: `test -f ~/.zen-code/settings.json && cp ~/.zen-code/settings.json ~/.zen-code/settings.json.bak || echo "no existing config"`
- [ ] [MANUAL] 确认 `rust-agent-tui/.env` 中至少配置了一个有效 API Key（ANTHROPIC_API_KEY 或 OPENAI_API_KEY），供 TUI 启动时测试 Provider 解析

---

## 验收项目

### 场景 1：数据结构与序列化

#### - [x] 1.1 新结构编译无错误且无新增 warning
- **来源:** Task 1 检查步骤
- **操作步骤:**
  1. [A] `cargo build -p rust-agent-tui 2>&1 | grep "^error"` → 期望: 无输出（零编译错误）
  2. [A] `cargo build -p rust-agent-tui 2>&1 | grep "^warning" | grep -E "unused.*ModelAlias|dead_code.*ModelAlias"` → 期望: 无输出（无新增 ModelAlias 相关 warning）
- **异常排查:**
  - 若有编译错误: `cargo build -p rust-agent-tui 2>&1 | head -40` 查看完整错误信息

#### - [x] 1.2 settings.json 新格式序列化正确
- **来源:** Task 1 检查步骤 / spec-design.md
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-tui -- config 2>&1 | tail -5` → 期望: `test result: ok` 且无 FAILED
  2. [A] 验证序列化输出含 active_alias 字段: `cargo test -p rust-agent-tui -- test_app_config_thinking_roundtrip 2>&1 | grep -E "ok|FAILED"` → 期望: `ok`
  3. [A] 验证 provider_id/model_id 旧字段不写入序列化输出: `cargo test -p rust-agent-tui -- test_app_config_thinking_roundtrip -- --nocapture 2>&1 | grep -E "FAILED"` → 期望: 无 FAILED
- **异常排查:**
  - 若 config 测试失败: `cargo test -p rust-agent-tui -- config -- --nocapture 2>&1 | grep -A 10 "FAILED"` 查看具体失败原因

---

### 场景 2：向后兼容迁移

#### - [x] 2.1 旧格式 JSON 自动迁移为新格式
- **来源:** Task 2 检查步骤 / spec-design.md
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-tui -- migration 2>&1 | tail -5` → 期望: `3 passed; 0 failed`
  2. [A] `cargo test -p rust-agent-tui -- test_migration_from_old_format -- --nocapture 2>&1 | grep -E "ok|FAILED"` → 期望: `ok`
  3. [A] `cargo test -p rust-agent-tui -- test_migration_active_alias_is_opus -- --nocapture 2>&1 | grep -E "ok|FAILED"` → 期望: `ok`
- **异常排查:**
  - 若迁移测试失败: 检查 `rust-agent-tui/src/config/store.rs` 中 `migrate_if_needed` 函数的条件判断

#### - [x] 2.2 迁移后数据完整性（旧 provider_id/model_id 填入 opus 别名）
- **来源:** spec-design.md 向后兼容迁移章节
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-tui -- test_migration_from_old_format -- --nocapture 2>&1 | tail -20` → 期望: 测试通过，无 assertion 失败
  2. [A] `cargo test -p rust-agent-tui -- store 2>&1 | tail -5` → 期望: `test result: ok`
- **异常排查:**
  - 若 assertion 失败说明迁移逻辑有误: 检查 `migrate_if_needed` 函数中对 opus/sonnet/haiku 的填充逻辑

---

### 场景 3：LlmProvider 别名解析

#### - [x] 3.1 从 active_alias 正确解析为 LlmProvider
- **来源:** Task 3 检查步骤 / spec-design.md LlmProvider 解析变更章节
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-tui -- provider 2>&1 | tail -10` → 期望: 全部 `ok`，0 FAILED
  2. [A] `cargo test -p rust-agent-tui -- test_from_config_opus_alias 2>&1 | grep -E "ok|FAILED"` → 期望: `ok`
  3. [A] `cargo test -p rust-agent-tui -- test_from_config_sonnet_alias 2>&1 | grep -E "ok|FAILED"` → 期望: `ok`
- **异常排查:**
  - 若解析失败: 检查 `provider.rs` 中 `from_config` 函数的 alias match 分支

#### - [x] 3.2 空 model_id 回退到 Provider 默认值，不 panic
- **来源:** Task 3 检查步骤 / spec-design.md 实现要点5
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-tui -- provider_default 2>&1 | tail -5` → 期望: `2 passed; 0 failed`
  2. [A] `cargo test -p rust-agent-tui -- test_provider_default -- --nocapture 2>&1 | grep -E "ok|FAILED"` → 期望: `ok`（空 model_id 时 anthropic 回退 claude-sonnet-4-6）
- **异常排查:**
  - 若 panic: 检查 `provider.rs` 中空 model_id 时的 match 分支

#### - [x] 3.3 未知 alias 名称 fallback 到 opus
- **来源:** Task 3 执行步骤（未知别名 fallback）
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-tui -- test_from_config_unknown_alias_fallback_to_opus 2>&1 | grep -E "ok|FAILED"` → 期望: `ok`
- **异常排查:**
  - 若失败: 检查 `from_config` 中 `_ => &app.model_aliases.opus` 分支

---

### 场景 4：TUI 面板交互（需要启动 TUI 目视验证）

> **启动说明：** 在有真实终端的环境中运行 `cargo run -p rust-agent-tui`，确保 `.env` 中有有效 API Key。
> 启动后按 `/model` 回车进入模型面板。

#### - [x] 4.1 /model 面板显示三个 Tab（Opus / Sonnet / Haiku）
- **来源:** Task 5 检查步骤 / spec-design.md TUI 面板交互设计
- **操作步骤:**
  1. [A] `cargo build -p rust-agent-tui 2>&1 | grep "^error"` → 期望: 无输出（可正常构建）
  2. [H] 启动 TUI（`cargo run -p rust-agent-tui`），输入 `/model` 并回车，观察弹出面板顶部是否显示 `[ Opus ]`、`[ Sonnet ]`、`[ Haiku ]` 三个并排 Tab → 是/否
  3. [H] 在面板中按 `Tab` 键，观察 Sonnet Tab 是否变为高亮（蓝色背景） → 是/否
- **异常排查:**
  - 若面板未显示 Tab: 检查 `main_ui.rs` 中 `ModelPanelMode::AliasConfig` 分支的渲染逻辑
  - 若高亮不正确: 检查 `render_model_panel` 中 `is_current` 判断

#### - [x] 4.2 状态栏显示 ★Alias → provider/model 格式
- **来源:** Task 5 / spec-design.md TUI 状态栏章节
- **操作步骤:**
  1. [A] 搜索源码确认状态栏格式代码存在: `grep -n "★" rust-agent-tui/src/ui/main_ui.rs | head -5` → 期望: 有包含 `★` 的代码行
  2. [H] 启动 TUI，观察底部状态栏（第二栏）是否显示类似 `★Opus → provider/model` 的格式（其中 provider 和 model 为实际配置值） → 是/否
- **异常排查:**
  - 若状态栏未显示: 检查 `render_status_bar` 函数中 `alias_display` 变量的赋值逻辑

#### - [x] 4.3 Provider 管理子面板可通过 p 键进入
- **来源:** Task 5 键盘事件适配 / spec-design.md Provider 管理入口保留
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-tui -- model_panel 2>&1 | tail -5` → 期望: `test result: ok`（所有 model_panel 单元测试通过）
  2. [H] 在 `/model` 面板主界面（AliasConfig 模式）按 `p` 键，观察面板是否切换为 Provider 列表管理界面（显示 Provider 名称列表） → 是/否
  3. [H] 在 Provider 管理界面按 `n` 键，观察是否进入新建 Provider 表单（显示 Name/Type/API Key/Base URL 等字段） → 是/否
  4. [H] 按 `Esc` 键，观察是否从 Provider 管理界面返回到别名配置主界面（重新显示 Tab 栏） → 是/否
  5. [H] 在别名配置主界面按 `Space` 键（当 Provider 行高亮时），观察 Provider 选项是否在列表中循环切换 → 是/否
  6. [H] 按 `Esc` 关闭整个 `/model` 面板，确认面板正常关闭 → 是/否
- **异常排查:**
  - 若 p 键无响应: 检查 `event.rs` 中 `ModelPanelMode::AliasConfig` 分支的 `Key::Char('p')` 处理
  - 若返回按键无效: 检查 `ModelPanelMode::Browse` 分支的 `Key::Esc` 处理

---

## 验收结果汇总

| 场景 | 序号 | 验收项 | 自动步骤 | 人工步骤 | 结果 | 备注 |
|------|------|--------|----------|----------|------|------|
| 数据结构与序列化 | 1.1 | 新结构编译无错误 | 2 | 0 | ✅ | |
| 数据结构与序列化 | 1.2 | settings.json 新格式序列化正确 | 3 | 0 | ✅ | |
| 向后兼容迁移 | 2.1 | 旧格式自动迁移 | 3 | 0 | ✅ | |
| 向后兼容迁移 | 2.2 | 迁移后数据完整性 | 2 | 0 | ✅ | |
| LlmProvider 别名解析 | 3.1 | active_alias 正确解析 | 3 | 0 | ✅ | |
| LlmProvider 别名解析 | 3.2 | 空 model_id 回退不 panic | 2 | 0 | ✅ | |
| LlmProvider 别名解析 | 3.3 | 未知 alias fallback 到 opus | 1 | 0 | ✅ | |
| TUI 面板交互 | 4.1 | /model 面板三 Tab 布局 | 1 | 2 | ✅ | 验收期间调整：Tab 键切换 Tab，↑↓ 切换字段，移除 ★ 星号标记需求 |
| TUI 面板交互 | 4.2 | 状态栏 ★Alias 格式 | 1 | 1 | ✅ | |
| TUI 面板交互 | 4.3 | Provider 管理子面板 | 1 | 5 | ✅ | |

**验收结论:** ✅ 全部通过
