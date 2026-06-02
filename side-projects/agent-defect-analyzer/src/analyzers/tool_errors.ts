//! 工具失败模式分析器。
//!
//! 分析维度：
//! 1. 错误分类（工具不存在 / 执行失败 / 用户中断 / 权限拒绝）
//! 2. 错误热力图（哪些工具 + 哪些错误类型组合最频繁）
//! 3. 连续失败链检测（同一会话中连续多次失败）
//! 4. 幻觉工具调用检测（调用不存在的工具名）

import type { MessageRow, ToolErrorRecord, DefectReport } from "../../types.js";
import { DataLoader } from "../utils/data_loader.js";
import { printSection, printMetric, printTable, printFinding, printWarning, printCodeBlock } from "../utils/report.js";

// ── 错误分类 ──

interface ErrorCategory {
  name: string;
  pattern: RegExp;
  description: string;
  severity: "critical" | "high" | "medium" | "low";
}

const ERROR_CATEGORIES: ErrorCategory[] = [
  {
    name: "tool_not_found",
    pattern: /工具\s+['"].+['"]\s+不存在|tool.*does not exist|tool.*not found/i,
    description: "Agent 调用了不存在的工具（幻觉工具名）",
    severity: "high",
  },
  {
    name: "user_interrupt",
    pattern: /interrupted by user|cancelled|canceled/i,
    description: "用户主动中断了工具执行",
    severity: "low",
  },
  {
    name: "execution_failed",
    pattern: /Tool execution failed|命令执行失败/i,
    description: "工具执行过程中出错",
    severity: "medium",
  },
  {
    name: "permission_denied",
    pattern: /permission denied|权限|not authorized/i,
    description: "权限不足导致工具调用失败",
    severity: "medium",
  },
  {
    name: "timeout",
    pattern: /timeout|timed out|超时/i,
    description: "工具执行超时",
    severity: "medium",
  },
  {
    name: "file_not_found",
    pattern: /No such file|file not found|not found|ENOENT/i,
    description: "操作了不存在的文件",
    severity: "low",
  },
  {
    name: "api_error",
    pattern: /\d{3}\s+(error|bad request|unauthorized|forbidden|not found)/i,
    description: "外部 API 返回错误",
    severity: "medium",
  },
  {
    name: "parse_error",
    pattern: /parse error|invalid json|unexpected token|deserializ/i,
    description: "数据解析错误",
    severity: "medium",
  },
  {
    name: "context_overflow",
    pattern: /context.*exceed|too many tokens|max.*context|context.*limit/i,
    description: "上下文窗口溢出",
    severity: "critical",
  },
];

interface AnalyzedError {
  messageId: string;
  threadId: string;
  errorMessage: string;
  category: string;
  severity: "critical" | "high" | "medium" | "low";
  /** 关联的 tool_use 名称（从 tool_call_id 前缀推断） */
  inferredToolName: string;
}

export function analyzeToolErrors(loader: DataLoader): DefectReport[] {
  printSection("工具失败模式分析");

  const errorMessages = loader.loadToolErrors();
  printMetric("工具错误总数", errorMessages.length);

  // Step 1: 分类每个错误
  const analyzed: AnalyzedError[] = errorMessages.map((msg) => {
    const parsed = DataLoader.parseContent(msg.content);
    const errorMessage = parsed && "content" in parsed
      ? String((parsed as any).content || msg.content)
      : msg.content;

    const category = classifyError(errorMessage);
    // tool_call_id 格式: call_{nn}_{ToolName}{random}
    const toolCallId = parsed && "tool_call_id" in parsed
      ? (parsed as any).tool_call_id
      : "";
    const inferredToolName = inferToolName(toolCallId, errorMessage);

    return {
      messageId: msg.message_id,
      threadId: msg.thread_id,
      errorMessage: errorMessage.slice(0, 200),
      category: category.name,
      severity: category.severity,
      inferredToolName,
    };
  });

  // Step 2: 按分类聚合
  const categoryCount = new Map<string, number>();
  const categoryByTool = new Map<string, Map<string, number>>(); // tool -> category -> count
  const toolErrorCount = new Map<string, number>();

  for (const err of analyzed) {
    categoryCount.set(err.category, (categoryCount.get(err.category) || 0) + 1);
    toolErrorCount.set(err.inferredToolName, (toolErrorCount.get(err.inferredToolName) || 0) + 1);

    if (!categoryByTool.has(err.inferredToolName)) {
      categoryByTool.set(err.inferredToolName, new Map());
    }
    const toolCats = categoryByTool.get(err.inferredToolName)!;
    toolCats.set(err.category, (toolCats.get(err.category) || 0) + 1);
  }

  // 输出分类统计
  console.log("\n  错误分类分布:");
  const catRows = [...categoryCount.entries()]
    .sort((a, b) => b[1] - a[1])
    .map(([cat, count]) => [cat, String(count), (count / analyzed.length * 100).toFixed(1) + "%"]);
  printTable(["分类", "数量", "占比"], catRows);

  // 输出工具错误热力图
  console.log("\n  工具错误排行 (Top 10):");
  const toolRows = [...toolErrorCount.entries()]
    .sort((a, b) => b[1] - a[1])
    .slice(0, 10)
    .map(([tool, count]) => [tool, String(count)]);
  printTable(["工具名", "错误数"], toolRows);

  // Step 3: 幻觉工具检测
  printSection("幻觉工具调用检测");
  const hallucinatedTools = analyzed
    .filter((e) => e.category === "tool_not_found")
    .map((e) => e.inferredToolName);
  const hallucinationCount = new Map<string, number>();
  for (const tool of hallucinatedTools) {
    hallucinationCount.set(tool, (hallucinationCount.get(tool) || 0) + 1);
  }

  if (hallucinationCount.size > 0) {
    printWarning("发现幻觉工具调用", `共 ${hallucinatedTools.length} 次，涉及 ${hallucinationCount.size} 个不存在的工具`);
    const hallRows = [...hallucinationCount.entries()]
      .sort((a, b) => b[1] - a[1])
      .map(([tool, count]) => [tool, String(count)]);
    printTable(["幻觉工具名", "调用次数"], hallRows);
  } else {
    console.log("  ✅ 未发现幻觉工具调用");
  }

  // Step 4: 连续失败链检测（按 thread 聚合错误序列）
  printSection("连续失败链检测");
  const errorsByThread = new Map<string, AnalyzedError[]>();
  for (const err of analyzed) {
    if (!errorsByThread.has(err.threadId)) errorsByThread.set(err.threadId, []);
    errorsByThread.get(err.threadId)!.push(err);
  }

  let maxChain = 0;
  let maxChainThread = "";
  const chainLengths: number[] = [];
  for (const [threadId, errors] of errorsByThread) {
    // 连续 error 消息算一条链
    const chainLen = errors.length;
    chainLengths.push(chainLen);
    if (chainLen > maxChain) {
      maxChain = chainLen;
      maxChainThread = threadId;
    }
  }

  printMetric("含错误的会话数", errorsByThread.size);
  printMetric("最长连续错误链", maxChain, ` 条 (session: ${maxChainThread.slice(0, 12)}...)`);
  printMetric("平均每错误会话错误数",
    chainLengths.length > 0
      ? (chainLengths.reduce((a, b) => a + b, 0) / chainLengths.length).toFixed(1)
      : "0"
  );

  // Step 5: 生成缺陷报告
  const reports: DefectReport[] = [];

  // 幻觉工具报告
  if (hallucinationCount.size > 0) {
    reports.push({
      id: "DEF-001",
      severity: "high",
      category: "工具幻觉",
      title: "Agent 调用不存在的工具",
      description: `Agent 在 ${hallucinationCount.size} 种不存在的工具上共产生了 ${hallucinatedTools.length} 次调用。这表明 Agent 对可用工具列表的认知存在偏差。`,
      evidence: [...hallucinationCount.entries()]
        .sort((a, b) => b[1] - a[1])
        .slice(0, 5)
        .map(([tool, count]) => `${tool}: ${count}次`),
      affectedSessions: [...new Set(
        analyzed
          .filter((e) => e.category === "tool_not_found")
          .map((e) => e.threadId)
      )],
      recommendation: "检查 system prompt 中的工具描述是否与实际注册工具一致。考虑在 ToolSearch 中增加工具名纠错逻辑（如模糊匹配）。",
      confidence: 0.9,
    });
  }

  // 执行失败报告
  const execErrors = analyzed.filter((e) => e.category === "execution_failed");
  if (execErrors.length > 0) {
    reports.push({
      id: "DEF-002",
      severity: "medium",
      category: "执行失败",
      title: "工具执行频繁失败",
      description: `共 ${execErrors.length} 次工具执行失败。主要集中在: ${[...new Set(execErrors.map((e) => e.inferredToolName))].slice(0, 5).join(", ")}`,
      evidence: execErrors.slice(0, 5).map((e) => `[${e.inferredToolName}] ${e.errorMessage.slice(0, 80)}`),
      affectedSessions: [...new Set(execErrors.map((e) => e.threadId))],
      recommendation: "对高频失败工具增加前置校验和更好的错误恢复策略。",
      confidence: 0.7,
    });
  }

  // 文件不存在报告
  const fileErrors = analyzed.filter((e) => e.category === "file_not_found");
  if (fileErrors.length > 3) {
    reports.push({
      id: "DEF-003",
      severity: "low",
      category: "文件操作",
      title: "Agent 频繁操作不存在的文件",
      description: `共 ${fileErrors.length} 次文件不存在错误。Agent 在执行前未验证文件是否存在。`,
      evidence: fileErrors.slice(0, 5).map((e) => e.errorMessage.slice(0, 80)),
      affectedSessions: [...new Set(fileErrors.map((e) => e.threadId))],
      recommendation: "在 Read/Write/Edit 工具执行前增加 Glob 预检查。在 agent 系统提示中强调'先验证路径再操作'。",
      confidence: 0.6,
    });
  }

  return reports;
}

// ── Helpers ──

function classifyError(message: string): ErrorCategory {
  for (const cat of ERROR_CATEGORIES) {
    if (cat.pattern.test(message)) return cat;
  }
  return {
    name: "unknown",
    pattern: /.*/,
    description: "未分类错误",
    severity: "low",
  };
}

/** 从 tool_call_id (如 call_00_ReadXXX) 或错误消息中推断工具名 */
function inferToolName(toolCallId: string, errorMessage: string): string {
  // 尝试从错误消息中提取
  const toolMatch = errorMessage.match(/(?:工具|tool)\s+['"]?(\w+)['"]?\s+(?:不存在|not exist|not found|failed)/i);
  if (toolMatch) return toolMatch[1];

  // 尝试从错误消息中提取 Bash 等关键词
  const bashMatch = errorMessage.match(/Bash\s*-/i);
  if (bashMatch) return "Bash";

  // tool_call_id 格式不可靠，返回 unknown
  return "unknown";
}
