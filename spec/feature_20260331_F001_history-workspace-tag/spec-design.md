# Feature: 20260331_F001 - 历史面板工作区过滤

## 需求背景

当前 `/history` 面板显示所有工作区的对话记录，混杂在一起。用户在不同项目目录下使用 TUI 时，历史面板会显示其他项目的对话，造成混乱。

## 目标

- 打开历史面板时，默认只显示当前工作目录（cwd）下的对话
- 无需新增数据库字段，利用现有的 `ThreadMeta.cwd` 字段实现过滤
- 改动范围最小化

## 方案设计

### 数据流

```
用户输入 /history
  └─ open_thread_browser()
       └─ store.list_threads()           # 加载所有 thread
       └─ 按 app.cwd 过滤               # 新增：只保留 cwd 匹配的 thread
       └─ ThreadBrowser::new(filtered)   # 传入过滤后的列表
```

### 改动点

#### 1. `rust-agent-tui/src/app/thread_ops.rs`

`open_thread_browser()` 方法增加 cwd 过滤逻辑：

```rust
pub fn open_thread_browser(&mut self) {
    let store = self.thread_store.clone();
    let cwd = self.cwd.clone();  // 当前工作目录
    let threads = tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current()
            .block_on(store.list_threads())
            .unwrap_or_default()
    });
    // 过滤：只保留当前工作区的 thread
    let filtered: Vec<ThreadMeta> = threads
        .into_iter()
        .filter(|t| t.cwd == cwd)
        .collect();
    self.core.thread_browser = Some(ThreadBrowser::new(filtered, self.thread_store.clone()));
}
```

#### 2. Thread 浏览面板标题（可选优化）

`rust-agent-tui/src/ui/main_ui/panels/thread_browser.rs` 标题栏可显示当前工作区路径：

```
📝 选择对话 [/Users/konghayao/project]  ↑↓:移动 Enter:确认 d:删除 Esc:关闭
```

### 不改动的部分

- `ThreadStore` trait — 无需新增 `list_threads_by_cwd` 方法
- `SqliteThreadStore` — 无需改 SQL
- `ThreadMeta` — 无需新增字段
- `ThreadBrowser` — 无需改动

## 实现要点

- 过滤逻辑放在 TUI 层（`thread_ops.rs`），不在 Store 层，保持 Store 通用性
- `cwd` 匹配使用精确字符串相等（`==`），不处理 symlink 差异
- 新建对话时 `ThreadMeta::new(cwd)` 已正确设置 cwd，历史数据也已有 cwd 字段

## 约束一致性

- 符合 `spec/global/constraints.md` 中的架构约束
- 符合 `spec/global/architecture.md` 中的 Workspace 依赖关系（改动仅在 rust-agent-tui）
- 无新增约束

## 验收标准

- [ ] 打开 `/history` 只显示当前工作目录的对话
- [ ] 切换到不同目录打开 `/history`，显示该目录的对话
- [ ] 新建对话功能不受影响
- [ ] 删除对话功能不受影响
