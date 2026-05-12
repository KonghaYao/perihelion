# Design Review Progress

## 2026-05-11 第49轮：LSP 按需初始化
纯 Rust 项目中 typescript-lsp 启动失败导致所有 LSP 操作报错。修复：`initialized` 从 `bool` 改为 `HashSet<String>`，新增 `ensure_server_for_file()` 按文件扩展名只启动匹配服务器，无匹配返回 `NoServerForFile`。28 + 711 测试通过。

## 2026-05-11 第48轮：旧缓存 LSP 插件加载 fallback
修复前已安装的 LSP 插件缓存目录缺少 plugin.json 导致 loader 静默跳过。新增 `try_generate_synthetic_manifest_fallback()` 从 marketplace 缓存生成合成 manifest。711 测试通过。

## 2026-05-11 第47轮：Marketplace LSP 插件安装失败
13 个纯 LSP 插件无 plugin.json 导致安装失败。MarketplacePlugin 添加 `#[serde(flatten)]` 保留 lspServers 字段，安装时从 marketplace 条目生成合成 manifest，TUI 启动时合并全局+插件 LSP 配置。708 测试通过。

## 2026-05-11 第46轮：LSP clippy 警告清除
消除 6 个 clippy 警告：shutdown 中 guard 跨 await、复杂类型提取 type alias、DiagnosticsRegistry 补 Default impl。perihelion-lsp 和 middlewares clippy 零警告。

## 2026-05-11 第45轮：LSP 集成 Code Review 修复
修复 3 个关键 bug：dispatch loop `_rx` 丢弃导致无响应、`RwLock` read→write 死锁、`file://` 双重前缀。修复 6 个 warning。补充 32 个测试，共 1428 测试通过。

## 2026-05-07 第44-43轮：ContentBlock 测试 + re-export 收紧
content.rs 补 11 个测试（image/reasoning/Document roundtrip）。langfuse-client 移除 11 个未使用 re-export。净减 46 行，275 测试通过。

## 2026-05-07 第42-41轮：LLM adapter 测试 + widgets 死代码删除
ChatAnthropic 补 6 个测试，ChatOpenAI 补 4 个测试。widgets 删除 115 行死代码（TableBuilder::render + make_data_line），移除 compact 无用 re-export。

## 2026-05-07 第40-38轮：API 可见性收紧 + 死代码清除
MCP 模块 10 个内部函数从 pub 收紧为 pub(crate)；plugin installer 补 15 个测试；FilesystemThreadStore 补 13 个测试。消除 9 个 clippy 警告。1403 测试通过。

## 2026-05-02 第35-33轮：CI 修复 + 测试补充
修复 `test_subagent_group_basic` CI 失败；langfuse-client/compact 补 3 个测试；widgets 11 个组件补 12 个测试。74 测试通过。

## 2026-05-02 第31-32轮：核心框架 + 中间件审查
合并 executor 重复逻辑，ChatAnthropic 声明 context_window，删除 grep.rs 115 行死代码，StopReason 补 9 个测试。中间件补 14 个测试。504 测试通过。

## 2026-04-30 第14-30轮：核心逻辑审查与 UX 打磨
ContextBudget 事件链路修复、Prompt Caching 稳定缓存边界、SubAgent cancel 令牌、HITL 批量审批、Thread Browser UX（引导提示/删除确认/反馈消息）、面板操作反馈、快捷键合规。

## 2026-04-29 第1-13轮：初始 UX 全面审查
Thread Browser/Login 面板删除功能、Welcome Card 引导、命令栏精简、单字母快捷键合规、系统消息颜色分级、ToolBlock 错误高亮、/compact 防重复触发。
