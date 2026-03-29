# Perihelion 开发日志

> **项目周期**：2026-03-20 ~ 2026-03-29（10 天，146 次提交，作者：KonghaYao）

---

## 2026-03-29 · CLI 发布 & UI 打磨（11 commits）

**里程碑：peri CLI 发布、Welcome 组件、渲染优化**

- `9259887` **peri CLI 发布完成**
- `ba37a0a` **升级 peri 分发方式**
- `24e2b03` 加入配置文件 env
- `a54c867` YOLO 为默认状态
- `9259887` peri 更新会自动安装
- `39b32d2` **完成 Welcome 组件**
- `dba3165` **加强渲染线程与正确的行数统计**
- `6879e28` 添加 table 展示能力
- `3328112` **添加 Agent model 字段支持**
- `13cdb6c` 集中修复问题；修复 compact
- `a54c867` 文档归档

---

## 2026-03-28 · 多用户 Relay & CLI & 跨平台构建（21 commits）

**里程碑：Relay 多用户、前端样式重构、跨平台 CI/CD**

### Relay Server
- `993176e` **改动 Relay Server 支持多用户**
- `3b76cb1` **Relay 协议层解耦**
- `ecdcc2a` AskUser 工具变更
- `67c7fe4` 完成 ThreadStore 和 AgentState 合并计划
- `1ee25e3` 交互统一化
- `4c47866` TUI 发送 `#skill-name` 时自动预加载 Skill 全文
- `6277c51` 调整新 Skill 及文档布局

### 前端
- `8f54e67` **样式重构完成**
- `493a5f0` 添加 style 更改

### CLI & 构建
- `1ecd535` **添加 CLI 命令**
- `547d5f5` 更新 Agent 构建发布脚本
- `0f0efc8` **使用 rustls-tls 替代 native-tls，消除 OpenSSL 依赖**
- `e2a0f01` 尝试修复 cross 问题
- `6ab6b4c` 使用 cross-rs/cross-action 修复 Linux 构建
- `8d97e17` 修复 Linux 平台构建
- `d4b0281` 修复 macOS 构建
- `d84dccc` 丰富测试
- `83f97e3` 修复 AI Messages 空白问题
- `7163f9d` 删除 header
- `7736d1e` Merge `feature/20260327`
- `309fc5a` 归档 features

---

## 2026-03-27 · 前端重构 & Relay 完备（22 commits）

**里程碑：前端迁移 Preact、Relay 数据同步、移动端适配**

### 前端重构
- `da0eb1c` **完成前端重构**——迁移到 Preact + Signals 架构
- `3b771c1` 修正 build
- `f76a98c` **完成移动端适配**
- `8ee019d` 文档归档完成

### Relay Server
- `2eea260` **完成 Relay 数据同步功能**
- `916ba81` 完成 Relay 命令传递
- `c92f715` 完成 Relay 样式小修改
- `ba77675` Relay 弹窗修复
- `83c9f0b` 支持 Relay 构建

### 安全与重构
- `5a5dcc9` 消除 DashMap shard lock 跨 `.await` 反模式
- `fc01d09` forward_to_web DashMap 锁跨 await + retain 清理
- `fae010f` WebSocket 安全——消息限制/心跳/超时
- `234e86d` 内存无界增长防护
- `9e6a37d` 测试代码 `panic!` 改为 `unreachable!`
- `80dd9be` 修复 lowercase

### 架构重构
- `c0a2eb7` **M3 — 消除 PrependSystemMiddleware 排序约束**
- `3fc9921` **修复 Arch.md 中架构问题（M1/M2/L3/M4）**
- `e01d3bf` 添加构建优化代码
- `da7c956` 更新 CLAUDE.md
- `5bbeaa3` 更改 review 文档
- `1cc0e1b` Merge `feature/ui-to-preact`
- `ef67918` Merge commit

---

## 2026-03-26 · Relay Server 安全 & 性能优化（24 commits）

**里程碑：Relay Server 安全加固、前端状态管理、性能优化**

### Relay Server
- `1abb717` **添加连接数上限防 DoS**
- `a7a8a82` 修复 Relay Server 四处安全与健壮性问题
- `91f1975` 修复 LangfuseTracer JoinHandle 泄漏
- `c4c8755` Relay Server spawn 错误可观测性补全
- `f19eb0d` 提升 spawn 任务错误可观测性

### 前端 & TUI
- `ea34198` 完成远程控制面板设置
- `bf9c5fb` 完成 Loading 状态传递
- `4117144` 前端修复数据传递
- `76eb766` 修复 Remote Control 和 Status Bar 问题
- `92f8fc5` 完成消息 ID 挂载
- `ad3ab09` 修复 Model 弹窗粘贴问题
- `c3083d8` SubAgent 显示优化
- `3ce1332` 面板性能优化
- `e3d3db5` 完成 Skill preload
- `abeff05` 样式改动完成

### 安全与健壮性
- `234e86d` WebSocket 安全——消息限制/心跳/超时/字段校验
- `234e86d` 内存无界增长防护——AgentState 消息数告警 + Relay history 字节上限
- `1084c99` 修复生产路径三处 unwrap panic 风险
- `e8d66d8` 消除生产路径静默错误与 unwrap panic
- `55b5774` 降低高频路径日志级别
- `91dbb8f` 补全 poll_agent Disconnected 路径清理

### 重构
- `df889d0` 模块化拆分 langfuse 和 AgentEvent
- `b4c44ba` 修复 Langfuse 两处追踪质量问题
- `2ce8d2a` 添加新方向
- `5dd60c5` 更新 claude 依赖

---

## 2026-03-25 · Langfuse 完善 & Web Crawler（20 commits）

**里程碑：Langfuse 完善、Web Crawler、代码质量清零**

### 新功能
- `0658f26` Langfuse 初层接入成功
- `dd84cb6` 完善缓存 Token 上报
- `874f7e0` SubAgent 加入 middleware
- `7f7c758` **添加 Web Crawler 能力**
- `0221347` 修改工具名，改善展示

### Bug 修复
- `d97ce28` 修复系统提示词注入问题
- `2e38641` 修复 SubAgent 未加入系统提示词
- `20b163f` 修复 Langfuse tools 定义传递
- `59385e5` 修复 Langfuse session 问题
- `dbffdc1` 修复地址及参数问题
- `ddd40be` Done/Error 事件后清理残留弹窗状态
- `e7cc9dc` SubAgent tools/disallowedTools 大小写不敏感
- `5399280` 修复 LLM 适配层三处代码质量问题
- `2e51bf2` 修复 headless 测试通知顺序竞态
- `1f9cd87` 补全持久化错误日志

### 重构与质量
- `dd3e8a7` **完成大文件重构**
- `d31ba01` **修复全部 clippy lint 警告，workspace 警告清零**
- `ea6b241` 补全 MockLLM 单元测试（6 个用例）

---

## 2026-03-24 · Langfuse & Markdown & 安全加固（19 commits）

**里程碑：Langfuse 追踪、Markdown 渲染、图片上传、安全修复**

### 新功能
- `ee485c2` **完成 Langfuse 接入**——LLM 调用追踪
- `1aef842` **新增 Markdown 渲染**——消息内容支持 MD 格式
- `47724e7` **新增 Compact 指令**——上下文压缩
- `3a026df` **支持图片上传**——多模态能力
- `3250baa` Relay 样式更新完成
- `5305b80` 样式问题修复
- `1e84561` 文档归档

### 安全与健壮性
- `5c91745` WebSocket HITL 支持全部 4 种决策路径（Edit/Respond）
- `309b954` bash 超时后自动 SIGKILL 清理子进程
- `8da1bd6` `launch_agent` 加入 HITL 白名单，防止绕过审批
- `7eba086` 渲染 channel 改为无界，消除 `try_send` 静默丢弃

### 重构与测试
- `cd55292` 文件系统工具统一路径解析，加入 canonicalize 防路径遍历
- `19305a6` MockLLM 用 AtomicUsize 替换双 Mutex
- `f8edd5c` 修复 Windows 兼容问题
- `32c6b6a` 修复发送数据发两次问题
- `8ba5017` 修复构建问题
- `dcbd9b2` SubAgent 防递归边界测试
- `54406e2` oneshot 发送端 drop 行为测试

---

## 2026-03-23 · TUI 双线程重构（11 commits）

**里程碑：TUI 双线程架构、Headless 测试、Remote Control**

- `845962d` **完成 TUI 双线程重构**——渲染与 Agent 逻辑分离
- `c9c834c` **数据管道重构，统一渲染方式**
- `a176a6c` **完成 Headless Mode**——无终端集成测试能力
- `a0e529b` **Remote Control 完成**——远程控制 WebSocket 通信
- `9565a31` 完成工具调用展示优化
- `de6e59b` 重构模型面板
- `c50738a` TUI bug 修复
- `1b7000b` **HITL 拒绝不终止 Agent**——改为反馈错误继续循环
- `da8f002` 添加信息传递规范
- `ca283ba` 初始化 checklist
- `5948bb9` 新增消息双写一致性 roundtrip 测试

---

## 2026-03-22 · 架构重构（4 commits）

**里程碑：SubAgent、UI 与存储层重构**

- `2638322` **SubAgent 完成**——支持 `launch_agent` 工具委派子任务
- `5e57689` **完成底层存储层设计**——持久化架构确立
- `ab8edf2` **完成 UI 层架构重构**——TUI 渲染层解耦
- `b9aba87` 修复历史数据展示

---

## 2026-03-21 · Agent 配置（2 commits）

- `8110403` **实现 Agent 配置文件读取**——支持 `.claude/agents/` 定义
- `bc2a224` 代码审查修复

---

## 2026-03-20 · 项目启动（12 commits）

**里程碑：首次发版、核心 ReAct 循环、工具系统**

- 🎉 `80c031d` 第一次发版——项目从零启动
- `f54b706` 改名为 ReactAgent，确立 ReAct 循环架构命名
- `b59d77c` 提供 route map，规划项目路线
- `783b301` 修正 Skill 中间件，建立 Skill 加载机制
- `68721f5` 修复 Skill 位置问题
- `0bf472b` **支持工具并发调用**——ReAct 循环核心能力
- `1144498` 修改 OT 行为
- `b6b36f7` **完成断点续跑**——Agent 可从中断处恢复
- `a02c207` **添加 Thinking 模式**——支持 LLM 推理过程展示
- `5b0e3d1` 调整提示词；修复 status bar 颜色
- `fbe24fb` 修复 YOLO 模式下的 HITL 失效
- `e03d88d` **Agent 面板完成**——TUI 基础交互就绪

---

## 统计概览

| 指标 | 数值 |
|------|------|
| 总提交数 | 146 |
| 开发天数 | 10 天 |
| 日均提交 | 14.6 |
| 最多提交日 | 03-26（24 commits） |
| 活跃作者 | KonghaYao |

### 按类型分布

| 类型 | 数量 | 说明 |
|------|------|------|
| `feat` | ~65 | 新功能 |
| `fix` | ~48 | Bug 修复 |
| `refactor` | ~15 | 重构 |
| `test` | ~5 | 测试 |
| `docs` | ~5 | 文档 |
| `build`/`chore` | ~8 | 构建/杂项 |

### 关键里程碑时间线

```
03-20  项目启动 → ReAct 循环 → 工具并发 → 首次发版
03-22  SubAgent → 存储/架构重构
03-23  TUI 双线程 → Headless 测试 → Remote Control
03-24  Langfuse → Markdown 渲染 → 图片上传 → 安全加固
03-25  Web Crawler → clippy 清零 → 大文件重构
03-26  Relay 安全 → 前端状态 → 性能优化
03-27  前端 Preact 迁移 → 移动端适配 → 架构重构
03-28  多用户 Relay → CLI → 跨平台 CI/CD
03-29  peri CLI 发布 → Welcome 组件 → 渲染优化
```
