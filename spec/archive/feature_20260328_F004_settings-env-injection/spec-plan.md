# settings-env-injection 执行计划

**目标:** 在 `settings.json` 中增加 `env` 字段，TUI 启动时自动注入环境变量，进程环境变量优先于配置文件。

**技术栈:** Rust 2021, serde, std::collections::HashMap, std::env

**设计文档:** ./spec-design.md

---

### Task 1: AppConfig 扩展 env 字段

**涉及文件:**
- 修改: `rust-agent-tui/src/config/types.rs`

**执行步骤:**
- [x] 在 `AppConfig` struct 中添加 `env` 字段
  ```rust
  use std::collections::HashMap;

  /// 环境变量注入（扁平键值对）
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub env: Option<HashMap<String, String>>,
  ```
- [x] 添加 `HashMap` import（如不存在）

**检查步骤:**
- [x] 验证编译通过
  - `cargo build -p rust-agent-tui 2>&1 | tail -5`
  - 预期: 输出无 error，显示 "Compiling rust-agent-tui" 或 "Finished"
- [x] 验证 serde 反序列化
  - `cargo test -p rust-agent-tui test_app_config_env --lib 2>&1 | tail -10`
  - 预期: 测试通过（Task 3 添加后验证）

---

### Task 2: main.rs 环境变量注入

**涉及文件:**
- 修改: `rust-agent-tui/src/main.rs`
- 修改: `rust-agent-tui/src/config/store.rs`（可能需要导出 `config_path`）

**执行步骤:**
- [x] 在 `main.rs` 顶部添加 `inject_env_from_settings()` 函数
  - 构建 settings.json 路径：`~/.zen-code/settings.json`
  - 使用 `dirs_next::config_dir()` 获取配置目录
  - 读取并解析 JSON，提取 `config.env` 字段
  - 遍历键值对，仅在进程环境变量不存在时设置
- [x] 在 `fn main()` 最开始调用 `inject_env_from_settings()`
  - 位于任何其他初始化之前
- [x] 移除现有的 `dotenvy::dotenv()` 调用（如仍有残留）

**检查步骤:**
- [x] 验证编译通过
  - `cargo build -p rust-agent-tui 2>&1 | tail -5`
  - 预期: 无 error
- [x] 验证 dotenvy 依赖已移除（或未被使用）
  - `grep -n "dotenvy" rust-agent-tui/src/main.rs`
  - 预期: 无匹配（或仅注释）

---

### Task 3: 单元测试

**涉及文件:**
- 修改: `rust-agent-tui/src/config/types.rs`（添加测试模块）
- 修改: `rust-agent-tui/src/main.rs`（添加测试）

**执行步骤:**
- [x] 在 `config/types.rs` 的 `#[cfg(test)] mod tests` 中添加测试
  - `test_app_config_env_serde_roundtrip`: 验证 env 字段序列化/反序列化
  - `test_app_config_env_optional`: 验证 env 字段缺失时为 None
  - `test_app_config_env_skip_when_none`: 验证 env 为 None 时不输出
- [x] 在 `main.rs` 的 `#[cfg(test)] mod tests` 中添加测试
  - `test_env_priority_process_over_settings`: 验证进程环境变量优先级
  - 使用临时环境变量设置，验证 inject 函数不会覆盖已存在的变量

**检查步骤:**
- [x] 运行 types.rs 测试
  - `cargo test -p rust-agent-tui --lib test_app_config_env 2>&1 | tail -10`
  - 预期: 3 个测试通过
- [x] 运行 main.rs 测试
  - `cargo test -p rust-agent-tui --lib test_env_priority 2>&1 | tail -10`
  - 预期: 测试通过

---

### Task 4: 更新 constraints.md

**涉及文件:**
- 修改: `spec/global/constraints.md`

**执行步骤:**
- [x] 将部署方式章节中的 `.env` 文件描述替换为 `settings.json` 的 `env` 字段
  - 原文："配置通过 `.env` 文件（`rust-agent-tui/.env`）和环境变量"
  - 改为："配置通过 `~/.zen-code/settings.json` 的 `env` 字段和环境变量"
- [x] 更新 API Key 安全约束描述
  - 补充说明 API Key 可通过 `settings.json` 的 `env` 字段配置
  - 强调 `settings.json` 已 gitignore（用户目录）

**检查步骤:**
- [x] 验证无 `.env` 文件相关描述
  - `grep -n "\.env" spec/global/constraints.md`
  - 预期: 无匹配（或仅与 `dotenvy` 依赖移除相关的历史说明）
- [x] 验证 `env` 字段描述存在
  - `grep -n "env" spec/global/constraints.md | head -5`
  - 预期: 包含 `settings.json` 和 `env` 相关描述

---

### Task 5: settings-env-injection Acceptance

**Prerequisites:**
- Start command: `cargo build -p rust-agent-tui --release`
- Test config: 创建临时 `~/.zen-code/settings.json` 包含 `env` 字段
- Environment: 清理相关环境变量（避免干扰）

**End-to-end verification:**

1. **env 字段注入验证** ✅
   - `cat > ~/.zen-code/settings.json << 'EOF'...`
   - `TEST_ENV_VAR_123="" cargo run -p rust-agent-tui -- --help 2>&1 | head -5`
   - Expected: TUI 启动成功，无 panic（env 变量被注入）
   - On failure: check Task 2 [inject_env_from_settings 函数]

2. **进程环境变量优先级验证** ✅
   - `TEST_ENV_VAR_123="from_process" cargo run -p rust-agent-tui -- --help 2>&1 | head -5`
   - Expected: TUI 启动成功，进程环境变量不被覆盖
   - On failure: check Task 2 [优先级逻辑] 或 Task 3 [test_env_priority 测试]

3. **settings.json 缺失时静默跳过** ✅
   - `mv ~/.zen-code/settings.json ~/.zen-code/settings.json.bak`
   - `cargo run -p rust-agent-tui -- --help 2>&1 | head -5`
   - `mv ~/.zen-code/settings.json.bak ~/.zen-code/settings.json`
   - Expected: TUI 启动成功，无 panic
   - On failure: check Task 2 [错误处理逻辑]

4. **env 字段缺失时静默跳过** ✅
   - `cat > ~/.zen-code/settings.json << 'EOF'...`
   - `cargo run -p rust-agent-tui -- --help 2>&1 | head -5`
   - Expected: TUI 启动成功，无 panic
   - On failure: check Task 1 [env 字段 Option 类型] 或 Task 2 [错误处理]
