### Task 4: 文件写入 + 路径穿越防护

**背景:**
本 Task 实现 receiver 端的数据落地——接收解密后的 SyncItems，将配置写入本地文件系统。核心难点是路径穿越防护：SyncItems 中的 FileEntry.path 可能被篡改为 `../.ssh/authorized_keys` 或 `/etc/passwd` 等恶意路径，必须在写入前进行严格校验。本 Task 依赖 Task 2 中定义的协议类型（SyncPackage/SyncItems/SettingsItem/FilesItem/McpItem/FileEntry），Task 5（sender/receiver 交互）调用 `write_sync_items` 完成最终写入。

**涉及文件:**
- 新建: `peri-tui/src/sync/writer.rs`
- 新建: `peri-tui/src/sync/writer_test.rs`
- 修改: `peri-tui/Cargo.toml`
- 修改: `peri-tui/src/sync/mod.rs`

**执行步骤:**
- [x] 在 `peri-tui/Cargo.toml` 新增 `thiserror` 依赖
  - 位置: `peri-tui/Cargo.toml` `[dependencies]` 段末尾，在 Task 2 新增的 `rmp-serde = "1.3"` 之后
  - 追加一行: `thiserror = "1"`
  - 原因: `writer.rs` 用 `#[derive(Error)]` 定义 WriteError，thiserror 是项目其他 crate（peri-agent/peri-lsp/peri-middlewares）已使用的标准错误库

- [x] 在 `peri-tui/src/sync/mod.rs` 声明 writer 子模块
  - 位置: `peri-tui/src/sync/mod.rs` — 在 Task 3 新增的 `pub mod packer;` 之后追加
  - 追加一行:
    ```rust
    pub mod writer;
    ```
  - 原因: 将 writer 加入 sync 模块可见作用域

- [x] 创建 `peri-tui/src/sync/writer.rs` — 定义 WriteError 错误类型
  - 导入:
    ```rust
    use std::path::{Path, PathBuf};
    use std::fs;
    use crate::sync::protocol::{SyncItems, FileEntry};
    use tracing;
    ```
  - 定义 `WriteError` 枚举（两变体）:
    ```rust
    /// 文件写入错误类型
    #[derive(Debug, thiserror::Error)]
    pub enum WriteError {
        /// 路径穿越攻击或非法路径
        #[error("路径穿越攻击：{0}")]
        PathTraversal(String),
        /// 文件 I/O 错误
        #[error("文件写入失败：{0}")]
        Io(#[from] std::io::Error),
    }
    ```
    - `PathTraversal(String)` 携带被拒绝的路径信息，方便日志记录
    - `Io` 变体使用 `#[from]` 实现自动 From 转换，调用方可使用 `?` 传播 I/O 错误

- [x] 实现 `validate_and_resolve` 路径安全校验函数
  - 位置: `peri-tui/src/sync/writer.rs` — 在 WriteError 定义之后
  - 函数签名:
    ```rust
    /// 验证相对路径安全并解析为绝对路径
    ///
    /// 安全检查：
    /// 1. 拒绝绝对路径（Unix / 开头、Windows C:\ 或 \\ 开头）
    /// 2. 拒绝包含 .. 父目录组件的路径（防止 ../ 穿越到 base_dir 外部）
    /// 3. 解析后验证最终路径仍以 base_dir 为前缀（兜底防护）
    pub fn validate_and_resolve(base_dir: &Path, relative_path: &str) -> Result<PathBuf, WriteError>
    ```
  - 实现逻辑:
    ```rust
    pub fn validate_and_resolve(base_dir: &Path, relative_path: &str) -> Result<PathBuf, WriteError> {
        // Step 1: 拒绝绝对路径
        let rel = Path::new(relative_path);
        if rel.is_absolute()
            || relative_path.starts_with('/')
            || relative_path.starts_with('\\')
            || (relative_path.len() > 2 && relative_path.as_bytes()[1] == b':') // Windows C:\
        {
            tracing::warn!("拒绝绝对路径或非法路径前缀: {}", relative_path);
            return Err(WriteError::PathTraversal(format!("绝对路径被拒绝: {}", relative_path)));
        }

        // Step 2: 逐组件检查 —— 拒绝任何 ParentDir 组件
        let mut depth: i32 = 0;
        for component in rel.components() {
            match component {
                std::path::Component::ParentDir => {
                    depth -= 1;
                    if depth < 0 {
                        tracing::warn!("路径穿越攻击拒绝: {} (base: {:?})", relative_path, base_dir);
                        return Err(WriteError::PathTraversal(format!("路径包含 .. 穿越: {}", relative_path)));
                    }
                }
                std::path::Component::Normal(_) => depth += 1,
                _ => {} // RootDir 和 Prefix 已在 is_absolute() 中拒绝
            }
        }

        // Step 3: 解析绝对路径并验证仍在 base_dir 内（兜底）
        let resolved = base_dir.join(rel);
        // 规范化路径（消除 ./ 和内部 ../ 残留）
        let normalized = normalize_path(&resolved);
        if !normalized.starts_with(base_dir) {
            tracing::warn!("路径解析后逃逸 base_dir: {:?} (base: {:?})", normalized, base_dir);
            return Err(WriteError::PathTraversal(format!("路径逃逸 base_dir: {}", relative_path)));
        }

        Ok(normalized)
    }
    ```
  - 辅助函数 `normalize_path`:
    ```rust
    /// 规范化路径：消除 . 和 .. 组件，返回纯绝对路径
    fn normalize_path(path: &Path) -> PathBuf {
        let mut result = PathBuf::new();
        for component in path.components() {
            match component {
                std::path::Component::ParentDir => {
                    result.pop();
                }
                std::path::Component::CurDir => {}
                other => {
                    result.push(other);
                }
            }
        }
        result
    }
    ```
  - 路径穿越检测要点:
    - 深度计数器 `depth` 从 0 开始，普通组件 +1，ParentDir 组件 -1
    - 当 `depth < 0` 表示路径尝试跳出 base_dir（如 `../etc/passwd`），立即拒绝
    - 通过 `../` 后跟正常目录的写法（如 `foo/../../bar`）也能检测
    - Step 3 的 `starts_with` 兜底确保即使深度检测被绕过也能拦截

- [x] 实现 `write_file_entry` — 写入单个文件条目
  - 位置: `peri-tui/src/sync/writer.rs` — 在 `validate_and_resolve` 之后
  - 函数签名:
    ```rust
    /// 向 base_dir 下安全写入一个 FileEntry
    /// 自动创建父目录，原子写入（写临时文件 → rename）
    pub fn write_file_entry(base_dir: &Path, entry: &FileEntry) -> Result<(), WriteError>
    ```
  - 实现逻辑:
    ```rust
    pub fn write_file_entry(base_dir: &Path, entry: &FileEntry) -> Result<(), WriteError> {
        let target_path = validate_and_resolve(base_dir, &entry.path)?;

        // 确保父目录存在
        if let Some(parent) = target_path.parent() {
            fs::create_dir_all(parent)?;
        }

        // 原子写入：先写临时文件，再 rename
        let tmp_path = target_path.with_extension("tmp");
        fs::write(&tmp_path, &entry.content)?;
        fs::rename(&tmp_path, &target_path)?;

        tracing::info!("已写入文件: {:?}", target_path);
        Ok(())
    }
    ```
  - 原子写入确保写入过程中断时不会留下半截文件

- [x] 实现 `write_sync_items` — 顶层写入派发函数
  - 位置: `peri-tui/src/sync/writer.rs` — 在文件末尾
  - 函数签名:
    ```rust
    /// 将同步项写入本地文件系统
    ///
    /// 路径映射：
    /// - settings → {home_dir}/.peri/settings.json（先备份为 .bak）
    /// - skills   → {home_dir}/.claude/skills/{relative_path}
    /// - mcp      → {home_dir}/.mcp.json + {cwd}/.mcp.json（如有）
    /// - plugins  → {home_dir}/.claude/plugins/cache/{relative_path}
    pub fn write_sync_items(home_dir: &Path, cwd: &Path, items: &SyncItems) -> Result<(), WriteError>
    ```
  - 实现逻辑:
    ```rust
    pub fn write_sync_items(home_dir: &Path, cwd: &Path, items: &SyncItems) -> Result<(), WriteError> {
        // 1. 写入 settings.json（原子写入 + 备份）
        if let Some(ref settings) = items.settings {
            let settings_path = home_dir.join(".peri").join("settings.json");
            let bak_path = home_dir.join(".peri").join("settings.json.bak");

            // 备份现有文件（如存在）
            if settings_path.exists() {
                fs::copy(&settings_path, &bak_path)?;
                tracing::info!("已备份 settings.json → settings.json.bak");
            }

            // 确保 .peri 目录存在
            if let Some(parent) = settings_path.parent() {
                fs::create_dir_all(parent)?;
            }

            // 原子写入
            let tmp_path = settings_path.with_extension("tmp");
            fs::write(&tmp_path, settings.content.as_bytes())?;
            fs::rename(&tmp_path, &settings_path)?;

            tracing::info!("已写入 settings.json ({})", settings.content.len());
        }

        // 2. 写入 skills
        if let Some(ref skills) = items.skills {
            let skills_base = home_dir.join(".claude").join("skills");
            for entry in &skills.files {
                write_file_entry(&skills_base, entry)?;
            }
            tracing::info!("已写入 {} 个 skills 文件", skills.files.len());
        }

        // 3. 写入 MCP 配置
        if let Some(ref mcp) = items.mcp {
            // 全局 .mcp.json
            if let Some(ref global_content) = mcp.global {
                let global_path = home_dir.join(".mcp.json");
                let tmp_path = global_path.with_extension("tmp");
                fs::write(&tmp_path, global_content.as_bytes())?;
                fs::rename(&tmp_path, &global_path)?;
                tracing::info!("已写入全局 .mcp.json");
            }
            // 项目级 .mcp.json
            if let Some(ref project_content) = mcp.project {
                let project_path = cwd.join(".mcp.json");
                let tmp_path = project_path.with_extension("tmp");
                fs::write(&tmp_path, project_content.as_bytes())?;
                fs::rename(&tmp_path, &project_path)?;
                tracing::info!("已写入项目级 .mcp.json");
            }
        }

        // 4. 写入 plugins
        if let Some(ref plugins) = items.plugins {
            let plugins_base = home_dir.join(".claude").join("plugins").join("cache");
            for entry in &plugins.files {
                write_file_entry(&plugins_base, entry)?;
            }
            tracing::info!("已写入 {} 个 plugin 文件", plugins.files.len());
        }

        Ok(())
    }
    ```
  - 写入顺序: settings → skills → mcp → plugins，每步独立错误传播
  - settings.json 写入前先备份：`settings.json` → `settings.json.bak`（copy，保留原文件以防写入失败）
  - 所有路径写入均使用原子写入模式（write tmp → rename），settings 和 mcp 的文本内容通过 `as_bytes()` 转换

- [x] 为 writer 模块编写单元测试
  - 测试文件: `peri-tui/src/sync/writer_test.rs`（新建，按项目规范测试分离）
  - 使用 `tempfile::TempDir` 构造临时目录。`tempfile` 已在 Task 3 添加到 `[dev-dependencies]` 段，无需重复添加。
  - 导入:
    ```rust
    use super::writer::*;
    use crate::sync::protocol::{SyncItems, SettingsItem, FilesItem, FileEntry, McpItem};
    use std::path::Path;
    ```
  - 测试场景（共 8 个）:
    - `test_validate_normal_relative_path`：输入 `"my-skill/SKILL.md"`，base_dir 为 `/tmp/base` → 返回 `Ok("/tmp/base/my-skill/SKILL.md")`
    - `test_validate_rejects_absolute_path`：输入 `"/etc/passwd"` → 返回 `Err(WriteError::PathTraversal(_))`
    - `test_validate_rejects_parent_dir_traversal`：输入 `"../.ssh/authorized_keys"` → 返回 `Err(WriteError::PathTraversal(_))`
    - `test_validate_rejects_hidden_traversal`：输入 `"foo/../../bar"`（depth 从 1 降为 -1）→ 返回 `Err(WriteError::PathTraversal(_))`
    - `test_write_file_entry_creates_parent_dirs`：在临时目录中写入 `FileEntry { path: "a/b/c.txt", content: b"hi" }` → 文件存在且内容正确，中间目录自动创建
    - `test_write_file_entry_rejects_traversal`：传入 `FileEntry { path: "../bad.txt", content: b"x" }` → 返回 `Err(WriteError::PathTraversal(_))`
    - `test_write_sync_items_settings_with_backup`：在临时 home 创建预先存在的 `{tmp}/.peri/settings.json`（内容 `"old"`），传入 `SyncItems { settings: Some(SettingsItem { content: "new".into() }), ..Default::default() }` 调用 `write_sync_items` → 新文件内容为 `"new"`，备份文件 `settings.json.bak` 内容为 `"old"`
    - `test_write_sync_items_all_categories`：构造完整的 SyncItems（含 settings、skills、mcp global+project、plugins），调用 `write_sync_items` → 所有文件均写入到目标路径且内容正确
  - 运行命令: `cargo test -p peri-tui -- sync::writer_test`
  - 预期: 8 个测试全部通过

**检查步骤:**
- [x] 验证 writer.rs 所有公有函数存在
  - `grep -cE 'pub (fn|enum)' peri-tui/src/sync/writer.rs`
  - 预期: 输出 4（WriteError enum + validate_and_resolve fn + write_file_entry fn + write_sync_items fn）

- [x] 验证 thiserror 依赖已添加到 Cargo.toml
  - `grep 'thiserror' peri-tui/Cargo.toml`
  - 预期: 输出含 `thiserror = "1"` 的一行

- [x] 验证 mod.rs 声明了 writer 模块
  - `grep 'pub mod writer' peri-tui/src/sync/mod.rs`
  - 预期: 输出 `pub mod writer;`

- [x] 验证编译通过
  - `cargo check -p peri-tui 2>&1 | tail -5`
  - 预期: 无 error 信息

- [x] 运行 writer 单元测试
  - `cargo test -p peri-tui -- sync::writer_test 2>&1 | tail -10`
  - 预期: 输出包含 "test result: ok"，8 个测试通过，无 FAILED

**认知变更:**
- [x] [CLAUDE.md] sync 模块的 `validate_and_resolve` 是项目标准的路径穿越防护入口，使用三层校验（绝对路径拒绝 + 深度计数器检测 ParentDir + 解析后前缀验证）。任何需要接收用户侧相对路径并写入 base_dir 的场景都必须复用此函数。新增类似写入功能时禁止自行实现路径校验。
- [x] [CLAUDE.md] [TRAP] sync 模块中 FileEntry.path 来自外部不可信数据（网络传输的解密结果），写入前必须经过 `validate_and_resolve` 校验，禁止直接拼接路径或使用未校验的相对路径

---
