# TUI Setup Wizard 验收清单

## 场景 1: 环境准备验证

### - [ ] 1.1 构建工具可用性验证
[A] `cargo build -p rust-agent-tui 2>&1 | grep -c "error"`
→ 期望包含: 0
**来源:** spec-plan.md Task 0 检查步骤
**目的:** 验证构建环境配置正确

### - [ ] 1.2 测试工具可用性验证
[A] `cargo test -p rust-agent-tui --lib -- test_thinking_effort_low 2>&1 | grep -c "ok"`
→ 期望包含: ≥ 1
**来源:** spec-plan.md Task 0 检查步骤
**目的:** 验证测试环境配置正确

---

## 场景 2: Setup 检测与数据模型验证

### - [ ] 2.1 模块注册验证
[A] `grep -c "pub mod setup_wizard" rust-agent-tui/src/app/mod.rs`
→ 期望包含: 1
**来源:** spec-plan.md Task 1 检查步骤
**目的:** 验证 setup_wizard 模块正确注册

### - [ ] 2.2 App 结构体字段验证
[A] `grep -c "setup_wizard" rust-agent-tui/src/app/mod.rs`
→ 期望包含: ≥ 3
**来源:** spec-plan.md Task 1 检查步骤
**目的:** 验证 App 结构体包含 setup_wizard 相关字段

### - [ ] 2.3 needs_setup 调用验证
[A] `grep -c "needs_setup" rust-agent-tui/src/main.rs`
→ 期望包含: 1
**来源:** spec-plan.md Task 1 检查步骤
**目的:** 验证 main.rs 正确调用 needs_setup 检测

### - [ ] 2.4 编译验证
[A] `cargo build -p rust-agent-tui 2>&1 | grep -c "error"`
→ 期望包含: 0
**来源:** spec-plan.md Task 1 检查步骤
**目的:** 验证代码编译通过无错误

### - [ ] 2.5 单元测试验证
[A] `cargo test -p rust-agent-tui --lib -- setup_wizard 2>&1 | grep "test result"`
→ 期望包含: ok
**来源:** spec-plan.md Task 1 检查步骤
**目的:** 验证数据模型单元测试通过

---

## 场景 3: Setup 向导 UI 渲染验证

### - [ ] 3.1 渲染文件存在验证
[A] `test -f rust-agent-tui/src/ui/main_ui/popups/setup_wizard.rs && echo OK`
→ 期望包含: OK
**来源:** spec-plan.md Task 2 检查步骤
**目的:** 验证渲染文件已创建

### - [ ] 3.2 popup 模块注册验证
[A] `grep -c "pub mod setup_wizard" rust-agent-tui/src/ui/main_ui/popups/mod.rs`
→ 期望包含: 1
**来源:** spec-plan.md Task 2 检查步骤
**目的:** 验证 popup 模块正确注册

### - [ ] 3.3 main_ui 优先检查验证
[A] `grep -c "setup_wizard" rust-agent-tui/src/ui/main_ui.rs`
→ 期望包含: ≥ 2
**来源:** spec-plan.md Task 2 检查步骤
**目的:** 验证 main_ui 正确拦截 setup_wizard 渲染

### - [ ] 3.4 渲染编译验证
[A] `cargo build -p rust-agent-tui 2>&1 | grep -c "error"`
→ 期望包含: 0
**来源:** spec-plan.md Task 2 检查步骤
**目的:** 验证渲染代码编译通过

### - [ ] 3.5 渲染单元测试验证
[A] `cargo test -p rust-agent-tui --lib -- setup_wizard 2>&1 | grep "test result"`
→ 期望包含: ok
**来源:** spec-plan.md Task 2 检查步骤
**目的:** 验证渲染逻辑单元测试通过

---

## 场景 4: 事件处理与持久化验证

### - [ ] 4.1 event.rs 拦截验证
[A] `grep -c "setup_wizard" rust-agent-tui/src/event.rs`
→ 期望包含: ≥ 3
**来源:** spec-plan.md Task 3 检查步骤
**目的:** 验证事件层正确拦截 setup_wizard

### - [ ] 4.2 按键处理函数验证
[A] `grep -c "pub fn handle_setup_wizard_key\|pub fn save_setup\|pub enum SetupWizardAction" rust-agent-tui/src/app/setup_wizard.rs`
→ 期望包含: 3
**来源:** spec-plan.md Task 3 检查步骤
**目的:** 验证按键处理和保存函数存在

### - [ ] 4.3 App 刷新方法验证
[A] `grep -c "fn refresh_after_setup" rust-agent-tui/src/app/mod.rs`
→ 期望包含: 1
**来源:** spec-plan.md Task 3 检查步骤
**目的:** 验证 App::refresh_after_setup 方法存在

### - [ ] 4.4 new_headless 字段验证
[A] `grep -c "setup_wizard" rust-agent-tui/src/app/panel_ops.rs`
→ 期望包含: 1
**来源:** spec-plan.md Task 3 检查步骤
**目的:** 验证 headless 模式包含 setup_wizard 字段

### - [ ] 4.5 事件处理编译验证
[A] `cargo build -p rust-agent-tui 2>&1 | grep -c "error"`
→ 期望包含: 0
**来源:** spec-plan.md Task 3 检查步骤
**目的:** 验证事件处理代码编译通过

### - [ ] 4.6 事件处理测试验证
[A] `cargo test -p rust-agent-tui --lib -- setup_wizard 2>&1 | grep "test result"`
→ 期望包含: ok
**来源:** spec-plan.md Task 3 检查步骤
**目的:** 验证事件处理单元测试通过

---

## 场景 5: Headless 集成测试验证

### - [ ] 5.1 headless 模块存在验证
[A] `grep -c "mod setup_wizard_e2e" rust-agent-tui/src/ui/headless.rs`
→ 期望包含: 1
**来源:** spec-plan.md Task 4 检查步骤
**目的:** 验证 headless 集成测试模块已添加

### - [ ] 5.2 save_setup_to 函数验证
[A] `grep -c "pub fn save_setup_to" rust-agent-tui/src/app/setup_wizard.rs`
→ 期望包含: 1
**来源:** spec-plan.md Task 4 检查步骤
**目的:** 验证测试用保存函数存在

### - [ ] 5.3 集成测试编译验证
[A] `cargo build -p rust-agent-tui 2>&1 | grep -c "error"`
→ 期望包含: 0
**来源:** spec-plan.md Task 4 检查步骤
**目的:** 验证集成测试代码编译通过

### - [ ] 5.4 setup_wizard 测试套件验证
[A] `cargo test -p rust-agent-tui --lib -- setup_wizard 2>&1 | grep "test result"`
→ 期望包含: ok (且 passed ≥ 10)
**来源:** spec-plan.md Task 4 检查步骤
**目的:** 验证所有 setup_wizard 测试通过

### - [ ] 5.5 headless 测试套件无回归验证
[A] `cargo test -p rust-agent-tui --lib -- ui::headless 2>&1 | grep "test result"`
→ 期望包含: ok
**来源:** spec-plan.md Task 4 检查步骤
**目的:** 验证原有 headless 测试无回归

---

## 场景 6: TUI 端到端验收验证

### - [ ] 6.1 完整测试套件验证
[A] `cargo test -p rust-agent-tui --lib 2>&1 | grep "test result"`
→ 期望包含: ok (0 failed)
**来源:** spec-plan.md Task 5 端到端验证
**目的:** 验证所有单元测试通过无回归

### - [ ] 6.2 Headless 端到端测试验证
[A] `cargo test -p rust-agent-tui --lib -- setup_wizard_e2e 2>&1 | grep "test result"`
→ 期望包含: ok (8 个测试)
**来源:** spec-plan.md Task 5 端到端验证
**目的:** 验证 8 个集成测试全部通过

### - [ ] 6.3 首次启动触发验证
[A] `cargo test -p rust-agent-tui --lib -- test_needs_setup_triggers_for_empty_config 2>&1 | grep "ok"`
→ 期望包含: ok
**来源:** spec-plan.md Task 5 端到端验证
**目的:** 验证首次启动自动触发 setup 向导

### - [ ] 6.4 配置完成不再触发验证
[A] `cargo test -p rust-agent-tui --lib -- test_setup_wizard_saves_and_clears 2>&1 | grep "ok"`
→ 期望包含: ok
**来源:** spec-plan.md Task 5 端到端验证
**目的:** 验证配置完成后不再触发 setup

### - [ ] 6.5 跳过二次确认验证
[A] `cargo test -p rust-agent-tui --lib -- test_setup_wizard_skip_with_confirm 2>&1 | grep "ok"`
→ 期望包含: ok
**来源:** spec-plan.md Task 5 端到端验证
**目的:** 验证跳过 setup 时二次确认

### - [ ] 6.6 编译无警告验证
[A] `cargo build -p rust-agent-tui 2>&1 | grep -c "warning"`
→ 期望包含: 0 (setup_wizard 相关)
**来源:** spec-plan.md Task 5 端到端验证
**目的:** 验证编译无警告

---

## 场景 7: 设计文档业务验收

### - [ ] 7.1 首次启动自动触发
[H] 打开 TUI 应用 → 观察是否自动弹出全屏 setup 向导（无 settings.json 或 providers 为空时）
→ 是/否
**来源:** spec-design.md 验收标准
**目的:** 验证首次启动自动引导

### - [ ] 7.2 API Key 缺失时触发
[H] 打开有 provider 但无 API Key 的配置 → 观察是否触发 setup 向导
→ 是/否
**来源:** spec-design.md 验收标准
**目的:** 验证配置不完整时自动触发

### - [ ] 7.3 三步流程导航验证
[H] 操作 Tab/Enter/Esc → 观察步骤切换是否正常（Provider → API Key → 模型别名 → 完成）
→ 是/否
**来源:** spec-design.md 验收标准
**目的:** 验证三步流程导航正常

### - [ ] 7.4 Anthropic 自动填充验证
[H] 选择 Anthropic → 观察 base_url 是否自动填充为只读
→ 是/否
**来源:** spec-design.md 验收标准
**目的:** 验证 Anthropic 选项自动填充

### - [ ] 7.5 OpenAI 手动填写验证
[H] 选择 OpenAI Compatible → 观察 base_url 是否可编辑
→ 是/否
**来源:** spec-design.md 验收标准
**目的:** 验证 OpenAI 手动填写 base_url

### - [ ] 7.6 API Key 掩码显示验证
[H] 在 Step 2 输入 API Key → 观察是否显示为 ••••••••
→ 是/否
**来源:** spec-design.md 验收标准
**目的:** 验证 API Key 密码模式显示

### - [ ] 7.7 模型别名必填验证
[H] 留空任一 model_id → 按 Enter → 观察是否阻止进入下一步
→ 是/否
**来源:** spec-design.md 验收标准
**目的:** 验证三个模型别名必填

### - [ ] 7.8 配置写入验证
[H] 完成 setup → 打开 `~/.zen-code/settings.json` → 观察是否包含新配置
→ 是/否
**来源:** spec-design.md 验收标准
**目的:** 验证配置正确写入文件

### - [ ] 7.9 内存状态即时刷新验证
[H] 完成 setup → 观察 TUI 状态栏/provider_name/model_name 是否更新
→ 是/否
**来源:** spec-design.md 验收标准
**目的:** 验证配置即时生效

### - [ ] 7.10 跳过二次确认验证
[H] 在 Step 1 按 Esc → 观察是否弹出确认提示 → 按 Enter 确认跳过
→ 是/否
**来源:** spec-design.md 验收标准
**目的:** 验证跳过 setup 时二次确认

### - [ ] 7.11 完成后不再触发验证
[H] 完成 setup 后重启 TUI → 观察是否直接进入对话界面（不弹出 setup）
→ 是/否
**来源:** spec-design.md 验收标准
**目的:** 验证配置完整后不再触发

### - [ ] 7.12 Headless 代码驱动验证
[H] 运行 `cargo test -p rust-agent-tui --lib -- setup_wizard_e2e` → 观察所有测试是否通过
→ 是/否
**来源:** spec-design.md 验收标准
**目的:** 验证 headless 模式下可代码驱动完成

---

## 场景 8: 边界与回归验证

### - [ ] 8.1 现有面板功能回归验证
[H] 按 `/model` 打开模型面板 → 观察是否正常显示和工作
→ 是/否
**来源:** spec-design.md "面板模式一致"
**目的:** 验证现有面板功能未受影响

### - [ ] 8.2 Relay 面板功能回归验证
[H] 按 `/relay` 打开 Relay 面板 → 观察是否正常显示和工作
→ 是/否
**来源:** spec-design.md "面板模式一致"
**目的:** 验证 Relay 面板功能未受影响

### - [ ] 8.3 HITL 功能回归验证
[H] 触发 HITL 审批 → 观察弹窗是否正常显示和响应
→ 是/否
**来源:** CLAUDE.md "事件处理集成"
**目的:** 验证 HITL 功能未受影响

### - [ ] 8.4 AskUser 功能回归验证
[H] 触发 ask_user_question → 观察弹窗是否正常显示和响应
→ 是/否
**来源:** CLAUDE.md "事件处理集成"
**目的:** 验证 AskUser 功能未受影响

### - [ ] 8.5 配置文件格式兼容验证
[H] 打开现有的 settings.json → 观察字段结构是否正确
→ 是/否
**来源:** spec-design.md "配置持久化"
**目的:** 验证配置文件格式兼容性

### - [ ] 8.6 环境变量回退验证
[H] 删除 API Key → 设置 `ANTHROPIC_API_KEY` → 观察是否不触发 setup
→ 是/否
**来源:** spec-plan.md "needs_setup 逻辑"
**目的:** 验证环境变量回退机制

### - [ ] 8.7 空字段校验边界验证
[H] 清空 provider_id/base_url/api_key → 按 Enter → 观察是否阻止前进
→ 是/否
**来源:** spec-plan.md "空字段校验"
**目的:** 验证空字段校验边界情况

### - [ ] 8.8 Esc 返回保留内容验证
[H] 填写 Step 2 → Esc → 回到 Step 1 → 再次 Enter → 观察已填内容是否保留
→ 是/否
**来源:** spec-design.md "按键处理"
**目的:** 验证 Esc 返回时内容保留

### - [ ] 8.9 Provider 切换默认值验证
[H] 选择 Anthropic → 填写自定义字段 → 切换到 OpenAI → 观察默认值是否刷新
→ 是/否
**来源:** spec-plan.md "refresh_provider_defaults"
**目的:** 验证 Provider 切换默认值刷新

### - [ ] 8.10 多配置文件场景验证
[H] 删除 settings.json → 运行 setup → 再次运行 TUI → 观察是否直接进入对话
→ 是/否
**来源:** spec-design.md "首次启动检测"
**目的:** 验证多配置文件场景正确性

---

## 验收后清理

[AUTO] 如在验收过程中创建了临时配置文件或修改了现有配置：
```bash
# 恢复原有配置（如有备份）
cp ~/.zen-code/settings.json.backup ~/.zen-code/settings.json 2>/dev/null || echo "无备份需要恢复"
```

[AUTO] 如在验收过程中创建了临时测试目录：
```bash
# 清理临时测试目录（路径基于测试中实际创建）
rm -rf /tmp/zen-setup-test-* 2>/dev/null || echo "无临时目录需要清理"
```

---

## 验收清单说明

**类型标记:**
- [A]: 自动化验证 - 可通过命令自动检查
- [H]: 人工验证 - 需要人工观察和判断

**匹配模式:**
- `→ 期望包含:` 子字符串匹配（默认）
- `→ 期望精确:` 精确匹配

**场景分类:**
- 场景 1-5: 基于 spec-plan.md 的执行步骤验证（主要是自动化）
- 场景 6: TUI 端到端验收验证（混合自动+人工）
- 场景 7: 基于 spec-design.md 的业务验收标准（主要是人工）
- 场景 8: 边界情况与回归验证（主要是人工）
