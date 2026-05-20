# Feature: 20260517_F001 - config-sync

## 需求背景

Perihelion 用户在多台机器上使用时，需要手动复制 settings.json、skills、MCP 配置、插件等文��来保持环境一致。缺乏一个便捷的配置同步机制，导致新环境搭建成本高、配置漂移难以管理。

## 目标

- 提供一键式的配置同步功能，sender 端申请配对码，receiver 端输入码后即可同步
- 支持选择性同步：receiver 可勾选需要同步的项（settings/skills/mcp/plugins）
- 端到端加密：配对码派生 AES-256-GCM 密钥，relay 服务端只转发密文，无法读取内容
- 单向覆盖式同步：sender → receiver，无合并逻辑

## 方案设计

### 整体架构

```
┌──────────┐     WebSocket      ┌──────────────┐     WebSocket      ┌──────────┐
│  Sender   │◄─────────────────►│  Relay Server │◄─────────────────►│ Receiver  │
│  (Client) │                   │  (Hono.js)    │                   │  (Client) │
└──────────┘                   └──────────────┘                   └──────────┘
                                     │
                              配对码映射表 (内存)
                              WS 连接管理
                              消息转发（密文透传）
```

两个组件：

| 位置 | 职责 |
|------|------|
| `side-projects/peri-sync/server/` | Relay Server，Hono.js + WebSocket，配对码管理 + 消息转发 |
| `peri-tui/src/sync/` | Rust 同步客户端，作为 `peri sync` 子命令集成 |

Relay Server 无状态转发，不存储任何用户数据。配对码过期后自动清理。

**子命令入口**：`peri sync sender` / `peri sync receiver`，在 `main.rs` 的 `Commands` enum 中新增 `Sync` 变体。

### WebSocket 协议

所有消息均为 JSON 格式：

```rust
#[derive(Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
enum WsMessage {
    // Client → Server
    RequestPair,
    JoinPair { pair_code: String },
    SyncConfig { items: SyncItems },
    DataChunk { seq: u32, data: Vec<u8> },  // data 为加密后的密文
    TransferComplete { checksum: String },
    // Server → Client
    PairCreated { pair_code: String },
    PairJoined { peer_info: PeerInfo },
    DataChunk { seq: u32, data: Vec<u8> },
    TransferComplete { checksum: String },
    Error { code: String, message: String },
}
```

**消息类型**：

| 方向 | 类型 | 说明 |
|------|------|------|
| Client → Server | `request_pair` | sender 请求生成配对码 |
| Client → Server | `join_pair` | receiver 输入配对码加入 |
| Client → Server | `sync_config` | receiver 告知 sender 要同步的项 |
| Client → Server | `data_chunk` | sender 发送加密数据块 |
| Client → Server | `transfer_complete` | sender 传输完成 |
| Server → Client | `pair_created` | 返回配对码给 sender |
| Server → Client | `pair_joined` | 通知双方配对成功 |
| Server → Client | `data_chunk` | 转发数据块给 receiver |
| Server → Client | `transfer_complete` | 转发传输完成 |
| Server → Client | `error` | 错误（码不存在/已过期/已使用） |

### E2E 加密方案

```
配对码 "482917"
    │
    ▼ PBKDF2-SHA256 (salt=pairCode, iterations=100000, keyLen=32)
    │
    ▼
AES-256-GCM Key (32 bytes)
    │
    ▼ AES-256-GCM Encrypt (随机 12 字节 IV)
    │
加密数据包 = IV(12B) + Ciphertext + AuthTag(16B)
```

- 配对码作为共享密钥种子，双方各自用 PBKDF2-SHA256 派生 AES-256 密钥
- 每次传输随机生成 12 字节 IV，保证相同明文产生不同密文
- AES-GCM 同时提供加密和完整性校验（AuthTag）
- Relay Server 只转发密文，无法解密

### 同步数据打包格式

```rust
#[derive(Serialize, Deserialize)]
struct SyncPackage {
    version: u32,          // 1
    timestamp: u64,        // Unix timestamp
    items: SyncItems,
}

#[derive(Serialize, Deserialize, Default)]
struct SyncItems {
    settings: Option<SettingsItem>,
    skills: Option<FilesItem>,
    mcp: Option<McpItem>,
    plugins: Option<FilesItem>,
}

struct SettingsItem {
    content: String,       // settings.json 原文 JSON
}

struct FilesItem {
    files: Vec<FileEntry>,
}

struct FileEntry {
    path: String,          // 相对路径，如 "skills/xxx/SKILL.md"
    content: Vec<u8>,
}

struct McpItem {
    global: Option<String>,    // ~/.mcp.json 内容
    project: Option<String>,   // 项目级 .mcp.json 内容（如有）
}
```

打包流程：收集文件 → 构建 SyncPackage → rmp-serde 序列化 → AES-256-GCM 加密 → 分片（每片 64KB）→ WS 逐片发送。

解包流程：接收所有分片 → 合并 → AES-256-GCM 解密 → rmp-serde 反序列化 → 写入文件。

### 交互流程

```
Sender                           Relay                          Receiver
  │                                │                                │
  │── request_pair ───────────────►│                                │
  │◄── pair_created("482917") ─────│                                │
  │   显示: 配对码 482917          │                                │
  │                                │◄── join_pair("482917") ────────│
  │◄── pair_joined ────────────────│─── pair_joined ───────────────►│
  │                                │                                │
  │   (等待选择)                    │                                │
  │                                │◄── sync_config({items}) ───────│
  │◄── sync_config({items}) ───────│   receiver 选择同步项 → confirm │
  │                                │                                │
  │   展示传输清单 → 打包加密        │                                │
  │── data_chunk(encrypted) ──────►│── data_chunk(encrypted) ──────►│
  │── data_chunk(encrypted) ──────►│── data_chunk(encrypted) ──────►│
  │── transfer_complete ──────────►│── transfer_complete ──────────►│
  │   ✅ 传输完成                   │              解密 → 解压 → 写入  │
  │                                │              ✅ 同步完成          │
```

### CLI 交互设计

**Sender 模式**（`peri sync sender`）：

```
$ peri sync sender [--server <relay-url>]

Requesting pair code...
Your pair code: 482917
Waiting for receiver...

Receiver connected!

Sync items requested:
  ✓ Settings (settings.json)
  ✓ Skills (3 files)
  ✓ MCP Config (~/.mcp.json)
  ✓ Plugins (2 plugins)

Packing and encrypting...
Sending: ████████████████░░░ 87%

✅ Transfer complete!
```

**Receiver 模式**（`peri sync receiver`）：

```
$ peri sync receiver [--server <relay-url>]
Enter pair code: 482917

Connected! Select items to sync:

  [x] Settings (settings.json)
  [x] Skills (3 files in ~/.claude/skills/)
  [ ] MCP Config (~/.mcp.json)
  [x] Plugins (2 plugins)

  ↑↓ Navigate  Space Toggle  Enter Confirm

Ready to sync 3 items. Confirm? [y/N]: y

Receiving data... ████████████████░░░ 87%
Decrypting... done
Writing files... done

✅ Synced: settings.json, 3 skills, 2 plugins
```

**main.rs 子命令定义**：

```rust
#[derive(Subcommand)]
enum Commands {
    // ... existing Acp, Update ...
    /// 配置同步：在设备间同步 settings/skills/mcp/plugins
    Sync {
        #[command(subcommand)]
        action: SyncAction,
        /// Relay server URL
        #[arg(long, default_value = "ws://localhost:8080")]
        server: String,
    },
}

#[derive(Subcommand)]
enum SyncAction {
    /// 发送本地配置到远端设备
    Sender,
    /// 从远端设备接收配置
    Receiver,
}
```

### 项目结构

**Relay Server**（`side-projects/peri-sync/server/`，Hono.js）：

```
side-projects/peri-sync/server/
├── package.json
├── src/
│   ├── index.ts          # 入口，Hono app
│   ├── pair-manager.ts   # 配对码生成、校验、过期清理
│   ├── relay.ts          # WS 连接管理 + 消息转发
│   └── types.ts          # 消息类型定义
└── tsconfig.json
```

**同步客户端**（`peri-tui/src/sync/`，Rust）：

```
peri-tui/src/sync/
├── mod.rs            # 模块入口，pub async fn run_sync_sender/receiver
├── protocol.rs       # WS 消息协议类型（WsMessage 等）
├── crypto.rs         # PBKDF2 密钥派生 + AES-256-GCM 加解密
├── packer.rs         # SyncPackage 序列化/反序列化（rmp-serde）
├── scanner.rs        # 扫描本地配置文件（settings/skills/mcp/plugins）
├── writer.rs         # 写入同步文件 + 路径穿越防护
├── sender.rs         # sender 模式：请求配对码 → 等待 → 打包加密 → 发送
├── receiver.rs       # receiver 模式：输入码 → 选择项 → 接收解密 → 写入
└── ui.rs             # CLI 交互（crossterm：选择列表、进度条、确认提示）
```

### 配对码管理

- 格式：6 位随机数字（100000-999999）
- 有效期：5 分钟
- 一次性使用：配对成功后自动失效
- 清理：定时器每 60 秒清理过期配对码
- 存储：内存 Map，无需持久化

## 实现要点

### 关键技术决策

1. **Relay Server（Hono.js）**：轻量、支持多运行时（Node.js / Bun / Deno），WebSocket 原生支持
2. **同步客户端（Rust）**：集成到 peri-tui，复用 clap 子命令体系、crossterm 交互、tokio 异步运行时
3. **MessagePack（rmp-serde）**：比 JSON 更紧凑的二进制序列化，适合传输文件内容
4. **PBKDF2 + AES-256-GCM**：Rust aes-gcm + ring/hmac crate 实现
5. **64KB 分片**：避免 WebSocket 大帧导致的内存压力和超时

### 难点

1. **大文件传输**：skills 目录可能较大，需分片 + 进度条 + 断点处理
2. **文件路径安全**：解包时必须校验路径无 `..` 穿越，防止恶意包覆盖系统文件
3. **并发配对码冲突**：6 位数字空间约 90 万，低并发场景足够，高并发需扩位或加前缀

### 依赖

**Relay Server（Node.js）**：
- `hono` — HTTP 框架 + WebSocket

**同步客户端（Rust，新增到 peri-tui/Cargo.toml）**：
- `tokio-tungstenite` — WebSocket 客户端（已有 tokio）
- `aes-gcm` — AES-256-GCM 加解密
- `ring` — PBKDF2 密钥派生（或 `hmac` + `sha2`）
- `rmp-serde` — MessagePack 序列化
- `crossterm` — CLI 交互（已有，TUI 依赖）

## 约束一致性

客户端集成到 peri-tui，需遵守主项目约束：

- **编码规范**：Rust 2021 edition，tokio async/await，库用 thiserror，日志用 tracing
- **模块结构**：每模块一目录（`sync/` 下扁平文件即可），mod.rs 入口
- **测试**：与源码同目录 `_test.rs` 文件
- **禁止 println!/eprintln!**：使用 tracing 宏
- **字符串截断**：字符级操作（本项目涉及路径处理需注意）

Relay Server 为独立 side-project，不修改主项目核心逻辑。新增的 `sync` 模块仅在子命令触发时使用，不影响 TUI 主循环。

## 验收标准

- [ ] Relay Server 可启动，支持 sender 申请配对码和 receiver 加入配对
- [ ] 配对码 6 位数字，5 分钟过期，一次性使用
- [ ] `peri sync sender` 子命令可用，请求配对码并等待 receiver
- [ ] `peri sync receiver` 子命令可用，输入配对码并交互式选择同步项
- [ ] Sender 可打包 settings.json + skills + .mcp.json + plugins 为 SyncPackage
- [ ] 数据传输端到端加密（AES-256-GCM），relay 无法解密
- [ ] Receiver 显示进度条，同步完成后文件正确写入目标路径
- [ ] 路径穿越防护：解包时拒绝 `..` 和绝对路径
