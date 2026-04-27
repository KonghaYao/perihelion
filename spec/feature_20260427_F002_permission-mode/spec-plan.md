# Permission Mode（多级权限模式）执行计划

**目标:** 支持 5 种权限模式（Default/AcceptEdits/Auto/BypassPermissions/DontAsk），Shift+Tab 循环切换，状态栏实时显示，模式切换仅影响后续工具调用。

**技术栈:** Rust, tokio async, AtomicU8 + Ordering::Relaxed 无锁共享状态, ratatui TUI 渲染, async-trait

**设计文档:** spec/feature_20260427_F002_permission-mode/spec-design.md

## 改动总览

- 本次改动涉及两个 crate：`rust-agent-middlewares`（新建 `shared_mode.rs`、`auto_classifier.rs`，重构 `hitl/mod.rs`，更新 `lib.rs` 导出）和 `rust-agent-tui`（修改 `app/mod.rs`、`app/agent.rs`、`app/agent_ops.rs`、`app/panel_ops.rs`、`event.rs`、`ui/main_ui/status_bar.rs`、`main.rs`）。按职责分为数据层（Task 1-3）和 UI 集成层（Task 4-5）。
- 依赖链：Task 1（基础枚举+共享状态）→ Task 2（AutoClassifier trait）→ Task 3（HITL 多模式改造，依赖 1+2）→ Task 4（TUI 注入共享状态）→ Task 5（键绑定+状态栏渲染）。Task 4 和 5 依赖 Task 1-3 的全部产出。
- 关键设计决策：`Arc<AtomicU8>` 无锁共享跨线程权限模式；HITL middleware 通过 `Option<Arc<SharedPermissionMode>>` 保持 `disabled()/new()` 向后兼容；`auto_classifier` 暂传 `None`（LLM 分类器实现到位但 TUI 层不注入），后续可扩展。

---

### Task 0: 环境准备

**背景:**
确保构建和测试工具链在当前开发环境中可用，避免后续 Task 因环境问题阻塞。

**执行步骤:**
- [x] 验证全量构建可用
  - `cargo build --workspace`
- [x] 验证测试框架可用
  - `cargo test --workspace 2>&1 | tail -5`

**检查步骤:**
- [x] 构建命令执行成功
  - `cargo build --workspace 2>&1 | tail -5`
  - 预期: 构建成功，无错误
- [x] 测试命令可用
  - `cargo test --workspace 2>&1 | tail -5`
  - 预期: 所有现有测试通过

---

### Task 1: PermissionMode 枚举与 SharedPermissionMode

**背景:**
当前 HITL 系统只有 YOLO/审批二态（`broker: Option<...>`），无法支持多级权限模式。本 Task 新增 `PermissionMode` 枚举（5 种模式）和 `SharedPermissionMode` 原子共享状态类，为后续 Task 的多模式决策逻辑和 TUI 切换提供基础类型。Task 3 的 `HumanInTheLoopMiddleware` 改造和 Task 4 的 TUI 集成都依赖本 Task 的产出。

**涉及文件:**
- 新建: `rust-agent-middlewares/src/hitl/shared_mode.rs`
- 修改: `rust-agent-middlewares/src/hitl/mod.rs`（添加 `pub mod shared_mode` 和 `pub use`）
- 修改: `rust-agent-middlewares/src/lib.rs`（更新 prelude 导出）

**执行步骤:**

- [x] 新建 `rust-agent-middlewares/src/hitl/shared_mode.rs`，定义 `PermissionMode` 枚举和 `SharedPermissionMode` 结构体
  - 位置: 新文件 `rust-agent-middlewares/src/hitl/shared_mode.rs`
  - 内容要点:
    ```rust
    use std::sync::atomic::{AtomicU8, Ordering};
    use std::sync::Arc;

    /// 权限模式枚举，控制 HITL 审批行为
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    #[repr(u8)]
    pub enum PermissionMode {
        /// 所有敏感工具弹窗审批（默认）
        Default = 0,
        /// 自动放行文件编辑类工具，其他敏感操作仍需审批
        AcceptEdits = 1,
        /// 使用 LLM 分类器自动决定放行/拒绝
        Auto = 2,
        /// 跳过所有审批（当前 YOLO 行为）
        BypassPermissions = 3,
        /// 自动拒绝所有审批请求
        DontAsk = 4,
    }

    impl Default for PermissionMode {
        fn default() -> Self {
            Self::Default
        }
    }

    impl PermissionMode {
        /// 循环切换到下一个模式：Default → AcceptEdits → Auto → BypassPermissions → DontAsk → Default
        pub fn next(self) -> Self {
            match self {
                Self::Default => Self::AcceptEdits,
                Self::AcceptEdits => Self::Auto,
                Self::Auto => Self::BypassPermissions,
                Self::BypassPermissions => Self::DontAsk,
                Self::DontAsk => Self::Default,
            }
        }

        /// 状态栏显示文本
        pub fn display_name(self) -> &'static str {
            match self {
                Self::Default => "DEFAULT",
                Self::AcceptEdits => "AUTO-EDIT",
                Self::Auto => "AUTO",
                Self::BypassPermissions => "YOLO",
                Self::DontAsk => "NO-ASK",
            }
        }
    }

    /// TryFrom<u8> 实现：异常值（>4）回退到 Default
    impl TryFrom<u8> for PermissionMode {
        type Error = std::convert::Infallible;

        fn try_from(value: u8) -> Result<Self, Self::Error> {
            Ok(match value {
                0 => Self::Default,
                1 => Self::AcceptEdits,
                2 => Self::Auto,
                3 => Self::BypassPermissions,
                4 => Self::DontAsk,
                _ => Self::Default, // 异常值回退到 Default
            })
        }
    }

    /// 跨线程共享的权限模式状态（Arc<AtomicU8> 封装）
    pub struct SharedPermissionMode {
        inner: AtomicU8,
    }

    impl SharedPermissionMode {
        /// 创建新的共享权限模式实例，返回 Arc<Self>
        pub fn new(mode: PermissionMode) -> Arc<Self> {
            Arc::new(Self {
                inner: AtomicU8::new(mode as u8),
            })
        }

        /// 读取当前权限模式
        pub fn load(&self) -> PermissionMode {
            let v = self.inner.load(Ordering::Relaxed);
            PermissionMode::try_from(v).unwrap_or(PermissionMode::Default)
        }

        /// 设置权限模式
        pub fn store(&self, mode: PermissionMode) {
            self.inner.store(mode as u8, Ordering::Relaxed);
        }

        /// CAS 循环切换到下一个模式，返回切换后的模式
        /// 使用 compare_exchange 防止并发竞争
        pub fn cycle(&self) -> PermissionMode {
            loop {
                let current = self.inner.load(Ordering::Relaxed);
                let current_mode = PermissionMode::try_from(current).unwrap_or(PermissionMode::Default);
                let next_mode = current_mode.next();
                let next = next_mode as u8;
                match self.inner.compare_exchange(
                    current,
                    next,
                    Ordering::Relaxed,
                    Ordering::Relaxed,
                ) {
                    Ok(_) => return next_mode,
                    Err(_) => continue, // 并发修改，重试
                }
            }
        }
    }
    ```
  - 原因: `#[repr(u8)]` 保证枚举值可直接转换为 `u8` 存入 `AtomicU8`；`TryFrom` 的 `Error` 类型设为 `Infallible` 表示转换不会失败（异常值回退而非报错），简化调用方代码；CAS 循环避免并发切换时的竞态条件。

- [x] 修改 `rust-agent-middlewares/src/hitl/mod.rs`，添加模块声明和导出
  - 位置: 文件顶部，`use std::sync::Arc;` 之后（~L2 后），添加 `pub mod shared_mode;`
  - 位置: 现有 `pub use rust_create_agent::hitl::{BatchItem, HitlDecision};` 之后（~L12 后），添加:
    ```rust
    pub use shared_mode::{PermissionMode, SharedPermissionMode};
    ```
  - 原因: 将 `PermissionMode` 和 `SharedPermissionMode` 通过 `hitl` 模块导出，供 `lib.rs` prelude 使用。

- [x] 修改 `rust-agent-middlewares/src/lib.rs`，更新导出和 prelude
  - 位置: 现有 `pub use hitl::{...}` 块（~L34-L37），在导出列表中追加 `PermissionMode` 和 `SharedPermissionMode`:
    ```rust
    pub use hitl::{
        default_requires_approval, is_yolo_mode, BatchItem, HitlDecision,
        HumanInTheLoopMiddleware, PermissionMode, SharedPermissionMode,
    };
    ```
  - 位置: prelude 模块中的 `pub use crate::hitl::{...}` 块（~L51-L54），同样追加:
    ```rust
    pub use crate::hitl::{
        default_requires_approval, is_yolo_mode, BatchItem, HitlDecision,
        HumanInTheLoopMiddleware, PermissionMode, SharedPermissionMode,
    };
    ```
  - 原因: 确保 `PermissionMode` 和 `SharedPermissionMode` 通过 crate 根和 prelude 对外可见，下游（TUI、测试）可直接 `use rust_agent_middlewares::prelude::*`。

- [x] 为 `PermissionMode` 和 `SharedPermissionMode` 编写单元测试
  - 测试文件: `rust-agent-middlewares/src/hitl/shared_mode.rs` 底部 `#[cfg(test)] mod tests`
  - 测试场景:
    - `PermissionMode::next()` 循环顺序: Default → AcceptEdits → Auto → BypassPermissions → DontAsk → Default
    - `PermissionMode::default()` 返回 `Default`
    - `PermissionMode::display_name()` 各变体返回正确字符串
    - `TryFrom<u8>` 正常值 0-4 映射到正确变体
    - `TryFrom<u8>` 异常值 5/255 回退到 `Default`
    - `SharedPermissionMode::new()` + `load()` 读写一致
    - `SharedPermissionMode::store()` 后 `load()` 返回新值
    - `SharedPermissionMode::cycle()` 单线程依次遍历所有模式
    - `SharedPermissionMode::cycle()` 多线程并发调用不 panic，最终状态合法（是有效 PermissionMode）
  - 运行命令: `cargo test -p rust-agent-middlewares --lib -- shared_mode::tests`
  - 预期: 所有测试通过

**检查步骤:**

- [x] 验证 `shared_mode.rs` 文件已创建
  - `ls -la rust-agent-middlewares/src/hitl/shared_mode.rs`
  - 预期: 文件存在

- [x] 验证 `hitl/mod.rs` 包含 `pub mod shared_mode` 和 `pub use`
  - `grep -n 'shared_mode\|PermissionMode\|SharedPermissionMode' rust-agent-middlewares/src/hitl/mod.rs`
  - 预期: 输出包含 `pub mod shared_mode;` 和 `pub use shared_mode::{PermissionMode, SharedPermissionMode};`

- [x] 验证 `lib.rs` 导出包含新类型
  - `grep -n 'PermissionMode\|SharedPermissionMode' rust-agent-middlewares/src/lib.rs`
  - 预期: 两处导出（`pub use hitl` 和 prelude 内 `pub use crate::hitl`）均包含新类型

- [x] 验证编译通过
  - `cargo build -p rust-agent-middlewares 2>&1 | tail -5`
  - 预期: 构建成功，无错误

- [x] 验证单元测试全部通过
  - `cargo test -p rust-agent-middlewares --lib -- shared_mode::tests`
  - 预期: 所有测试通过，输出包含 `test result: ok`

- [x] 验证已有 HITL 测试无回归
  - `cargo test -p rust-agent-middlewares --lib -- hitl::tests`
  - 预期: 所有现有测试通过

---

### Task 2: AutoClassifier 接口与 LLM 分类器

**背景:**
当前 HITL 系统只有人工审批和全放行两种路径，`Auto` 权限模式需要一个程序化决策器来判断工具调用的安全性。本 Task 新增 `AutoClassifier` trait（定义 `Classification` 枚举和 `async classify` 方法）及其 `LlmAutoClassifier` 实现（调用 LLM 做分类，带缓存）。Task 3 的 `HumanInTheLoopMiddleware` 改造将持有 `Option<Arc<dyn AutoClassifier>>`，在 `Auto` 模式下调用分类器替代人工审批。本 Task 依赖 Task 1 的 `shared_mode.rs` 已存在（模块结构已就绪），但本 Task 新增的 `auto_classifier.rs` 不直接引用 `PermissionMode`/`SharedPermissionMode`。

**涉及文件:**
- 新建: `rust-agent-middlewares/src/hitl/auto_classifier.rs`
- 修改: `rust-agent-middlewares/src/hitl/mod.rs`（添加 `pub mod auto_classifier` 和 `pub use`）
- 修改: `rust-agent-middlewares/src/lib.rs`（更新导出和 prelude）

**执行步骤:**

- [x] 新建 `rust-agent-middlewares/src/hitl/auto_classifier.rs`，定义 `Classification` 枚举、`AutoClassifier` trait 和 `LlmAutoClassifier` 实现类
  - 位置: 新文件 `rust-agent-middlewares/src/hitl/auto_classifier.rs`
  - 内容要点:
    ```rust
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use async_trait::async_trait;
    use parking_lot::Mutex;
    use rust_create_agent::llm::{BaseModel, LlmRequest};
    use rust_create_agent::messages::BaseMessage;

    /// 分类结果枚举
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Classification {
        /// 允许执行
        Allow,
        /// 拒绝执行
        Deny,
        /// 不确定，回退到人工审批
        Unsure,
    }

    /// 自动分类器 trait — 根据工具名称和输入判断是否放行
    #[async_trait]
    pub trait AutoClassifier: Send + Sync {
        async fn classify(
            &self,
            tool_name: &str,
            tool_input: &serde_json::Value,
        ) -> Classification;
    }

    // ─── 缓存条目 ────────────────────────────────────────────────────────────────

    /// 缓存条目：存储分类结果和过期时间
    struct CacheEntry {
        classification: Classification,
        expires_at: Instant,
    }

    // ─── LlmAutoClassifier ───────────────────────────────────────────────────────

    /// 基于 LLM 的自动分类器实现
    ///
    /// 持有 `Arc<Mutex<Box<dyn BaseModel>>>` 调用 LLM 做分类，
    /// 内置基于 `(tool_name, input_hash)` 的缓存，有效期 5 分钟。
    pub struct LlmAutoClassifier {
        model: Arc<Mutex<Box<dyn BaseModel>>>,
        cache: Mutex<HashMap<(String, u64), CacheEntry>>,
        cache_ttl: Duration,
    }

    impl LlmAutoClassifier {
        /// 创建新的 LLM 分类器
        ///
        /// - `model`: 包装为 `Arc<Mutex<Box<dyn BaseModel>>>` 的 LLM 实例
        /// - 缓存 TTL 默认 5 分钟
        pub fn new(model: Arc<Mutex<Box<dyn BaseModel>>>) -> Self {
            Self {
                model,
                cache: Mutex::new(HashMap::new()),
                cache_ttl: Duration::from_secs(5 * 60),
            }
        }

        /// 使用自定义缓存 TTL 创建分类器
        pub fn with_cache_ttl(
            model: Arc<Mutex<Box<dyn BaseModel>>>,
            cache_ttl: Duration,
        ) -> Self {
            Self {
                model,
                cache: Mutex::new(HashMap::new()),
                cache_ttl,
            }
        }

        /// 计算缓存键：(tool_name, input 的 64 位 hash)
        fn cache_key(tool_name: &str, tool_input: &serde_json::Value) -> (String, u64) {
            let input_str = serde_json::to_string(tool_input).unwrap_or_default();
            let hash = seahash::hash(input_str.as_bytes());
            (tool_name.to_string(), hash)
        }

        /// 查询缓存，命中且未过期返回 Some，否则 None
        fn lookup_cache(&self, key: &(String, u64)) -> Option<Classification> {
            let cache = self.cache.lock();
            cache.get(key).and_then(|entry| {
                if entry.expires_at > Instant::now() {
                    Some(entry.classification)
                } else {
                    None
                }
            })
        }

        /// 写入缓存
        fn insert_cache(&self, key: (String, u64), classification: Classification) {
            let mut cache = self.cache.lock();
            cache.insert(key, CacheEntry {
                classification,
                expires_at: Instant::now() + self.cache_ttl,
            });
        }

        /// 调用 LLM 进行分类
        async fn call_llm(
            &self,
            tool_name: &str,
            tool_input: &serde_json::Value,
        ) -> Classification {
            let prompt = format!(
                "判断以下工具调用是否安全。工具: {}, 输入: {}\n只回答 ALLOW / DENY / UNSURE，不要解释。",
                tool_name,
                serde_json::to_string(tool_input).unwrap_or_default(),
            );

            let request = LlmRequest::new(vec![BaseMessage::human(&prompt)])
                .with_system("你是一个工具调用安全分类器。只回答 ALLOW、DENY 或 UNSURE 中的一个词。")
                .with_max_tokens(32);

            // 持有 Mutex 锁期间调用 LLM
            let response = {
                let model = self.model.lock();
                model.invoke(request).await
            };

            match response {
                Ok(resp) => {
                    let text = resp.message.content().trim().to_uppercase();
                    if text.contains("ALLOW") {
                        Classification::Allow
                    } else if text.contains("DENY") {
                        Classification::Deny
                    } else {
                        Classification::Unsure
                    }
                }
                Err(_) => {
                    // 调用失败降级为 Unsure（fail-safe 原则）
                    Classification::Unsure
                }
            }
        }
    }

    #[async_trait]
    impl AutoClassifier for LlmAutoClassifier {
        async fn classify(
            &self,
            tool_name: &str,
            tool_input: &serde_json::Value,
        ) -> Classification {
            let key = Self::cache_key(tool_name, tool_input);

            // 1. 查缓存
            if let Some(cached) = self.lookup_cache(&key) {
                return cached;
            }

            // 2. 调用 LLM
            let result = self.call_llm(tool_name, tool_input).await;

            // 3. 写缓存
            self.insert_cache(key, result);

            result
        }
    }
    ```
  - 原因: `Classification` 三值枚举覆盖允许/拒绝/不确定三种路径，`Unsure` 回退到人工审批实现 fail-safe；`AutoClassifier` trait 使用 `async-trait` 与项目现有 trait 风格一致；`LlmAutoClassifier` 使用 `parking_lot::Mutex` 包装 `BaseModel`（已确认 Cargo.toml 中 `parking_lot = "0.12"` 可用）；缓存使用 `(tool_name, input_json_hash)` 为键，`Instant` 记录过期时间，避免重复 LLM 调用；LLM 调用失败时降级为 `Unsure`；`BaseModel::invoke` 和 `LlmRequest` 的用法经源码确认（`rust-create-agent/src/llm/mod.rs` L15-16，`types.rs` L5-39）。

- [x] 在 `auto_classifier.rs` 中引入 `seahash` 依赖的替代方案：使用 `std::hash::Hasher` + 自定义简单 hash 或直接使用 `serde_json` 字符串的 `hash` 方法
  - 位置: `rust-agent-middlewares/src/hitl/auto_classifier.rs` 的 `cache_key` 方法
  - 修改内容: 项目未依赖 `seahash` crate，使用标准库实现 hash:
    ```rust
    use std::hash::{Hash, Hasher};
    use std::collections::hash_map::DefaultHasher;

    fn cache_key(tool_name: &str, tool_input: &serde_json::Value) -> (String, u64) {
        let input_str = serde_json::to_string(tool_input).unwrap_or_default();
        let mut hasher = DefaultHasher::new();
        input_str.hash(&mut hasher);
        (tool_name.to_string(), hasher.finish())
    }
    ```
  - 原因: 避免新增外部 crate 依赖，使用标准库 `DefaultHasher` 满足 hash 需求。

- [x] 修改 `rust-agent-middlewares/src/hitl/mod.rs`，添加模块声明和导出
  - 位置: 文件顶部，`use std::sync::Arc;` 块之后（~L2 后），与 Task 1 添加的 `pub mod shared_mode;` 并列，添加:
    ```rust
    pub mod auto_classifier;
    ```
  - 位置: 现有 `pub use rust_create_agent::hitl::{BatchItem, HitlDecision};` 行之后（~L12 后），与 Task 1 添加的 `pub use shared_mode::...` 并列，添加:
    ```rust
    pub use auto_classifier::{AutoClassifier, Classification, LlmAutoClassifier};
    ```
  - 原因: 将 `AutoClassifier` trait 和 `Classification` 枚举通过 `hitl` 模块导出，供 `lib.rs` 统一导出和 Task 3 的 `HumanInTheLoopMiddleware` 使用。

- [x] 修改 `rust-agent-middlewares/src/lib.rs`，更新 crate 根导出和 prelude
  - 位置: 现有 `pub use hitl::{...}` 块（~L34-L37），在导出列表中追加 `AutoClassifier`、`Classification`、`LlmAutoClassifier`:
    ```rust
    pub use hitl::{
        default_requires_approval, is_yolo_mode, BatchItem, HitlDecision,
        HumanInTheLoopMiddleware, PermissionMode, SharedPermissionMode,
        AutoClassifier, Classification, LlmAutoClassifier,
    };
    ```
  - 位置: prelude 模块中的 `pub use crate::hitl::{...}` 块（~L51-L54），同样追加:
    ```rust
    pub use crate::hitl::{
        default_requires_approval, is_yolo_mode, BatchItem, HitlDecision,
        HumanInTheLoopMiddleware, PermissionMode, SharedPermissionMode,
        AutoClassifier, Classification, LlmAutoClassifier,
    };
    ```
  - 原因: 确保所有新类型通过 crate 根和 prelude 对外可见，下游 Task 3/4/5 可直接 `use rust_agent_middlewares::prelude::*` 获取。

- [x] 为 `Classification` 枚举和 `LlmAutoClassifier` 编写单元测试
  - 测试文件: `rust-agent-middlewares/src/hitl/auto_classifier.rs` 底部 `#[cfg(test)] mod tests`
  - 测试场景:
    - `Classification` 枚举变体比较: `Classification::Allow != Classification::Deny`，`Classification::Unsure` 存在且可 copy
    - `LlmAutoClassifier::cache_key` 相同输入产生相同 key，不同输入产生不同 key
    - `LlmAutoClassifier` 调用 LLM 返回 "ALLOW" 文本时 `classify()` 返回 `Classification::Allow`
    - `LlmAutoClassifier` 调用 LLM 返回 "DENY" 文本时 `classify()` 返回 `Classification::Deny`
    - `LlmAutoClassifier` 调用 LLM 返回 "UNSURE" 文本时 `classify()` 返回 `Classification::Unsure`
    - `LlmAutoClassifier` 调用 LLM 返回乱码文本时 `classify()` 返回 `Classification::Unsure`
    - `LlmAutoClassifier` LLM 调用失败（返回 Err）时 `classify()` 返回 `Classification::Unsure`（fail-safe）
    - `LlmAutoClassifier` 缓存命中：第二次相同输入不调用 LLM（通过调用次数计数器验证）
    - `LlmAutoClassifier` 缓存过期后重新调用 LLM
  - Mock LLM 实现:
    ```rust
    struct MockClassifyModel {
        response: Mutex<String>,
        call_count: AtomicUsize,
        should_fail: Mutex<bool>,
    }
    impl MockClassifyModel {
        fn new(response: &str) -> Self { ... }
        fn call_count(&self) -> usize { ... }
        fn set_should_fail(&self, fail: bool) { ... }
    }
    #[async_trait]
    impl BaseModel for MockClassifyModel {
        async fn invoke(&self, _request: LlmRequest) -> AgentResult<LlmResponse> {
            if *self.should_fail.lock() {
                return Err(AgentError::LlmError("mock failure".into()));
            }
            self.call_count.fetch_add(1, Ordering::Relaxed);
            Ok(LlmResponse {
                message: BaseMessage::ai(self.response.lock().clone()),
                stop_reason: StopReason::EndTurn,
                usage: None,
            })
        }
        fn provider_name(&self) -> &str { "mock" }
        fn model_id(&self) -> &str { "mock-classifier" }
    }
    ```
  - 缓存过期测试使用 `LlmAutoClassifier::with_cache_ttl(model, Duration::from_millis(50))` 设置极短 TTL
  - 运行命令: `cargo test -p rust-agent-middlewares --lib -- auto_classifier::tests`
  - 预期: 所有测试通过

**检查步骤:**

- [x] 验证 `auto_classifier.rs` 文件已创建
  - `ls -la rust-agent-middlewares/src/hitl/auto_classifier.rs`
  - 预期: 文件存在

- [x] 验证 `hitl/mod.rs` 包含模块声明和导出
  - `grep -n 'auto_classifier\|AutoClassifier\|Classification\|LlmAutoClassifier' rust-agent-middlewares/src/hitl/mod.rs`
  - 预期: 输出包含 `pub mod auto_classifier;` 和 `pub use auto_classifier::{AutoClassifier, Classification, LlmAutoClassifier};`

- [x] 验证 `lib.rs` 导出包含新类型
  - `grep -n 'AutoClassifier\|Classification\|LlmAutoClassifier' rust-agent-middlewares/src/lib.rs`
  - 预期: 两处导出（`pub use hitl` 和 prelude 内 `pub use crate::hitl`）均包含 `AutoClassifier`、`Classification`、`LlmAutoClassifier`

- [x] 验证编译通过
  - `cargo build -p rust-agent-middlewares 2>&1 | tail -5`
  - 预期: 构建成功，无错误

- [x] 验证单元测试全部通过
  - `cargo test -p rust-agent-middlewares --lib -- auto_classifier::tests`
  - 预期: 所有测试通过，输出包含 `test result: ok`

- [x] 验证已有 HITL 测试无回归
  - `cargo test -p rust-agent-middlewares --lib -- hitl::tests`
  - 预期: 所有现有测试通过

---

### Task 3: HumanInTheLoopMiddleware 多模式改造

**背景:**
当前 `HumanInTheLoopMiddleware` 只支持"启用审批/全放行"二态（通过 `broker: Option<...>` 的 `Some/None` 判断），无法根据用户实时切换的权限模式做出差异化决策。本 Task 重构 `HumanInTheLoopMiddleware`，新增 `mode: Option<Arc<SharedPermissionMode>>` 和 `auto_classifier: Option<Arc<dyn AutoClassifier>>` 字段，在 `before_tool` 和 `process_batch` 中读取共享模式动态决策，支持 5 种权限模式的完整行为。Task 4（TUI 集成：共享状态注入）将使用本 Task 新增的 `with_shared_mode` 构造函数构建 HITL middleware。本 Task 依赖 Task 1 产出的 `SharedPermissionMode`/`PermissionMode` 和 Task 2 产出的 `AutoClassifier`/`Classification`。

**涉及文件:**
- 修改: `rust-agent-middlewares/src/hitl/mod.rs`（重构 `HumanInTheLoopMiddleware` 结构体、构造函数、`before_tool`、`process_batch`，新增 `is_edit_tool` 辅助函数，扩展测试模块）

**执行步骤:**

- [x] 在 `HumanInTheLoopMiddleware` 结构体中新增 `mode` 和 `auto_classifier` 字段
  - 位置: `rust-agent-middlewares/src/hitl/mod.rs`，`pub struct HumanInTheLoopMiddleware` 定义处（~L53-L56）
  - 将结构体改为:
    ```rust
    pub struct HumanInTheLoopMiddleware {
        broker: Option<Arc<dyn UserInteractionBroker>>,
        requires_approval: fn(&str) -> bool,
        /// 共享权限模式（动态切换），None 时走原有 Some/None broker 逻辑（向后兼容）
        mode: Option<Arc<SharedPermissionMode>>,
        /// Auto 模式的 LLM 分类器，仅在 mode=Auto 时使用
        auto_classifier: Option<Arc<dyn AutoClassifier>>,
    }
    ```
  - 原因: `mode` 为 `Option` 保证 `disabled()` 和 `new()` 构造函数的向后兼容（`mode=None`），避免破坏现有调用方。

- [x] 修改 `new()` 和 `disabled()` 构造函数，为新增字段填充默认值
  - 位置: `rust-agent-middlewares/src/hitl/mod.rs`，`impl HumanInTheLoopMiddleware` 的 `new()` 方法（~L60-L65）
  - 在 `Self { ... }` 块中追加 `mode: None, auto_classifier: None,`:
    ```rust
    pub fn new(broker: Arc<dyn UserInteractionBroker>, requires_approval: fn(&str) -> bool) -> Self {
        Self {
            broker: Some(broker),
            requires_approval,
            mode: None,
            auto_classifier: None,
        }
    }
    ```
  - 位置: `disabled()` 方法（~L68-L73）
  - 在 `Self { ... }` 块中追加 `mode: None, auto_classifier: None,`:
    ```rust
    pub fn disabled() -> Self {
        Self {
            broker: None,
            requires_approval: default_requires_approval,
            mode: None,
            auto_classifier: None,
        }
    }
    ```
  - 原因: `mode: None` 使得这两个构造函数完全保留原有行为——`disabled()` 全放行（broker=None 直接返回 Ok），`new()` 全部敏感工具走审批（broker=Some，mode=None 跳过模式判断直接弹窗）。

- [x] 新增 `with_shared_mode()` 构造函数
  - 位置: `rust-agent-middlewares/src/hitl/mod.rs`，`from_env()` 方法之后（~L83 后）
  - 添加:
    ```rust
    /// 创建带共享权限模式的 HITL 中间件
    ///
    /// - `broker`: 用户交互代理（弹窗审批、ask_user 等）
    /// - `requires_approval`: 判断工具是否为敏感工具的规则函数
    /// - `mode`: 共享权限模式，通过 `Arc<SharedPermissionMode>` 跨线程共享
    /// - `auto_classifier`: Auto 模式的分类器（可选，Auto 模式下为 None 则降级为 Unsure 走审批）
    pub fn with_shared_mode(
        broker: Arc<dyn UserInteractionBroker>,
        requires_approval: fn(&str) -> bool,
        mode: Arc<SharedPermissionMode>,
        auto_classifier: Option<Arc<dyn AutoClassifier>>,
    ) -> Self {
        Self {
            broker: Some(broker),
            requires_approval,
            mode: Some(mode),
            auto_classifier,
        }
    }
    ```
  - 原因: 这是 Task 4 TUI 层调用的主要构造函数，同时传入 `SharedPermissionMode` 和可选的 `AutoClassifier`。

- [x] 新增 `is_edit_tool()` 辅助函数
  - 位置: `rust-agent-middlewares/src/hitl/mod.rs`，`default_requires_approval()` 函数之后（~L43 后），`pub struct HumanInTheLoopMiddleware` 定义之前
  - 添加:
    ```rust
    /// 判断工具是否为文件编辑类工具（AcceptEdits 模式使用）
    ///
    /// `write_*`、`edit_*`、`folder_operations` 归类为编辑工具，在 AcceptEdits 模式下自动放行。
    /// `bash`、`launch_agent`、`delete_*`、`rm_*` 不属于编辑工具，仍需审批。
    pub fn is_edit_tool(tool_name: &str) -> bool {
        tool_name.starts_with("write_")
            || tool_name.starts_with("edit_")
            || tool_name == "folder_operations"
    }
    ```
  - 原因: `AcceptEdits` 模式区分编辑工具和其他敏感工具，此函数作为公共辅助函数便于测试和复用。

- [x] 重构 `before_tool` 方法，实现多模式决策逻辑
  - 位置: `rust-agent-middlewares/src/hitl/mod.rs`，`async fn before_tool()` 方法（~L159-L185）
  - 替换整个方法体为:
    ```rust
    async fn before_tool(&self, _state: &mut S, tool_call: &ToolCall) -> AgentResult<ToolCall> {
        // 1. 非敏感工具 → 所有模式都放行
        if !(self.requires_approval)(&tool_call.name) {
            return Ok(tool_call.clone());
        }

        // 2. 有 mode → 按权限模式决策
        if let Some(mode) = &self.mode {
            return self.decide_by_mode(mode, tool_call).await;
        }

        // 3. 无 mode 且无 broker → 放行（disabled() 路径）
        let Some(broker) = &self.broker else {
            return Ok(tool_call.clone());
        };

        // 4. 无 mode 但有 broker → 原有弹窗审批逻辑
        self.broker_approve(broker, tool_call).await
    }
    ```
  - 原因: 将决策流程拆分为清晰的分支——非敏感工具优先放行，有 mode 走新模式决策，无 mode 走原 broker 逻辑，保证向后兼容。

- [x] 新增 `broker_approve()` 私有异步方法，提取原有弹窗审批逻辑
  - 位置: `rust-agent-middlewares/src/hitl/mod.rs`，`impl HumanInTheLoopMiddleware` 块内（在 `process_batch` 方法之前）
  - 添加:
    ```rust
    /// 通过 broker 请求用户审批单个工具调用
    async fn broker_approve(
        &self,
        broker: &Arc<dyn UserInteractionBroker>,
        tool_call: &ToolCall,
    ) -> AgentResult<ToolCall> {
        let ctx = InteractionContext::Approval {
            items: vec![ApprovalItem {
                tool_call_id: tool_call.id.clone(),
                tool_name: tool_call.name.clone(),
                tool_input: tool_call.input.clone(),
            }],
        };
        let response = broker.request(ctx).await;
        let decision = match response {
            InteractionResponse::Decisions(mut d) => d
                .pop()
                .unwrap_or(ApprovalDecision::Reject { reason: "用户拒绝".to_string() }),
            _ => ApprovalDecision::Reject { reason: "用户拒绝".to_string() },
        };
        apply_decision(tool_call, decision)
    }
    ```
  - 原因: 提取公共的 broker 审批逻辑为独立方法，避免在 `before_tool` 和 `decide_by_mode` 中重复相同的弹窗审批代码。

- [x] 新增 `decide_by_mode()` 私有异步方法，实现 5 种模式的具体决策
  - 位置: `rust-agent-middlewares/src/hitl/mod.rs`，`impl HumanInTheLoopMiddleware` 块内（在 `broker_approve` 方法之后）
  - 添加:
    ```rust
    /// 根据共享权限模式决策单个工具调用
    async fn decide_by_mode(
        &self,
        mode: &Arc<SharedPermissionMode>,
        tool_call: &ToolCall,
    ) -> AgentResult<ToolCall> {
        match mode.load() {
            PermissionMode::BypassPermissions => Ok(tool_call.clone()),
            PermissionMode::DontAsk => Err(AgentError::ToolRejected {
                tool: tool_call.name.clone(),
                reason: "DontAsk 模式：自动拒绝".to_string(),
            }),
            PermissionMode::AcceptEdits => {
                if is_edit_tool(&tool_call.name) {
                    Ok(tool_call.clone())
                } else {
                    match &self.broker {
                        Some(broker) => self.broker_approve(broker, tool_call).await,
                        None => Ok(tool_call.clone()),
                    }
                }
            }
            PermissionMode::Auto => {
                match &self.auto_classifier {
                    Some(classifier) => {
                        let result = classifier.classify(&tool_call.name, &tool_call.input).await;
                        match result {
                            Classification::Allow => Ok(tool_call.clone()),
                            Classification::Deny => Err(AgentError::ToolRejected {
                                tool: tool_call.name.clone(),
                                reason: "Auto 模式：分类器拒绝".to_string(),
                            }),
                            Classification::Unsure => {
                                match &self.broker {
                                    Some(broker) => self.broker_approve(broker, tool_call).await,
                                    None => Err(AgentError::ToolRejected {
                                        tool: tool_call.name.clone(),
                                        reason: "Auto 模式：分类器不确定且无 broker".to_string(),
                                    }),
                                }
                            }
                        }
                    }
                    None => {
                        // 无分类器，降级为 Unsure → 走 broker 审批
                        match &self.broker {
                            Some(broker) => self.broker_approve(broker, tool_call).await,
                            None => Err(AgentError::ToolRejected {
                                tool: tool_call.name.clone(),
                                reason: "Auto 模式：无分类器且无 broker".to_string(),
                            }),
                        }
                    }
                }
            }
            PermissionMode::Default => {
                match &self.broker {
                    Some(broker) => self.broker_approve(broker, tool_call).await,
                    None => Ok(tool_call.clone()),
                }
            }
        }
    }
    ```
  - 原因: 每种模式有独立且明确的决策路径：`BypassPermissions` 全放行，`DontAsk` 全拒绝，`AcceptEdits` 区分编辑/非编辑，`Auto` 调用分类器，`Default` 走 broker 审批。无 broker 时除 `Default/BypassPermissions` 外返回错误，确保 fail-safe。

- [x] 重构 `process_batch` 方法，支持逐个读取模式动态决策
  - 位置: `rust-agent-middlewares/src/hitl/mod.rs`，`pub async fn process_batch()` 方法（~L107-L150）
  - 替换整个方法体为:
    ```rust
    pub async fn process_batch(&self, calls: &[ToolCall]) -> Vec<AgentResult<ToolCall>> {
        let mut results: Vec<AgentResult<ToolCall>> = Vec::with_capacity(calls.len());

        for (i, call) in calls.iter().enumerate() {
            // 非敏感工具 → 直接放行
            if !(self.requires_approval)(&call.name) {
                results.push(Ok(call.clone()));
                continue;
            }

            // 有 mode → 逐个读取最新模式（模式可能在批量处理期间被切换）
            if let Some(mode) = &self.mode {
                results.push(self.decide_by_mode(mode, call).await);
                continue;
            }

            // 无 mode 且无 broker → 放行
            let Some(broker) = &self.broker else {
                results.push(Ok(call.clone()));
                continue;
            };

            // 无 mode 但有 broker → 收集后批量弹窗（原有逻辑）
            // 为保持向后兼容，无 mode 时仍使用原来的批量收集+一次性弹窗逻辑
            return self.batch_broker_approve(broker, calls, i, &mut results).await;
        }

        results
    }
    ```
  - 原因: 有 mode 时逐个决策确保模式切换的实时性（每次调用 `mode.load()` 读最新值）；无 mode 时保留原有的批量弹窗逻辑保证向后兼容。

- [x] 新增 `batch_broker_approve()` 私有异步方法，保留原有批量审批逻辑
  - 位置: `rust-agent-middlewares/src/hitl/mod.rs`，`impl HumanInTheLoopMiddleware` 块内（在 `process_batch` 方法之后）
  - 添加:
    ```rust
    /// 无 mode 时原有的批量 broker 审批逻辑（向后兼容）
    ///
    /// `start_idx` 表示已通过逐个处理放行的非敏感工具数量，
    /// 从 `calls[start_idx]` 开始收集剩余需要审批的敏感工具。
    async fn batch_broker_approve(
        &self,
        broker: &Arc<dyn UserInteractionBroker>,
        calls: &[ToolCall],
        start_idx: usize,
        initial_results: &mut Vec<AgentResult<ToolCall>>,
    ) -> Vec<AgentResult<ToolCall>> {
        let mut results: Vec<AgentResult<ToolCall>> = initial_results.drain(..).collect();

        // 收集从 start_idx 开始需要审批的项
        let needs_approval: Vec<(usize, &ToolCall)> = calls
            .iter()
            .enumerate()
            .skip(start_idx)
            .filter(|(_, c)| (self.requires_approval)(&c.name))
            .collect();

        if needs_approval.is_empty() {
            results.extend(calls.iter().skip(start_idx).map(|c| Ok(c.clone())));
            return results;
        }

        let items: Vec<ApprovalItem> = needs_approval
            .iter()
            .map(|(_, c)| ApprovalItem {
                tool_call_id: c.id.clone(),
                tool_name: c.name.clone(),
                tool_input: c.input.clone(),
            })
            .collect();

        let ctx = InteractionContext::Approval { items };
        let response = broker.request(ctx).await;

        let decisions = match response {
            InteractionResponse::Decisions(d) => d,
            _ => vec![ApprovalDecision::Reject { reason: "unexpected response".to_string() }; needs_approval.len()],
        };

        let mut decision_iter = decisions.into_iter();

        for idx in start_idx..calls.len() {
            let call = &calls[idx];
            if (self.requires_approval)(&call.name) {
                let decision = decision_iter
                    .next()
                    .unwrap_or(ApprovalDecision::Reject { reason: "用户拒绝".to_string() });
                results.push(apply_decision(call, decision));
            } else {
                results.push(Ok(call.clone()));
            }
        }

        results
    }
    ```
  - 原因: 保持无 mode 时的批量弹窗行为与改造前完全一致（一次性收集所有敏感工具调用，一次性弹窗，逐项 apply_decision），保证现有测试不回归。

- [x] 在文件顶部添加必要的 `use` 导入
  - 位置: `rust-agent-middlewares/src/hitl/mod.rs`，现有 `use` 块中（~L1-L10）
  - 确认已有 `use rust_create_agent::error::{AgentError, AgentResult};`（~L6），在 `pub use rust_create_agent::hitl::{BatchItem, HitlDecision};` 行之后（~L12 后）添加:
    ```rust
    use shared_mode::{PermissionMode, SharedPermissionMode};
    use auto_classifier::{AutoClassifier, Classification};
    ```
  - 原因: `decide_by_mode` 和 `with_shared_mode` 需要引用这些类型。此处 `use` 而非 `pub use`，因为这些类型已通过 Task 1/2 的 `pub use` 导出，此处仅内部使用。

- [x] 为 `is_edit_tool` 函数和 `HumanInTheLoopMiddleware` 多模式行为编写单元测试
  - 测试文件: `rust-agent-middlewares/src/hitl/mod.rs` 底部 `mod tests` 块内（~L343 后），追加以下测试
  - Mock Classifier 实现:
    ```rust
    /// Mock 自动分类器，返回预设的分类结果
    struct MockClassifier {
        result: Classification,
    }
    impl MockClassifier {
        fn new(result: Classification) -> Self {
            Self { result }
        }
    }
    #[async_trait]
    impl AutoClassifier for MockClassifier {
        async fn classify(&self, _tool_name: &str, _tool_input: &serde_json::Value) -> Classification {
            self.result
        }
    }
    ```
  - 辅助构造函数:
    ```rust
    fn make_mw_with_mode(mode: PermissionMode, classifier: Option<Arc<dyn AutoClassifier>>) -> HumanInTheLoopMiddleware {
        let broker = Arc::new(AutoApproveBroker);
        let shared = SharedPermissionMode::new(mode);
        HumanInTheLoopMiddleware::with_shared_mode(broker, default_requires_approval, shared, classifier)
    }
    ```
  - 测试场景:
    - `is_edit_tool` 函数正确性: `write_file`/`edit_file`/`folder_operations` 返回 `true`，`bash`/`launch_agent`/`delete_x`/`rm_x`/`read_file` 返回 `false`
    - `with_shared_mode` + `BypassPermissions` 模式: 敏感工具 `bash` 直接放行
    - `with_shared_mode` + `DontAsk` 模式: 敏感工具 `bash` 返回 `Err(ToolRejected)`
    - `with_shared_mode` + `AcceptEdits` 模式 + `write_file`（编辑工具）: 直接放行，不调用 broker
    - `with_shared_mode` + `AcceptEdits` 模式 + `bash`（非编辑敏感工具）: 走 broker 审批
    - `with_shared_mode` + `Default` 模式 + `bash`: 走 broker 审批
    - `with_shared_mode` + `Auto` 模式 + mock classifier 返回 `Allow`: 直接放行
    - `with_shared_mode` + `Auto` 模式 + mock classifier 返回 `Deny`: 返回 `Err(ToolRejected)`
    - `with_shared_mode` + `Auto` 模式 + mock classifier 返回 `Unsure`: 走 broker 审批
    - `with_shared_mode` + `Auto` 模式 + 无 classifier (`None`): 走 broker 审批（降级）
    - `process_batch` 在 `BypassPermissions` 模式下全放行
    - `process_batch` 在 `DontAsk` 模式下敏感工具全拒绝
    - `process_batch` 在 `AcceptEdits` 模式下混合工具调用：编辑工具放行 + bash 走审批
    - `disabled()` 行为不变: 全放行（确认已有 `test_disabled_allows_all` 仍通过）
    - `new()` 行为不变: 敏感工具走审批（确认已有 `test_approve_passes_through`/`test_reject_returns_error` 仍通过）
  - 运行命令: `cargo test -p rust-agent-middlewares --lib -- hitl::tests`
  - 预期: 所有测试通过（包括原有 7 个测试和新增测试）

**检查步骤:**

- [x] 验证 `is_edit_tool` 函数存在且为 `pub`
  - `grep -n 'pub fn is_edit_tool' rust-agent-middlewares/src/hitl/mod.rs`
  - 预期: 输出包含该函数声明

- [x] 验证 `with_shared_mode` 构造函数存在
  - `grep -n 'pub fn with_shared_mode' rust-agent-middlewares/src/hitl/mod.rs`
  - 预期: 输出包含该构造函数声明

- [x] 验证 `HumanInTheLoopMiddleware` 结构体包含 `mode` 和 `auto_classifier` 字段
  - `grep -n 'mode:\|auto_classifier:' rust-agent-middlewares/src/hitl/mod.rs | head -10`
  - 预期: 结构体定义和构造函数中均包含这两个字段

- [x] 验证编译通过
  - `cargo build -p rust-agent-middlewares 2>&1 | tail -5`
  - 预期: 构建成功，无错误

- [x] 验证所有 HITL 单元测试通过（原 7 个 + 新增）
  - `cargo test -p rust-agent-middlewares --lib -- hitl::tests 2>&1`
  - 预期: 所有测试通过，输出包含 `test result: ok`

- [x] 验证下游 `rust-agent-tui` 编译通过（不破坏现有调用方）
  - `cargo build -p rust-agent-tui 2>&1 | tail -5`
  - 预期: 构建成功（`disabled()` 和 `new()` 签名未变）

---

### Task 4: TUI 集成：共享状态注入与 Agent 构建

**背景:**
TUI 层需要在 `App` 启动时创建 `Arc<SharedPermissionMode>` 并注入到 agent task，使 HITL 中间件能实时读取用户当前选择的权限模式。当前代码中 `App::new()` 无权限模式字段，`submit_message()` 构建 `AgentRunConfig` 时未传入共享状态，`run_universal_agent()` 使用 `HumanInTheLoopMiddleware::from_env()` 构建 HITL（仅支持 YOLO/审批二态）。本 Task 在 TUI 层打通从启动参数到 agent 构建的完整链路，依赖 Task 1 产出的 `SharedPermissionMode`/`PermissionMode` 和 Task 3 产出的 `with_shared_mode()` 构造函数。Task 5（Shift+Tab 键绑定与状态栏显示）将使用本 Task 注入的 `permission_mode` 字段实现运行时切换。

**涉及文件:**
- 修改: `rust-agent-tui/src/app/mod.rs`（`App` struct 新增 `permission_mode` 字段，`App::new()` 初始化，`new_headless()` 初始化）
- 修改: `rust-agent-tui/src/app/agent.rs`（`AgentRunConfig` 新增 `permission_mode` 字段，`run_universal_agent` 使用 `with_shared_mode` 构建 HITL）
- 修改: `rust-agent-tui/src/app/agent_ops.rs`（`submit_message` 传入 `permission_mode`)
- 修改: `rust-agent-tui/src/main.rs`（解析初始模式从 `YOLO_MODE` 环境变量和 `-a` CLI 参数）
- 修改: `rust-agent-tui/src/app/panel_ops.rs`（`new_headless` 初始化 `permission_mode` 字段）

**执行步骤:**

- [x] 在 `App` struct 中新增 `permission_mode` 字段
  - 位置: `rust-agent-tui/src/app/mod.rs`，`pub struct App` 定义处（~L71-L87），在 `setup_wizard` 字段之后（~L86 后）
  - 在结构体末尾 `pub setup_wizard: Option<SetupWizardPanel>,` 之后添加:
    ```rust
    pub permission_mode: Arc<rust_agent_middlewares::prelude::SharedPermissionMode>,
    ```
  - 在文件顶部的 `use std::sync::Arc;`（~L57）保持不变，`Arc` 已导入
  - 原因: `Arc<SharedPermissionMode>` 是 TUI 线程和 agent task 之间的共享状态桥梁，`App` 持有此 `Arc` 并在 `submit_message` 时 clone 传入 agent task。

- [x] 修改 `App::new()` 初始化 `permission_mode` 字段
  - 位置: `rust-agent-tui/src/app/mod.rs`，`App::new()` 方法的 `Self { ... }` 构造块（~L148-L163）
  - 在 `setup_wizard: None,` 之后（~L162 后）添加:
    ```rust
    permission_mode: rust_agent_middlewares::prelude::SharedPermissionMode::new(
                    rust_agent_middlewares::prelude::PermissionMode::BypassPermissions,
                ),
    ```
  - 原因: `App::new()` 不接收参数，初始模式硬编码为 `BypassPermissions`（保持当前 YOLO 默认行为不变）。`main.rs` 在创建 `App` 后通过 `app.permission_mode.store(...)` 覆盖为从环境变量解析的初始值。

- [x] 修改 `main.rs` 中解析初始模式并在 `App::new()` 后覆盖
  - 位置: `rust-agent-tui/src/main.rs`，`run_app` 函数内 `let mut app = App::new();` 之后（~L111 后）
  - 在 `let mut app = App::new();` 之后，`// 检测是否需要 Setup 向导` 之前，添加:
    ```rust
    // 根据环境变量/CLI 参数设置初始权限模式
    // YOLO_MODE=true 或未设置 → BypassPermissions（默认，全放行）
    // YOLO_MODE=false 或 -a/--approve → Default（弹窗审批）
    {
        use rust_agent_middlewares::prelude::{PermissionMode, SharedPermissionMode};
        let initial_mode = if std::env::var("YOLO_MODE")
            .map(|v| !v.eq_ignore_ascii_case("false") && v != "0")
            .unwrap_or(true)
        {
            PermissionMode::BypassPermissions
        } else {
            PermissionMode::Default
        };
        app.permission_mode.store(initial_mode);
    }
    ```
  - 原因: `main.rs` 已在更早位置处理 `-a`/`--approve` 参数（~L65-L67 设置 `YOLO_MODE=false`），此处只需读取 `YOLO_MODE` 环境变量即可决定初始模式。初始模式通过 `store()` 覆盖 `App::new()` 的默认值，避免修改 `App::new()` 的签名（保持 `new_headless` 等调用方兼容）。

- [x] 修改 `AgentRunConfig` 新增 `permission_mode` 字段
  - 位置: `rust-agent-tui/src/app/agent.rs`，`pub struct AgentRunConfig` 定义处（~L18-L33），在 `cron_scheduler` 字段之后（~L32 后）
  - 添加:
    ```rust
    pub permission_mode: Arc<rust_agent_middlewares::prelude::SharedPermissionMode>,
    ```
  - 在文件顶部（~L7 `use rust_agent_middlewares::prelude::*;`）保持不变，`prelude` 中 Task 1 已添加 `SharedPermissionMode` 和 `PermissionMode` 导出
  - 原因: `AgentRunConfig` 是 `submit_message` → `run_universal_agent` 的参数集合，新增此字段使 agent task 可访问共享权限模式。

- [x] 修改 `run_universal_agent` 解构新增字段并使用 `with_shared_mode` 构建 HITL
  - 位置: `rust-agent-tui/src/app/agent.rs`，`run_universal_agent` 函数的参数解构处（~L36-L51）
  - 在解构块 `let AgentRunConfig { ... } = cfg;` 中 `cron_scheduler,` 之后添加 `permission_mode,`:
    ```rust
    let AgentRunConfig {
        provider,
        input,
        cwd,
        history,
        tx,
        cancel,
        agent_id,
        relay_client,
        langfuse_tracer,
        thread_store,
        thread_id,
        preload_skills,
        config: zen_config,
        cron_scheduler,
        permission_mode,
    } = cfg;
    ```
  - 位置: 同一函数内 HITL middleware 构建处（~L88-L91），替换:
    ```rust
    // 原代码:
    let hitl = HumanInTheLoopMiddleware::from_env(
        broker.clone() as Arc<dyn rust_create_agent::interaction::UserInteractionBroker>,
        default_requires_approval,
    );
    ```
    替换为:
    ```rust
    // 新代码：使用 with_shared_mode 注入共享权限模式
    let hitl = HumanInTheLoopMiddleware::with_shared_mode(
        broker.clone() as Arc<dyn rust_create_agent::interaction::UserInteractionBroker>,
        default_requires_approval,
        permission_mode,
        None, // auto_classifier 暂传 None，后续可扩展
    );
    ```
  - 原因: `with_shared_mode` 是 Task 3 新增的构造函数，接收 `Arc<SharedPermissionMode>` 和可选的 `AutoClassifier`。此处 `auto_classifier` 传 `None`，Auto 模式下将降级为 Unsure 走审批路径（fail-safe）。`from_env()` 不再使用，避免 YOLO/审批二态的硬编码。

- [x] 修改 `submit_message` 传入 `permission_mode` 到 `AgentRunConfig`
  - 位置: `rust-agent-tui/src/app/agent_ops.rs`，`submit_message` 方法中 `AgentRunConfig { ... }` 构造处（~L139-L154）
  - 在 `cron_scheduler,` 之后（~L153 后）添加:
    ```rust
    permission_mode: self.permission_mode.clone(),
    ```
  - 原因: 每次 `submit_message` 时 clone `Arc<SharedPermissionMode>` 传入 agent task，agent task 通过 `mode.load()` 读取最新权限模式。`Arc::clone` 仅复制指针，开销极低。

- [x] 修改 `new_headless` 初始化 `permission_mode` 字段
  - 位置: `rust-agent-tui/src/app/panel_ops.rs`，`new_headless` 方法的 `App { ... }` 构造处（~L264-L279），在 `setup_wizard: None,` 之后（~L278 后）
  - 添加:
    ```rust
    permission_mode: rust_agent_middlewares::prelude::SharedPermissionMode::new(
        rust_agent_middlewares::prelude::PermissionMode::BypassPermissions,
    ),
    ```
  - 原因: headless 测试的 `App` 也必须包含 `permission_mode` 字段，否则编译失败。默认 `BypassPermissions` 与 headless 测试不涉及审批的行为一致。

- [x] 为 `permission_mode` 注入链路编写单元测试
  - 测试文件: `rust-agent-tui/src/app/panel_ops.rs`（在 `new_headless` 同模块内）或 `rust-agent-tui/src/ui/headless.rs`（现有 headless 测试模块）
  - 在 `rust-agent-tui/src/ui/headless.rs` 的 `mod tests` 块内追加以下测试:
    ```rust
    #[tokio::test]
    async fn test_app_default_permission_mode_is_bypass() {
        let (app, _handle) = App::new_headless(80, 24);
        use rust_agent_middlewares::prelude::PermissionMode;
        assert_eq!(
            app.permission_mode.load(),
            PermissionMode::BypassPermissions,
            "headless App 默认应为 BypassPermissions"
        );
    }

    #[tokio::test]
    async fn test_permission_mode_store_and_load() {
        let (mut app, _handle) = App::new_headless(80, 24);
        use rust_agent_middlewares::prelude::PermissionMode;
        // 验证所有 5 种模式可正确 store/load
        for mode in [
            PermissionMode::Default,
            PermissionMode::AcceptEdits,
            PermissionMode::Auto,
            PermissionMode::BypassPermissions,
            PermissionMode::DontAsk,
        ] {
            app.permission_mode.store(mode);
            assert_eq!(app.permission_mode.load(), mode, "store/load 应一致: {:?}", mode);
        }
    }

    #[tokio::test]
    async fn test_permission_mode_cycle() {
        let (app, _handle) = App::new_headless(80, 24);
        use rust_agent_middlewares::prelude::PermissionMode;
        // cycle 从 BypassPermissions 开始 → DontAsk
        let next = app.permission_mode.cycle();
        assert_eq!(next, PermissionMode::DontAsk);
        // 继续循环 → Default
        let next2 = app.permission_mode.cycle();
        assert_eq!(next2, PermissionMode::Default);
    }
    ```
  - 运行命令: `cargo test -p rust-agent-tui --lib -- headless::tests::test_app_default_permission_mode_is_bypass headless::tests::test_permission_mode_store_and_load headless::tests::test_permission_mode_cycle`
  - 预期: 所有 3 个测试通过

**检查步骤:**

- [x] 验证 `App` struct 包含 `permission_mode` 字段
  - `grep -n 'permission_mode' rust-agent-tui/src/app/mod.rs`
  - 预期: 结构体定义和 `App::new()` 中均包含 `permission_mode`

- [x] 验证 `AgentRunConfig` 包含 `permission_mode` 字段
  - `grep -n 'permission_mode' rust-agent-tui/src/app/agent.rs`
  - 预期: 结构体定义、参数解构和 `with_shared_mode` 调用处均包含 `permission_mode`

- [x] 验证 `submit_message` 传入 `permission_mode`
  - `grep -n 'permission_mode' rust-agent-tui/src/app/agent_ops.rs`
  - 预期: `AgentRunConfig` 构造处包含 `permission_mode: self.permission_mode.clone()`

- [x] 验证 `main.rs` 包含初始模式设置逻辑
  - `grep -n 'permission_mode\|PermissionMode\|initial_mode' rust-agent-tui/src/main.rs`
  - 预期: 包含 `app.permission_mode.store(initial_mode)`

- [x] 验证 `new_headless` 包含 `permission_mode` 字段
  - `grep -n 'permission_mode' rust-agent-tui/src/app/panel_ops.rs`
  - 预期: `new_headless` 的 `App { ... }` 构造中包含 `permission_mode`

- [x] 验证 `run_universal_agent` 使用 `with_shared_mode` 而非 `from_env`
  - `grep -n 'from_env\|with_shared_mode' rust-agent-tui/src/app/agent.rs`
  - 预期: 包含 `with_shared_mode` 调用，不包含 `from_env` 调用

- [x] 验证编译通过
  - `cargo build -p rust-agent-tui 2>&1 | tail -5`
  - 预期: 构建成功，无错误

- [x] 验证所有 TUI 测试通过（包括新增的 3 个权限模式测试）
  - `cargo test -p rust-agent-tui --lib -- headless::tests 2>&1 | tail -10`
  - 预期: 所有测试通过，输出包含 `test result: ok`

- [x] 验证所有 Workspace 测试无回归
  - `cargo test --workspace 2>&1 | tail -10`
  - 预期: 所有测试通过

---

### Task 5: TUI 集成：Shift+Tab 键绑定与状态栏显示

**背景:**
Task 4 已将 `Arc<SharedPermissionMode>` 注入到 `App` struct，用户可在内存中切换权限模式，但 TUI 界面缺少触发入口和可视化反馈。本 Task 在 `event.rs` 的主键盘匹配块中添加 `Shift+Tab` 键绑定（调用 `SharedPermissionMode::cycle()`），在 `status_bar.rs` 的状态栏中渲染当前模式名称及对应颜色标签，并在 `App` 中新增 `mode_highlight_until: Option<Instant>` 字段控制模式切换后的 1.5 秒闪烁高亮效果。本 Task 依赖 Task 4 注入的 `permission_mode` 字段和 Task 1 产出的 `PermissionMode::display_name()` 方法。本 Task 是 feature F002 的最后一个 Task。

**涉及文件:**
- 修改: `rust-agent-tui/src/event.rs`（主匹配块中新增 `Shift+Tab` 分支）
- 修改: `rust-agent-tui/src/ui/main_ui/status_bar.rs`（消息计数之后插入权限模式标签，添加颜色映射和闪烁高亮逻辑）
- 修改: `rust-agent-tui/src/app/mod.rs`（App struct 新增 `mode_highlight_until` 字段，`App::new()` 和 `new_headless` 初始化）

**执行步骤:**

- [x] 在 `App` struct 中新增 `mode_highlight_until` 字段
  - 位置: `rust-agent-tui/src/app/mod.rs`，`pub struct App` 定义处（~L71-L87），在 `pub permission_mode: Arc<...>` 字段之后（Task 4 已添加该字段）
  - 添加:
    ```rust
    /// 权限模式切换后的闪烁高亮截止时间，None 表示不闪烁
    pub mode_highlight_until: Option<std::time::Instant>,
    ```
  - 原因: 状态栏渲染时检查此字段决定模式标签是否加 BOLD 高亮；`Instant` 单调递增，无需考虑系统时间回拨。

- [x] 修改 `App::new()` 初始化 `mode_highlight_until` 字段
  - 位置: `rust-agent-tui/src/app/mod.rs`，`App::new()` 方法的 `Self { ... }` 构造块（~L148-L163），在 `permission_mode: ...,` 之后添加:
    ```rust
    mode_highlight_until: None,
    ```
  - 原因: 初始状态无闪烁。

- [x] 修改 `new_headless` 初始化 `mode_highlight_until` 字段
  - 位置: `rust-agent-tui/src/app/panel_ops.rs`，`new_headless` 方法的 `App { ... }` 构造处（~L264-L279），在 `setup_wizard: None,` 之后（或在 Task 4 添加的 `permission_mode: ...,` 之后）添加:
    ```rust
    mode_highlight_until: None,
    ```
  - 原因: headless 测试的 `App` 也必须包含此字段，否则编译失败。

- [x] 在 `event.rs` 主匹配块中添加 `Shift+Tab` 键绑定
  - 位置: `rust-agent-tui/src/event.rs`，主 `match input { ... }` 块内（~L212-L362），在 `Input { key: Key::Tab, shift: false, .. }` 分支（~L248-L270，处理 Tab 提示浮层导航）之后，`// Enter 在提示浮层激活时` 分支（~L273）之前，插入新分支:
    ```rust
    // Shift+Tab：循环切换权限模式
    Input {
        key: Key::Tab,
        shift: true,
        ..
    } => {
        let _new_mode = app.permission_mode.cycle();
        app.mode_highlight_until = Some(std::time::Instant::now() + std::time::Duration::from_millis(1500));
    }
    ```
  - 原因: `Shift+Tab` 在主匹配块中（即无弹窗激活时）拦截，与 AskUser 弹窗中的 `Shift+Tab`（~L126，`app.ask_user_prev_tab()`）不冲突——弹窗激活时键盘事件在弹窗分支提前 return（~L157/209），不会进入主匹配块。`cycle()` 返回切换后的模式，此处仅记录高亮截止时间，状态栏在下一帧渲染时自动读取 `app.permission_mode.load()` 获取最新模式。

- [x] 在 `status_bar.rs` 中添加权限模式标签渲染
  - 位置: `rust-agent-tui/src/ui/main_ui/status_bar.rs`，`render_status_bar` 函数（~L12-L149），在"消息计数" spans 块（~L64-L68）之后，"Agent 面板选中信息"块（~L71）之前，插入:
    ```rust
    // 权限模式标签
    {
        use rust_agent_middlewares::prelude::PermissionMode;
        let mode = app.permission_mode.load();
        let (label, color) = match mode {
            PermissionMode::Default           => ("DEFAULT",    theme::TEXT),
            PermissionMode::AcceptEdits       => ("AUTO-EDIT",  theme::SAGE),
            PermissionMode::Auto              => ("AUTO",       theme::LOADING),
            PermissionMode::BypassPermissions => ("YOLO",       theme::WARNING),
            PermissionMode::DontAsk           => ("NO-ASK",     theme::ERROR),
        };
        let is_highlight = app.mode_highlight_until
            .map_or(false, |until| std::time::Instant::now() < until);
        let mut style = Style::default().fg(color);
        if is_highlight {
            style = style.add_modifier(Modifier::BOLD | Modifier::SLOW_BLINK);
        }
        left_spans.push(Span::styled(" │ ", Style::default().fg(theme::MUTED)));
        left_spans.push(Span::styled(format!(" {}", label), style));
    }
    ```
  - 原因: `PermissionMode::display_name()` 已在 Task 1 定义，此处直接使用硬编码 match 确保 label 和 color 的一一对应关系清晰可维护。闪烁效果使用 `SLOW_BLINK` + `BOLD` 修饰符，由 `mode_highlight_until` 的 `Instant` 判断是否激活——`Instant::now() < until` 时为高亮，超时后自动恢复为普通样式，无需额外的定时器或状态重置。颜色映射遵循 spec-design.md 的设计：Default=白(TEXT)、AcceptEdits=绿(SAGE)、Auto=青(LOADING)、BypassPermissions=暖米灰(WARNING)、DontAsk=红(ERROR)。

- [x] 在 `status_bar.rs` 文件顶部添加必要的导入
  - 位置: `rust-agent-tui/src/ui/main_ui/status_bar.rs`，现有 `use ratatui::style::{Modifier, Style};`（~L3）已包含 `Modifier`，无需额外导入
  - `rust_agent_middlewares::prelude::PermissionMode` 在函数体内局部导入（`use rust_agent_middlewares::prelude::PermissionMode;`），避免污染模块级命名空间
  - 原因: 保持导入最小化，`PermissionMode` 仅在 status_bar 渲染中使用。

- [x] 为 Shift+Tab 键绑定和状态栏渲染编写单元测试
  - 测试文件: `rust-agent-tui/src/ui/headless.rs`（现有 headless 测试模块 `mod tests` 块内）
  - 在 `mod tests` 块内追加以下测试:
    ```rust
    #[tokio::test]
    async fn test_status_bar_shows_permission_mode() {
        use rust_agent_middlewares::prelude::PermissionMode;
        let (mut app, mut handle) = App::new_headless(120, 24);
        // 默认 BypassPermissions → 应显示 "YOLO"
        handle.terminal.draw(|f| crate::ui::main_ui::render(f, &mut app)).unwrap();
        assert!(handle.contains("YOLO"), "状态栏应显示 YOLO 模式，实际:\n{}", handle.snapshot().join("\n"));
    }

    #[tokio::test]
    async fn test_status_bar_updates_after_mode_switch() {
        use rust_agent_middlewares::prelude::PermissionMode;
        let (mut app, mut handle) = App::new_headless(120, 24);
        // 切换到 Default
        app.permission_mode.store(PermissionMode::Default);
        handle.terminal.draw(|f| crate::ui::main_ui::render(f, &mut app)).unwrap();
        assert!(handle.contains("DEFAULT"), "切换后状态栏应显示 DEFAULT，实际:\n{}", handle.snapshot().join("\n"));

        // 切换到 AcceptEdits
        app.permission_mode.store(PermissionMode::AcceptEdits);
        handle.terminal.draw(|f| crate::ui::main_ui::render(f, &mut app)).unwrap();
        assert!(handle.contains("AUTO-EDIT"), "切换后状态栏应显示 AUTO-EDIT，实际:\n{}", handle.snapshot().join("\n"));

        // 切换到 Auto
        app.permission_mode.store(PermissionMode::Auto);
        handle.terminal.draw(|f| crate::ui::main_ui::render(f, &mut app)).unwrap();
        assert!(handle.contains("AUTO"), "切换后状态栏应显示 AUTO，实际:\n{}", handle.snapshot().join("\n"));

        // 切换到 DontAsk
        app.permission_mode.store(PermissionMode::DontAsk);
        handle.terminal.draw(|f| crate::ui::main_ui::render(f, &mut app)).unwrap();
        assert!(handle.contains("NO-ASK"), "切换后状态栏应显示 NO-ASK，实际:\n{}", handle.snapshot().join("\n"));
    }

    #[tokio::test]
    async fn test_shift_tab_cycles_permission_mode() {
        use rust_agent_middlewares::prelude::PermissionMode;
        let (mut app, mut handle) = App::new_headless(120, 24);
        // 初始 BypassPermissions
        assert_eq!(app.permission_mode.load(), PermissionMode::BypassPermissions);
        // 模拟 Shift+Tab 按键效果（直接调用 cycle）
        let next = app.permission_mode.cycle();
        assert_eq!(next, PermissionMode::DontAsk, "BypassPermissions 之后应为 DontAsk");
        assert_eq!(app.permission_mode.load(), PermissionMode::DontAsk);
        // 继续循环 4 次回到 BypassPermissions
        app.permission_mode.cycle(); // Default
        app.permission_mode.cycle(); // AcceptEdits
        app.permission_mode.cycle(); // Auto
        let final_mode = app.permission_mode.cycle(); // BypassPermissions
        assert_eq!(final_mode, PermissionMode::BypassPermissions, "循环 5 次回到起点");
    }

    #[tokio::test]
    async fn test_mode_highlight_until_set_on_cycle() {
        let (mut app, _handle) = App::new_headless(120, 24);
        // 初始无闪烁
        assert!(app.mode_highlight_until.is_none(), "初始不应有闪烁");
        // 模拟 Shift+Tab: cycle + 设置 highlight
        app.permission_mode.cycle();
        app.mode_highlight_until = Some(std::time::Instant::now() + std::time::Duration::from_millis(1500));
        assert!(app.mode_highlight_until.is_some(), "cycle 后应设置闪烁截止时间");
        // 验证截止时间在未来
        let until = app.mode_highlight_until.unwrap();
        assert!(std::time::Instant::now() < until, "截止时间应在未来");
    }
    ```
  - 运行命令: `cargo test -p rust-agent-tui --lib -- headless::tests::test_status_bar_shows_permission_mode headless::tests::test_status_bar_updates_after_mode_switch headless::tests::test_shift_tab_cycles_permission_mode headless::tests::test_mode_highlight_until_set_on_cycle`
  - 预期: 所有 4 个测试通过

**检查步骤:**

- [x] 验证 `App` struct 包含 `mode_highlight_until` 字段
  - `grep -n 'mode_highlight_until' rust-agent-tui/src/app/mod.rs`
  - 预期: 结构体定义和 `App::new()` 中均包含 `mode_highlight_until`

- [x] 验证 `new_headless` 包含 `mode_highlight_until` 字段
  - `grep -n 'mode_highlight_until' rust-agent-tui/src/app/panel_ops.rs`
  - 预期: `new_headless` 的 `App { ... }` 构造中包含 `mode_highlight_until: None`

- [x] 验证 `event.rs` 包含 Shift+Tab 键绑定
  - `grep -n 'shift: true' rust-agent-tui/src/event.rs | grep -i tab`
  - 预期: 主匹配块中存在 `key: Key::Tab, shift: true` 分支处理权限模式切换

- [x] 验证 `status_bar.rs` 包含权限模式标签渲染
  - `grep -n 'PermissionMode\|permission_mode' rust-agent-tui/src/ui/main_ui/status_bar.rs`
  - 预期: 包含 `PermissionMode` 的 match 和 `app.permission_mode.load()` 调用

- [x] 验证编译通过
  - `cargo build -p rust-agent-tui 2>&1 | tail -5`
  - 预期: 构建成功，无错误

- [x] 验证新增单元测试全部通过
  - `cargo test -p rust-agent-tui --lib -- headless::tests::test_status_bar_shows_permission_mode headless::tests::test_status_bar_updates_after_mode_switch headless::tests::test_shift_tab_cycles_permission_mode headless::tests::test_mode_highlight_until_set_on_cycle 2>&1 | tail -10`
  - 预期: 4 个测试全部通过，输出包含 `test result: ok`

- [x] 验证所有 Workspace 测试无回归
  - `cargo test --workspace 2>&1 | tail -10`
  - 预期: 所有测试通过

---

### Task 6: Permission Mode 验收

**前置条件:**
- 构建命令: `cargo build --workspace`
- 测试命令: `cargo test --workspace`
- 环境准备: 设置 `ANTHROPIC_API_KEY` 或 `OPENAI_API_KEY` 环境变量（仅 Auto 模式 E2E 需要）

**端到端验证:**

1. 运行完整测试套件确保无回归
   - `cargo test --workspace 2>&1 | tail -20`
   - 预期: 全部测试通过
   - 失败排查: 检查各 Task 的测试步骤，优先排查 Task 3（HITL 改造，影响面最大）

2. 验证 `PermissionMode` 枚举循环和 `SharedPermissionMode` 原子操作
   - `cargo test -p rust-agent-middlewares --lib -- shared_mode::tests 2>&1 | tail -10`
   - 预期: 9 个测试全部通过（next 循环、TryFrom 边界、load/store、cycle 并发安全）
   - 失败排查: 检查 Task 1

3. 验证 `AutoClassifier` 分类逻辑和缓存
   - `cargo test -p rust-agent-middlewares --lib -- auto_classifier::tests 2>&1 | tail -10`
   - 预期: 9 个测试全部通过（ALLOW/DENY/UNSURE 分类、LLM 失败降级、缓存命中/过期）
   - 失败排查: 检查 Task 2

4. 验证 `HumanInTheLoopMiddleware` 5 种模式决策正确性
   - `cargo test -p rust-agent-middlewares --lib -- hitl::tests 2>&1 | tail -10`
   - 预期: 原有 7 个测试 + 新增 ~15 个测试全部通过
   - 失败排查: 检查 Task 3（is_edit_tool、decide_by_mode 各模式分支、process_batch 混合工具）

5. 验证 TUI 编译通过（`disabled()` 和 `new()` 向后兼容）
   - `cargo build -p rust-agent-tui 2>&1 | tail -5`
   - 预期: 构建成功，无编译错误
   - 失败排查: 检查 Task 4（AgentRunConfig 字段、submit_message 构造）

6. 验证 headless 测试中状态栏渲染权限模式标签
   - `cargo test -p rust-agent-tui --lib -- headless::tests 2>&1 | tail -10`
   - 预期: 包含权限模式相关测试全部通过
   - 失败排查: 检查 Task 5（status_bar 渲染、Shift+Tab 键绑定）

7. 验证 YOLO_MODE 环境变量兼容性
   - `YOLO_MODE=true cargo test -p rust-agent-middlewares --lib -- hitl::tests::test_disabled_allows_all 2>&1`
   - 预期: 测试通过（disabled() 行为不变）
   - 失败排查: 检查 Task 3 和 Task 4 的 from_env 兼容逻辑
