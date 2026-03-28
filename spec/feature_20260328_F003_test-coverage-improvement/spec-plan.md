# 测试覆盖度提升 执行计划

**目标:** 为文件系统工具、Relay Server、AskUserTool、TUI 命令系统四个高风险区域补充单元测试，新增 ≥55 个测试用例

**技术栈:** Rust 2021, tokio 1.x, tempfile 3.x, serde_json, async-trait

**设计文档:** ./spec-design.md

---

### Task 1: 文件系统工具单元测试

**涉及文件:**
- 修改: `rust-agent-middlewares/src/tools/filesystem/read.rs`
- 修改: `rust-agent-middlewares/src/tools/filesystem/write.rs`
- 修改: `rust-agent-middlewares/src/tools/filesystem/edit.rs`
- 修改: `rust-agent-middlewares/src/tools/filesystem/glob.rs`
- 修改: `rust-agent-middlewares/src/tools/filesystem/grep.rs`
- 修改: `rust-agent-middlewares/src/tools/filesystem/folder.rs`

**执行步骤:**

- [ ] 在 `read.rs` 末尾添加 `#[cfg(test)] mod tests`，包含以下测试：
  - `test_read_file_basic`：TempDir 写入 "hello\nworld"，invoke 返回含行号的输出（`"     1\thello"`）
  - `test_read_file_not_found`：invoke 不存在的路径，返回含 "File not found" 的 Ok 字符串
  - `test_read_file_offset_limit`：写入 5 行，offset=2, limit=2，只返回第 3-4 行
  - `test_read_file_binary_extension`：invoke `test.png` 路径（可不真实存在），直接返回 "BINARY FILE DETECTED"（需在 tempdir 创建 0 字节文件）
  - `test_read_file_absolute_path`：用绝对路径读取 tempdir 内文件
  ```rust
  // 示例结构：
  #[tokio::test]
  async fn test_read_file_basic() {
      let dir = tempfile::tempdir().unwrap();
      let path = dir.path().join("file.txt");
      std::fs::write(&path, "hello\nworld").unwrap();
      let tool = ReadFileTool::new(dir.path().to_str().unwrap());
      let result = tool.invoke(serde_json::json!({"file_path": "file.txt"})).await.unwrap();
      assert!(result.contains("1\thello"));
  }
  ```

- [ ] 在 `write.rs` 末尾添加 `#[cfg(test)] mod tests`，包含：
  - `test_write_file_creates_new`：write "content"，验证磁盘文件内容一致
  - `test_write_file_overwrites_existing`：先写 "old"，再写 "new"，验证内容更新
  - `test_write_file_creates_parent_dirs`：invoke `"sub/dir/file.txt"`，验证父目录自动创建
  - `test_write_file_missing_content_param`：invoke `{"file_path":"f.txt"}` 缺少 content，返回 Err
  - `test_write_file_success_message`：验证返回字符串含 "written successfully"

- [ ] 在 `edit.rs` 末尾添加 `#[cfg(test)] mod tests`，包含：
  - `test_edit_file_single_replace`：文件含唯一 "foo"，替换为 "bar"，验证磁盘内容
  - `test_edit_file_old_string_not_found`：old_string 不存在，返回含 "not found" 的 Ok
  - `test_edit_file_replace_all`：文件含 3 个 "x"，replace_all=true，全部替换为 "y"
  - `test_edit_file_ambiguous`：文件含 2 个 "foo"，replace_all=false，返回含 "not unique" 的 Ok
  - `test_edit_file_not_found`：invoke 不存在的文件，返回含 "File not found" 的 Ok

- [ ] 在 `glob.rs` 末尾添加 `#[cfg(test)] mod tests`，包含：
  - `test_glob_match_simple`：TempDir 建 a.rs/b.rs/c.txt，pattern="*.rs"，返回 2 个文件
  - `test_glob_no_match`：pattern="*.go"，返回 "No files found."
  - `test_glob_recursive`：建 sub/d.rs，pattern="**/*.rs"，能找到
  - `test_glob_dir_not_found`：path 指向不存在目录，返回含 "Directory not found" 的 Ok

- [ ] 在 `grep.rs` 末尾添加 `#[cfg(test)] mod tests`，包含（依赖系统 rg 二进制）：
  - `test_search_files_rg_hit`：TempDir 写文件含 "needle"，args=["-n","needle","./"]，返回含 "needle" 的输出
  - `test_search_files_rg_no_match`：搜索不存在的词，返回 "No matches found."
  - `test_search_files_rg_empty_args`：空 args 数组，返回含 "No arguments" 的 Ok
  - `test_search_files_rg_regex`：args=["-n","need.*","./"]，正则匹配成功

- [ ] 在 `folder.rs` 末尾添加 `#[cfg(test)] mod tests`，包含（仅 create/list/exists）：
  - `test_folder_create`：invoke create 操作，验证目录实际创建
  - `test_folder_create_recursive`：invoke create "a/b/c"（多级），验证目录链创建
  - `test_folder_exists_true`：先创建目录，exists 返回含 "✓ Folder exists" 的 Ok
  - `test_folder_exists_false`：不存在路径，返回含 "✗ Folder does not exist" 的 Ok
  - `test_folder_list`：创建含子文件的目录，list 返回含文件名的输出

**检查步骤:**

- [ ] 文件系统工具测试全部通过
  - `cargo test -p rust-agent-middlewares -- tools::filesystem 2>&1 | tail -20`
  - 预期: 输出含 `test result: ok. N passed; 0 failed`，N ≥ 24

---

### Task 2: Relay Server 单元测试

**涉及文件:**
- 修改: `rust-relay-server/src/auth.rs`
- 修改: `rust-relay-server/src/client/mod.rs`

**执行步骤:**

- [ ] 在 `auth.rs` 末尾添加 `#[cfg(test)] mod tests`，包含：
  - `test_validate_token_correct`：Some("abc") vs "abc" → Ok(())
  - `test_validate_token_wrong`：Some("xyz") vs "abc" → Err(UNAUTHORIZED)
  - `test_validate_token_none`：None vs "abc" → Err(UNAUTHORIZED)
  - `test_validate_token_empty_string`：Some("") vs "abc" → Err(UNAUTHORIZED)
  - `test_validate_token_correct_unicode`：含 unicode 字符的 token，相同时 Ok，不同时 Err

- [ ] 在 `client/mod.rs` 的 `#[cfg(test)] mod tests` 块中添加 `RelayClient::new_for_testing()`：
  ```rust
  #[cfg(test)]
  impl RelayClient {
      fn new_for_testing() -> Self {
          let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
          RelayClient {
              tx,
              session_id: Arc::new(tokio::sync::RwLock::new(None)),
              connected: Arc::new(AtomicBool::new(true)),
              _tasks: vec![],
              seq: Arc::new(AtomicU64::new(1)),
              history: Arc::new(Mutex::new(VecDeque::new())),
          }
      }
  }
  ```
  注意：`_rx` 被 drop，`tx.send()` 会静默失败，但 history 写入不受影响（send 在 history 写入之后）

- [ ] 在 `client/mod.rs` 添加 7 个测试（依赖 `send_with_seq` 写入 history）：
  - `test_history_push_single`：send_with_seq 一条，`get_history_since(0)` 返回该条
  - `test_get_history_since_filter`：push seq=1,2,3，`since(1)` 返回 seq=2,3 两条
  - `test_get_history_since_empty_history`：无数据，`since(0)` 返回空 Vec
  - `test_seq_starts_at_one`：`new_for_testing()` 后首次 send，历史中 seq=1
  - `test_seq_increments`：连续 send 三次，seq 分别为 1,2,3
  - `test_history_cap_at_1000`：循环 push 1001 条，history 长度恰好 1000，最旧已淘汰
  - `test_clear_history`：push 若干条，`clear_history()` 后 `since(0)` 为空

  注意：`send_with_seq` 是私有方法，测试在同模块内可直接调用；验证 history 通过 `get_history_since(0)` 间接读取

**检查步骤:**

- [ ] auth 测试通过
  - `cargo test -p rust-relay-server auth 2>&1 | tail -10`
  - 预期: `test result: ok. 5 passed; 0 failed`

- [ ] client 历史缓存测试通过
  - `cargo test -p rust-relay-server client 2>&1 | tail -10`
  - 预期: `test result: ok. 7 passed; 0 failed`

---

### Task 3: AskUserTool 单元测试

**涉及文件:**
- 修改: `rust-agent-middlewares/src/tools/ask_user_tool.rs`

**执行步骤:**

- [ ] 在 `ask_user_tool.rs` 末尾添加 `#[cfg(test)] mod tests`，先定义 `MockBroker`：
  ```rust
  struct MockBroker(InteractionResponse);
  #[async_trait::async_trait]
  impl UserInteractionBroker for MockBroker {
      async fn request(&self, _ctx: InteractionContext) -> InteractionResponse {
          self.0.clone()
      }
  }

  fn make_answer(selected: &[&str], text: Option<&str>) -> InteractionResponse {
      InteractionResponse::Answers(vec![QuestionAnswer {
          id: "ask_user_question_0".to_string(),
          selected: selected.iter().map(|s| s.to_string()).collect(),
          text: text.map(|s| s.to_string()),
      }])
  }
  ```

- [ ] 添加参数解析测试（`parse_questions` 是私有函数，通过 `invoke` 黑盒测试）：
  - `test_invalid_json_returns_err`：invoke `serde_json::Value::Null`，broker 不会被调用，`invoke` 直接返回 Err
  - `test_missing_questions_key_returns_err`：invoke `json!({})` → Err
  - `test_valid_single_question_parsed`：invoke 格式正确的单问题，broker 返回答案，invoke 返回 Ok

- [ ] 添加单问题返回格式测试：
  - `test_single_question_selected_answer`：selected=["选项A"]，text=None → "选项A"
  - `test_single_question_text_input`：selected=[]，text=Some("自定义输入") → "自定义输入"
  - `test_single_question_text_priority_over_selected`：selected=["选项A"]，text=Some("自定义") → "自定义"（text 优先）
  - `test_single_question_empty_selected`：selected=[]，text=None → ""

- [ ] 添加多问题返回格式测试：
  ```rust
  // 需为 InteractionResponse::Answers 构建多条 QuestionAnswer
  ```
  - `test_multi_question_format`：2 题，返回 "[问: H1]\n回答: v1\n\n[问: H2]\n回答: v2"
  - `test_multi_question_multi_select_join`：selected=["A","B"] → "A, B"

- [ ] 添加异常响应测试：
  - `test_unexpected_response_type`：实现 MockBroker 返回 `InteractionResponse::Decisions(vec![])`，invoke 返回 Err

**检查步骤:**

- [ ] ask_user_tool 测试全部通过
  - `cargo test -p rust-agent-middlewares -- tools::ask_user_tool 2>&1 | tail -10`
  - 预期: `test result: ok. N passed; 0 failed`，N ≥ 10

---

### Task 4: TUI 命令系统单元测试

**涉及文件:**
- 修改: `rust-agent-tui/src/command/mod.rs`

**执行步骤:**

- [ ] 在 `command/mod.rs` 末尾添加 `#[cfg(test)] mod tests`，定义 StubCommand：
  ```rust
  use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
  use std::sync::Arc;

  struct StubCommand {
      n: &'static str,
      called: Arc<AtomicBool>,
      last_args: Arc<parking_lot::Mutex<String>>,
  }
  impl Command for StubCommand {
      fn name(&self) -> &str { self.n }
      fn description(&self) -> &str { "stub" }
      fn execute(&self, _app: &mut crate::app::App, args: &str) {
          self.called.store(true, Ordering::Relaxed);
          *self.last_args.lock() = args.to_string();
      }
  }
  fn make_stub(name: &'static str) -> (StubCommand, Arc<AtomicBool>, Arc<parking_lot::Mutex<String>>) {
      let called = Arc::new(AtomicBool::new(false));
      let last_args = Arc::new(parking_lot::Mutex::new(String::new()));
      (StubCommand { n: name, called: called.clone(), last_args: last_args.clone() }, called, last_args)
  }
  fn headless_app() -> crate::app::App {
      crate::app::panel_ops::App::new_headless(80, 24).0
  }
  ```

- [ ] 添加 dispatch 精确匹配测试：
  - `test_dispatch_exact_match`：注册 "model" stub，dispatch "/model" → true，called=true
  - `test_dispatch_no_match`：dispatch "/unknown" → false

- [ ] 添加前缀唯一匹配测试：
  - `test_dispatch_prefix_unique`：只注册 "model"，dispatch "/mo" → true，called=true
  - `test_dispatch_prefix_ambiguous`：注册 "model" 和 "mock"，dispatch "/m" → false（歧义）

- [ ] 添加参数传递测试：
  - `test_dispatch_with_args`：dispatch "/model opus"，last_args == "opus"

- [ ] 添加辅助方法测试：
  - `test_match_prefix_returns_matching`：注册 "model"/"mock"/"clear"，match_prefix("mo") 返回 2 项
  - `test_list_returns_all`：注册 3 个 stub，list() 返回 3 项
  - `test_dispatch_empty_prefix`：dispatch "/" 或 "/  "（空命令名）→ false

**检查步骤:**

- [ ] TUI 命令系统测试全部通过
  - `cargo test -p rust-agent-tui -- command 2>&1 | tail -10`
  - 预期: `test result: ok. N passed; 0 failed`，N ≥ 8

---

### Task 5: 测试覆盖度提升 Acceptance

**前置条件:**
- 所有 Task 1-4 已完成
- 构建环境：`cargo build` 无错误

**端到端验证:**

1. rust-agent-middlewares 全量测试通过
   - `cargo test -p rust-agent-middlewares 2>&1 | tail -5`
   - 预期: `test result: ok. N passed; 0 failed`，N ≥ 125（原有约 65 + 新增约 37）
   - 失败时：检查 Task 1（文件系统工具）、Task 3（AskUserTool）

2. rust-relay-server 全量测试通过
   - `cargo test -p rust-relay-server 2>&1 | tail -5`
   - 预期: `test result: ok. N passed; 0 failed`，N ≥ 25（原有约 13 + 新增约 12）
   - 失败时：检查 Task 2（Relay Server auth + client）

3. rust-agent-tui 全量测试通过
   - `cargo test -p rust-agent-tui 2>&1 | tail -5`
   - 预期: `test result: ok. N passed; 0 failed`，N ≥ 63（原有约 55 + 新增约 8）
   - 失败时：检查 Task 4（TUI 命令系统）

4. 全 workspace 无编译警告影响测试
   - `cargo test --workspace 2>&1 | grep -E "^error|FAILED|test result"`
   - 预期: 无 error 行，无 FAILED 行；所有 `test result` 行均为 `ok`
