### Task 0: 环境准备

**背景:**
确保构建和测试工具链在当前开发环境中可用，避免后续 Task 因环境问题阻塞。本项目包含 Rust workspace 和 Node.js Relay Server 两个独立部分，需分别验证。

**执行步骤:**
- [x] 验证 Rust 工具链可用
  - `cargo --version`
  - 预期: 输出 cargo 版本号（≥1.80）
- [x] 验证 Rust workspace 可编译
  - `cargo check -p peri-tui 2>&1 | tail -5`
  - 预期: 无 error，只有 warnings 或 "Checking peri-tui" 然后成功
- [x] 验证 Node.js 可用
  - `node --version`
  - 预期: 输出 Node.js 版本号（≥18）
- [x] 验证 TypeScript 编译器可用（将通过项目依赖安装）
  - `npx tsc --version`
  - 预期: 输出 TypeScript 版本号

**检查步骤:**
- [x] Rust 构建环境正常
  - `cargo check -p peri-tui 2>&1 | grep -c "error"`
  - 预期: 0
- [x] Node.js 环境正常
  - `node -e "console.log('ok')" 2>&1`
  - 预期: 输出 "ok"

---
