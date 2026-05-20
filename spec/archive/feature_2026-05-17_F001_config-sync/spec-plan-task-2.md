### Task 2: 协议类型 + 加密模块

**背景:**
定义 WebSocket 消息协议类型（供后续 Task 3 sender、Task 4 receiver 共用）和端到端加密实现（PBKDF2-SHA256 密钥派生 + AES-256-GCM 加解密）。peri-tui 当前无 `sync` 模块，需从零搭建模块骨架。本 Task 的输出是后续所有 sync 子模块的公共基础，被 sender/receiver/packer 等模块直接依赖。

**涉及文件:**
- 新建: `peri-tui/src/sync/mod.rs`
- 新建: `peri-tui/src/sync/protocol.rs`
- 新建: `peri-tui/src/sync/crypto.rs`
- 修改: `peri-tui/src/lib.rs`
- 修改: `peri-tui/Cargo.toml`

**执行步骤:**
- [x] 在 `peri-tui/Cargo.toml` 新增依赖项
  - 位置: `peri-tui/Cargo.toml` `[dependencies]` 段末尾（~L53）
  - 新增以下依赖:
    - `tokio-tungstenite = "0.24"` — WebSocket 客户端，匹配 tokio 异步运行时
    - `aes-gcm = "0.10"` — AES-256-GCM 加解密
    - `ring = "0.17"` — PBKDF2-SHA256 密钥派生 + 安全随机数
    - `rmp-serde = "1.3"` — MessagePack 序列化/反序列化

- [x] 在 `peri-tui/src/lib.rs` 添加 sync 模块声明
  - 位置: `peri-tui/src/lib.rs` 在 `pub mod prompt;` (~L21) 之后插入一行 `pub mod sync;`

- [x] 创建 `peri-tui/src/sync/mod.rs` — 模块入口
  - 声明子模块 `pub mod protocol;` 和 `pub mod crypto;`

- [x] 创建 `peri-tui/src/sync/protocol.rs` — 定义 WebSocket 消息协议类型
  - 导入: `use serde::{Deserialize, Serialize};`
  - 定义 `WsMessage` 枚举，使用 `#[serde(tag = "type", content = "payload")]` 的 serde 内部标签模式:
    - `RequestPair` — sender 请求配对码（Client → Server）
    - `JoinPair { pair_code: String }` — receiver 输入码加入（Client → Server）
    - `SyncConfig { items: SyncItems }` — receiver 告知同步项（Client → Server）
    - `DataChunk { seq: u32, data: Vec<u8> }` — 加密数据块（双向: Client → Server 和 Server → Client）
    - `TransferComplete { checksum: String }` — 传输完成（双向）
    - `PairCreated { pair_code: String }` — 返回配对码（Server → Client）
    - `PairJoined { peer_info: PeerInfo }` — 配对成功通知（Server → Client）
    - `Error { code: String, message: String }` — 错误消息（Server → Client）
  - 注意: `DataChunk` 和 `TransferComplete` 是双向共用变体，serde tag 模式下同名字段自动统一
  - 定义辅助结构体:
    - `SyncPackage { version: u32, timestamp: u64, items: SyncItems }` — 同步数据包
    - `SyncItems { settings: Option<SettingsItem>, skills: Option<FilesItem>, mcp: Option<McpItem>, plugins: Option<FilesItem> }` — 同步项集合，派生 Default
    - `SettingsItem { content: String }` — 配置文件内容
    - `FilesItem { files: Vec<FileEntry> }` — 文件列表
    - `FileEntry { path: String, content: Vec<u8> }` — 单个文件（相对路径 + 二进制内容）
    - `McpItem { global: Option<String>, project: Option<String> }` — MCP 配置
    - `PeerInfo { version: String, os: String, hostname: String }` — 对端信息（后续可扩展）

- [x] 创建 `peri-tui/src/sync/crypto.rs` — 实现端到端加密
  - 导入:
    - `aes_gcm::{Aes256Gcm, KeyInit, Nonce, aead::Aead}`
    - `ring::pbkdf2::{self, PBKDF2_HMAC_SHA256}`
    - `ring::rand::{SecureRandom, SystemRandom}`
    - `std::num::NonZeroU32`
  - 定义公���常量:
    - `pub const AES_KEY_LEN: usize = 32;` — PBKDF2 输出 32 字节密钥
    - `pub const IV_LEN: usize = 12;` — AES-GCM 标准 nonce 长度
    - `pub const PBKDF2_ITERATIONS: u32 = 100_000;` — 迭代次数
    - `pub const CHUNK_SIZE: usize = 65536;` — 分片大小 64KB（供后续 sender 使用）
  - 实现 `pub fn derive_key(pair_code: &str) -> [u8; AES_KEY_LEN]`:
    - 调用 `pbkdf2::derive(PBKDF2_HMAC_SHA256, NonZeroU32::new(PBKDF2_ITERATIONS).unwrap(), pair_code.as_bytes(), &mut key)`
    - 返回 32 字节密钥数组
  - 实现 `pub fn encrypt(plaintext: &[u8], key: &[u8; AES_KEY_LEN]) -> Vec<u8>`:
    - 用 `SystemRandom::new().fill(&mut iv_bytes)` 生成随机 12 字节 IV
    - 用 `Aes256Gcm::new_from_slice(key).unwrap()` 构造 cipher（32 字节 key 保证不会失败）
    - 调用 `cipher.encrypt(Nonce::from_slice(&iv_bytes), plaintext).expect("AES-GCM encryption failed")` 得到 ciphertext（含 16 字节 AuthTag）
    - 返回 `[IV(12B) | ciphertext + tag(16B)]` 的字节数组
  - 实现 `pub fn decrypt(encrypted_data: &[u8], key: &[u8; AES_KEY_LEN]) -> anyhow::Result<Vec<u8>>`:
    - 校验 `encrypted_data.len() >= IV_LEN`，不足返回 `anyhow::anyhow!("data too short")`
    - 切分: `iv = &encrypted_data[..IV_LEN]`, `ciphertext = &encrypted_data[IV_LEN..]`
    - 用 `Aes256Gcm::new_from_slice(key).unwrap()` 构造 cipher
    - 调用 `cipher.decrypt(Nonce::from_slice(iv), ciphertext)` 返回 `anyhow::Result<Vec<u8>>`（解密失败由 `?` 自动转为 anyhow 错误）

- [x] 为协议类型和加密模块编写单元测试
  - 测试文件: `peri-tui/src/sync/crypto_test.rs`（按项目规范，≥30 行分离）
  - 测试场景:
    - `test_derive_key_deterministic`: 相同 pair_code 产生相同密钥，不同 pair_code 产生不同密钥
    - `test_encrypt_decrypt_roundtrip`: 加密任意明文后解密，结果与原文一致
    - `test_decrypt_wrong_key_fails`: 不同密钥解密应失败
    - `test_decrypt_truncated_data_fails`: 密文长度不足 IV_LEN 时 `decrypt` 返回 Err
    - `test_ws_message_serde_roundtrip`: 将 `WsMessage::RequestPair`、`WsMessage::JoinPair{...}`、`WsMessage::DataChunk{...}` 等序列化为 JSON 再反序列化，验证 `#[serde(tag = "type")]` 的 tag 分发正确
    - `test_sync_package_rmp_serde`: 用 `rmp_serde::from_slice` 和 `rmp_serde::to_vec` 验证 SyncPackage 的 MessagePack 序列化/反序列化循环
  - 运行命令: `cargo test -p peri-tui -- sync::crypto_test sync::protocol`
  - 预期: 所有测试通过

**检查步骤:**
- [x] 验证新增依赖在 Cargo.toml 中声明正确
  - `grep -n 'tokio-tungstenite\|aes-gcm\|ring\|rmp-serde' peri-tui/Cargo.toml`
  - 预期: 各输出一行，版本号与执行步骤一致
- [x] 验证 lib.rs 包含 sync 模块声明
  - `grep 'pub mod sync' peri-tui/src/lib.rs`
  - 预期: 输出 `pub mod sync;`
- [x] 验证 protocol.rs 导出所有结构体
  - `grep -c 'pub (struct|enum)' peri-tui/src/sync/protocol.rs`
  - 预期: 输出 9（WsMessage 枚举 + 7 个结构体 = 8，但 WsMessage 是 enum 不是 struct。Let me correct: 1 enum WsMessage + 7 structs = 8 public items. Actually: WsMessage, SyncPackage, SyncItems, SettingsItem, FilesItem, FileEntry, McpItem, PeerInfo = 8 items. But grep for `pub (struct|enum)` would match WsMessage as enum and 7 as struct = 8 total. Hmm, let me count: SyncPackage, SyncItems, SettingsItem, FilesItem, FileEntry, McpItem, PeerInfo = 7 structs + 1 enum WsMessage = 8 items total. So `grep -cE 'pub (struct|enum)' ...` should output 8.)
  - 修正: `grep -cE 'pub (struct|enum)' peri-tui/src/sync/protocol.rs`
  - 预期: 输出 8（1 个 enum + 7 个 struct）
- [x] 验证编译通过
  - `cargo check -p peri-tui 2>&1 | tail -3`
  - 预期: 无 error 信息，若输出中无 "error" 字样则通过
- [x] 运行本 Task 的单元测试
  - `cargo test -p peri-tui -- sync::crypto_test sync::protocol 2>&1 | tail -10`
  - 预期: 输出包含 "test result: ok"，无 FAILED

---

