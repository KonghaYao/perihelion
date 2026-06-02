# Skill 在 Hints 浮层重复显示（一个有 description，一个没有）

**状态**：Open
**优先级**：高
**创建日期**：2026-06-01

## 问题描述

Skill 在 TUI Hints 浮层中重复显示为两个条目：一个有 description（来自本地 `skills` 列表），一个没有 description（来自 ACP `agent_commands` HashSet）。用户看到同一个 skill 名出现两次，且第二个没有描述文字。

## 症状详情

### 重复显示

同一个 skill（如 `caveman`）在 Hints 浮层中出现两次：

| 来源 | 显示名 | Description | 类型 |
|------|--------|-------------|------|
| `CommandSystem.skills` | `caveman` | "Ultra-compressed communication mode..." | HintItem::Skill |
| `CommandSystem.agent_commands` | `caveman` | （空字符串） | HintItem::AgentCmd |

### 根因链路

1. `build_available_commands()` (`peri-acp/src/dispatch/commands.rs:35-40`) 将每个 skill 以 `skill.name`（如 `"caveman"`，**无前缀**）加入 ACP 命令列表
2. `acp_bridge.rs:262-264` 收到 `AvailableCommandsUpdate` 后，提取**所有**命令名（包括 skill 名）存入 `agent_commands` HashSet
3. `update_agent_commands()` (`command_system.rs:35-37`) 无条件 `collect()`，不过滤已存在于 `skills` 列表的条目
4. `hints.rs:44-59` 和 `hint_ops.rs:42-58` 分别从 `skills` 和 `agent_commands` 两个独立列表构建候选项，无去重

### 代码审计

```
[ACP 生成]
  build_available_commands() → 静态命令 + skill.name（如 "caveman"）
    ↓
[TUI 路径]
  acp_server/notify.rs:87 → send_available_commands_update()
  → acp_bridge.rs:254-278 → update_agent_commands(["compact", ..., "caveman"])
  → agent_commands HashSet 含 "caveman"（与 skills 列表重复）
  → hints.rs / hint_ops.rs 分别遍历 skills + agent_commands → 重复显示
```

`hint_ops.rs:60-73` 无去重逻辑：

```rust
for skill in &skill_candidates {
    items.push(HintItem::Skill { name: skill.name.clone() });  // 有 description
}
for name in &agent_cmd_candidates {
    items.push(HintItem::AgentCmd { name: (*name).clone() });  // 无 description
}
// "caveman" 同时出现在两个列表中 → 重复
```

## 根因

`update_agent_commands()` 无条件接收所有 ACP 命令名，不过滤已存在于本地 `skills` 列表的条目。这导致同一个 skill 名同时存在于 `skills`（有 description）和 `agent_commands`（无 description）两个数据源中，Hints 渲染时无去重机制。

## 涉及文件

- `peri-acp/src/dispatch/commands.rs:35-40` —— `build_available_commands()` 将 skill 加入命令列表
- `peri-tui/src/acp_server/notify.rs:79-103` —— TUI 路径发送 AvailableCommandsUpdate
- `peri-tui/src/app/agent_ops/acp_bridge.rs:254-278` —— 解析通知并更新 agent_commands
- `peri-tui/src/app/command_system.rs:34-37` —— `update_agent_commands()` 无过滤
- `peri-tui/src/ui/main_ui/popups/hints.rs:44-77` —— Hints 渲染，两列表无去重
- `peri-tui/src/app/hint_ops.rs:42-73` —— 候选项构建，两列表无去重

## 修复方向

在 `update_agent_commands()` 中过滤已存在于 `skills` 列表的条目：

```rust
pub fn update_agent_commands(&mut self, names: Vec<String>) {
    let skill_names: HashSet<&str> = self.skills.iter().map(|s| s.name.as_str()).collect();
    self.agent_commands = names
        .into_iter()
        .filter(|n| !skill_names.contains(n.as_str()))
        .collect();
}
```

这样 skill 名只出现在 `skills` 列表（有 description），不会同时出现在 `agent_commands`（无 description）中。
