# Feature: 20260328_F004 - settings-env-injection

## 需求背景

当前 TUI Agent 通过 `.env` 文件（dotenvy）加载环境变量，存在以下问题：
- `.env` 文件分散，与 `settings.json` 配置割裂
- 已移除 dotenvy 依赖，需要替代方案
- 希望集中管理所有配置（API Key、环境变量、远程控制等）到单一配置文件

## 目标

- 在 `settings.json` 中增加 `env` 字段存储环境变量
- TUI 启动时自动读取并注入环境变量
- 进程环境变量优先于配置文件（允许系统环境变量覆盖配置）

## 方案设计

### 数据模型

在 `AppConfig` 中增加 `env` 字段：

```rust
// rust-agent-tui/src/config/types.rs
use std::collections::HashMap;

pub struct AppConfig {
    // ... 现有字段 ...

    /// 环境变量注入（扁平键值对）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub env: Option<HashMap<String, String>>,
}
```

**配置示例（~/.zen-code/settings.json）：**

```json
{
  "config": {
    "active_alias": "opus",
    "providers": [...],
    "env": {
      "ANTHROPIC_API_KEY": "sk-ant-...",
      "OTEL_EXPORTER_OTLP_ENDPOINT": "http://localhost:4318",
      "RUST_LOG": "debug"
    }
  }
}
```

### 注入逻辑

在 `main.rs` 最开始位置注入环境变量：

```rust
// rust-agent-tui/src/main.rs
fn main() -> Result<()> {
    // 最先注入环境变量（进程环境变量优先）
    inject_env_from_settings();

    // 解析命令行参数
    let args: Vec<String> = std::env::args().collect();
    // ...
}

/// 从 settings.json 读取 env 字段并注入进程环境变量
/// 仅在进程环境变量不存在时设置（进程环境优先）
fn inject_env_from_settings() {
    // 1. 构建 settings.json 路径（~/.zen-code/settings.json）
    // 2. 读取并解析 JSON
    // 3. 提取 config.env 字段
    // 4. 遍历键值对，仅在进程环境变量不存在时设置
}
```

**优先级规则：**
- 进程环境变量 > settings.json env 字段
- 使用 `std::env::var(key).is_err()` 判断不存在，再 `std::env::set_var(key, value)` 设置

### 错误处理

- `settings.json` 不存在：静默跳过，不报错
- `env` 字段缺失：静默跳过
- JSON 解析失败：静默跳过（不影响后续配置加载）
- 环境变量值非 UTF-8：Rust String 类型天然保证 UTF-8，无需额外处理

## 实现要点

1. **修改文件：**
   - `rust-agent-tui/src/config/types.rs`：增加 `env` 字段
   - `rust-agent-tui/src/main.rs`：增加 `inject_env_from_settings()` 函数

2. **不引入新依赖：** 复用 `serde_json` 和 `std::collections::HashMap`

3. **向后兼容：** `env` 字段为 `Option<HashMap>`，缺失时为 `None`，不影响现有配置

4. **更新 constraints.md：** 移除 `.env` 文件相关描述

## 约束一致性

- 符合 `constraints.md` 中「配置持久化: ~/.zen-code/settings.json」的架构决策
- 符合「API Key 安全: 只通过环境变量传递」的安全约束（settings.json 已 gitignore）
- 无架构偏离

## 验收标准

- [ ] `AppConfig` 包含 `env: Option<HashMap<String, String>>` 字段
- [ ] TUI 启动时自动注入 `env` 字段中的环境变量
- [ ] 进程环境变量优先于 settings.json 配置
- [ ] settings.json 不存在或 env 字段缺失时正常启动
- [ ] 更新 `constraints.md` 移除 `.env` 相关描述
- [ ] 单元测试覆盖：serde roundtrip、优先级验证
