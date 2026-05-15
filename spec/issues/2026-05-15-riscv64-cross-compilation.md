# RISC-V (riscv64gc-unknown-linux-gnu) 交叉编译支持

**状态**：Open
**优先级**：中
**创建日期**：2026-05-15

## 问题描述

Release workflow（`.github/workflows/release-agent.yml`）目前构建 5 个目标（linux x86_64/aarch64、macOS x86_64/aarch64、windows x86_64），缺少 RISC-V 支持。新增 `riscv64gc-unknown-linux-gnu` 目标受 `aws-lc-sys` 依赖阻塞——AWS-LC（Amazon BoringSSL 分支）的手写汇编不支持 RISC-V 架构，CMake 构建直接失败。

## 症状详情

### 当前 Release Workflow 覆盖

| 目标 | 架构 | 编译器 |
|------|------|--------|
| x86_64-unknown-linux-gnu | linux-x86_64 | cross |
| aarch64-unknown-linux-gnu | linux-aarch64 | cross |
| x86_64-apple-darwin | macos-x86_64 | cargo |
| aarch64-apple-darwin | macos-aarch64 | cargo |
| x86_64-pc-windows-msvc | windows-x86_64 | cargo |

需要新增：`riscv64gc-unknown-linux-gnu` → `linux-riscv64`。

### 阻塞点：`aws-lc-sys` 依赖链

#### 传导路径（2 条独立入口）

| # | 入口 | 链 | 影响 Crate |
|---|------|-----|-----------|
| 1 | workspace `reqwest` (Cargo.toml:39-42)<br>`features = ["rustls"]` | `reqwest` 0.13 → `rustls` 0.23 → **`aws-lc-rs`** 1.16 → **`aws-lc-sys`** 0.39 | `rust-create-agent`<br>`rust-agent-middlewares`<br>`rust-agent-tui`<br>`langfuse-client` |
| 2 | rmcp `reqwest` feature<br>(rust-mcp-patch/Cargo.toml:90-93)<br>`"reqwest?/rustls"` | rmcp → `reqwest` 0.13 → `rustls` 0.23 → **`aws-lc-rs`** → **`aws-lc-sys`** | `rust-agent-middlewares`（通过 `transport-streamable-http-client-reqwest`） |

**根因**：`rustls` 0.23 起默认加密后端改为 `aws-lc-rs`（替代 `ring`），`aws-lc-sys` 的 `build.rs` 调用 CMake 编译 C/手写汇编，其构建系统不识别 `riscv64gc` 目标。

#### Cargo.lock 确证

```
rustls 0.23.x → default-features = ["aws-lc-rs", "std"]
              → aws-lc-rs 1.16.2 → aws-lc-sys 0.39.1  (CMake + 手写汇编)

rustls-webpki 0.103.10 → aws-lc-rs + ring 0.17.14  (同时依赖两个后端)
```

### 代码层面审计结果

✅ 无风险——项目无内联汇编、无 SIMD、无 FFI/C 依赖、无 `#[cfg(target_arch)]`。所有条件编译都是 OS 级，与 CPU 架构无关。

其他依赖风险评估：

| 依赖 | 使用的 Crate | 风险 |
|------|-------------|------|
| `libsqlite3-sys` (sqlx sqlite) | `rust-create-agent` | 低——纯 C89 sqlite3 源码编译，RISC-V 交叉工具链即可 |
| `crossterm` / `ratatui` | `rust-agent-tui`、`perihelion-widgets` | 低——纯 Rust + POSIX，与架构无关 |
| `sysinfo` / `num_cpus` | 多 crate | 低——读取 `/proc`/`/sys`，架构无关 |

## 期望改进方向

使 `cross build --target riscv64gc-unknown-linux-gnu` 成功编译，并加入 `release-agent.yml` 的构建矩阵。

### 修复方案

**思路**：强制 `rustls` 使用 `ring` 后端代替 `aws-lc-rs`。`ring` 0.17 已有初步 RISC-V 支持。

#### Step 1：workspace Cargo.toml

```toml
# 将 reqwest 的 rustls feature 改为 rustls-no-provider
reqwest = { version = "0.13", default-features = false, features = [
    "json",
    "rustls-no-provider",  # 不激活 aws-lc-rs
] }

# 显式引入 rustls，指定 ring 后端
rustls = { version = "0.23", default-features = false, features = [
    "ring",
    "std",
] }
```

#### Step 2：rust-mcp-patch/Cargo.toml

将 L92 的 `"reqwest?/rustls"` 改为 `"reqwest?/rustls-no-provider"`，防止 rmcp 的 `reqwest` feature 重新激活 `aws-lc-rs`。

```toml
reqwest = [
    "__reqwest",
    "reqwest?/rustls-no-provider",  # 对齐 workspace
]
```

#### Step 3：release-agent.yml

在构建矩阵中追加 RISC-V 条目：

```yaml
- os: ubuntu-latest
  target: riscv64gc-unknown-linux-gnu
  platform: linux-riscv64
  ext: ""
```

### 验证方式

1. 本地：`cross build -p rust-agent-tui --release --target riscv64gc-unknown-linux-gnu`（需先确认 `aws-lc-sys` 已消除）
2. CI 测试：手动触发 `.github/workflows/test-riscv.yml`（保留此工作流，方便日常验证 RISC-V 构建状态）
3. Release：tag `agent-v*` 自动触发全矩阵构建（含 RISC-V）

## 涉及文件

| 文件 | 变更内容 |
|------|---------|
| `Cargo.toml` | `reqwest` features 改为 `rustls-no-provider`；新增 `rustls` workspace dep（ring 后端） |
| `rust-mcp-patch/Cargo.toml` | `reqwest` feature 的转发 target 改为 `rustls-no-provider` |
| `.github/workflows/release-agent.yml` | 构建矩阵新增 `riscv64gc-unknown-linux-gnu` 条目 |
| `.github/workflows/test-riscv.yml` | 已存在，保留用于手动触发验证 |

## 相关 Issue

无。
