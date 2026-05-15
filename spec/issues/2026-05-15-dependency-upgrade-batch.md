# 依赖批量升级

**状态**：Closed
**优先级**：中
**创建日期**：2026-05-15
**完成日期**：2026-05-15

## 问题描述

工作区存在大量依赖版本落后。`cargo update --dry-run` 显示 84 个锁文件包可更新（patch 级），同时 10+ 个直接依赖有 MINOR/MAJOR 版本可升级。部分依赖（sysinfo 落后 5 个 minor、lru 落后 6 个 minor）版本差距较大，积累的技术债可能影响安全补丁接收和后续功能开发。

## 涉及数据

### Patch 级（仅需 `cargo update`，零风险）

| 依赖 | 当前 | 最新 | 说明 |
|------|------|------|------|
| tokio | 1.50.0 | 1.52.3 | 异步运行时 |
| sqlx | 0.8.0 | 0.8.6 | SQLite ORM，6 个补丁 |
| reqwest | 0.13.2 | 0.13.3 | HTTP 客户端 |
| hyper | 1.8.1 | 1.9.0 | HTTP 实现 |
| rustls | 0.23.37 | 0.23.40 | TLS |
| serde_with | 3.18.0 | 3.20.0 | 序列化辅助（传递依赖） |
| uuid | 1.23.0 | 1.23.1 | UUID |
| wasm-bindgen | 0.2.116 | 0.2.121 | WASM 绑定 |
| icu_* 系列 | 2.1.x | 2.2.0 | Unicode 国际化 |
| aws-lc-* | 1.16/0.39 | 1.17/0.41 | 加密库 |

共 84 个包，执行 `cargo update` 即可。

### MINOR/MAJOR（需改 Cargo.toml + 编译验证）

| 依赖 | 当前要求 | 最新 | 差距 | 使用位置 |
|------|---------|------|------|----------|
| **sysinfo** | `0.34` | 0.39.1 | 5 minor | rust-agent-tui |
| **lru** | `0.12` | 0.18.0 | 6 minor | perihelion-lsp |
| **html2text** | `0.14` | 0.17.1 | 3 minor | rust-agent-middlewares |
| **pulldown-cmark** | `0.12` | 0.13.3 | 1 minor | perihelion-widgets |
| **tui-textarea-2** | `0.10` | 0.11.0 | 1 minor | rust-agent-tui |
| **rand** | `0.9` | 0.10.1 | 0.x 不兼容 | rust-create-agent, perihelion-widgets |
| **walkdir** | `2.4` | 2.5.0 | 1 minor | rust-agent-middlewares |
| **fluent** | `0.16` | 0.17.0 | 1 minor | rust-agent-tui |
| **fluent-bundle** | `0.15` | 0.16.0 | 1 minor | rust-agent-tui |
| **rmcp** | `1.6`（本地补丁） | 1.7.0 | 1 minor | rust-agent-middlewares |

## 升级策略建议

### 第一步：安全补丁（立即可做）
```bash
cargo update
```
应用全部 84 个 patch 级升级，变更仅限于 `Cargo.lock`。

### 第二步：低风险 minor（逐个升级）
- `walkdir` 2.4→2.5
- `fluent` 0.16→0.17、`fluent-bundle` 0.15→0.16
- `tui-textarea-2` 0.10→0.11

改 `Cargo.toml` 版本号后 `cargo build && cargo test` 验证。

### 第三步：需功能验证
- `pulldown-cmark` 0.12→0.13 — Markdown 解析行为变化，需检查 widget 渲染是否正常
- `rand` 0.9→0.10 — API 可能不兼容，需检查所有 `rand::` 调用点
- `html2text` 0.14→0.17 — 输出格式和 API 变化

### 第四步：大跨度升级
- `sysinfo` 0.34→0.39 ✅ 零代码修改
- `lru` 0.12→0.18 ✅ 零代码修改

### 阻塞项：rmcp
- 当前通过 `[patch.crates-io]` 指向 `rust-mcp-patch/`（v1.6.0），修复了 Streamable HTTP 200 OK + empty body 的 bug
- 需确认 rmcp 1.7.0 是否已包含此修复，如已包含则可删除 patch 目录直接升

## 涉及文件

| 文件 | 说明 |
|------|------|
| `Cargo.toml` | ✅ workspace 依赖升级：sysinfo 0.34→0.39, lru 0.12→0.18, 移除 rmcp patch |
| `Cargo.lock` | ✅ 84 patch 升级 + 10 direct 升级 |
| `rust-create-agent/Cargo.toml` | ✅ rand 0.9→0.10 |
| `rust-agent-middlewares/Cargo.toml` | ✅ rmcp 1.6→1.7, walkdir 2.4→2.5, html2text 0.14→0.17 |
| `rust-agent-tui/Cargo.toml` | ✅ fluent 0.16→0.17, fluent-bundle 0.15→0.16, tui-textarea-2 0.10→0.11 |
| `perihelion-widgets/Cargo.toml` | ✅ pulldown-cmark 0.12→0.13, rand 0.9→0.10 |
| `perihelion-lsp/Cargo.toml` | lru via workspace |
| `rust-mcp-patch/` | ✅ 已删除 — rmcp 1.7.0 已包含 Streamable HTTP 修复 |

## 执行结果

### 已完成

| 步骤 | 内容 | 变更 | 验证 |
|------|------|------|------|
| Step 1 | `cargo update` | Cargo.lock（84 包） | ✅ 编译 + 测试 1820 pass |
| Step 2 | walkdir 2.4→2.5, fluent 0.16→0.17, fluent-bundle 0.15→0.16, tui-textarea-2 0.10→0.11 | 3 个 Cargo.toml | ✅ 编译 + 测试 1820 pass |
| Step 3 | pulldown-cmark 0.12→0.13, rand 0.9→0.10, html2text 0.14→0.17 | 3 个 Cargo.toml + 2 源码修复 (Rng→RngExt) | ✅ 编译 + 测试 1820 pass |
| rmcp | 1.6 (patch) → 1.7 (crates.io) | Cargo.toml（移除 patch 段）+ 删除 rust-mcp-patch/ | ✅ 编译 + 测试 1820 pass |
| Step 4 | sysinfo 0.34→0.39, lru 0.12→0.18 | workspace Cargo.toml | ✅ 编译 + 测试 1820 pass，零代码修改 |

### 源码修复

- `rand::Rng` → `rand::RngExt`（rand 0.10 breaking change）：`rust-create-agent/src/llm/retry.rs:5`, `perihelion-widgets/src/spinner/verb.rs:1`

### 延期

无。所有依赖全部升级完毕。

### 统计

- Cargo.lock: 依赖变更涉及约 90 个包
- 源码修改: 2 行 import（rand 0.10 breaking change）
- 7 个 Cargo.toml 版本号更新
- 删除 rust-mcp-patch/ 目录（~58,000 行）
- 测试: 1820 passed, 0 failed
- Clippy: 无新增警告
