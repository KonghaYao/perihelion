# settings-env-injection 人工验收清单

**生成时间:** 2026-03-29
**关联计划:** ./spec-plan.md
**关联设计:** ./spec-design.md

---

## 验收前准备

### 环境要求
- [ ] [AUTO] 检查 Rust 工具链: `rustc --version && cargo --version`
- [ ] [AUTO] 编译 release 版本: `cargo build -p rust-agent-tui --release`
- [ ] [AUTO] 备份现有配置（如存在）: `test -f ~/.zen-code/settings.json && cp ~/.zen-code/settings.json ~/.zen-code/settings.json.backup || true`

---

## 验收项目

### 场景 1：编译与单元测试

#### - [x] 1.1 编译通过验证
- **来源:** Task 1 检查步骤
- **操作步骤:**
  1. [A] `cargo build -p rust-agent-tui 2>&1 | tail -5` → 期望: 输出包含 "Finished" 且无 error
- **异常排查:**
  - 如果出现编译错误: 检查 `rust-agent-tui/src/config/types.rs` 中 `env` 字段定义

#### - [x] 1.2 serde 反序列化测试
- **来源:** Task 1/3 检查步骤
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-tui --lib test_app_config_env 2>&1 | tail -15` → 期望: 3 个测试通过
- **异常排查:**
  - 如果测试失败: 检查 `types.rs` 中 `test_app_config_env_*` 系列测试代码

#### - [x] 1.3 dotenvy 移除验证
- **来源:** Task 2 检查步骤
- **操作步骤:**
  1. [A] `grep -n "dotenvy" rust-agent-tui/src/main.rs` → 期望: 无匹配输出
- **异常排查:**
  - 如果有匹配: 移除 main.rs 中的 dotenvy 相关代码

#### - [x] 1.4 env 字段单元测试
- **来源:** Task 3 检查步骤
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-tui --lib test_app_config_env 2>&1 | grep -E "^test|^running|ok|FAILED"` → 期望: 显示 3 个 test ... ok
  2. [A] `cargo test -p rust-agent-tui --bin agent-tui test_env_priority 2>&1 | grep -E "^test|^running|ok|FAILED"` → 期望: 显示 1 个 test ... ok
- **异常排查:**
  - types.rs 测试失败: 检查 serde 序列化/反序列化逻辑
  - main.rs 测试失败: 检查 `inject_env_from_settings()` 函数优先级逻辑

---

### 场景 2：文档更新

#### - [x] 2.1 `.env` 描述移除验证
- **来源:** Task 4 检查步骤
- **操作步骤:**
  1. [A] `grep -n "\.env" spec/global/constraints.md` → 期望: 无匹配输出
- **异常排查:**
  - 如果有匹配: 确认是否为历史说明，否则移除 `.env` 相关描述

#### - [x] 2.2 `env` 字段描述存在验证
- **来源:** Task 4 检查步骤
- **操作步骤:**
  1. [A] `grep -n "env" spec/global/constraints.md | head -5` → 期望: 输出包含 `settings.json` 和 `env` 字段描述
- **异常排查:**
  - 如果无匹配: 检查 constraints.md 中部署方式和安全约束章节

---

### 场景 3：端到端功能验证

#### - [x] 3.1 env 字段注入验证
- **来源:** Task 5 End-to-end verification
- **操作步骤:**
  1. [A] `cat > ~/.zen-code/settings.json << 'EOF'
{
  "config": {
    "active_alias": "opus",
    "providers": [],
    "env": {
      "TEST_ENV_VAR_123": "test_value_456"
    }
  }
}
EOF` → 期望: 文件创建成功（无输出）
  2. [A] `TEST_ENV_VAR_123="" ./target/release/agent-tui --help 2>&1; echo "Exit code: $?"` → 期望: Exit code 为 0（无 panic）
- **异常排查:**
  - 如果 panic: 检查 `inject_env_from_settings()` 函数的 JSON 解析逻辑

#### - [x] 3.2 进程环境变量优先级验证
- **来源:** Task 5 End-to-end verification
- **操作步骤:**
  1. [A] `TEST_ENV_VAR_123="from_process" ./target/release/agent-tui --help 2>&1; echo "Exit code: $?"` → 期望: Exit code 为 0（进程环境变量不被覆盖）
  2. [A] `cargo test -p rust-agent-tui --bin agent-tui test_env_priority 2>&1 | tail -5` → 期望: 测试通过
- **异常排查:**
  - 如果优先级错误: 检查 `std::env::var(key).is_err()` 判断逻辑

#### - [x] 3.3 静默跳过验证
- **来源:** Task 5 End-to-end verification
- **操作步骤:**
  1. [A] `mv ~/.zen-code/settings.json ~/.zen-code/settings.json.bak 2>/dev/null; ./target/release/agent-tui --help 2>&1; echo "Exit code: $?"; mv ~/.zen-code/settings.json.bak ~/.zen-code/settings.json 2>/dev/null; true` → 期望: Exit code 为 0（settings.json 缺失时正常启动）
  2. [A] `cat > ~/.zen-code/settings.json << 'EOF'
{
  "config": {
    "active_alias": "opus",
    "providers": []
  }
}
EOF
./target/release/agent-tui --help 2>&1; echo "Exit code: $?"` → 期望: Exit code 为 0（env 字段缺失时正常启动）
- **异常排查:**
  - 如果 panic: 检查 `inject_env_from_settings()` 函数的错误处理逻辑（应静默跳过）

---

## 验收后清理

- [ ] [AUTO] 恢复原配置: `test -f ~/.zen-code/settings.json.backup && mv ~/.zen-code/settings.json.backup ~/.zen-code/settings.json || rm -f ~/.zen-code/settings.json`
- [ ] [AUTO] 验证清理完成: `ls -la ~/.zen-code/`

---

## 验收结果汇总

| 场景 | 序号 | 验收项 | 自动步骤 | 人工步骤 | 结果 | 备注 |
|------|------|--------|----------|----------|------|------|
| 场景 1 | 1.1 | 编译通过验证 | 1 | 0 | ⬜ | |
| 场景 1 | 1.2 | serde 反序列化测试 | 1 | 0 | ⬜ | |
| 场景 1 | 1.3 | dotenvy 移除验证 | 1 | 0 | ⬜ | |
| 场景 1 | 1.4 | env 字段单元测试 | 2 | 0 | ⬜ | |
| 场景 2 | 2.1 | `.env` 描述移除验证 | 1 | 0 | ⬜ | |
| 场景 2 | 2.2 | `env` 字段描述存在验证 | 1 | 0 | ⬜ | |
| 场景 3 | 3.1 | env 字段注入验证 | 2 | 0 | ⬜ | |
| 场景 3 | 3.2 | 进程环境变量优先级 | 2 | 0 | ⬜ | |
| 场景 3 | 3.3 | 静默跳过验证 | 2 | 0 | ⬜ | |

**验收结论:** ⬜ 全部通过 / ⬜ 存在问题
