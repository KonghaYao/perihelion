//! 只读 SQLite 数据加载层。
//!
//! 使用 bun:sqlite（Bun 内置 SQLite 驱动），避免 better-sqlite3 兼容问题。
//! 所有操作均为只读，不写入数据库。

import { Database } from "bun:sqlite";
import { homedir } from "os";
import { join } from "path";
import type {
  ThreadRow,
  MessageRow,
  ParsedMessage,
  AiContent,
  ToolContent,
  ToolCallRequest,
} from "../types.js";

export const DEFAULT_DB_PATH = join(
  homedir(),
  ".peri/threads/threads.db"
);

export class DataLoader {
  private db: Database;

  constructor(dbPath: string = DEFAULT_DB_PATH) {
    this.db = new Database(dbPath, { readonly: true });
  }

  close() {
    this.db.close();
  }

  // ── Threads ──

  /** 加载所有可见（非 sub-agent）会话 */
  loadVisibleThreads(): ThreadRow[] {
    return this.db
      .query("SELECT * FROM threads WHERE hidden = 0 ORDER BY created_at ASC")
      .all() as ThreadRow[];
  }

  /** 加载所有会话（含 sub-agent） */
  loadAllThreads(): ThreadRow[] {
    return this.db
      .query("SELECT * FROM threads ORDER BY created_at ASC")
      .all() as ThreadRow[];
  }

  /** 加载指定会话的子 agent */
  loadSubAgents(parentThreadId: string): ThreadRow[] {
    return this.db
      .query("SELECT * FROM threads WHERE parent_thread_id = ? ORDER BY created_at ASC")
      .all(parentThreadId) as ThreadRow[];
  }

  /** 按 ID 批量加载 threads */
  loadThreadsByIds(ids: string[]): ThreadRow[] {
    if (ids.length === 0) return [];
    const placeholders = ids.map(() => "?").join(",");
    return this.db
      .query(`SELECT * FROM threads WHERE id IN (${placeholders})`)
      .all(...ids) as ThreadRow[];
  }

  // ── Messages ──

  /** 加载指定会话的所有消息（按插入顺序） */
  loadMessages(threadId: string): MessageRow[] {
    return this.db
      .query("SELECT * FROM messages WHERE thread_id = ? ORDER BY rowid ASC")
      .all(threadId) as MessageRow[];
  }

  /** 流式处理指定会话的消息 */
  processMessages(threadId: string, handler: (msg: MessageRow, idx: number) => void): void {
    const rows = this.db
      .query("SELECT * FROM messages WHERE thread_id = ? ORDER BY rowid ASC")
      .all(threadId) as MessageRow[];
    rows.forEach((row, idx) => handler(row, idx));
  }

  /** 加载所有错误 tool 消息 */
  loadToolErrors(): MessageRow[] {
    return this.db
      .query(
        `SELECT * FROM messages
         WHERE role = 'tool' AND content LIKE '%is_error":true%'
         ORDER BY rowid ASC`
      )
      .all() as MessageRow[];
  }

  // ── Statistics ──

  getStats() {
    const threads = this.db
      .query("SELECT COUNT(*) as total, SUM(CASE WHEN hidden=0 THEN 1 ELSE 0 END) as visible FROM threads")
      .get() as any;
    const messages = this.db
      .query("SELECT COUNT(*) as total FROM messages")
      .get() as any;
    const roleDistribution = this.db
      .query("SELECT role, COUNT(*) as count FROM messages GROUP BY role")
      .all() as { role: string; count: number }[];
    const errors = this.db
      .query("SELECT COUNT(*) as total FROM messages WHERE role='tool' AND content LIKE '%is_error\":true%'")
      .get() as any;
    const topSessions = this.db
      .query(
        `SELECT thread_id, COUNT(*) as msg_count
         FROM messages GROUP BY thread_id
         ORDER BY msg_count DESC LIMIT 10`
      )
      .all() as { thread_id: string; msg_count: number }[];

    return {
      totalThreads: threads.total,
      visibleThreads: threads.visible,
      totalMessages: messages.total,
      roleDistribution: Object.fromEntries(
        roleDistribution.map((r) => [r.role, r.count])
      ),
      totalToolErrors: errors.total,
      topSessions,
    };
  }

  // ── Parsing Helpers ──

  /** 安全解析消息 content JSON */
  static parseContent(raw: string): ParsedMessage | null {
    try {
      return JSON.parse(raw);
    } catch {
      return null;
    }
  }

  /** 从 AiContent 提取 tool_use 调用 */
  static extractToolCalls(msg: ParsedMessage | null): ToolCallRequest[] {
    if (!msg || msg.role !== "assistant") return [];
    const ai = msg as AiContent;
    const rawContent = ai.content;
    // content 可能是 string（纯文本回复）或 ContentBlock[]
    const blocks: any[] = Array.isArray(rawContent) ? rawContent : [];
    const fromContent = blocks
      .filter((b: any) => b.type === "tool_use")
      .map((b: any) => ({
        id: b.id,
        name: b.name,
        arguments: b.input ?? {},
      }));
    const fromToolCalls = ai.tool_calls || [];
    // 去重（content.tool_use 和 tool_calls 可能重复）
    const seen = new Set<string>();
    const merged = [...fromContent, ...fromToolCalls];
    return merged.filter((tc) => {
      if (seen.has(tc.id)) return false;
      seen.add(tc.id);
      return true;
    });
  }

  /** 从 ToolContent 提取错误信息 */
  static parseToolError(msg: ParsedMessage | null): {
    toolCallId: string;
    content: string;
    isError: boolean;
  } | null {
    if (!msg || msg.role !== "tool") return null;
    const tc = msg as ToolContent;
    return {
      toolCallId: tc.tool_call_id,
      content: typeof tc.content === "string" ? tc.content : JSON.stringify(tc.content),
      isError: tc.is_error,
    };
  }
}
