# Feature: 20260328_F003 - test-coverage-improvement

## 需求背景

当前项目存在明显测试盲区（详见 `human/TESTING.md`），尤其集中在以下四个高风险区域：

1. **文件系统工具**（6 个工具）：`read_file`、`write_file`、`edit_file`、`glob_files`、`search_files_rg`、`folder_operations` 完全无单元测试，工具逻辑只靠中间件集成测试间接覆盖，边界条件和错误路径几乎空白。
2. **Relay Server**：`auth.rs`（Token 验证）、`client/mod.rs`（历史缓存 + 序列号）无测试，协议层之外的核心逻辑缺乏保障。
3. **ask_user_tool**：`ask_user_question` 工具跨两个 crate，参数解析和 oneshot broker 挂起/恢复流程完全无覆盖。
4. **TUI 命令系统**：`CommandRegistry` 的前缀唯一匹配和 dispatch 逻辑无测试。

整体测试覆盖率估算：工具实现层 ~40%，Relay Server ~20%，其余中高风险模块覆盖度不足。

## 目标

- 为 6 个文件系统工具添加单元测试，覆盖正常路径、边界条件和错误处理
- 为 Relay Server 的 `auth.rs` 和 `client/mod.rs` 补充单元测试
- 为 `ask_user_tool` 补充参数解析和 broker 流程测试
- 为 TUI `CommandRegistry` 补充 dispatch 和前缀匹配逻辑测试
- 整体新增 ~60 个测试用例，将工具实现层覆盖率提升至 ~80%，Relay 提升至 ~50%

![测试覆盖度提升方案总览](./images/01-coverage-overview.png)

## 方案设计

### 测试策略

- **内嵌单元测试为主**：遵循项目规范（`constraints.md`），所有测试写在 `src/` 内的 `#[cfg(test)] mod tests` 块中，不新建 `tests/` 文件
- **tempfile 隔离文件系统**：`tempfile` crate 已在 `rust-agent-middlewares` dev-dependencies 中，文件系统工具测试使用 `TempDir` 隔离，不污染真实文件系统
- **Mock trait 替代外部依赖**：`ask_user_tool` 测试实现 `MockBroker`（`UserInteractionBroker` trait），直接返回预设答案，不依赖 TUI 或 oneshot channel 基础设施
- **test-only 构造器**：`RelayClient` 需要 WebSocket 连接才能构建，为其在 `#[cfg(test)]` 块中添加 `new_for_testing()` 私有构造器，绕过网络连接
- **stub 命令**：TUI 命令测试中注册实现 `Command` trait 的 stub，测试 dispatch 返回值和匹配行为，无需真实 App 状态

### 模块 A：文件系统工具（rust-agent-middlewares）

测试通过 `BaseTool::invoke(serde_json::Value)` 接口调用，cwd 设置为 `TempDir` 路径。

| 工具 | 测试文件 | 关键场景 |
|------|---------|---------|
| `read_file` | `tools/filesystem/read.rs` | 正常读取（带行号输出）、文件不存在返回 Error 消息、offset/limit 分页、二进制扩展名检测 |
| `write_file` | `tools/filesystem/write.rs` | 创建新文件、覆盖已有文件、自动创建多级父目录 |
| `edit_file` | `tools/filesystem/edit.rs` | 精确替换成功、old_string 不存在返回错误、replace_all 批量替换 |
| `glob_files` | `tools/filesystem/glob.rs` | 通配符 `*.rs` 匹配多文件、`**` 递归匹配、无匹配返回空结果 |
| `search_files_rg` | `tools/filesystem/grep.rs` | 关键词搜索命中、无匹配、正则模式、多文件结果 |
| `folder_operations` | `tools/filesystem/folder.rs` | mkdir 创建目录、delete 删除文件、move/rename 重命名 |

**路径穿越测试（公共关注点）**：`file_path: "../../etc/passwd"` 经 `resolve_path` 规范化后应被限制在 cwd 内，已有测试覆盖此逻辑（`test_resolve_path_traversal_canonicalized`），工具层在此基础上验证实际 IO 行为。

### 模块 B：Relay Server（rust-relay-server）

**auth.rs（5 个测试）**：

```
test_validate_token_correct      — Some("abc") vs "abc" → Ok(())
test_validate_token_wrong        — Some("xyz") vs "abc" → Err(UNAUTHORIZED)
test_validate_token_none         — None vs "abc" → Err(UNAUTHORIZED)
test_validate_token_empty_str    — Some("") vs "abc" → Err(UNAUTHORIZED)
test_validate_token_same_content_different_len — timing: "abc" vs "abcd" → Err（长度差异短路可接受）
```

**client/mod.rs（7 个测试）**：

在 `#[cfg(test)]` 块中添加 `RelayClient::new_for_testing()`，使用 `mpsc::unbounded_channel` 构建假 tx，`seq` 从 1 开始，`history` 为空 VecDeque：

```
test_seq_increments            — 连续 send_with_seq 后 seq 从 1 累加
test_history_push              — send_with_seq 后 get_history_since(0) 返回该条
test_get_history_since_filter  — seq=3,4,5 的记录，since(3) 返回 seq=4,5
test_history_cap_at_1000       — push 1001 条，len==1000 且最旧已淘汰
test_clear_history             — clear_history() 后 get_history_since(0) 为空
test_oversized_entry_skipped   — 超过 512KB 的条目不进入 history
test_get_history_since_empty   — 空历史返回空 Vec
```

### 模块 C：ask_user_tool（rust-agent-middlewares）

实现 `MockBroker`：

```rust
struct MockBroker(InteractionResponse);
#[async_trait]
impl UserInteractionBroker for MockBroker {
    async fn request(&self, _ctx: InteractionContext) -> InteractionResponse {
        self.0.clone()
    }
}
```

测试场景（10 个）：

```
test_parse_valid_single_question   — 1 题正常解析为 QuestionItem
test_parse_valid_multi_question    — 3 题批次解析
test_parse_invalid_json            — 非 JSON 输入 → 返回 Err
test_single_question_selected      — 单题，broker 返回 selected=["选项A"] → "选项A"
test_single_question_text_input    — 单题，broker 返回 text="自定义" → "自定义"
test_single_question_text_priority — selected+text 同时存在，text 优先
test_multi_question_format         — 2 题，返回 "[问: H1]\n回答: v1\n\n[问: H2]\n回答: v2"
test_multi_question_selected_join  — 多选 selected=["A","B"] → "A, B"
test_unexpected_response_type      — broker 返回非 Answers → Err
test_empty_selected_returns_empty  — selected=[], text=None → ""
```

### 模块 D：TUI 命令系统（rust-agent-tui）

在 `command/mod.rs` 中添加测试，注册 stub Command：

```rust
struct StubCommand { name: &'static str, called: std::sync::Arc<AtomicBool> }
impl Command for StubCommand {
    fn name(&self) -> &str { self.name }
    fn description(&self) -> &str { "" }
    fn execute(&self, _app: &mut App, _args: &str) { self.called.store(true, Ordering::Relaxed) }
}
```

测试场景（8 个）：

```
test_dispatch_exact_match           — "/model" 匹配 "model" → true，命令被调用
test_dispatch_prefix_unique         — "/mo" 唯一前缀匹配 "model" → true
test_dispatch_prefix_ambiguous      — "/m" 同时匹配 "model"+"mock" → false（未知）
test_dispatch_no_match              — "/unknown" → false
test_dispatch_with_args             — "/model opus" 解析 args="opus" 传给 command
test_match_prefix_returns_all       — match_prefix("m") 返回所有 m 开头命令
test_list_returns_all_registered    — list() 返回所有注册命令
test_dispatch_empty_name            — "/" 空命令名 → false
```

> **注**：`execute` 需要 `&mut App`，测试中使用 `App::new_headless(80, 24).0` 构建最小化 App。

## 实现要点

1. **`RelayClient::new_for_testing()`**：需使用 `Mutex<VecDeque>` 和 `AtomicU64` 直接构建，`tx` 指向一个 `UnboundedSender`（Receiver 可 drop），`send_with_seq` 发送失败静默忽略，不影响 history 写入逻辑
2. **ask_user MockBroker**：`InteractionResponse` 需要 `Clone`，若未实现需在测试内手动构建或用 `Arc<Mutex>` 共享
3. **TUI dispatch 测试**：`App::new_headless(80, 24)` 返回 `(App, HeadlessHandle)`，只需要 `App` 部分；HeadlessHandle 可 drop
4. **edit_file replace_all**：需要查阅 `edit.rs` 确认 `replace_all` 参数名（JSON key），确保测试参数与实现一致
5. **glob_files 递归**：`TempDir` 内需预先创建多级目录结构，`**/*.rs` 模式需确认 glob crate 行为

## 约束一致性

- **测试位置**：遵循 `constraints.md` 规范，测试写在 `src/` 内的 `#[cfg(test)] mod tests`；bin crate（`rust-agent-tui`）命令测试同理
- **异步运行时**：所有 async 测试使用 `#[tokio::test]`，符合 tokio 1.x 规范
- **无新 crate 依赖**：`tempfile` 已在 dev-dependencies，`MockBroker` 在测试块内定义，不引入额外依赖
- **无下层依赖上层**：`RelayClient::new_for_testing()` 只在 `#[cfg(test)]` 块存在，不影响生产代码的 crate 依赖关系

## 验收标准

- [ ] `rust-agent-middlewares` 6 个文件系统工具各有 ≥4 个测试，覆盖正常路径、文件不存在、边界条件
- [ ] `rust-relay-server` `auth.rs` 有 5 个 token 验证测试
- [ ] `rust-relay-server` `client/mod.rs` 有 7 个历史缓存测试（含 1000 条滚动、since 过滤）
- [ ] `rust-agent-middlewares` `ask_user_tool.rs` 有 10 个测试，覆盖参数解析和返回格式
- [ ] `rust-agent-tui` `command/mod.rs` 有 8 个 dispatch/prefix 测试
- [ ] `cargo test` 全量通过，无新增 warning
- [ ] 新增测试总数 ≥ 55 个
