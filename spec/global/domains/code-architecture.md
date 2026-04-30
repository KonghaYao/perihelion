# 代码架构 领域

## 领域综述

代码架构领域记录影响整体项目结构的重大变更，包括 crate 增删、依赖关系重构等。

核心职责：
- Workspace crate 结构管理
- 废弃功能完整清理
- 依赖关系调整

## 核心流程

### Relay Server 移除流程

```
1. 删除 rust-relay-server crate 目录
2. Workspace Cargo.toml members 移除引用
3. TUI 中清理 20+ 文件的 Relay 集成:
   - 面板（RelayPanel）
   - 命令（/relay）
   - 事件转发（RelayMessage）
   - CLI 参数（--remote-control）
   - 配置类型（RemoteControlConfig）
4. App 结构体从 4 子结构体缩减为 3（去掉 RelayState）
5. 评估 MessageAdded 事件若仅被 Relay 使用则从核心框架移除
6. 旧配置文件中 remote_control 字段无需主动清理，serde 自然忽略
```

## 技术方案总结

| 维度 | 选型 |
|------|------|
| Workspace 结构 | 3 crate → 移除 relay 后维持 3+perihelion-widgets |
| 配置兼容 | serde 忽略旧字段，无需主动清理 |
| 遗留文件 | Dockerfile.relay 保留作历史记录 |

## Feature 附录

### feature_20260427_F001_relay-removal
**摘要:** 完整删除废弃的 Relay Server 远程控制功能及相关代码
**关键决策:**
- 整体删除 rust-relay-server crate（含 server/client feature 及 web 前端）
- 清理 TUI 中 20+ 文件的 Relay 集成
- App 结构体从 4 子结构体缩减为 3（去掉 RelayState）
- 评估 MessageAdded 事件若仅被 Relay 使用则从核心框架一并移除
- 旧配置文件中 remote_control 字段无需主动清理，serde 自然忽略
- workspace 从 4 crate 减为 3 crate
**归档:** [链接](../../archive/feature_20260427_F001_relay-removal/)
**归档日期:** 2026-04-30

---

## 相关 Feature
- → [tui.md](./tui.md) — TUI App 结构体变更
