# Perihelion 开发日志

> 2026-03-20 ~ 03-29 · 10 天 · 146 次提交 · KonghaYao

---

## 03-29 · CLI 发布 & UI 打磨（11 commits）

- `peri` CLI 发布完成，自动更新安装
- Welcome 组件完成
- 渲染线程加强，行数统计修正
- table 展示能力
- Agent model 字段支持
- YOLO 为默认状态，`-a` 启用审批
- compact 修复，文档归档

## 03-28 · 多用户 Relay & CLI & 跨平台构建（21 commits）

Relay 多用户支持，CLI 发布，跨平台 CI/CD。

- Relay Server 支持多用户，协议层解耦
- AskUser 工具完善（批量提问，单选/多选/自定义）
- 添加 CLI 命令（`peri`）
- 跨平台构建：rustls-tls 替代 native-tls（消除 OpenSSL 依赖）
- cross-rs CI/CD 修复 Linux/macOS 构建
- 前端样式重构完成
- TUI 发送 `#skill-name` 时自动预加载 Skill 全文

## 03-27 · 前端重构 & 架构清理（22 commits）

前端迁移 Preact，Relay 数据同步，移动端适配。

- 前端迁移到 Preact + Signals 架构
  - 组件化拆分（Sidebar / Pane / MessageList / HitlDialog / AskUserDialog）
  - `useSignalValue` hook 解决 esm.sh 多版本 auto-tracking 失效
  - 移动端适配，1/2/3 分屏布局
- Relay 数据同步功能完成，命令传递完成
- 架构清理：消除 PrependSystemMiddleware 排序约束，修复多处架构问题
- 安全：forward_to_web DashMap 锁跨 await 修复

## 03-26 · Relay Server 安全 & 性能优化（24 commits）

Relay Server 安全加固，TUI 性能优化。

- Relay 安全：连接数上限防 DoS（Agent 50 / Web 200）、spawn 错误可观测性
- WebSocket 安全：消息限制/心跳/超时/字段校验
- 内存防护：AgentState 消息数告警 + Relay history 字节上限
- 生产路径 unwrap panic 风险清零
- DashMap shard lock 跨 `.await` 反模式消除
- TUI：远程控制面板、加载状态、Skill preload、面板性能优化
- LangfuseTracer JoinHandle 泄漏修复

## 03-25 · Web Crawler & 代码质量（20 commits）

- 新增 Web Crawler 能力
- SubAgent 加入中间件链
- 修复系统提示词注入、SubAgent 未加入系统提示词等问题
- 完成 clippy lint 警告清零
- 大文件拆分重构
- 补全 MockLLM 单元测试

## 03-24 · Langfuse & 多模态 & 安全加固（19 commits）

可观测性、多模态能力落地，安全全面加固。

- 完成 Langfuse 接入——LLM 调用追踪
- 新增 Markdown 渲染——消息支持 MD 格式
- 新增 `/compact` 指令——上下文压缩
- 支持图片上传——多模态能力（Base64 / URL）
- 安全修复：
  - WebSocket HITL 支持 4 种决策（Approve/Edit/Reject/Respond）
  - bash 超时自动 SIGKILL 清理子进程
  - `launch_agent` 加入 HITL 白名单
  - 文件系统工具 canonicalize 防路径遍历
  - 渲染 channel 改为无界，消除静默丢弃

## 03-23 · TUI 双线程重构（11 commits）

渲染与 Agent 逻辑彻底分离，确立双线程架构。

- 完成双线程重构——渲染线程与 Agent 线程独立，mpsc 通道通信
- 完成 Headless 模式——无终端集成测试（TestBackend）
- 远程控制 WebSocket 通信雏形完成
- HITL 拒绝改为反馈错误继续循环（不再终止 Agent）
- 工具调用展示优化、模型面板重构

## 03-22 · 架构重构（4 commits）

- SubAgent 完成——`launch_agent` 工具委派子任务（防递归）
- 完成底层存储层设计——持久化架构确立
- 完成 UI 层架构重构——TUI 渲染层解耦

## 03-21 · Agent 配置（2 commits）

- 支持 `.claude/agents/` 目录定义专用子 Agent 配置

## 03-20 · 项目启动（12 commits）

从零搭建 Rust Agent 框架，首日完成核心 ReAct 循环。

- 项目从零启动，确立 ReAct 循环架构命名（ReactAgent）
- 实现工具并发调用——ReAct 循环核心能力
- 建立技能加载机制（skill 中间件）
- 完成断点续跑——Agent 可从中断处恢复
- 添加思考模式——展示 LLM 推理过程
- TUI 基础交互就绪（Agent 面板、状态栏）

---

## 关键指标

| 指标 | 数值 |
|------|------|
| 开发天数 | 10 天 |
| 总提交数 | 146（日均 14.6） |
| 最多提交日 | 03-26（24 commits） |
| Workspace Crates | 4 个 |

## 架构全景（最终态）

```
rust-create-agent        核心框架（ReAct 循环、Middleware trait、LLM 适配、工具系统）
    ↑
rust-agent-middlewares   中间件实现（文件系统、终端、Skills、HITL、SubAgent、AskUser）
    ↑
rust-agent-tui           TUI 交互应用（双线程、Headless 测试、Remote Control）
    ↑
rust-relay-server        WebSocket 中继服务（多用户、前端静态页面）
```
