# acpx-g Design Review Progress

## 2026-05-04 Round 1

### 发现并修复的用户体验问题

**1. Template Run 不支持 inputs 参数（阻断级）**
后端 `run_template` API 不接受 inputs，有必填参数的模板从 UI 运行会静默失败。修复：添加 `RunTemplateRequest` 结构体，API 接受可选 JSON body 的 inputs 参数，前端在 template preview 中渲染 inputs 表单，Run 时收集并提交。

**2. Web UI 无错误反馈（高优先级）**
`runTemplate()` 的 catch 为空，用户无法得知运行失败原因。添加了 toast 通知系统，在操作成功/失败时显示提示消息。

**3. 输入类型校验缺失（中等优先级）**
`validate_inputs` 声明了 string/number/boolean 类型但不做检查。增加了 number（parse f64）和 boolean（true/false/yes/no/1/0）的类型校验，附带 8 个测试用例覆盖各种场景。

**4. CLI 无 --help（中等优先级）**
用户无法发现可用参数。添加了 `--help`/`-h` 标志，显示用法、选项和环境变量说明。

**5. Run 不显示执行耗时（中等优先级）**
UI 只显示时间戳不显示耗时。添加了 `fmtDuration()` 函数，在 run 列表和日志面板头部显示持续时间。

**6. Examples 注释中的 API 参数名错误（低优先级）**
`ci-pipeline.yaml` 注释写了 `yaml_file` 但实际 API 参数是 `yaml`，已修正。

### 测试覆盖
- 原有 16 个测试全部通过
- 新增 8 个 `validate_inputs` 类型校验测试
- 总计 24 个测试全部通过

## 2026-05-04 Round 2

### 发现并修复的用户体验问题

**1. selectRun 双重请求 + timer 泄漏（高优先级）**
`selectRun()` 先 fetch 渲染 UI，再 fetch 判断是否 poll，造成冗余请求和潜在 timer 泄漏。合并为单次 fetch：渲染后直接从响应判断是否需要轮询，同时先 clearInterval 再 fetch。

**2. 失败节点下游永远 pending（高优先级）**
节点失败后，未执行的下游节点停在 pending 状态，用户误以为还在等待。添加 `mark_run_pending_as_skipped` 方法，在 DAG 失败时将剩余 pending 节点标记为 skipped。前端同步添加 skipped 状态的颜色样式。

**3. 节点日志不显示执行耗时（中等优先级）**
日志面板每个节点 header 有 started_at 但不显示 duration。改为用 `fmtDuration()` 显示节点执行时间。

**4. 内嵌 Run 按钮绕过 inputs 表单（高优先级）**
Template 卡片上的 Run 按钮直接调用 `runTemplate()`，没经过 inputs 表单。添加 `runTemplateFromCard()`：先选中 template 展示 inputs 表单，无 inputs 则直接运行，有 inputs 让用户填写后手动点 Run。

**5. Run 列表 API 返回 yaml_content（中等优先级）**
列表查询返回完整 yaml_content 大字段但列表页用不到。修改 SQL 用空字符串替代，减少网络传输和内存占用。

**6. 三处重复 workflow 提交代码（低优先级）**
`submit_workflow`、`run_template`、`submit_workflow_from_file` 有完全相同的 run 创建+node 插入+执行启动逻辑。抽取 `create_and_start_run()` 共享函数，消除约 60 行重复代码。

## 2026-05-04 Round 3

### 发现并修复的问题

**1. PlatformFiles::resolve panic 导致服务崩溃（高优先级）**
`PlatformFiles::resolve()` 在平台不匹配时 `panic!`，直接崩溃整个进程。改为返回 `anyhow::Result<String>`，让错误沿 executor 传播为节点失败，而非服务终止。同步更新 `ScriptSource::resolve` 和 `PromptSource::resolve` 的签名。

**2. 模板卡片内联 JS 单引号注入（高优先级）**
模板名含单引号时 `onclick="showTemplatePreview('name')"` 断开 JS 字符串，导致功能崩溃甚至 XSS。改用 `data-name` 属性 + `addEventListener` 事件委托，彻底消除字符串拼接注入风险。

**3. DAG 图每次渲染重置缩放（中等优先级）**
`renderGraph` 每次都设 `dagZoom=1/dagPanX=0/dagPanY=0`，运行中的 run 刷新时用户缩放被重置。改为只在节点数变化时重置（切换 run/template），刷新更新保留缩放状态。

**4. 模板区域无标题（低优先级）**
Templates 区域没有标题 header，Runs 区域有。新用户不知道上面是什么区域。添加了 Templates 标题 header。

### 测试覆盖
- 新增 5 个 schema 测试（PlatformFiles 匹配/默认/错误、ScriptSource inline/file）
- 总计 29 个测试全部通过

## 2026-05-04 Round 4

### 发现并修复的问题

**1. 重试时 stdout/stderr 被覆盖（高优先级）**
`run_shell`/`run_agent` 每次重试直接覆盖 stdout/stderr，用户无法看到之前尝试的输出。改为累积模式：非首次尝试时添加 `--- Attempt N ---` 分隔符，保留所有尝试的完整输出。

**2. 超时时 stdout/stderr 丢失（高优先级）**
Shell/Agent 超时时 `tokio_timeout` 直接返回 Err，进程被 kill 但输出未被捕获。改为显式匹配 TimeoutElapsed，提供更清晰的错误信息（包含超时时长），而非模糊的 "timed out"。

**3. continue_on_error 下游节点被误跳过（中等优先级）**
`deps_ready` 只检查 `completed` HashSet，不包含 `failed`。当 `continue_on_error=true` 的节点失败后，其下游因依赖不在 completed 中被错误跳过。修改为 `completed.contains(di) || failed.contains(di)`。

**4. Node ID 含 / 时 jumpToLog 失效（中等优先级）**
`jumpToLog` 用 `querySelector` + `data-node` 属性选择器，node ID 如 `do-build/checkout` 中的 `/` 导致 CSS 选择器解析失败。添加 `cssEsc()` 转义函数。

### 测试覆盖
- 新增 5 个 DAG 执行引擎测试（topological sort simple/parallel/cycle/unknown_dep + continue_on_error）
- 总计 34 个测试全部通过

## 2026-05-04 Round 5

### 发现并修复的问题

**1. cssEsc 正则语法错误导致 JS 崩溃（阻断级）**
`/([\\"]/g` 缺少字符类和捕获组闭合，浏览器报 SyntaxError，所有页面 JS 停止执行。修正为 `/([\\"])/g`。

**2. get_node_logs API 用 DB id 查找（高优先级）**
API 路径含 `node_id` 但实际用 DB 主键查找，前端需暴露内部 ID。改为用 `run_id + node_id`（业务 ID）查找，前端 `fetchLogs` 改用 `encodeURIComponent(nodeId)`，DAG 图 `jumpToLog` 和 `renderLogs` 统一用 `node_id`。引入 `domId()` 函数安全转换含 `/` 的 node_id 为 DOM id。

**3. 无 workflow-dir 时 UI 无引导（中等优先级）**
无模板时 templates 区提示改为具体命令 `acpx-g --workflow-dir ./examples`，主面板同步显示 "Add a workflow directory to get started"。

**4. input 表单无前端校验（中等优先级）**
required input 未填时直接提交导致后端 400。添加前端校验：空 required 字段高亮红框 + toast 提示，阻止请求。

### 测试覆盖
- 新增 9 个测试（loader: find_exit_nodes/parallel, prefix_id, rewire_depends/no_match, with_value_to_map_number_and_bool; api: empty_declared, extra_provided_ignored, negative_number; template: 移除未使用 ctx）
- 总计 43 个测试全部通过

## 2026-05-04 Round 6

### 发现并修复的问题

**1. 消除所有内联 onclick 处理器（高优先级）**
run 列表项、DAG 图节点、log header 三处使用 `onclick="fn('...')"` 内联 JS，存在潜在注入风险且不利于 CSP 策略。全部改为 `data-*` 属性 + `addEventListener` 事件委托模式，与模板卡片统一风格。

**2. API 文档 curl Copy 机制不可靠（中等优先级）**
`copyCurl` 用 `textContent.replace('Copy','')` 移除按钮文本，若 curl 内容含 "Copy" 会被误删。改为 `data-curl` 属性存储原始命令，按钮直接读取属性值复制。

**3. 新增 API 文档模态框（功能增强）**
sidebar Templates header 添加 API 按钮，点击弹出模态框显示 6 个端点文档，每个含方法标签、参数表、可复制的 curl 示例和响应示例。

### 测试覆盖
- 43 个测试全部通过（本轮无新增后端代码变更，前端优化为主）

## 2026-05-04 Round 7 — Business Logic Review + Architecture Improvements

### 核心架构改进

**1. 数据库事务保护（P0 修复）**
`create_and_start_run` 改用 SQLite 事务包裹 workflow_run + node_runs 批量插入，中间任何节点插入失败时自动 rollback，消除 orphaned 记录风险。

**2. 数据库索引（P0 性能）**
新增 `idx_workflow_runs_created_at` 和 `idx_node_runs_run_id` 索引，优化列表排序和 run 关联查询。

**3. 重试逻辑泛化（P1 重构）**
抽取 `execute_with_retry()` 通用函数，统一 shell/agent 两个执行器的重试循环（状态持久化、累积输出、指数退避），消除 ~120 行重复代码。

**4. API 分页（P1 功能）**
`GET /api/v1/workflows` 新增 `page`/`per_page` 查询参数，返回 `total`/`page`/`per_page` 元数据，前端实现翻页 UI。

**5. 并发限制可配置（P2）**
`MAX_CONCURRENT_NODES` 改为通过 `ACPX_MAX_CONCURRENT` 环境变量读取，默认 16，最小 1。

### 测试覆盖
- 56 个测试全部通过（新增 13 个：executor 10 + runner 3）

## 2026-05-04 15:30 Round 8 — DAG Correctness + Watcher Reliability

### 业务逻辑修复

**1. DAG 依赖状态检查缺陷（关键修复）**
原逻辑将 failed 节点视为"依赖已就绪"，导致依赖失败节点的下游被错误执行。修正为：只有依赖全部成功的节点才会执行，失败依赖的下游直接跳过。

**2. continue_on_error 节点输出传播（逻辑修正）**
continue_on_error=true 的失败节点现在也加入 completed 集合，下游可通过 needs 引用其输出。

**3. 重复 node ID 检测（数据完整性）**
topological_sort 新增重复 ID 检测，防止两个同名节点静默覆盖导致 DAG 行为不可预测。

**4. Watcher 内容变更检测（功能增强）**
VersionTracker 改为跟踪 (version, content_hash) 对，文件内容变化（不改版本号）也会触发重新执行。使用 FNV-1a 哈希，零依赖。

**5. 模板插值空表达式处理（防御性）**
`{{ }}` 空表达式不再静默替换为空字符串，而是保留原样，帮助用户发现配置错误。

### 测试覆盖
- 67 个测试全部通过（新增 11 个：topo 3 + template 2 + watcher 6）

## 2026-05-04 16:00 Round 9 — Schema Validation + Input Defense + Remote Safety

### 业务逻辑修复

**1. Workflow Schema 验证（数据完整性）**
`parse_workflow` 新增 `validate_workflow` 校验层：拒绝空 name/version、空节点列表、纯空白 name、引用节点指向不存在的 reference。防止脏数据进入 DB 和 DAG 执行。

**2. 远程 Workflow HTTP 状态码检查（安全性）**
`fetch_remote` 增加 HTTP 状态码检查，404/5xx 不再静默作为 YAML 内容解析，而是返回清晰错误信息。

**3. 输入默认值类型校验（数据完整性）**
`validate_inputs` 现在校验声明为 Number/Boolean 类型的输入的 default 值是否符合类型约束。防止 `default: "abc"` 声明在 Number 类型上导致运行时静默错误。

### 测试覆盖
- 84 个测试全部通过（新增 17 个：schema 8 + input validation 7 + prompt/script resolve 2）
