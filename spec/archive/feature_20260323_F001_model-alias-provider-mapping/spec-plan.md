# 模型别名 Provider 映射 执行计划

**目标:** 引入 Opus/Sonnet/Haiku 三级别名，每个别名独立绑定 provider+model，TUI 面板重构为三 Tab 式

**技术栈:** Rust, ratatui, serde_json, tokio

**设计文档:** ./spec-design.md

---

### Task 1: 数据结构变更

**涉及文件:**
- 修改: `rust-agent-tui/src/config/types.rs`

**执行步骤:**
- [x] 新增 `ModelAliasConfig` 结构（`provider_id: String`, `model_id: String`），派生 `Serialize, Deserialize, Debug, Clone, Default`
- [x] 新增 `ModelAliasMap` 结构（`opus / sonnet / haiku: ModelAliasConfig`），派生同上
  - serde 字段名与 JSON 键一致（小写）
  - `Default` 实现返回三个空 `ModelAliasConfig`
- [x] `AppConfig` 修改：
  - 移除 `provider_id: String`（保留 serde `#[serde(default)]` 用于旧格式迁移读取，加 `#[serde(skip_serializing)]` 避免写回）
  - 移除 `model_id: String`（同上，只读旧格式，不序列化）
  - 新增 `active_alias: String`（`#[serde(default = "default_alias")]`，默认 "opus"）
  - 新增 `model_aliases: ModelAliasMap`（`#[serde(default)]`）
- [x] 更新测试中直接使用 `provider_id / model_id` 的测试用例，改为使用新字段
  - 受影响测试：`test_app_config_thinking_optional`, `test_app_config_thinking_roundtrip`, `test_model_panel_apply_edit_saves_thinking`（后两个 Task 会进一步修复）

**检查步骤:**
- [x] 编译通过，无 unused field warning
  - `cargo build -p rust-agent-tui 2>&1 | grep -E "^error"`
  - 预期: 无输出（零编译错误）
- [x] 新结构序列化格式正确
  - `cargo test -p rust-agent-tui --lib -- config 2>&1 | tail -20`
  - 预期: 所有 config 测试通过

---

### Task 2: 向后兼容迁移

**涉及文件:**
- 修改: `rust-agent-tui/src/config/store.rs`

**执行步骤:**
- [x] 在 `load()` 函数中，反序列化后检测旧格式：
  - 条件：`cfg.config.model_aliases.opus.provider_id` 为空，但 `cfg.config.provider_id`（旧字段，保留为临时字段）不为空
  - 迁移逻辑：将旧 `provider_id + model_id` 填入 `opus` 别名；`sonnet / haiku` 填入相同 `provider_id`，`model_id` 留空；`active_alias` 设为 "opus"
- [x] 迁移后立即调用 `save(cfg)` 将新格式写回文件（防止下次启动重复迁移）
- [x] 添加迁移单元测试：构造旧格式 JSON，调用迁移逻辑，断言新字段正确填充

**检查步骤:**
- [x] 旧格式 JSON 加载后字段迁移正确
  - `cargo test -p rust-agent-tui --lib -- migration 2>&1 | tail -20`
  - 预期: 迁移测试通过
- [x] 迁移后 `active_alias` 为 "opus"
  - `cargo test -p rust-agent-tui --lib -- store 2>&1 | tail -20`
  - 预期: 测试通过

---

### Task 3: LlmProvider 解析变更

**涉及文件:**
- 修改: `rust-agent-tui/src/app/provider.rs`

**执行步骤:**
- [x] `LlmProvider::from_config` 重写为按 `active_alias` 查 `model_aliases` 表：
  ```rust
  let alias = cfg.config.active_alias.as_str();  // "opus" | "sonnet" | "haiku"
  let mapping = match alias {
      "opus"   => &cfg.config.model_aliases.opus,
      "sonnet" => &cfg.config.model_aliases.sonnet,
      "haiku"  => &cfg.config.model_aliases.haiku,
      _        => &cfg.config.model_aliases.opus,  // 未知别名 fallback
  };
  let provider = cfg.config.providers.iter().find(|p| p.id == mapping.provider_id)?;
  ```
- [x] `model_id` 空值处理：若 `mapping.model_id` 为空，按 `provider.provider_type` 回退默认 model（anthropic → "claude-sonnet-4-6"，其他 → "gpt-4o"）
- [x] 更新 `display_name()` / `model_name()` 不变，`from_env()` 不变
- [x] 添加单元测试：构造含 `model_aliases` 的 `ZenConfig`，验证 `from_config` 返回正确 `LlmProvider`

**检查步骤:**
- [x] from_config 正确解析 opus 别名
  - `cargo test -p rust-agent-tui --lib -- provider 2>&1 | tail -20`
  - 预期: 所有 provider 测试通过
- [x] 空 model_id 不 panic，回退到默认 model
  - `cargo test -p rust-agent-tui --lib -- provider_default 2>&1 | tail -20`
  - 预期: 测试通过

---

### Task 4: ModelPanel 重构

**涉及文件:**
- 修改: `rust-agent-tui/src/app/model_panel.rs`

**执行步骤:**
- [x] 新增 `AliasTab` 枚举（`Opus / Sonnet / Haiku`），实现 `next() / prev() / label() / to_key()`
  - `to_key()` 返回 "opus" / "sonnet" / "haiku" 字符串（写入 `active_alias` 用）
- [x] `ModelPanel` 结构增加字段：
  - `active_tab: AliasTab`（当前选中的 Tab，初始化为 `active_alias` 对应值）
  - `buf_alias_provider: [String; 3]`（三个 Tab 各自的 provider_id 缓冲，按索引对应 opus/sonnet/haiku）
  - `buf_alias_model: [String; 3]`（三个 Tab 各自的 model_id 缓冲）
  - `alias_edit_field: AliasEditField`（`Provider / ModelId`，在别名编辑区内切换）
- [x] `ModelPanel::from_config` 从 `model_aliases` 初始化三组缓冲，`active_tab` 由 `active_alias` 决定
- [x] 新增方法：
  - `tab_next() / tab_prev()`：切换 `active_tab`
  - `alias_field_next() / alias_field_prev()`：在 Provider / ModelId 间切换
  - `cycle_alias_provider()`：在 providers 列表中循环切换当前 Tab 的 provider（Space 键）
  - `push_alias_char(c) / pop_alias_char()`：写入当前 Tab 的 model_id 缓冲
  - `apply_alias_edit(cfg)`：将当前 Tab 的缓冲写回 `cfg.config.model_aliases` 对应字段
  - `activate_current_tab(cfg)`：将 `active_tab.to_key()` 写入 `cfg.config.active_alias` 并保存
- [x] 旧有 `confirm_select()` 保留（provider 管理功能仍需），但内部不再写 `provider_id`（`provider_id` 字段已 skip_serializing）
- [x] 更新受影响的旧测试，使其通过新 API

**检查步骤:**
- [x] ModelPanel 所有方法编译通过
  - `cargo build -p rust-agent-tui 2>&1 | grep "^error"`
  - 预期: 无错误
- [x] ModelPanel 单元测试通过
  - `cargo test -p rust-agent-tui --lib -- model_panel 2>&1 | tail -30`
  - 预期: 全部通过

---

### Task 5: TUI 渲染适配

**涉及文件:**
- 修改: `rust-agent-tui/src/ui/main_ui.rs`
- 修改: `rust-agent-tui/src/app/mod.rs`

**执行步骤:**
- [x] **`main_ui.rs` 模型面板渲染重写**（`render_model_panel` 相关区域）：
  - 上半区域改为三个 Tab 横向排列，高亮当前 `active_tab`，激活 Tab 旁加 `★`
  - 下半区域根据当前 `active_tab` 显示对应别名的 Provider（循环选择行）和 Model ID（文本输入行）
  - Provider 行渲染：显示所有 provider 列表，当前选中用 `[name]` 包裹
  - 快捷键提示：`←/→:切换Tab  Enter:激活  Tab:切换字段  Space:切换Provider  p:管理Providers  Esc:关闭`
  - 移除旧有基于 `provider cursor` 的渲染逻辑（Browse 模式现在只在 `p` 键弹出的子面板展示）
- [x] **状态栏更新** (`app/mod.rs` 或 `main_ui.rs` 中的状态行)：
  - 将原来的 `provider_name / model_name` 显示改为 `[★Alias → provider/model]` 格式
  - 例：`★Opus → openrouter / gpt-5.4`
- [x] **键盘事件适配** (`app/mod.rs` 中 `model_panel` 相关的按键处理)：
  - `Tab` / `Shift+Tab` → `panel.tab_next() / tab_prev()`（切换 Alias Tab）
  - `↑` / `↓` → `panel.alias_field_prev() / alias_field_next()`（切换编辑字段）
  - `Space` → `panel.cycle_alias_provider()`（当 alias_edit_field == Provider）
  - `Enter` → `panel.activate_current_tab(cfg)` + 保存
  - `s` → `panel.apply_alias_edit(cfg)` + 保存
  - `p` → 进入旧的 provider 管理子面板（Browse/Edit/New/Delete 模式，键位保持不变）
  - 字符输入 → `panel.push_alias_char(c)`（不被任何快捷键拦截）
  - `Backspace` → `panel.pop_alias_char()`
- [x] 状态栏中 `provider_name / model_name` 的赋值点同步更新，改从 `active_alias` + `model_aliases` 读取

**检查步骤:**
- [x] 全量编译通过
  - `cargo build -p rust-agent-tui 2>&1 | grep "^error"`
  - 预期: 无错误
- [x] 全量测试通过（含 headless）
  - `cargo test -p rust-agent-tui 2>&1 | tail -30`
  - 预期: `test result: ok`
- [x] Headless 渲染无 panic
  - `cargo test -p rust-agent-tui headless 2>&1 | tail -20`
  - 预期: headless 测试通过

---

### Task 6: 模型别名映射 Acceptance

**Prerequisites:**
- 启动命令: `cargo run -p rust-agent-tui`
- 环境: `rust-agent-tui/.env` 中至少配置一个有效 API Key
- 测试前确认: `~/.zen-code/settings.json` 备份旧配置（若存在）

**端到端验证:**

1. **旧格式配置自动迁移**（通过单元测试验证迁移逻辑）
   - `cargo test -p rust-agent-tui -- migration` → 3 passed ✓

2. **settings.json 存储新格式**（通过迁移单元测试间接验证）
   - model_aliases 含 opus/sonnet/haiku 三个字段 ✓

3. **active_alias 字段持久化**（test_migration_active_alias_is_opus 覆盖）
   - 迁移后 active_alias = "opus" ✓

4. **LlmProvider 正确解析别名**
   - `cargo test -p rust-agent-tui -- provider` → 7 passed ✓

5. **空 model_id 不 panic**
   - `cargo test -p rust-agent-tui -- provider_default` → 2 passed ✓

6. **全量单元测试通过**
   - `cargo test -p rust-agent-tui` → 40 passed; 0 failed ✓

7. **TUI 编译产物无新增 warning**
   - `cargo build -p rust-agent-tui` → 3 warnings（均为改动前已存在的旧 warning）✓

---

### Task 7: 验收后优化（验收期间发现）

**涉及文件:**
- 修改: `rust-agent-tui/src/event.rs`
- 修改: `rust-agent-tui/src/ui/main_ui.rs`
- 修改: `rust-agent-tui/src/command/model.rs`

**执行步骤:**
- [x] 键盘绑定调整（修复字符输入冲突）：
  - `Tab` / `Shift+Tab` → 切换 Alias Tab（原为切换字段）
  - `↑` / `↓` → 切换编辑字段（原为无绑定）
  - 移除 `←/→`、`h/l`、`j/k` 的模型面板快捷键绑定（避免在 ModelId 输入时拦截字符）
- [x] 快捷键提示文案同步更新（`main_ui.rs` hint_line）
- [x] `/model <alias>` 命令支持：
  - `ModelCommand::execute` 检测参数，若为 `opus`/`sonnet`/`haiku` 则直接切换 `active_alias` 并保存，不打开面板
  - 无参数或未知参数仍打开配置面板

**检查步骤:**
- [x] 全量测试通过
  - `cargo test -p rust-agent-tui` → 40 passed; 0 failed ✓
