### Task 3: 配置扫描 + 数据打包

**背景:**
实现 sender 端的配置扫描（读取本地 settings.json、skills、MCP 配置、插件）和数据打包（SyncPackage → MessagePack 序列化 → AES-256-GCM 加密 → 64KB 分片）。本 Task 是 sender 模式的核心数据准备层，输出供后续 sender.rs（Task 5）直接使用。依赖 Task 2 中定义的协议类型（SyncPackage/SyncItems/SettingsItem/FilesItem/McpItem/FileEntry）和加密函数（derive_key/encrypt/CHUNK_SIZE）。

**涉及文件:**
- 新建: `peri-tui/src/sync/scanner.rs`
- 新建: `peri-tui/src/sync/packer.rs`
- 新建: `peri-tui/src/sync/scanner_test.rs`
- 新建: `peri-tui/src/sync/packer_test.rs`
- 修改: `peri-tui/src/sync/mod.rs`

**执行步骤:**
- [x] 在 `peri-tui/src/sync/mod.rs` 声明新子模块
  - 位置: `peri-tui/src/sync/mod.rs` — 在 `pub mod crypto;` 之后追加
  - 追加两行：
    ```rust
    pub mod scanner;
    pub mod packer;
    ```
  - 原因: 将 scanner 和 packer 加入 sync 模块可见作用域

- [x] 创建 `peri-tui/src/sync/scanner.rs` — 实现本地配置文件扫描
  - 导入：
    ```rust
    use std::path::{Path, PathBuf};
    use std::fs;
    use crate::sync::protocol::{SyncPackage, SyncItems, SettingsItem, FilesItem, McpItem, FileEntry};
    use tracing;
    ```
  - 实现 `scan_directory(base: &Path) -> Vec<FileEntry>`（模块私有，不暴露）：
    ```rust
    fn scan_directory(base: &Path) -> Vec<FileEntry> {
        let mut files = Vec::new();
        if !base.exists() || !base.is_dir() {
            return files;
        }
        scan_dir_recursive(base, base, &mut files);
        files
    }

    fn scan_dir_recursive(base: &Path, dir: &Path, files: &mut Vec<FileEntry>) {
        let entries = match fs::read_dir(dir) {
            Ok(e) => e,
            Err(e) => {
                tracing::warn!("Failed to read directory {:?}: {}", dir, e);
                return;
            }
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                let rel = match path.strip_prefix(base) {
                    Ok(r) => r,
                    Err(_) => continue,
                };
                match fs::read(&path) {
                    Ok(content) => files.push(FileEntry {
                        path: rel.to_string_lossy().into_owned(),
                        content,
                    }),
                    Err(e) => tracing::warn!("Failed to read file {:?}: {}", path, e),
                }
            } else if path.is_dir() {
                scan_dir_recursive(base, &path, files);
            }
        }
    }
    ```
  - 实现 `pub fn scan_settings(home_dir: &Path) -> Option<SettingsItem>`：
    ```rust
    pub fn scan_settings(home_dir: &Path) -> Option<SettingsItem> {
        let path = home_dir.join(".peri").join("settings.json");
        match fs::read_to_string(&path) {
            Ok(content) => Some(SettingsItem { content }),
            Err(e) => {
                tracing::warn!("Failed to read settings.json at {:?}: {}", path, e);
                None
            }
        }
    }
    ```
    - 路径为 `{home_dir}/.peri/settings.json`，`home_dir` 由调用方通过 `dirs_next::home_dir()` 传入
  - 实现 `pub fn scan_skills(home_dir: &Path) -> FilesItem`：
    ```rust
    pub fn scan_skills(home_dir: &Path) -> FilesItem {
        let base = home_dir.join(".claude").join("skills");
        let files = scan_directory(&base);
        tracing::info!("Scanned {} files from skills directory", files.len());
        FilesItem { files }
    }
    ```
    - 路径为 `{home_dir}/.claude/skills/`
  - 实现 `pub fn scan_mcp(home_dir: &Path, cwd: &Path) -> McpItem`：
    ```rust
    pub fn scan_mcp(home_dir: &Path, cwd: &Path) -> McpItem {
        let global_path = home_dir.join(".mcp.json");
        let project_path = cwd.join(".mcp.json");
        let global = fs::read_to_string(&global_path).ok();
        let project = fs::read_to_string(&project_path).ok();
        McpItem { global, project }
    }
    ```
    - 全局：`{home_dir}/.mcp.json`；项目级：`{cwd}/.mcp.json`
    - 文件不存在时字段为 `None`，不报错
  - 实现 `pub fn scan_plugins(home_dir: &Path) -> FilesItem`：
    ```rust
    pub fn scan_plugins(home_dir: &Path) -> FilesItem {
        let base = home_dir.join(".claude").join("plugins").join("cache");
        let files = scan_directory(&base);
        tracing::info!("Scanned {} files from plugins cache", files.len());
        FilesItem { files }
    }
    ```
    - 路径为 `{home_dir}/.claude/plugins/cache/`
  - 实现 `pub fn scan_all(home_dir: &Path, cwd: &Path, items_filter: &SyncItems) -> SyncPackage`：
    ```rust
    pub fn scan_all(home_dir: &Path, cwd: &Path, items_filter: &SyncItems) -> SyncPackage {
        use std::time::{SystemTime, UNIX_EPOCH};

        let items = SyncItems {
            settings: if items_filter.settings.is_some() { scan_settings(home_dir) } else { None },
            skills: if items_filter.skills.is_some() { Some(scan_skills(home_dir)) } else { None },
            mcp: if items_filter.mcp.is_some() { Some(scan_mcp(home_dir, cwd)) } else { None },
            plugins: if items_filter.plugins.is_some() { Some(scan_plugins(home_dir)) } else { None },
        };

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        SyncPackage { version: 1, timestamp, items }
    }
    ```
    - `items_filter` 是 receiver 传回的 SyncItems，字段为 `Some` 表示需要同步该类别，`None` 跳过
    - version 固定为 1
    - timestamp 用 `SystemTime::now().duration_since(UNIX_EPOCH).as_secs()`

- [x] 创建 `peri-tui/src/sync/packer.rs` — 实现数据打包、加密和分片
  - 导入：
    ```rust
    use crate::sync::protocol::SyncPackage;
    use crate::sync::crypto;
    use anyhow::{anyhow, Result};
    use tracing;
    ```
  - 定义公共类型：
    ```rust
    /// 加密分片后的数据块
    #[derive(Debug, Clone)]
    pub struct ChunkData {
        pub seq: u32,
        pub data: Vec<u8>,   // 加密后的分片密文
    }

    /// 打包后的完整数据（含所有分片）
    #[derive(Debug)]
    pub struct PackedData {
        pub chunks: Vec<ChunkData>,
        pub encrypted_size: usize,
    }
    ```
  - 实现 `SyncPackage` 的 MessagePack 序列化方法：
    ```rust
    impl SyncPackage {
        pub fn to_msgpack(&self) -> Result<Vec<u8>> {
            rmp_serde::to_vec(self).map_err(|e| anyhow!("msgpack serialize failed: {}", e))
        }
    }
    ```
  - 实现 `pub fn pack(sync_pkg: &SyncPackage, pair_code: &str) -> Result<PackedData>`：
    ```rust
    pub fn pack(sync_pkg: &SyncPackage, pair_code: &str) -> Result<PackedData> {
        // Step 1: MessagePack 序列化
        let msgpack_bytes = sync_pkg.to_msgpack()?;
        tracing::debug!("Serialized package: {} bytes", msgpack_bytes.len());

        // Step 2: 密钥派生
        let key = crypto::derive_key(pair_code);

        // Step 3: AES-256-GCM 加密
        let encrypted = crypto::encrypt(&msgpack_bytes, &key);
        let total_size = encrypted.len();
        tracing::info!("Encrypted data: {} bytes, splitting into chunks", total_size);

        // Step 4: 按 CHUNK_SIZE 64KB 分片
        let chunks: Vec<ChunkData> = encrypted
            .chunks(crypto::CHUNK_SIZE)
            .enumerate()
            .map(|(i, chunk)| ChunkData {
                seq: i as u32,
                data: chunk.to_vec(),
            })
            .collect();

        tracing::info!("Packed into {} chunks", chunks.len());
        Ok(PackedData { chunks, encrypted_size: total_size })
    }
    ```
    - 返回值含 `encrypted_size`，供 sender 端计算传输进度百分比

  - 实现 `pub fn compute_checksum(data: &[u8]) -> String`：
    ```rust
    /// 计算加密数据的 SHA-256 校验和，用于 transfer_complete 完整性验证
    pub fn compute_checksum(data: &[u8]) -> String {
        use ring::digest::{digest, SHA256};
        let d = digest(&SHA256, data);
        d.as_ref().iter().map(|b| format!("{:02x}", b)).collect()
    }
    ```
    - ring crate 已在 Task 2 中添加为依赖，sha256 功能可用
    - 手动格式化为十六进制字符串，避免引入额外 `hex` 依赖

- [x] 为 scanner 模块编写单元测试
  - 测试文件: `peri-tui/src/sync/scanner_test.rs`（新建，按项目规范测试分离）
  - 使用 `tempfile::TempDir` 构造临时目录测试环境。检查 `peri-tui/Cargo.toml` 的 `[dev-dependencies]` 段是否已有 `tempfile`；若无，在 `[dev-dependencies]` 新增 `tempfile = "3"`。
  - 测试场景：
    - `test_scan_settings_existing_file`：在临时 home 目录创建 `{tmp}/.peri/settings.json`（内容 `{"key":"value"}`），调用 `scan_settings(&tmp)` → 返回 `Some(SettingsItem { content: "{\"key\":\"value\"}" })`
    - `test_scan_settings_missing_file`：临时 home 目录下不创建 settings.json，调用 `scan_settings(&tmp)` → 返回 `None`
    - `test_scan_skills_with_files`：在临时 home 目录创建 `{tmp}/.claude/skills/my-skill/SKILL.md`（内容 b"skill content"），调用 `scan_skills(&tmp)` → 返回 `FilesItem { files }` 含 1 条 `FileEntry { path: "my-skill/SKILL.md", content: b"skill content" }`
    - `test_scan_mcp_both_configs`：在临时 home 创建 `{tmp}/.mcp.json`（内容 `{"global":true}`），在临时 cwd 创建 `{cwd}/.mcp.json`（内容 `{"project":true}`），调用 `scan_mcp(&tmp, &cwd)` → `McpItem { global: Some("{\"global\":true}"), project: Some("{\"project\":true}") }`
    - `test_scan_all_respects_filter`：临时目录中准备 settings + skills，传入 `SyncItems { settings: Some(SettingsItem { content: String::new() }), skills: None, mcp: None, plugins: None }`，调用 `scan_all` → 返回的 SyncPackage 中 settings 为 Some，skills 为 None
    - `test_scan_all_timestamp_is_recent`：调用 `scan_all` → 返回的 `timestamp` 大于 0 且在 `std::time::UNIX_EPOCH.elapsed().unwrap().as_secs()` 合理范围内
  - 运行命令: `cargo test -p peri-tui -- sync::scanner_test`
  - 预期: 6 个测试全部通过

- [x] 为 packer 模块编写单元测试
  - 测试文件: `peri-tui/src/sync/packer_test.rs`（新建）
  - 测试场景：
    - `test_to_msgpack_roundtrip`：构造 `SyncPackage`，调用 `to_msgpack()` 序列化，再用 `rmp_serde::from_slice::<SyncPackage>(&bytes)` 反序列化 → 反序列化的 package 与原对象字段一致
    - `test_pack_produces_chunks`：构造 `SyncPackage { version: 1, timestamp: 0, items: SyncItems::default() }`（空内容），调用 `pack(&pkg, "test123")` → 返回 `Ok(PackedData { chunks, .. })` 且 `chunks.len() >= 1`（最小有 1 个分片）
    - `test_pack_same_code_same_key`：相同 pair_code 调用两次 `pack`（相同 package）→ 两次产生的加密数据不同（因随机 IV），但分片数量相同
    - `test_compute_checksum_deterministic`：相同数据调用两次 `compute_checksum` → 返回相同 64 字符十六进制字符串
    - `test_compute_checksum_different_data`：不同数据调用 `compute_checksum` → 返回不同字符串
  - 运行命令: `cargo test -p peri-tui -- sync::packer_test`
  - 预期: 5 个测试全部通过

**检查步骤:**
- [x] 验证 scanner.rs 所有公有函数存在
  - `grep -E 'pub fn scan_(settings|skills|mcp|plugins|all)' peri-tui/src/sync/scanner.rs`
  - 预期: 输出 5 行，依次对应 scan_settings、scan_skills、scan_mcp、scan_plugins、scan_all
- [x] 验证 packer.rs 公有类型和函数存在
  - `grep -cE 'pub (fn|struct)' peri-tui/src/sync/packer.rs`
  - 预期: 输出 4（ChunkData struct + PackedData struct + pack fn + compute_checksum fn）
- [x] 验证 mod.rs 声明了新模块
  - `grep -c 'pub mod (scanner|packer)' peri-tui/src/sync/mod.rs`
  - 预期: 输出 2
- [x] 运行 scanner 单元测试
  - `cargo test -p peri-tui -- sync::scanner_test 2>&1 | tail -5`
  - 预期: 输出包含 "test result: ok"，无 FAILED
- [x] 运行 packer 单元测试
  - `cargo test -p peri-tui -- sync::packer_test 2>&1 | tail -5`
  - 预期: 输出包含 "test result: ok"，无 FAILED

---
