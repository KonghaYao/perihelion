### Task 1: Relay Server（Hono.js WebSocket 中继服务）

**背景:**
本 Task 实现配置同步的 Relay Server 中继服务，负责 WebSocket 连接管理、配对码生成与校验、消息密文透传。Relay Server 不存储任何用户数据，不解析加密内容，仅作为 sender 与 receiver 之间的中转站。本 Task 的输出是独立可运行的服务，后续 Task（Rust 客户端）通过它完成端到端同步。本 Task 是全新独立项目，不依赖任何前置 Task。

**涉及文件:**
- 新建: `side-projects/peri-sync/server/package.json`
- 新建: `side-projects/peri-sync/server/tsconfig.json`
- 新建: `side-projects/peri-sync/server/src/types.ts`
- 新建: `side-projects/peri-sync/server/src/pair-manager.ts`
- 新建: `side-projects/peri-sync/server/src/relay.ts`
- 新建: `side-projects/peri-sync/server/src/index.ts`

**执行步骤:**
- [x] 创建项目 package.json
  - 位置: `side-projects/peri-sync/server/package.json`（新建）
  - 内容：`{"name": "peri-sync-server", "version": "1.0.0", "private": true, "scripts": {"dev": "bun run src/index.ts", "test": "bun test src/pair-manager.test.ts"}, "dependencies": {"hono": "^4"}}`
  - 参考 `side-projects/pty-server/package.json` 的 `"private": true` 和 bun 运行脚本模式

- [x] 创建 TypeScript 配置
  - 位置: `side-projects/peri-sync/server/tsconfig.json`（新建）
  - 内容：`{"compilerOptions": {"target": "ESNext", "module": "ESNext", "moduleResolution": "bundler", "types": ["bun"], "strict": true, "noEmit": true, "skipLibCheck": true}, "include": ["src/**/*.ts"]}`
  - 与 `side-projects/llm-gateway/tsconfig.json` 配置一致，include 改为 `src/**/*.ts`

- [x] 定义 WebSocket 消息类型
  - 位置: `side-projects/peri-sync/server/src/types.ts`（新建）
  - 定义枚举和接口：
    ```typescript
    // Client → Server 消息类型
    export interface RequestPairMessage { type: "request_pair" }
    export interface JoinPairMessage { type: "join_pair"; pair_code: string }
    export interface SyncConfigMessage { type: "sync_config"; payload: unknown }
    export interface DataChunkMessage { type: "data_chunk"; seq: number; data: number[] } // JSON 序列化后为 number[]
    export interface TransferCompleteMessage { type: "transfer_complete"; checksum: string }

    // Server → Client 消息类型
    export interface PairCreatedMessage { type: "pair_created"; pair_code: string }
    export interface PairJoinedMessage { type: "pair_joined"; peer_info?: string }
    export interface ErrorMessage { type: "error"; code: string; message: string }

    // 联合类型
    export type WsClientMessage = RequestPairMessage | JoinPairMessage | SyncConfigMessage | DataChunkMessage | TransferCompleteMessage
    export type WsServerMessage = PairCreatedMessage | PairJoinedMessage | DataChunkMessage | TransferCompleteMessage | ErrorMessage
    ```
  - 因 data_chunk 和 transfer_complete 同时出现在客户端和服务端消息中，需将 `DataChunkMessage` 与 `TransferCompleteMessage` 放入两个联合类型中

- [x] 实现配对码管理器（pair-manager.ts）
  - 位置: `side-projects/peri-sync/server/src/pair-manager.ts`（新建）
  - 定义接口和方法：
    ```typescript
    import type { ServerWebSocket } from "bun"

    // 配对信息接口
    interface PairInfo {
      code: string
      senderWs: ServerWebSocket<unknown> | null
      receiverWs: ServerWebSocket<unknown> | null
      createdAt: number
      used: boolean
    }

    // PairManager 类
    export class PairManager {
      private pairs: Map<string, PairInfo> = new Map()
      private cleanupTimer: Timer | null = null

      constructor() { this.startCleanup() }

      // 生成 6 位随机配对码（100000-999999）
      private generateCode(): string
      // 创建新配对，存储到 Map，返回配对码
      createPair(senderWs: ServerWebSocket<unknown>): string
      // 校验配对码：存在且未过期且未使用时返回 pairInfo，否则返回 null
      validateAndJoin(code: string, receiverWs: ServerWebSocket<unknown>): PairInfo | null
      // 移除指定配对码
      remove(code: string): void
      // 启动 60 秒定时器，清理过期（>5分钟）的配对码
      private startCleanup(): void
      // 停止定时器
      stop(): void
    }
    ```
  - `generateCode()` 实现：
    ```typescript
    private generateCode(): string {
      return String(Math.floor(100000 + Math.random() * 900000))
    }
    ```
  - 需处理极稀有冲突——当生成的 code 已存在于 Map 中时重新生成
  - `validateAndJoin` 校验逻辑：
    ```typescript
    validateAndJoin(code: string, receiverWs: ServerWebSocket<unknown>): PairInfo | null {
      const pair = this.pairs.get(code)
      if (!pair) return null // 码不存在
      if (pair.used) return null // 已被使用
      if (Date.now() - pair.createdAt > 5 * 60 * 1000) return null // 已过期
      pair.receiverWs = receiverWs
      pair.used = true
      return pair
    }
    ```
  - `startCleanup` 实现：
    ```typescript
    private startCleanup(): void {
      this.cleanupTimer = setInterval(() => {
        const now = Date.now()
        for (const [code, pair] of this.pairs) {
          if (now - pair.createdAt > 5 * 60 * 1000) this.pairs.delete(code)
        }
      }, 60_000)
    }
    ```

- [x] 实现 WebSocket 连接管理与消息转发（relay.ts）
  - 位置: `side-projects/peri-sync/server/src/relay.ts`（新建）
  - 导出 `createRelayHandler` 函数，返回 Hono WebSocket handler：
    ```typescript
    import type { ServerWebSocket } from "bun"
    import { PairManager } from "./pair-manager"
    import type { WsClientMessage, WsServerMessage } from "./types"

    export function createRelayHandler(pairManager: PairManager) {
      return {
        open(ws: ServerWebSocket<{ role: "sender" | "receiver"; code?: string }>): void {
          // 根据 role 处理：
          // - "sender": pairManager.createPair(ws) → 发送 pair_created 消息
          // - "receiver": pairManager.validateAndJoin(code!, ws) → 
          //   成功时通知双方 pair_joined（双发），失败时发送 error 消息
        },
        message(ws, raw): void {
          // 解析 JSON → WsClientMessage
          // 根据 ws.data.role 确定转发方向：
          // - sender 发来的消息 → 转发给 receiver
          // - receiver 发来的消息 → 转发给 sender
          // data 字段不做任何解析，直接透传原始 JSON
        },
        close(ws): void {
          // 清理配对码
          // 通知另一方连接已断开（可选，发送 error 消息）
        }
      }
    }
    ```
  - `open` 方法详细逻辑：
    ```typescript
    open(ws) {
      const { role, code } = ws.data
      if (role === "sender") {
        const pairCode = pairManager.createPair(ws)
        ws.send(JSON.stringify({ type: "pair_created", pair_code: pairCode }))
      } else if (role === "receiver" && code) {
        const pair = pairManager.validateAndJoin(code, ws)
        if (pair && pair.senderWs) {
          pair.senderWs.send(JSON.stringify({ type: "pair_joined" }))
          ws.send(JSON.stringify({ type: "pair_joined" }))
        } else {
          ws.send(JSON.stringify({ type: "error", code: "PAIR_INVALID", message: "无效或已过期的配对码" }))
          ws.close()
        }
      } else {
        ws.send(JSON.stringify({ type: "error", code: "BAD_REQUEST", message: "缺少 role 或 code 参数" }))
        ws.close()
      }
    }
    ```
  - `message` 方法详细逻辑：
    ```typescript
    message(ws, raw) {
      const pair = pairManager.findByWs(ws) // 需在 PairManager 中新增此方法
      if (!pair) return
      const target = ws.data.role === "sender" ? pair.receiverWs : pair.senderWs
      if (target) {
        // 直接转发原始消息（可能是文本或 Buffer）
        target.send(typeof raw === "string" ? raw : raw)
      }
    }
    ```
  - 维护 WebSocket → 配对码的反向映射：需在 `PairManager` 中新增 `findByWs(ws)` 方法和 `registerWs`/`unregisterWs` 方法，使用 `WeakMap<ServerWebSocket, string>` 存储反向映射

- [x] 组装 Hono 应用（index.ts）
  - 位置: `side-projects/peri-sync/server/src/index.ts`（新建）
  - 实现：
    ```typescript
    import { Hono } from "hono"
    import { PairManager } from "./pair-manager"
    import { createRelayHandler } from "./relay"

    const pairManager = new PairManager()
    const app = new Hono()

    // WebSocket 端点
    app.get("/ws", (c) => {
      const role = c.req.query("role")
      const code = c.req.query("code")

      if (role !== "sender" && role !== "receiver") {
        return c.text("role must be sender or receiver", 400)
      }

      const handler = createRelayHandler(pairManager)
      // 使用 Bun.serve 的 upgrade 机制
      const upgraded = c.req.raw.headers.get("upgrade") === "websocket"
      // ... WebSocket upgrade ...
    })
    ```
  - 需查阅 Hono 4.x 的 WebSocket API。在 Bun 运行时下，Hono 支持 `upgradeWebSocket` helper：
    ```typescript
    import { upgradeWebSocket } from "hono/bun"

    app.get("/ws", upgradeWebSocket(() => createRelayHandler(pairManager)))
    ```
  - 但需要向 WebSocket 传递 role 和 code 作为 data，使用 query params → custom create 函数：
    ```typescript
    app.get("/ws", (c) => {
      const role = c.req.query("role") as "sender" | "receiver"
      const code = c.req.query("code") || undefined
      return upgradeWebSocket(() => ({
        data: { role, code },
        ...createRelayHandler(pairManager)
      }))(c)
    })
    ```
  - 健康检查端点：
    ```typescript
    app.get("/health", (c) => c.text("ok"))
    ```

- [x] 为 PairManager 编写单元测试
  - 测试文件: `side-projects/peri-sync/server/src/pair-manager.test.ts`（新建）
  - 使用 Bun 内置测试框架（`bun:test`）：
    ```typescript
    import { describe, test, expect, beforeEach, mock } from "bun:test"
    import { PairManager } from "./pair-manager"
    ```
  - Mock `ServerWebSocket`：创建一个简单的对象，包含 `send()` mock
  - 测试场景（共 5 个）：
    - `test_createPair_生成6位数字码`：`createPair(mockWs)` → 返回的 code 满足 `>= 100000 && <= 999999`
    - `test_validateAndJoin_有效码返回pairInfo`：createPair 后，用相同 code 调用 validateAndJoin → 返回非 null 的 PairInfo、pair.used===true
    - `test_validateAndJoin_无效码返回null`：`validateAndJoin("000000", mockWs)` → `null`
    - `test_validateAndJoin_码已使用后返回null`：同码先成功 validateAndJoin 一次，再次调用 → `null`
    - `test_cleanup_过期码被移除`：用 `mock.module` 或直接操作 private field 插入一个 createdAt 为 6 分钟前的 pair，调用 cleanup → 该 code 不再存在于 Map 中。需在 PairManager 中暴露 `cleanupForTest()` 公开方法直接触发清理逻辑
  - 运行命令: `cd side-projects/peri-sync/server && bun test src/pair-manager.test.ts`
  - 预期: 5 个测试全部通过

**检查步骤:**
- [x] 验证项目结构完整
  - `ls side-projects/peri-sync/server/package.json side-projects/peri-sync/server/tsconfig.json side-projects/peri-sync/server/src/types.ts side-projects/peri-sync/server/src/pair-manager.ts side-projects/peri-sync/server/src/relay.ts side-projects/peri-sync/server/src/index.ts`
  - 预期: 6 个文件全部存在

- [x] 验证 TypeScript 类型检查通过
  - `cd side-projects/peri-sync/server && bun run tsc --noEmit 2>&1`
  - 预期: 无错误输出（如 bun 环境无 tsc，改为 `bun --check src/index.ts`）

- [x] 验证 Relay Server 可启动
  - `cd side-projects/peri-sync/server && timeout 3 bun run dev 2>&1 || true`
  - 预期: 输出包含 "listening" 或 "Listening"，无错误

- [x] 验证健康检查端点响应
  - 在后台启动服务器后：`curl -s http://localhost:8080/health`
  - 预期: 返回 "ok"

- [x] 验证 WebSocket 配对流程
  - 启动服务器后，用工具或脚本模拟 sender 连接 `/ws?role=sender`，验证收到 `{"type":"pair_created","pair_code":"..."}`
  - 再用 receiver 连接 `/ws?role=receiver&code=<收到的码>`，验证双方均收到 `{"type":"pair_joined"}`
  - 预期: sender 和 receiver 均收到对应的消息

- [x] 验证单元测试通过
  - `cd side-projects/peri-sync/server && bun test src/pair-manager.test.ts`
  - 预期: 5 个测试全部通过

---
