# Task 0: 环境准备

## 背景
确保构建和测试工具链在当前开发环境中可用，避免后续 Task 因环境问题阻塞。

## 执行步骤
- [ ] 验证 Rust 工具链可用: `cargo --version && rustc --version`
- [ ] 验证构建正常: `cargo build -p peri-tui 2>&1 | tail -5`
- [ ] 验证测试框架可用: `cargo test -p peri-tui --lib -- --list 2>&1 | tail -5`
