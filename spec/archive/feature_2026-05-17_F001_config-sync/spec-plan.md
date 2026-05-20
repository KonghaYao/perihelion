# 配置同步 执行计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**目标:** 实现跨设备的一键式配置同步（settings/skills/mcp/plugins），端到端加密，Relay 服务端只转发密文

**架构:** Relay Server（Hono.js + WebSocket，无状态密文转发）+ 同步客户端（Rust，集成到 peri-tui 的 `peri sync` 子命令）。配对码派生 AES-256-GCM 密钥，sender 打包加密后通过 relay 透传，receiver 解密写入

**技术栈:** Hono.js（Relay Server）、Rust 2021（同步客户端）：tokio-tungstenite + aes-gcm + ring + rmp-serde + crossterm

**设计文档:** [spec-design.md](./spec-design.md)

---

## 改动总览

本次新增两个独立组件：**Relay Server**（`side-projects/peri-sync/server/`，6 个 TypeScript 文件）和**同步客户端**（`peri-tui/src/sync/`，9 个 Rust 文件 + 5 个测试文件）。同步客户端集成到 `peri-tui` 的 clap 子命令体系中，新增 `Sync` 子命令及 `Sender`/`Receiver` 两个子动作。Task 依赖链：Task 2（协议+加密）是公共基础 → Task 3（扫描+打包）和 Task 4（写入+防护）分别面向 sender 和 receiver → Task 5（sender/receiver/UI/CLI）组装完整流程。Task 1（Relay Server）独立于 Rust 生态系统，可与 Rust Tasks 并行执行。修改现有文件仅 3 处：`peri-tui/src/main.rs`（新增 Sync 子命令）、`peri-tui/src/lib.rs`（新增 sync 模块声明）、`peri-tui/Cargo.toml`（新增 4 个依赖）。

---

## 任务索引

### Task 0: 环境准备
📄 详情见: `spec-plan-task-0.md`

验证 Rust 工具链、Node.js、TypeScript 编译器可用。

### Task 1: Relay Server（Hono.js WebSocket 中继服务）
📄 详情见: `spec-plan-task-1.md`

实现配对码生成/校验 + WebSocket 连接管理 + 密文透传转发。

### Task 2: 协议类型 + 加密模块
📄 详情见: `spec-plan-task-2.md`

定义 WsMessage 枚举 + SyncPackage 数据结构，实现 PBKDF2-SHA256 密钥派生 + AES-256-GCM 加解密。

### Task 3: 配置扫描 + 数据打包
📄 详情见: `spec-plan-task-3.md`

扫描本地 settings/skills/mcp/plugins 配置，构建 SyncPackage → MessagePack 序列化 → 加密 → 64KB 分片。

### Task 4: 文件写入 + 路径穿越防护
📄 详情见: `spec-plan-task-4.md`

三层路径穿越校验（绝对路径/`..`/前缀验证）+ 原子写入配置文件。

### Task 5: Sender/Receiver 模块 + UI + CLI 集成
📄 详情见: `spec-plan-task-5.md`

实现 sender/receiver 完整 WebSocket 通信流程 + 交互式勾选列表 + 进度条 + peri sync 子命令接入。

### Acceptance Task
📄 详情见: `spec-plan-acceptance.md`

端到端验证：全量测试 + peri sync --help + sender/receiver 完整同步流程 + 路径穿越防护。

