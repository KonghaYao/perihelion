//! SQLite 数据类型定义，对齐 peri-agent 持久化 schema。
//!
//! 所有类型均为只读（分析用途），不写入数据库。

// ── Thread ──

export interface ThreadRow {
  id: string;
  title: string | null;
  cwd: string;
  created_at: string;
  updated_at: string;
  message_count: number;
  parent_thread_id: string | null;
  snapshot_at_message_id: string | null;
  hidden: number; // 0 | 1
  cancel_policy: string;
  config: string | null;
  cached_context: string | null;
  agent_status: string;
}

// ── Message ──

export interface MessageRow {
  message_id: string;
  thread_id: string;
  role: "user" | "assistant" | "system" | "tool";
  content: string; // JSON
}

// ── Parsed Content Structures ──

export interface HumanContent {
  role: "user";
  id: string;
  content: string | ContentBlock[];
}

export interface AiContent {
  role: "assistant";
  id: string;
  content: ContentBlock[];
  tool_calls?: ToolCallRequest[];
}

export interface SystemContent {
  role: "system";
  id: string;
  content: string | ContentBlock[];
}

export interface ToolContent {
  role: "tool";
  id: string;
  tool_call_id: string;
  content: string;
  is_error: boolean;
}

export type ParsedMessage = HumanContent | AiContent | SystemContent | ToolContent;

// ── Content Blocks ──

export type ContentBlock =
  | { type: "text"; text: string }
  | { type: "image"; source: unknown }
  | { type: "tool_use"; id: string; name: string; input: Record<string, unknown> }
  | { type: "tool_result"; tool_use_id: string; content: string; is_error?: boolean }
  | { type: "reasoning"; text: string; signature?: string }
  | { type: "thinking"; text: string; signature?: string }
  | { type: "document"; title?: string; source: unknown }
  | { type: "unknown"; data: unknown };

// ── Tool Call ──

export interface ToolCallRequest {
  id: string;
  name: string;
  arguments: Record<string, unknown>;
}

// ── Analysis Result Types ──

export interface ToolErrorRecord {
  threadId: string;
  messageId: string;
  toolCallId: string;
  toolName: string;
  errorMessage: string;
  timestamp: string;
  /** 上一条 assistant 消息中的 tool_use 列表，用于还原上下文 */
  siblingToolCalls: ToolCallRequest[];
}

export interface SessionProfile {
  threadId: string;
  title: string | null;
  cwd: string;
  createdAt: Date;
  updatedAt: Date;
  durationMinutes: number;
  totalMessages: number;
  userMessages: number;
  assistantMessages: number;
  toolMessages: number;
  systemMessages: number;
  toolErrors: number;
  toolErrorRate: number; // errors / (tool_messages || 1)
  /** 子 agent 数量 */
  subAgentCount: number;
  /** 平均每轮工具调用数 */
  avgToolsPerTurn: number;
  /** 最大连续错误序列长度 */
  maxConsecutiveErrors: number;
  /** Agent 状态 */
  agentStatus: string;
  /** 每种工具的使用频次 */
  toolFrequency: Record<string, number>;
  /** reasoning 文本总字符数（代理思考量的代理指标） */
  totalReasoningChars: number;
}

export interface DefectReport {
  id: string;
  severity: "critical" | "high" | "medium" | "low";
  category: string;
  title: string;
  description: string;
  evidence: string[];
  affectedSessions: string[];
  recommendation: string;
  confidence: number; // 0-1
}
