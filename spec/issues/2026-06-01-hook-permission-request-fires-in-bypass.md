# PermissionRequest 钩子在 bypass 模式下不应触发

**状态**：Open
**优先级**：中
**创建日期**：2026-06-01

## 问题描述

PermissionRequest 钩子在 YOLO/bypass 权限模式下仍然触发，但 Claude Code 官方行为是仅在权限对话框即将展示给用户时才触发。bypass 模式下不会展示权限对话框，因此不应触发。

## 当前行为

```rust
// middleware.rs:412-414
// 不检查 permission_mode（YOLO/审批）：hook 始终触发以便观察/日志，HITL 弹窗是否显示
// 由 HITL 中间件独立决定。
let is_sensitive = (self.requires_approval)(&tool_call.name);
if is_sensitive {
    // PermissionRequest 始终触发
}
```

无论权限模式是 `Bypass`、`Default` 还是 `DontAsk`，只要工具是敏感工具，PermissionRequest 就会触发。

## 预期行为

| 权限模式 | 是否展示权限对话框 | PermissionRequest 是否触发 |
|---------|-------------------|--------------------------|
| bypass / auto-mode | 否 | **不触发** |
| default（审批模式） | 是 | 触发 |
| dont-ask | 否 | **不触发** |

## 影响范围

用户配置的 PermissionRequest 钩子（如 `herdr-agent-state.sh blocked`）在 YOLO 模式下也会被调用，可能执行不必要的副作用（如状态栏显示为 blocked 但实际上工具直接执行了）。

## 修复方向

1. `HookMiddleware` 持有当前 `permission_mode` 信息（已通过构造函数传入 `self.permission_mode`）
2. 在 `before_tool` 中判断：仅当 `permission_mode != "bypass"` 且工具需要审批时，才触发 PermissionRequest
3. 注意：PreToolUse 仍应始终触发（它独立于权限系统）

## 涉及文件

- `peri-middlewares/src/hooks/middleware.rs` — `before_tool` 中 PermissionRequest 门控逻辑（约 line 407-460）
