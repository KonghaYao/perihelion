# 工作区 .peri/settings.json 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在 `store::load()` 中自动检测工作区 `{cwd}/.peri/settings.json` 并与全局配置合并，工作区字段覆盖全局对应字段。

**Architecture:** `AppConfig::merge_overrides()` 在 `peri-acp` 中实现纯数据合并逻辑；`store::load()` 在加载全局后检测工作区文件并调用合并。`save()` 始终写全局，工作区配置只读。

**Tech Stack:** Rust, serde, tempfile (测试), std::env::current_dir

---

### Task 1: `AppConfig::merge_overrides()` 方法 + 单元测试

**Files:**
- Modify: `peri-acp/src/provider/config.rs` — 在 `impl AppConfig` 块内新增 `merge_overrides` 方法，文件末尾新增 `#[cfg(test)] mod tests`

- [ ] **Step 1: 在 `impl AppConfig` 块末尾添加 `merge_overrides` 方法**

找到 `peri-acp/src/provider/config.rs` 中 `pub struct AppConfig` 的 `impl` 块（当前是 `#[derive]` 后的空 impl，若不存在则创建）。在 `impl AppConfig` 内最后一个方法后添加：

```rust
    /// 用 workspace 配置覆盖全局配置。
    /// workspace 中出现的字段替换全局对应字段，未出现的保留全局值。
    pub fn merge_overrides(&mut self, workspace: AppConfig) {
        // providers — 空列表视为"未填写"，不覆盖
        if !workspace.providers.is_empty() {
            self.providers = workspace.providers;
        }
        // 字符串字段 — 非空则覆盖
        if !workspace.active_alias.is_empty() {
            self.active_alias = workspace.active_alias;
        }
        if !workspace.active_provider_id.is_empty() {
            self.active_provider_id = workspace.active_provider_id;
        }
        // Option<T> 字段 — is_some() 则覆盖
        if workspace.skills_dir.is_some() {
            self.skills_dir = workspace.skills_dir;
        }
        if workspace.thinking.is_some() {
            self.thinking = workspace.thinking;
        }
        if workspace.env.is_some() {
            self.env = workspace.env;
        }
        if workspace.compact.is_some() {
            self.compact = workspace.compact;
        }
        if workspace.language.is_some() {
            self.language = workspace.language;
        }
        if workspace.persona.is_some() {
            self.persona = workspace.persona;
        }
        if workspace.tone.is_some() {
            self.tone = workspace.tone;
        }
        if workspace.claude_md_excludes.is_some() {
            self.claude_md_excludes = workspace.claude_md_excludes;
        }
        if workspace.proactiveness.is_some() {
            self.proactiveness = workspace.proactiveness;
        }
        if workspace.context_1m.is_some() {
            self.context_1m = workspace.context_1m;
        }
        // diff_enabled: bool 直接覆盖（无法区分"未写 false"和"写了 false"）
        self.diff_enabled = workspace.diff_enabled;
        // 保留未知字段
        self.extra.extend(workspace.extra);
    }
```

**注意**：若 `impl AppConfig` 块不存在（当前 `AppConfig` 仅靠 `#[derive]`），需在结构体定义后新增：

```rust
impl AppConfig {
    pub fn merge_overrides(&mut self, workspace: AppConfig) {
        // ... 同上 ...
    }
}
```

- [ ] **Step 2: 在文件末尾添加测试模块**

在 `peri-acp/src/provider/config.rs` 最末尾追加：

```rust
#[cfg(test)]
#[path = "config_test.rs"]
mod tests;
```

- [ ] **Step 3: 创建测试文件**

新建 `peri-acp/src/provider/config_test.rs`：

```rust
use super::*;
use std::collections::HashMap;

fn make_global() -> AppConfig {
    AppConfig {
        active_alias: "sonnet".to_string(),
        active_provider_id: "openai-1".to_string(),
        providers: vec![ProviderConfig {
            id: "openai-1".to_string(),
            provider_type: "openai".to_string(),
            api_key: "sk-global".to_string(),
            ..Default::default()
        }],
        thinking: Some(ThinkingConfig {
            enabled: true,
            budget_tokens: 8000,
            effort: "medium".to_string(),
            max_tokens: 32000,
        }),
        language: Some("zh".to_string()),
        diff_enabled: true,
        ..Default::default()
    }
}

#[test]
fn test_merge_workspace_empty_changes_nothing() {
    let mut global = make_global();
    let workspace = AppConfig::default();
    global.merge_overrides(workspace);
    assert_eq!(global.active_alias, "sonnet");
    assert_eq!(global.providers.len(), 1);
    assert!(global.thinking.is_some());
    assert!(global.diff_enabled);
}

#[test]
fn test_merge_workspace_complete_overrides_all() {
    let mut global = make_global();
    let workspace = AppConfig {
        active_alias: "opus".to_string(),
        active_provider_id: "anthro-1".to_string(),
        providers: vec![ProviderConfig {
            id: "anthro-1".to_string(),
            provider_type: "anthropic".to_string(),
            api_key: "sk-ws".to_string(),
            ..Default::default()
        }],
        language: Some("en".to_string()),
        diff_enabled: false,
        ..Default::default()
    };
    global.merge_overrides(workspace);
    // 工作区出现的字段全部覆盖
    assert_eq!(global.active_alias, "opus");
    assert_eq!(global.active_provider_id, "anthro-1");
    assert_eq!(global.providers.len(), 1);
    assert_eq!(global.providers[0].provider_type, "anthropic");
    assert_eq!(global.language, Some("en".to_string()));
    assert!(!global.diff_enabled);
    // 工作区为 None 的字段保留全局值
    assert!(global.thinking.is_some());
}

#[test]
fn test_merge_providers_empty_array_does_not_override() {
    let mut global = make_global();
    let workspace = AppConfig {
        providers: vec![], // 空数组
        ..Default::default()
    };
    global.merge_overrides(workspace);
    assert_eq!(global.providers.len(), 1);
    assert_eq!(global.providers[0].api_key, "sk-global");
}

#[test]
fn test_merge_single_field_override() {
    let mut global = make_global();
    let workspace = AppConfig {
        active_alias: "haiku".to_string(),
        ..Default::default()
    };
    global.merge_overrides(workspace);
    assert_eq!(global.active_alias, "haiku");
    // 其他字段不变
    assert_eq!(global.providers.len(), 1);
    assert_eq!(global.providers[0].api_key, "sk-global");
}

#[test]
fn test_merge_env_override() {
    let mut global = AppConfig {
        env: Some(HashMap::from([("FOO".to_string(), "bar".to_string())])),
        ..make_global()
    };
    let workspace = AppConfig {
        env: Some(HashMap::from([("BAZ".to_string(), "qux".to_string())])),
        ..Default::default()
    };
    global.merge_overrides(workspace);
    let env = global.env.unwrap();
    assert!(!env.contains_key("FOO"));
    assert_eq!(env.get("BAZ"), Some(&"qux".to_string()));
}

#[test]
fn test_merge_diff_enabled_false_overrides_global_true() {
    let mut global = make_global(); // diff_enabled: true
    let workspace = AppConfig {
        diff_enabled: false,
        ..Default::default()
    };
    global.merge_overrides(workspace);
    assert!(!global.diff_enabled);
}
```

- [ ] **Step 4: 运行测试确认通过**

```bash
cargo test -p peri-acp --lib -- config_test
```
Expected: 6 个测试全部 PASS

- [ ] **Step 5: 运行全量测试确认无回归**

```bash
cargo test -p peri-acp --lib
```
Expected: 所有已存在测试 PASS

- [ ] **Step 6: Commit**

```bash
git add peri-acp/src/provider/config.rs peri-acp/src/provider/config_test.rs
git commit -m "feat(peri-acp): AppConfig::merge_overrides 工作区配置合并

Co-Authored-By: deepseek-v4-pro <deepseek-ai@claude-code-best.win>"
```

---

### Task 2: `store.rs` 工作区加载逻辑

**Files:**
- Modify: `peri-tui/src/config/store.rs`

- [ ] **Step 1: 新增 `workspace_config_path()` 函数**

在 `config_path()` 下方添加：

```rust
/// 工作区配置文件路径：{cwd}/.peri/settings.json
/// 文件不存在时返回 None
pub fn workspace_config_path() -> Option<PathBuf> {
    let cwd = std::env::current_dir().ok()?;
    let path = cwd.join(".peri").join("settings.json");
    if path.exists() {
        Some(path)
    } else {
        None
    }
}
```

- [ ] **Step 2: 修改 `load()` 函数**

将当前的：

```rust
/// 加载配置，文件不存在时返回默认空配置
pub fn load() -> Result<PeriConfig> {
    load_from(&config_path())
}
```

替换为：

```rust
/// 加载配置（全局 + 工作区合并），文件不存在时返回默认空配置
/// 
/// 先加载 ~/.peri/settings.json 获取全局配置，
/// 再检测当前工作目录的 .peri/settings.json 是否存在，
/// 若存在则加载并以工作区字段覆盖全局对应字段。
pub fn load() -> Result<PeriConfig> {
    let mut merged = load_from(&config_path())?;
    if let Some(ws_path) = workspace_config_path() {
        let workspace = load_from(&ws_path)?;
        merged.config.merge_overrides(workspace.config);
    }
    Ok(merged)
}
```

- [ ] **Step 3: 构建检查**

```bash
cargo build -p peri-tui
```
Expected: 编译成功，无警告

- [ ] **Step 4: 运行 store 相关测试**

```bash
cargo test -p peri-tui --lib -- config
```
Expected: 所有已存在测试 PASS

- [ ] **Step 5: Commit**

```bash
git add peri-tui/src/config/store.rs
git commit -m "feat(peri-tui): store::load 自动检测工作区 .peri/settings.json

Co-Authored-By: deepseek-v4-pro <deepseek-ai@claude-code-best.win>"
```

---

### Task 3: `store.rs` 集成测试

**Files:**
- Modify: `peri-tui/src/config/mod.rs` — 在测试模块声明处添加 `store_test`

- [ ] **Step 1: 声明测试模块**

在 `peri-tui/src/config/mod.rs` 中，紧接 `types_test.rs` 的测试声明后添加：

```rust
#[cfg(test)]
#[path = "store_test.rs"]
mod store_tests;
```

- [ ] **Step 2: 创建集成测试文件**

新建 `peri-tui/src/config/store_test.rs`：

```rust
use super::store::{load, load_from};
use std::io::Write;

/// 在临时目录创建 settings.json 并返回目录 guard
fn write_settings(dir: &std::path::Path, filename: &str, content: &str) {
    let peri_dir = dir.join(".peri");
    std::fs::create_dir_all(&peri_dir).unwrap();
    let mut f = std::fs::File::create(peri_dir.join(filename)).unwrap();
    f.write_all(content.as_bytes()).unwrap();
}

#[test]
fn test_load_global_only_no_workspace() {
    let tmp = tempfile::tempdir().unwrap();
    // 此测试用 load_from 直接验证，load() 依赖 current_dir 不便在测试中控制
    // 先验证 merge_overrides 路径在单元测试中已覆盖，
    // 这里验证 load_from 行为不变
    let cfg = load_from(&tmp.path().join("nonexistent.json")).unwrap();
    assert!(cfg.config.providers.is_empty());
}

#[test]
fn test_workspace_config_path() {
    // workspace_config_path 依赖 current_dir，集成测试中不做断言
    // 只验证函数不 panic
    let _ = super::store::workspace_config_path();
}
```

> **说明**：`load()` 的合并行为依赖 `std::env::current_dir()`，在单元测试中 mock cwd 不实际。合并逻辑的覆盖率由 Task 1 的 `merge_overrides` 单元测试保证。此处只做基本的 smoke test。

- [ ] **Step 3: 运行测试**

```bash
cargo test -p peri-tui --lib -- store_test
```
Expected: 2 个测试 PASS

- [ ] **Step 4: Commit**

```bash
git add peri-tui/src/config/mod.rs peri-tui/src/config/store_test.rs
git commit -m "test(peri-tui): store 工作区加载集成测试

Co-Authored-By: deepseek-v4-pro <deepseek-ai@claude-code-best.win>"
```

---

## 最终验证

- [ ] **全量编译**

```bash
cargo build
```
Expected: 所有 crate 编译成功

- [ ] **全量测试**

```bash
cargo test
```
Expected: 所有测试 PASS

- [ ] **clippy**

```bash
cargo clippy --all-targets
```
Expected: 无新增 warning
