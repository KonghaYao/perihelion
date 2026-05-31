# @ mention 文件搜索阻塞 UI 线程 + side-projects 搜不到

**状态**：Open
**优先级**：高
**创建日期**：2026-05-31

## 问题描述

输入框输入 `@` 后立刻出现严重卡顿和 CPU 飙升。`search_files()` 在 UI 主线程同步执行 `glob::glob()` 遍历整个项目目录树，阻塞事件循环。同时，输入 `@side` 时搜不到 `side-projects/` 目录。

## 症状详情

| 维度 | 表现 |
|------|------|
| 触发时机 | 输入 `@` 后打第一个字符立刻卡顿 |
| CPU 表现 | CPU 极高 |
| 搜索结果 | `@side` 搜不到 `side-projects/` 目录 |
| 持续时间 | 卡顿持续数秒 |

### 症状 1：UI 线程阻塞

`update_at_mention_detection()` 在键盘事件处理中同步调用 `search_files()`，后者执行 `glob::glob()` + `SkimMatcherV2` 模糊匹配。perihelion 项目中 `side-projects/` 子目录包含大量 `node_modules`（daytona 约 6000+ 文件）和 `target`（git-graph 编译产物），glob 遍历需要 stat 数十万文件。

虽有 300ms debounce（`SEARCH_DEBOUNCE_MS`）和缓存机制，但**第一次搜索**无法跳过，直接在主线程同步阻塞。

### 症状 2：side-projects 搜不到

用户输入 `@side`（query = `side`），glob pattern 为 `{cwd}/**/*side*`。`side-projects` 目录名包含 `side`，在文件系统遍历中应该是前几个匹配项之一。但 glob 遍历 `side-projects/` 内部时产生大量 `node_modules` 中的匹配，`MAX_GLOB_RESULTS = 200` 可能导致遍历停留在 `side-projects/` 子目录深处（遍历子目录是深度优先），导致主循环被 `.take(200)` 截断前只产生了子目录内的匹配，而 `side-projects` 本身虽然在早期被匹配到但 `should_ignore` 过滤后的有效结果数量很少，`side-projects` 可能被 fuzzy score 排名挤出前 15（`MAX_CANDIDATES`）。

## 复现条件

- **复现频率**：必现
- **触发步骤**：
  1. 在输入框输入 `@`
  2. 继续输入任意字符（如 `s` 或 `side`）
  3. 观察到 UI 卡顿，CPU 飙升
  4. 候选列表中 `side-projects/` 不可见

## 涉及文件

- `peri-tui/src/app/at_mention/file_search.rs` — `search_files()` 同步 glob + 模糊匹配
- `peri-tui/src/event/keyboard.rs` — `update_at_mention_detection()` 在主线程调用搜索
- `peri-tui/src/app/at_mention/mod.rs` — `AtMentionState` 状态管理、缓存和节流逻辑
