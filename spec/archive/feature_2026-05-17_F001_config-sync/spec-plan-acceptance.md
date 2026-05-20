### Acceptance Task: 配置同步 验收

**前置条件:**
- 所有 Task 已完成
- Rust workspace 编译通过
- Relay Server 及其依赖已安装
- 启动命令: `cargo run -p peri-tui -- sync sender --server ws://localhost:8080`

**端到端验证:**

1. 运行完整测试套件确保无回归
   - `cargo test -p peri-tui 2>&1 | tail -20`
   - 预期: 全部测试通过，"test result: ok"
   - 失败排查: 检查各 Task 的测试步骤，优先排查新增 sync 模块测试

2. Relay Server 可启动，健康检查可用
   - `cd side-projects/peri-sync/server && npx tsx src/index.ts & sleep 2 && curl -s http://localhost:8080/health`
   - 预期: 返回 status 200 且 body 包含 "ok"
   - 失败排查: 检查 Task 1（pair-manager/relay/index）

3. `peri sync --help` 子命令可见
   - `cargo run -p peri-tui -- sync --help`
   - 预期: 输出包含 "Sync" / "配置同步" / "sender" / "receiver" 等关键词
   - 失败排查: 检查 Task 5 中 main.rs Commands 枚举修改

4. `peri sync sender` 可启动并请求配对码
   - 先启动 relay: `npx tsx side-projects/peri-sync/server/src/index.ts &`
   - 然后: `echo "" | timeout 5 cargo run -p peri-tui -- sync sender --server ws://localhost:8080`
   - 预期: 输出 "Your pair code:" 后跟 6 位数字，或超时无 panic
   - 失败排查: 检查 Task 2（protocol）+ Task 3（scanner/packer）+ Task 5（sender）

5. `peri sync receiver` 可启动并等待输入配对码
   - 输入配对码后显示同步项选择列表
   - 预期: 终端展示勾选列表（settings/skills/mcp/plugins）
   - 失败排查: 检查 Task 5（receiver/ui）

6. 端到端同步：sender → relay → receiver 完整流程
   - 同时启动 sender 和 receiver，确认 relay 正确转发消息
   - 预期: sender 打包加密后通过 relay 透传，receiver 解密写入目标路径
   - 失败排查: 检查 Task 2（crypto）+ Task 5（sender/receiver flow）+ Task 4（writer）

7. 路径穿越防护验证（安全关键）
   - 手动构造包含 `../` 和绝对路径的 SyncItems，调用 `validate_path`
   - 预期: 返回 `PathTraversal` 错误
   - 失败排查: 检查 Task 4 中 validate_and_resolve 的三层校验逻辑

8. Rust workspace 全量编译无错误
   - `cargo build -p peri-tui 2>&1 | grep "^error"`
   - 预期: 无输出（无编译错误）
   - 失败排查: 检查各 Task 是否存在类型不匹配或导入错误

---
