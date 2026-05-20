### Task 5: Sender/Receiver 模块 + UI + CLI 集成

**背景:**
本 Task 实现配置同步的完整客户端流程——sender 端发起同步、receiver 端接收同步，包括 CLI 交互界面和 main.rs 子命令接入。依赖 Task 2（协议类型 + 加密）、Task 3（扫描 + 打包）、Task 4（写入 + 路径校验）的全部输出。`peri sync sender` 和 `peri sync receiver` 两个子命令使用 crossterm 进行标准终端 CLI 交互（非 TUI 模式），用户可按需选择同步项并查看进度。

**涉及文件:**
- 新建: `peri-tui/src/sync/sender.rs`
- 新建: `peri-tui/src/sync/receiver.rs`
- 新建: `peri-tui/src/sync/ui.rs`
- 修改: `peri-tui/src/sync/mod.rs`
- 修改: `peri-tui/src/main.rs`

**执行步骤:**

- [x] 新建 `peri-tui/src/sync/ui.rs` — CLI 交互界面组件
  - 位置: 新建文件
  - 原因: sender/receiver 模式共用交互组件
  - 内容:
    ```rust
    //! CLI 交互界面：勾选列表、进度条、确认提示
    //! 使用 ratatui 重导出的 crossterm（项目已有，无需新增 Cargo.toml 依赖）
    
    use anyhow::Result;
    use ratatui::crossterm::{
        cursor,
        event::{self, Event, KeyCode, KeyEventKind},
        execute,
        style::{Color, Print, SetForegroundColor, ResetColor},
        terminal::{self, Clear, ClearType},
    };
    use std::io::{self, Write};
    
    use crate::sync::protocol::SyncItems;
    use crate::sync::scanner::SyncItemManifest;
    
    /// 单条同步项目（用于勾选列表展示）
    pub struct SelectableItem {
        pub key: &'static str,
        pub label: &'static str,
        pub detail: String,
        pub selected: bool,
    }
    
    /// 构建勾选列表（从扫描结果生成）
    pub fn build_selectable_items(manifest: &SyncItemManifest) -> Vec<SelectableItem> {
        // 根据 manifest 中有数据的项构建列表，默认全选
        vec![...]
    }
    
    /// 交互式勾选列表。↑↓ 导航，Space 切换，Enter 确认。
    /// 返回用户选择后的 SyncItems。
    pub fn select_sync_items(items: &mut [SelectableItem]) -> Result<SyncItems> {
        // 启用 raw mode → 渲染列表 → 事件循环 → 恢复终端
        // 使用 execute!(stdout, Print(...), ...) 绘制
        // Space: 切换 selected
        // Enter: 确认并返回
        // ↑/↓: 移动光标
    }
    
    /// 确认提示：显示同步项摘要，等待用户输入 y/N
    pub fn confirm_sync(items: &SyncItems) -> Result<bool> {
        // 列出即将同步的项目，读取 stdin 一行，返回 y/Y 为 true
    }
    
    /// 进度条：接收 total（总字节/总块数）和进度回调
    pub struct ProgressBar {
        total: u64,
        label: &'static str,
    }
    
    impl ProgressBar {
        pub fn new(total: u64, label: &'static str) -> Self { ... }
        /// 更新进度并渲染一行 `Label: [████████░░░░] XX%`
        pub fn update(&self, current: u64) { ... }
        /// 渲染完成行（\\r + clear line）
        pub fn finish(&self) { ... }
    }
    
    /// 清空当前行并输出消息（带 \\r 覆盖进度条）
    pub fn println_overwrite(s: &str) {
        // execute!(stdout, Clear(ClearType::CurrentLine), Print(s), Print("\n"))
    }
    ```
  - 勾选列表渲染逻辑：遍历 items，每行渲染 `[x]` 或 `[ ]` + label + detail。高亮当前行用 `SetForegroundColor(Color::Cyan)`。
  - 事件循环中用 `event::read()` 阻塞读取，仅处理 `KeyEventKind::Press`。
  - 退出 raw mode 前恢复光标显示：`execute!(stdout, cursor::Show)`。
  - 所有 stdout 操作通过 `io::stdout()` 获取 `Stdout` 实例。

- [x] 新建 `peri-tui/src/sync/sender.rs` — Sender 模式流程
  - 位置: 新建文件
  - 原因: sender 端完整同步流程
  - 内容:
    ```rust
    //! Sender 模式：请求配对码 → 等待 receiver → 打包加密 → 发送
    
    use anyhow::{Context, Result};
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::connect_async;
    use tokio_tungstenite::tungstenite::Message;
    use tracing::{info, warn};
    
    use crate::sync::protocol::WsMessage;
    use crate::sync::crypto::derive_key;
    use crate::sync::scanner::{scan_all};
    use crate::sync::packer::{pack_and_chunk};
    use crate::sync::ui::{ProgressBar, println_overwrite};
    
    /// 执行 sender 模式完整流程
    pub async fn run_sync_sender(server_url: &str) -> Result<()> {
        // 1. 连接 WebSocket
        let (mut ws, _) = connect_async(server_url).await
            .context("Failed to connect to relay server")?;
    
        // 2. 发送 request_pair
        let msg = serde_json::to_string(&WsMessage::RequestPair)?;
        ws.send(Message::Text(msg.into())).await?;
    
        // 3. 等待 pair_created → 提取配对码并显示
        let pair_code = loop {
            match ws.next().await {
                Some(Ok(Message::Text(text))) => {
                    let msg: WsMessage = serde_json::from_str(&text)?;
                    if let WsMessage::PairCreated { pair_code } = msg {
                        println_overwrite(&format!("配对码: {pair_code}"));
                        println_overwrite("等待 receiver 加入...");
                        break pair_code;
                    }
                    if matches!(msg, WsMessage::Error { .. }) {
                        anyhow::bail!("配对失败: {text}");
                    }
                }
                Some(Err(e)) => anyhow::bail!("WebSocket 错误: {e}"),
                None => anyhow::bail!("连接关闭"),
            }
        };
    
        // 4. 派生加密密钥（用配对码作为种子）
        let key = derive_key(&pair_code);
    
        // 5. 等待 pair_joined
        loop {
            match ws.next().await {
                Some(Ok(Message::Text(text))) => {
                    let msg: WsMessage = serde_json::from_str(&text)?;
                    if matches!(msg, WsMessage::PairJoined { .. }) {
                        println_overwrite("Receiver 已连接！");
                        break;
                    }
                    if matches!(msg, WsMessage::Error { .. }) {
                        anyhow::bail!("配对失败: {text}");
                    }
                }
                Some(Err(e)) => anyhow::bail!("WebSocket 错误: {e}"),
                None => anyhow::bail!("连接关闭"),
            }
        }
    
        // 6. 等待 sync_config（receiver 的选择）
        let sync_items = loop {
            match ws.next().await {
                Some(Ok(Message::Text(text))) => {
                    let msg: WsMessage = serde_json::from_str(&text)?;
                    if let WsMessage::SyncConfig { items } = msg {
                        break items;
                    }
                }
                Some(Err(e)) => anyhow::bail!("WebSocket 错误: {e}"),
                None => anyhow::bail!("连接关闭"),
            }
        };
    
        // 7. 扫描本地配置 + 打包加密 + 分片
        info!("扫描并打包配置...");
        let manifest = scan_all()?;
        let chunks = pack_and_chunk(&manifest, &sync_items, &key)?;
        let total = chunks.len() as u64;
    
        println_overwrite(&format!("传输 {} 个分片...", total));
        let pb = ProgressBar::new(total, "发送");
    
        // 8. 逐片发送 data_chunk
        for (i, chunk) in chunks.iter().enumerate() {
            let msg = WsMessage::DataChunk {
                seq: i as u32,
                data: chunk.clone(),
            };
            ws.send(Message::Text(serde_json::to_string(&msg)?.into())).await?;
            pb.update(i as u64 + 1);
            
            // 等待 ACK（relay 转发回执或直接等待下一个消息——根据 relay 实现而定）
            // 简单实现：不等 ACK，连续发送
        }
    
        // 9. 发送 transfer_complete
        let checksum = calculate_checksum(&chunks);
        let msg = WsMessage::TransferComplete { checksum };
        ws.send(Message::Text(serde_json::to_string(&msg)?.into())).await?;
    
        pb.finish();
        println_overwrite("传输完成！");
        ws.close(None).await?;
        Ok(())
    }
    
    /// 计算分片数据的 SHA-256 校验和
    fn calculate_checksum(chunks: &[Vec<u8>]) -> String {
        use sha2::{Sha256, Digest};
        let mut hasher = Sha256::new();
        for chunk in chunks {
            hasher.update(chunk);
        }
        hex::encode(hasher.finalize())
    }
    ```
  - WebSocket URL 格式：`ws://host:port` 或 `wss://host:port`，由 CLI 参数传入。
  - 在等待 pair_joined 和 sync_config 的循环中，忽略非目标类型的消息（如重复的 PairCreated）。
  - `calculate_checksum` 使用 `sha2` + `hex` crate（`sha2` 已在 workspace 中作为 `ring` 的替代或其他依赖，需确认；若不可用则用 `ring::digest`）。

- [x] 新建 `peri-tui/src/sync/receiver.rs` — Receiver 模式流程
  - 位置: 新建文件
  - 原因: receiver 端完整同步流程
  - 内容:
    ```rust
    //! Receiver 模式：输入配对码 → 选择同步项 → 接收解密 → 写入
    
    use anyhow::{Context, Result};
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::connect_async;
    use tokio_tungstenite::tungstenite::Message;
    use tracing::{info, warn};
    
    use crate::sync::protocol::WsMessage;
    use crate::sync::crypto::derive_key;
    use crate::sync::packer::unpack_from_chunks;
    use crate::sync::writer::write_sync_items;
    use crate::sync::ui::{select_sync_items, build_selectable_items, confirm_sync, ProgressBar, println_overwrite};
    
    /// 执行 receiver 模式完整流程
    pub async fn run_sync_receiver(server_url: &str) -> Result<()> {
        // 1. 读取配对码
        print!("请输入配对码: ");
        std::io::Write::flush(&mut std::io::stdout())?;
        let mut pair_code = String::new();
        std::io::stdin().read_line(&mut pair_code)?;
        let pair_code = pair_code.trim().to_string();
        if pair_code.is_empty() {
            anyhow::bail!("配对码不能为空");
        }
    
        // 2. 连接 WebSocket
        let (mut ws, _) = connect_async(server_url).await
            .context("Failed to connect to relay server")?;
    
        // 3. 发送 join_pair
        let msg = WsMessage::JoinPair {
            pair_code: pair_code.clone(),
        };
        ws.send(Message::Text(serde_json::to_string(&msg)?.into())).await?;
    
        // 4. 等待 pair_joined
        loop {
            match ws.next().await {
                Some(Ok(Message::Text(text))) => {
                    let msg: WsMessage = serde_json::from_str(&text)?;
                    if matches!(msg, WsMessage::PairJoined { .. }) {
                        println_overwrite("已连接！选择要同步的项目：\n");
                        break;
                    }
                    if matches!(msg, WsMessage::Error { .. }) {
                        anyhow::bail!("加入失败: {text}");
                    }
                }
                Some(Err(e)) => anyhow::bail!("WebSocket 错误: {e}"),
                None => anyhow::bail!("连接关闭"),
            }
        }
    
        // 5. 展示勾选列表并获取用户选择
        // 构造预设列表（receiver 端不知道 sender 具体有哪些内容，预设四项）
        let mut items = vec![
            SelectableItem { key: "settings", label: "Settings", detail: "settings.json".into(), selected: true },
            SelectableItem { key: "skills", label: "Skills", detail: "~/.claude/skills/".into(), selected: true },
            SelectableItem { key: "mcp", label: "MCP Config", detail: "~/.mcp.json".into(), selected: true },
            SelectableItem { key: "plugins", label: "Plugins", detail: "~/.claude/plugins/".into(), selected: true },
        ];
        let selected = select_sync_items(&mut items)?;
    
        // 6. 确认同步
        if !confirm_sync(&selected)? {
            println_overwrite("已取消同步");
            return Ok(());
        }
    
        // 7. 发送 sync_config
        let msg = WsMessage::SyncConfig { items: selected };
        ws.send(Message::Text(serde_json::to_string(&msg)?.into())).await?;
    
        // 8. 接收 data_chunk + transfer_complete
        let mut chunks: Vec<(u32, Vec<u8>)> = Vec::new();
        loop {
            match ws.next().await {
                Some(Ok(Message::Text(text))) => {
                    let msg: WsMessage = serde_json::from_str(&text)?;
                    match msg {
                        WsMessage::DataChunk { seq, data } => {
                            chunks.push((seq, data));
                            // 显示进度（基于已接收块数，总数未知用简单计数）
                        }
                        WsMessage::TransferComplete { checksum: _ } => {
                            println_overwrite("接收完成，正在解密...");
                            break;
                        }
                        WsMessage::Error { .. } => {
                            anyhow::bail!("传输错误: {text}");
                        }
                        _ => {} // 忽略其他消息
                    }
                }
                Some(Err(e)) => anyhow::bail!("WebSocket 错误: {e}"),
                None => anyhow::bail!("连接关闭"),
            }
        }
    
        // 9. 合并分片、解密、解包
        chunks.sort_by_key(|(seq, _)| *seq);
        let data: Vec<Vec<u8>> = chunks.into_iter().map(|(_, d)| d).collect();
    
        let key = derive_key(&pair_code);
        let package = unpack_from_chunks(&data, &key)?;
    
        // 10. 写入文件
        println_overwrite("写入文件...");
        write_sync_items(&package)?;
    
        println_overwrite("同步完成！");
        ws.close(None).await?;
        Ok(())
    }
    ```
  - 配对码输入使用标准 `std::io::stdin().read_line()`，不启用 raw mode。
  - 勾选列表在 receiver 端构造预设四项（settings/skills/mcp/plugins），具体详情（如文件数）等 sender 回传 sync_config 后展示——但由于 receiver 在加入后先选再发，所以使用预设描述。
  - 分片接收按 seq 排序后合并，确保乱序到达时正确重组。

- [x] 修改 `peri-tui/src/sync/mod.rs` — 更新模块声明 + 添加入口函数
  - 位置: `peri-tui/src/sync/mod.rs`（文件应在 Task 2 中已创建并声明了 protocol/crypto；Task 3 新增 scanner/packer；Task 4 新增 writer）
  - 追加模块声明和入口函数:
    ```rust
    // 追加到文件末尾（在其他 pub mod 声明之后）
    pub mod ui;
    pub mod sender;
    pub mod receiver;
    
    pub use sender::run_sync_sender;
    pub use receiver::run_sync_receiver;
    ```
  - 原因: 统一入口，main.rs 通过 `peri_tui::sync::run_sync_*` 调用。

- [x] 修改 `peri-tui/src/main.rs` — 添加 Sync 子命令到 Commands enum
  - 位置: `Commands` enum 定义末尾（`Update,` 之后，第 51 行 `}` 之前）
  - 追加变体:
    ```rust
    /// 配置同步：在设备间同步 settings/skills/mcp/plugins
    Sync {
        #[command(subcommand)]
        action: SyncAction,
        /// Relay server URL
        #[arg(long, default_value = "ws://localhost:8080")]
        server: String,
    },
    ```
  - 在 `Commands` enum 下方（第 51 行 `}` 之后）新增:
    ```rust
    #[derive(Subcommand)]
    enum SyncAction {
        /// 发送本地配置到远端设备
        Sender,
        /// 从远端设备接收配置
        Receiver,
    }
    ```

- [x] 修改 `peri-tui/src/main.rs` — 在 match 分支中新增 Sync 派发
  - 位置: `Some(Commands::Update) => { ... }` 分支之后（第 126 行 `}` 之前）
  - 追加:
    ```rust
    Some(Commands::Sync { action, server }) => {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()?;
        rt.block_on(async {
            match action {
                SyncAction::Sender => {
                    peri_tui::sync::run_sync_sender(&server).await
                }
                SyncAction::Receiver => {
                    peri_tui::sync::run_sync_receiver(&server).await
                }
            }
        })
        .map(|_| println!("同步完成"))
        .map_err(|e| {
            eprintln!("同步失败: {e:#}");
            std::process::exit(1);
        })
    }
    ```
  - 原因: Sync 子命令不在 TUI 模式下运行，创建独立 tokio runtime 执行异步流程。

- [x] 为 sync 模块核心逻辑编写单元测试
  - 测试文件: `peri-tui/src/sync/ui_test.rs`（新建，遵循项目测试分离规范）
  - 测试场景:
    - `test_build_selectable_items_all_selected`: 构造完整 manifest → 四项全选
    - `test_build_selectable_items_partial`: 构造仅含 settings 的 manifest → 仅 settings 被选中
    - `test_confirm_sync_yes`: 模拟输入 "y\n" → 返回 `Ok(true)`
    - `test_confirm_sync_no`: 模拟输入 "n\n" → 返回 `Ok(false)`
    - `test_confirm_sync_default_no`: 模拟输入 "\n"（空行） → 返回 `Ok(false)`
    - `test_progress_bar_output`: 验证 ProgressBar::update 和 finish 的输出格式
  - 运行命令: `cargo test -p peri-tui --lib -- sync::ui_test`
  - 预期: 所有测试通过

**检查步骤:**

- [x] 验证 sender.rs / receiver.rs / ui.rs / mod.rs 四个文件存在且包含预期结构
  - `test -f peri-tui/src/sync/sender.rs && test -f peri-tui/src/sync/receiver.rs && test -f peri-tui/src/sync/ui.rs`
  - 预期: 三个文件均存在
  - `grep -c "pub async fn run_sync_sender" peri-tui/src/sync/sender.rs && grep -c "pub async fn run_sync_receiver" peri-tui/src/sync/receiver.rs`
  - 预期: 各输出 1

- [x] 验证 main.rs 包含 Sync 子命令定义
  - `grep "Commands::Sync" peri-tui/src/main.rs | wc -l`
  - 预期: ≥ 1

- [x] 编译检查
  - `cargo check -p peri-tui 2>&1 | tail -5`
  - 预期: 无 error，可能有未使用导入的 warning（发送/接收函数仅在 main.rs 调用，lib 层面可能报警告，可接受）

- [x] 运行单元测试
  - `cargo test -p peri-tui --lib -- sync::ui_test`
  - 预期: 全部通过

- [x] 验证 `peri sync --help` 子命令可见
  - `cargo run -p peri-tui -- sync --help 2>&1`
  - 预期: 显示 Sync 子命令的帮助信息，包含 sender 和 receiver 子命令

**认知变更:**
- [x] [CLAUDE.md] `peri sync` 子命令使用标准终端 CLI（crossterm 交互 + println!），不经过 TUI 主循环。`Commands::Sync` 分支在 main.rs 中创建独立 tokio runtime。
- [x] [CLAUDE.md] 同步模块的 sender.rs/receiver.rs 使用 crossterm 通过 ratatui 重导出路径 `ratatui::crossterm::*`（与 main.rs 中的模式一致），不引入独立 crossterm 依赖。

---
