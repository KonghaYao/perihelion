# model-config-refactor 人工验收清单

**生成时间:** 2026-04-27
**关联计划:** spec/feature_20260427_F003_model-config-refactor/spec-plan.md
**关联设计:** spec/feature_20260427_F003_model-config-refactor/spec-design.md

---

## 验收前准备

### 环境要求
- [ ] [AUTO] 检查 Rust 工具链: `rustc --version`
- [ ] [AUTO] 编译全部 crate: `cargo build`
- [ ] [MANUAL] 确保 `~/.zen-code/settings.json` 有可用的 Provider 配置（含 API Key），用于 TUI 交互测试

### 测试数据准备
- [ ] 备份当前 `~/.zen-code/settings.json`（如有），验收后可恢复

---

## 验收项目

### 场景 1：数据模型变更

#### - [x] 1.1 ProviderConfig 包含 models 字段
- **来源:** spec-plan.md 验收标准 #1 / spec-design.md §数据模型变更
- **目的:** 确认 ProviderModels 结构定义正确
- **操作步骤:**
  1. [A] `grep -n 'pub struct ProviderModels' rust-agent-tui/src/config/types.rs` → 期望包含: `ProviderModels`
  2. [A] `grep -n 'pub models: ProviderModels' rust-agent-tui/src/config/types.rs` → 期望包含: `pub models: ProviderModels`
  3. [A] `grep -n 'pub opus' rust-agent-tui/src/config/types.rs` → 期望包含: `pub opus`
---

#### - [x] 1.2 AppConfig 移除 model_aliases 并新增 active_provider_id
- **来源:** spec-plan.md 验收标准 #2 / spec-design.md §数据模型变更
- **目的:** 确认配置结构完成迁移
- **操作步骤:**
  1. [A] `grep -rn 'model_aliases' rust-agent-tui/src/config/` → 期望包含: 无结果（已移除）
  2. [A] `grep -n 'pub active_provider_id' rust-agent-tui/src/config/types.rs` → 期望包含: `pub active_provider_id`
  3. [A] `grep -n 'ModelAliasMap\|ModelAliasConfig' rust-agent-tui/src/config/types.rs` → 期望包含: 无结果（已移除）
---

#### - [x] 1.3 旧格式配置加载不崩溃
- **来源:** spec-plan.md 验收标准 #8 / spec-design.md §实现要点 #4
- **目的:** 旧 settings.json 中残留 model_aliases 字段不会导致解析失败
- **操作步骤:**
  1. [A] `grep -n 'serde.*flatten\|extra' rust-agent-tui/src/config/types.rs` → 期望包含: `flatten`（通过 `#[serde(flatten)] extra` 兼容未知字段）
  2. [A] `cargo test -p rust-agent-tui -- --nocapture 2>&1 | head -50` → 期望包含: `test result: ok`
---

### 场景 2：/login 命令注册与 Provider CRUD

#### - [x] 2.1 /login 命令注册到 CommandRegistry
- **来源:** spec-plan.md 验收标准 #3 / spec-design.md §/login 命令
- **目的:** 确认命令注册链路完整
- **操作步骤:**
  1. [A] `grep -rn 'login' rust-agent-tui/src/command/mod.rs` → 期望包含: `login` 模块注册
  2. [A] `test -f rust-agent-tui/src/command/login.rs && echo EXISTS` → 期望精确: `EXISTS`
  3. [A] `grep -n 'login' rust-agent-tui/src/command/login.rs | head -5` → 期望包含: `Command` trait 实现
---

#### - [x] 2.2 /help 显示 /login 命令
- **来源:** spec-plan.md 验收标准 #3 / spec-design.md §/login 命令
- **目的:** 确认帮助信息中包含 login 说明
- **操作步骤:**
  1. [A] `grep -in 'login' rust-agent-tui/src/command/help.rs` → 期望包含: `login`
---

#### - [x] 2.3 /login 面板 Provider 列表交互
- **来源:** spec-plan.md 验收标准 #4 / spec-design.md §/login 交互流程
- **目的:** 确认 Browse 模式列表正常展示与导航
- **操作步骤:**
  1. [H] 运行 `cargo run -p rust-agent-tui`，输入 `/login`，查看 Provider 列表是否显示 → 是/否
  2. [H] 使用 ↑↓ 键移动光标，观察 `▶` 是否正确跟随 → 是/否
  3. [H] 当前激活 Provider 是否有 `●` 标记 → 是/否
---

#### - [x] 2.4 /login 新建 Provider 表单
- **来源:** spec-plan.md 验收标准 #4 / spec-design.md §/login 交互流程
- **目的:** 确认新建流程完整（7 字段表单）
- **操作步骤:**
  1. [H] 在 /login 面板按 `n`，是否进入 New 模式并显示表单 → 是/否
  2. [H] 表单是否包含 Name / Type / Base URL / API Key / Opus Model / Sonnet Model / Haiku Model 7 个字段 → 是/否
  3. [H] 填写后按 Enter 保存，是否回到列表且新 Provider 出现 → 是/否
  4. [H] Name 输入含大写/空格的名称后，id 是否自动转为小写+下划线 → 是/否
---

#### - [x] 2.5 /login 编辑 Provider
- **来源:** spec-plan.md 验收标准 #4 / spec-design.md §/login 交互流程
- **目的:** 确认编辑流程加载已有数据并保存
- **操作步骤:**
  1. [H] 光标移到已有 Provider 上按 `e`，表单是否预填已有数据 → 是/否
  2. [H] 修改某个字段后按 Enter，是否保存成功 → 是/否
---

#### - [x] 2.6 /login 删除 Provider（含确认）
- **来源:** spec-plan.md 验收标准 #4 / spec-design.md §/login 交互流程
- **目的:** 确认删除有确认步骤且执行正确
- **操作步骤:**
  1. [H] 光标移到 Provider 上按 `d`，是否弹出确认提示（"确认删除 xxx？"） → 是/否
  2. [H] 按 `n` 或 `Esc` 取消，是否返回列表 → 是/否
  3. [H] 再次按 `d` 后按 `y` 确认，Provider 是否从列表中消失 → 是/否
---

#### - [x] 2.7 Type 切换自动填充模型名
- **来源:** spec-plan.md 验收标准 #5 / spec-design.md §实现要点 #1
- **目的:** 确认 provider_type 切换时模型名自动更新
- **操作步骤:**
  1. [H] 在新建/编辑表单中，将 Type 从 anthropic 切换到 openai，三个模型名字段是否自动变为 gpt-4o 等 → 是/否
  2. [H] 将 Type 切回 anthropic，模型名是否恢复为 claude 系列 → 是/否
  3. [H] 手动修改模型名后再切换 Type，自定义的模型名是否保留（不被覆盖） → 是/否
---

### 场景 3：/model 命令简化

#### - [x] 3.1 /model 只显示 Provider 选择 + 级别切换 + Thinking
- **来源:** spec-plan.md 验收标准 #6 / spec-design.md §/model 命令
- **目的:** 确认 /model 面板已简化，不含 CRUD 功能
- **操作步骤:**
  1. [H] 运行 TUI，输入 `/model`，面板是否只显示 Provider 列表 + 级别按钮 + Thinking 区域 → 是/否
  2. [H] 面板中是否不再有"新建/编辑/删除"操作入口 → 是/否
  3. [A] `grep -n 'AliasConfig\|Browse\|ConfirmDelete' rust-agent-tui/src/app/model_panel.rs` → 期望包含: 无结果（旧模式已移除）
---

#### - [x] 3.2 /model <alias> 快捷切换正常
- **来源:** spec-plan.md 验收标准 #7 / spec-design.md §/model 快捷键
- **目的:** 确认命令行直接切换 alias 仍可用
- **操作步骤:**
  1. [H] 在 TUI 输入 `/model sonnet`，状态栏是否显示切换到 sonnet → 是/否
  2. [H] 输入 `/model opus`，状态栏是否显示切换到 opus → 是/否
  3. [H] 输入 `/model haiku`，状态栏是否显示切换到 haiku → 是/否
---

#### - [x] 3.3 Provider 列表显示模型信息
- **来源:** spec-design.md §/model 交互流程
- **目的:** 确认 /model 面板 Provider 行显示各级别模型名
- **操作步骤:**
  1. [H] 打开 /model 面板，每个 Provider 行是否显示 opus=xxx sonnet=xxx haiku=xxx → 是/否
  2. [H] 选择不同 Provider 并确认，是否更新激活 Provider → 是/否
---

### 场景 4：测试与回归

#### - [x] 4.1 全量测试通过
- **来源:** spec-plan.md 验收标准 #9
- **目的:** 确认新数据结构不影响现有功能
- **操作步骤:**
  1. [A] `cargo test 2>&1` → 期望包含: `test result: ok`
---

#### - [x] 4.2 LoginPanel 单元测试
- **来源:** spec-plan.md 验收标准 #10 / spec-design.md §/login 命令
- **目的:** 确认 LoginPanel 有对应单元测试
- **操作步骤:**
  1. [A] `grep -rn '#\[test\]\|#\[tokio::test\]' rust-agent-tui/src/app/login_panel.rs` → 期望包含: 至少一个测试函数
  2. [A] `cargo test -p rust-agent-tui -- login 2>&1` → 期望包含: `test result: ok`
---

#### - [x] 4.3 ModelPanel 简化后单元测试
- **来源:** spec-plan.md 验收标准 #10 / spec-design.md §/model 命令
- **目的:** 确认简化后的 ModelPanel 有对应测试
- **操作步骤:**
  1. [A] `grep -rn '#\[test\]\|#\[tokio::test\]' rust-agent-tui/src/app/model_panel.rs` → 期望包含: 至少一个测试函数
  2. [A] `cargo test -p rust-agent-tui -- model 2>&1` → 期望包含: `test result: ok`
---

## 验收后清理

- [ ] [AUTO] 恢复备份的 `~/.zen-code/settings.json`（如验收前有备份）

---

## 验收结果汇总

| 场景 | 序号 | 验收项 | [A] | [H] | 结果 |
|------|------|--------|-----|-----|------|
| 数据模型 | 1.1 | ProviderConfig models 字段 | 3 | 0 | ✅ |
| 数据模型 | 1.2 | AppConfig 迁移完成 | 3 | 0 | ✅ |
| 数据模型 | 1.3 | 旧格式兼容 | 2 | 0 | ✅ |
| /login | 2.1 | 命令注册 | 3 | 0 | ✅ |
| /login | 2.2 | /help 显示 | 1 | 0 | ✅ |
| /login | 2.3 | Provider 列表交互 | 0 | 3 | ✅ |
| /login | 2.4 | 新建 Provider | 0 | 4 | ✅ |
| /login | 2.5 | 编辑 Provider | 0 | 2 | ✅ |
| /login | 2.6 | 删除 Provider | 0 | 3 | ✅ |
| /login | 2.7 | Type 自动填充 | 0 | 3 | ✅ |
| /model | 3.1 | 面板简化 | 1 | 2 | ✅ |
| /model | 3.2 | 快捷切换 | 0 | 3 | ✅ |
| /model | 3.3 | Provider 模型信息 | 0 | 2 | ✅ |
| 测试 | 4.1 | 全量测试 | 1 | 0 | ✅ |
| 测试 | 4.2 | LoginPanel 测试 | 2 | 0 | ✅ |
| 测试 | 4.3 | ModelPanel 测试 | 2 | 0 | ✅ |

**验收结论:** ✅ 全部通过 / ⬜ 存在问题
