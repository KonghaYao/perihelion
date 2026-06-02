//! Agent 策略质量分析器。
//!
//! 分析维度：
//! 1. 工具选择模式（哪些工具组合最常见？哪些工具从不联用？）
//! 2. 重试循环检测（Agent 对同一操作反复重试）
//! 3. 工具编排效率（并行 vs 串行、工具调用冗余度）
//! 4. Read-then-Write 成功率（Agent 是否先读后写？）
//! 5. 搜索-阅读 模式分析

import type { DefectReport } from "../../types.js";
import { DataLoader } from "../utils/data_loader.js";
import { printSection, printMetric, printTable, printFinding, printWarning } from "../utils/report.js";

interface ToolCallSequence {
  threadId: string;
  /** 每个 assistant turn 中的工具调用集合 */
  turns: { toolNames: string[]; isError: boolean }[];
}

interface RetryLoop {
  threadId: string;
  toolName: string;
  attemptCount: number;
  /** 各次尝试的 tool_call_id */
  attemptIds: string[];
  /** 各次尝试的错误消息摘要 */
  errorSnippets: string[];
}

export function analyzeStrategyQuality(loader: DataLoader): DefectReport[] {
  printSection("Agent 策略质量分析");

  const threads = loader.loadVisibleThreads();
  const sequences: ToolCallSequence[] = [];
  const retryLoops: RetryLoop[] = [];

  // Step 1: 提取每个会话的工具调用序列
  for (const thread of threads) {
    const messages = loader.loadMessages(thread.id);
    const turns: ToolCallSequence["turns"] = [];

    // 跟踪每个 tool_call_id → 错误状态
    const callIdToName = new Map<string, string>();
    const failedCallIds = new Set<string>();

    for (const msg of messages) {
      if (msg.role === "assistant") {
        const parsed = DataLoader.parseContent(msg.content);
        const toolCalls = DataLoader.extractToolCalls(parsed);
        if (toolCalls.length > 0) {
          const toolNames = toolCalls.map((tc) => tc.name);
          // 注册 callId → name 映射
          for (const tc of toolCalls) {
            callIdToName.set(tc.id, tc.name);
          }
          // 本轮是否有失败（后续 tool 消息会标记）
          turns.push({ toolNames, isError: false });
        }
      } else if (msg.role === "tool") {
        const parsed = DataLoader.parseContent(msg.content);
        const errInfo = DataLoader.parseToolError(parsed);
        if (errInfo?.isError && errInfo.toolCallId) {
          failedCallIds.add(errInfo.toolCallId);
        }
      }
    }

    // 回填错误标记
    for (const turn of turns) {
      // 简化：不精确回填，后续用 retry 检测
    }

    if (turns.length > 0) {
      sequences.push({ threadId: thread.id, turns });
    }

    // Step 2: 检测重试循环
    // 策略：在连续的 assistant turns 中，如果同一个工具名反复出现且有 error，
    // 则判定为重试循环
    detectRetryLoops(thread.id, messages, callIdToName, failedCallIds, retryLoops);
  }

  // Step 3: 工具使用频率排行
  printSection("工具使用频率排行");

  const toolUsage = new Map<string, number>();
  for (const seq of sequences) {
    for (const turn of seq.turns) {
      for (const tool of turn.toolNames) {
        toolUsage.set(tool, (toolUsage.get(tool) || 0) + 1);
      }
    }
  }

  const toolRows = [...toolUsage.entries()]
    .sort((a, b) => b[1] - a[1])
    .slice(0, 20)
    .map(([tool, count]) => [tool, String(count)]);
  printTable(["工具名", "调用次数"], toolRows);

  // Step 4: 工具共现矩阵（哪些工具经常在同一次 turn 中一起被调用）
  printSection("工具共现对（Top 15）");

  const cooccurrence = new Map<string, number>();
  for (const seq of sequences) {
    for (const turn of seq.turns) {
      if (turn.toolNames.length >= 2) {
        const sorted = [...new Set(turn.toolNames)].sort();
        for (let i = 0; i < sorted.length; i++) {
          for (let j = i + 1; j < sorted.length; j++) {
            const key = `${sorted[i]} + ${sorted[j]}`;
            cooccurrence.set(key, (cooccurrence.get(key) || 0) + 1);
          }
        }
      }
    }
  }

  const coRows = [...cooccurrence.entries()]
    .sort((a, b) => b[1] - a[1])
    .slice(0, 15)
    .map(([pair, count]) => [pair, String(count)]);
  printTable(["工具组合", "共现次数"], coRows);

  // Step 5: 并行工具调用率
  printSection("并行工具调用分析");

  const totalTurns = sequences.reduce((a, s) => a + s.turns.length, 0);
  const parallelTurns = sequences.reduce(
    (a, s) => a + s.turns.filter((t) => t.toolNames.length >= 2).length,
    0
  );
  const maxParallel = Math.max(
    ...sequences.flatMap((s) => s.turns.map((t) => t.toolNames.length)),
    0
  );

  printMetric("总工具调用轮次", totalTurns);
  printMetric("并行调用轮次", parallelTurns, ` (${(parallelTurns / (totalTurns || 1) * 100).toFixed(1)}%)`);
  printMetric("最大并行工具数", maxParallel);

  // Step 6: 重试循环报告
  printSection("重试循环检测");

  printMetric("检测到的重试循环", retryLoops.length);
  if (retryLoops.length > 0) {
    const retryRows = retryLoops
      .sort((a, b) => b.attemptCount - a.attemptCount)
      .slice(0, 10)
      .map((r) => [
        r.threadId.slice(0, 12) + "...",
        r.toolName,
        String(r.attemptCount),
        r.errorSnippets[0]?.slice(0, 40) || "",
      ]);
    printTable(["Session", "工具", "重试次数", "首次错误"], retryRows);
  }

  // Step 7: Read → Edit/Write 成功率
  printSection("Read → Edit/Write 模式分析");

  let readThenWriteCount = 0;
  let writeWithoutReadCount = 0;

  for (const seq of sequences) {
    const readFiles = new Set<string>();
    for (const turn of seq.turns) {
      for (const tool of turn.toolNames) {
        // 简化：只看工具名
        if (tool === "Read" || tool === "Glob" || tool === "Grep") {
          // 标记为已读（简化，无法提取具体文件路径）
        }
      }
      const hasWrite = turn.toolNames.some(
        (t) => t === "Write" || t === "Edit"
      );
      if (hasWrite) {
        // 检查前面是否有过 Read（简化检查：任何 turn 之前有 Read）
        const turnIdx = seq.turns.indexOf(turn);
        const hasPriorRead = seq.turns
          .slice(0, turnIdx)
          .some((t) => t.toolNames.includes("Read"));
        if (hasPriorRead) {
          readThenWriteCount++;
        } else {
          writeWithoutReadCount++;
        }
      }
    }
  }

  printMetric("Read → Write/Edit", readThenWriteCount, " 次");
  printMetric("未 Read 直接 Write/Edit", writeWithoutReadCount, " 次");
  if (writeWithoutReadCount > 0 && readThenWriteCount + writeWithoutReadCount > 0) {
    const blindWritePct = (
      (writeWithoutReadCount / (readThenWriteCount + writeWithoutReadCount)) *
      100
    ).toFixed(1);
    printWarning("盲写比例", `${blindWritePct}%`);
  }

  // 生成缺陷报告
  const reports: DefectReport[] = [];

  if (retryLoops.length > 5) {
    reports.push({
      id: "STR-001",
      severity: "high",
      category: "重试策略",
      title: "Agent 存在大量无效重试循环",
      description: `检测到 ${retryLoops.length} 个重试循环。Agent 在工具失败后未能有效调整策略，反复使用相同方式重试。`,
      evidence: retryLoops
        .sort((a, b) => b.attemptCount - a.attemptCount)
        .slice(0, 3)
        .map((r) => `${r.toolName}: ${r.attemptCount}次重试 - ${r.errorSnippets[0]?.slice(0, 50)}`),
      affectedSessions: [...new Set(retryLoops.map((r) => r.threadId))],
      recommendation: "在重试逻辑中增加策略变化检测——如果连续2次相同工具+相同参数失败，应切换策略或报告给用户。考虑在 agent 系统提示中增加'失败后应先分析原因再重试'的指导。",
      confidence: 0.8,
    });
  }

  if (writeWithoutReadCount > readThenWriteCount * 0.2) {
    reports.push({
      id: "STR-002",
      severity: "medium",
      category: "工具编排",
      title: "Agent 存在较高比例的'盲写'行为",
      description: `${writeWithoutReadCount} 次 Write/Edit 操作前未执行 Read。Agent 可能凭记忆或猜测直接修改文件。`,
      evidence: [
        `Read→Write: ${readThenWriteCount}`,
        `盲写: ${writeWithoutReadCount}`,
      ],
      affectedSessions: [],
      recommendation: "在系统提示中强化'Write/Edit 前必须 Read'的约束。可在 middleware 层对 Write/Edit 检查是否在近期消息中有过 Read 该文件。",
      confidence: 0.65,
    });
  }

  // 工具多样性过低
  const singleToolSessions = sequences.filter(
    (s) => s.turns.length > 5 && new Set(s.turns.flatMap((t) => t.toolNames)).size <= 2
  );
  if (singleToolSessions.length > 5) {
    reports.push({
      id: "STR-003",
      severity: "medium",
      category: "工具多样性",
      title: "部分长会话工具使用过于单一",
      description: `${singleToolSessions.length} 个超过5轮的会话只使用了 ≤2 种工具。Agent 可能未能充分利用可用工具集。`,
      evidence: singleToolSessions.slice(0, 3).map((s) =>
        `${s.threadId.slice(0, 12)}: ${[...new Set(s.turns.flatMap((t) => t.toolNames))].join(", ")}`
      ),
      affectedSessions: singleToolSessions.map((s) => s.threadId),
      recommendation: "检查 ToolSearch 是否正确暴露了 deferred 工具。在系统提示中增加工具发现引导。",
      confidence: 0.5,
    });
  }

  return reports;
}

// ── Retry Loop Detection ──

function detectRetryLoops(
  threadId: string,
  messages: any[],
  callIdToName: Map<string, string>,
  failedCallIds: Set<string>,
  retryLoops: RetryLoop[]
): void {
  // 按时间顺序追踪失败的同一工具调用
  const toolFailuresByTool = new Map<string, { callIds: string[]; snippets: string[] }>();

  for (const msg of messages) {
    if (msg.role === "tool") {
      const parsed = DataLoader.parseContent(msg.content);
      const errInfo = DataLoader.parseToolError(parsed);
      if (errInfo?.isError) {
        const toolName = callIdToName.get(errInfo.toolCallId) || "unknown";
        if (!toolFailuresByTool.has(toolName)) {
          toolFailuresByTool.set(toolName, { callIds: [], snippets: [] });
        }
        const entry = toolFailuresByTool.get(toolName)!;
        entry.callIds.push(errInfo.toolCallId);
        entry.snippets.push(errInfo.content.slice(0, 100));
      }
    }
  }

  // 同一工具失败 ≥2 次即为重试循环
  for (const [toolName, entry] of toolFailuresByTool) {
    if (entry.callIds.length >= 2) {
      retryLoops.push({
        threadId,
        toolName,
        attemptCount: entry.callIds.length,
        attemptIds: entry.callIds,
        errorSnippets: entry.snippets,
      });
    }
  }
}
