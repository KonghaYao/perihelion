//! Agent 死循环 / 重复调用检测器。
//!
//! 检测 Agent 在同一会话中对同一工具/模式反复调用的异常行为。
//!
//! 检测维度：
//! 1. **完全重复**：同一工具 + 相同参数（或等价参数）多次调用
//! 2. **振荡循环**：A → B → A → B 的交替模式
//! 3. **语义循环**：同一工具不同参数但目标等价（如反复搜索相近 pattern）
//! 4. **无进展循环**：工具调用后结果未变，但 Agent 仍然重复调用

import type { DefectReport, MessageRow } from "../../types.js";
import { DataLoader } from "../utils/data_loader.js";
import { printSection, printMetric, printTable, printWarning } from "../utils/report.js";

// ── 数据结构 ──

interface ToolInvocation {
  callId: string;
  toolName: string;
  /** 序列化后的参数（用于比较） */
  argsHash: string;
  /** 原始参数（用于展示） */
  argsPreview: string;
  /** 工具结果长度 */
  resultLength: number;
  /** 在消息序列中的位置 */
  messageIndex: number;
}

interface LoopPattern {
  threadId: string;
  threadTitle: string;
  type: "exact_repeat" | "oscillation" | "no_progress" | "semantic_loop";
  toolName: string;
  /** 循环长度（调用次数） */
  loopLength: number;
  /** 涉及的调用 ID */
  callIds: string[];
  /** 人类可读的描述 */
  description: string;
  severity: "critical" | "high" | "medium" | "low";
}

// ── 参数指纹 ──

/** 对工具参数做轻量级指纹，用于重复检测 */
function hashArgs(args: Record<string, unknown>): string {
  // 按键排序后序列化，忽略值的微小差异
  const sorted = Object.keys(args).sort();
  const parts = sorted.map((k) => {
    const v = args[k];
    if (typeof v === "string") {
      // 路径归一化：去掉末尾斜杠、统一分隔符
      return `${k}=${v.replace(/\/+$/, "").replace(/\\/g, "/")}`;
    }
    return `${k}=${JSON.stringify(v)}`;
  });
  return parts.join("|");
}

/** 截取参数预览 */
function previewArgs(args: Record<string, unknown>, maxLen = 80): string {
  const str = JSON.stringify(args);
  if (str.length <= maxLen) return str;
  return str.slice(0, maxLen) + "…";
}

// ── 主分析 ──

export function analyzeDeathLoops(loader: DataLoader): DefectReport[] {
  printSection("Agent 死循环 / 重复调用检测");

  const threads = loader.loadVisibleThreads();
  const allLoops: LoopPattern[] = [];

  for (const thread of threads) {
    const messages = loader.loadMessages(thread.id);
    const invocations = extractInvocationSequence(messages);

    if (invocations.length < 3) continue;

    // 检测 1: 完全重复（同工具 + 同参数指纹）
    const exactRepeats = detectExactRepeats(thread.id, thread.title || "", invocations);
    allLoops.push(...exactRepeats);

    // 检测 2: 振荡循环（A→B→A→B 模式）
    const oscillations = detectOscillations(thread.id, thread.title || "", invocations);
    allLoops.push(...oscillations);

    // 检测 3: 无进展循环（同工具，结果长度相同 = 可能读到了相同内容）
    const noProgress = detectNoProgressLoops(thread.id, thread.title || "", invocations, messages);
    allLoops.push(...noProgress);
  }

  // ── 输出报告 ──

  printMetric("检测到的循环模式总数", allLoops.length);

  // 按类型分组
  const byType = new Map<string, LoopPattern[]>();
  for (const loop of allLoops) {
    if (!byType.has(loop.type)) byType.set(loop.type, []);
    byType.get(loop.type)!.push(loop);
  }

  const typeLabels: Record<string, string> = {
    exact_repeat: "完全重复（同工具+同参数）",
    oscillation: "振荡循环（A→B→A→B）",
    no_progress: "无进展循环（结果未变）",
  };

  for (const [type, loops] of byType) {
    printSection(`  ${typeLabels[type] || type} (${loops.length})`);

    const topLoops = loops
      .sort((a, b) => b.loopLength - a.loopLength)
      .slice(0, 10);

    const rows = topLoops.map((l) => [
      l.threadId.slice(0, 12) + "...",
      l.toolName,
      String(l.loopLength),
      l.description.slice(0, 50),
    ]);
    printTable(["Session", "工具", "循环长度", "描述"], rows);
  }

  // 按严重性统计
  const critical = allLoops.filter((l) => l.severity === "critical");
  const high = allLoops.filter((l) => l.severity === "high");

  if (critical.length > 0) {
    printWarning("严重循环", `${critical.length} 个会话存在 ≥10 次的完全重复调用`);
  }
  if (high.length > 0) {
    printWarning("高频循环", `${high.length} 个会话存在振荡或无进展循环`);
  }

  // ── 生成缺陷报告 ──

  const reports: DefectReport[] = [];

  // 完全重复报告
  const exactLoops = allLoops.filter((l) => l.type === "exact_repeat");
  if (exactLoops.length > 0) {
    const totalRepeatedCalls = exactLoops.reduce((a, l) => a + l.loopLength, 0);
    const avgLoopLen = (totalRepeatedCalls / exactLoops.length).toFixed(1);
    const maxLoop = exactLoops.reduce((a, l) => Math.max(a, l.loopLength), 0);

    reports.push({
      id: "LOOP-001",
      severity: maxLoop >= 10 ? "critical" : "high",
      category: "死循环",
      title: "Agent 完全重复调用同一工具",
      description: `${exactLoops.length} 个会话中存在完全重复的工具调用（同工具+同参数）。总计 ${totalRepeatedCalls} 次无效调用，平均循环长度 ${avgLoopLen}，最长 ${maxLoop} 次。Agent 陷入死循环，无法从错误或结果中学习。`,
      evidence: exactLoops
        .sort((a, b) => b.loopLength - a.loopLength)
        .slice(0, 5)
        .map((l) => `[${l.toolName}] x${l.loopLength} — ${l.description}`),
      affectedSessions: [...new Set(exactLoops.map((l) => l.threadId))],
      recommendation: "在 ReAct 循环中增加'调用历史去重'检测：如果连续 3 次调用同工具+同参数，注入系统消息强制打断。在 tool_dispatch 的 after_tool 中检查与最近 N 次调用的相似度。",
      confidence: 0.92,
    });
  }

  // 振荡循环报告
  const oscLoops = allLoops.filter((l) => l.type === "oscillation");
  if (oscLoops.length > 0) {
    reports.push({
      id: "LOOP-002",
      severity: "medium",
      category: "振荡循环",
      title: "Agent 在两个工具间振荡",
      description: `${oscLoops.length} 个会话中存在 A→B→A→B 的工具振荡模式。Agent 在两个工具之间反复切换，无法收敛到解决方案。`,
      evidence: oscLoops.slice(0, 5).map((l) => `[${l.toolName}] x${l.loopLength} — ${l.description}`),
      affectedSessions: [...new Set(oscLoops.map((l) => l.threadId))],
      recommendation: "在 Agent 系统提示中增加'当你发现自己重复使用相同的工具组合时，停下来分析当前方法的局限性'。在 ReAct 循环中检测工具序列的周期性。",
      confidence: 0.75,
    });
  }

  // 无进展循环报告
  const noProgressLoops = allLoops.filter((l) => l.type === "no_progress");
  if (noProgressLoops.length > 0) {
    reports.push({
      id: "LOOP-003",
      severity: "medium",
      category: "无进展循环",
      title: "Agent 重复调用但结果无变化",
      description: `${noProgressLoops.length} 个会话中 Agent 重复调用工具但得到相同结果。可能的原因：搜索条件不变、文件内容未改变、或 Agent 未利用前次结果。`,
      evidence: noProgressLoops.slice(0, 5).map((l) => `[${l.toolName}] x${l.loopLength} — ${l.description}`),
      affectedSessions: [...new Set(noProgressLoops.map((l) => l.threadId))],
      recommendation: "在工具结果返回后增加'进展检测'：如果连续 N 次工具返回等长/相似的结果，提示 Agent 已穷尽该搜索方向。",
      confidence: 0.65,
    });
  }

  return reports;
}

// ── 调用序列提取 ──

function extractInvocationSequence(messages: MessageRow[]): ToolInvocation[] {
  const invocations: ToolInvocation[] = [];
  // 建立 callId → resultLength 映射
  const callResults = new Map<string, number>();

  // 第一遍：收集 tool 结果长度
  for (const msg of messages) {
    if (msg.role === "tool") {
      const parsed = DataLoader.parseContent(msg.content);
      if (parsed && "tool_call_id" in parsed) {
        const tc = parsed as any;
        callResults.set(tc.tool_call_id, msg.content.length);
      }
    }
  }

  // 第二遍：从 assistant 消息提取 tool_use
  for (let i = 0; i < messages.length; i++) {
    const msg = messages[i];
    if (msg.role !== "assistant") continue;

    const parsed = DataLoader.parseContent(msg.content);
    if (!parsed || parsed.role !== "assistant") continue;

    const ai = parsed as any;
    const blocks = Array.isArray(ai.content) ? ai.content : [];

    for (const block of blocks) {
      if (block.type === "tool_use") {
        const args = block.input || {};
        invocations.push({
          callId: block.id,
          toolName: block.name,
          argsHash: hashArgs(args),
          argsPreview: previewArgs(args),
          resultLength: callResults.get(block.id) || 0,
          messageIndex: i,
        });
      }
    }
  }

  return invocations;
}

// ── 检测器 ──

/** 检测完全重复：同工具 + 同参数指纹出现 ≥3 次 */
function detectExactRepeats(
  threadId: string,
  threadTitle: string,
  invocations: ToolInvocation[]
): LoopPattern[] {
  const loops: LoopPattern[] = [];

  // 按滑动窗口检测：连续的同工具+同参数
  let streakStart = 0;
  for (let i = 1; i <= invocations.length; i++) {
    const current = invocations[i];
    const prev = invocations[streakStart];

    const sameToolAndArgs = current &&
      current.toolName === prev.toolName &&
      current.argsHash === prev.argsHash;

    if (!sameToolAndArgs && i - streakStart >= 3) {
      // 找到一段重复序列
      const streak = invocations.slice(streakStart, i);
      loops.push({
        threadId,
        threadTitle,
        type: "exact_repeat",
        toolName: streak[0].toolName,
        loopLength: streak.length,
        callIds: streak.map((s) => s.callId),
        description: `${streak[0].toolName}(${streak[0].argsPreview})`,
        severity: streak.length >= 10 ? "critical" : streak.length >= 5 ? "high" : "medium",
      });
    }

    if (!sameToolAndArgs) {
      streakStart = i;
    }
  }

  return loops;
}

/** 检测振荡循环：A→B→A→B 模式（工具名交替 ≥4 次） */
function detectOscillations(
  threadId: string,
  threadTitle: string,
  invocations: ToolInvocation[]
): LoopPattern[] {
  const loops: LoopPattern[] = [];
  if (invocations.length < 4) return loops;

  // 滑动窗口检测 ABAB 模式
  for (let windowSize = 4; windowSize <= Math.min(invocations.length, 20); windowSize += 2) {
    for (let i = 0; i <= invocations.length - windowSize; i++) {
      const window = invocations.slice(i, i + windowSize);
      const toolA = window[0].toolName;
      const toolB = window[1].toolName;

      if (toolA === toolB) continue; // 不是交替

      let isOscillation = true;
      for (let j = 0; j < windowSize; j++) {
        const expected = j % 2 === 0 ? toolA : toolB;
        if (window[j].toolName !== expected) {
          isOscillation = false;
          break;
        }
      }

      if (isOscillation) {
        // 检查是否已被更长循环覆盖
        const alreadyCovered = loops.some(
          (l) => l.threadId === threadId &&
            l.type === "oscillation" &&
            l.callIds.includes(window[0].callId)
        );
        if (!alreadyCovered) {
          loops.push({
            threadId,
            threadTitle,
            type: "oscillation",
            toolName: `${toolA}↔${toolB}`,
            loopLength: windowSize,
            callIds: window.map((w) => w.callId),
            description: `${toolA} → ${toolB} 交替 x${windowSize / 2} 轮`,
            severity: windowSize >= 8 ? "high" : "medium",
          });
        }
        break; // 跳过已被匹配的位置
      }
    }
  }

  return loops;
}

/** 检测无进展循环：同工具连续调用 ≥3 次，且结果长度相同 */
function detectNoProgressLoops(
  threadId: string,
  threadTitle: string,
  invocations: ToolInvocation[],
  _messages: MessageRow[]
): LoopPattern[] {
  const loops: LoopPattern[] = [];

  // 按工具分组连续调用
  let streakStart = 0;
  for (let i = 1; i <= invocations.length; i++) {
    const current = invocations[i];
    const prev = invocations[streakStart];

    const sameTool = current && current.toolName === prev.toolName;
    const differentArgs = current && current.argsHash !== prev.argsHash;

    if ((!sameTool || !differentArgs) && i - streakStart >= 3) {
      const streak = invocations.slice(streakStart, i);

      // 检查结果长度是否全部相同（无进展信号）
      const resultLengths = streak.map((s) => s.resultLength);
      const allSameLength = resultLengths.length >= 3 &&
        resultLengths.every((l) => l > 0 && l === resultLengths[0]);

      if (allSameLength) {
        loops.push({
          threadId,
          threadTitle,
          type: "no_progress",
          toolName: streak[0].toolName,
          loopLength: streak.length,
          callIds: streak.map((s) => s.callId),
          description: `${streak[0].toolName} x${streak.length} 次，结果长度均为 ${resultLengths[0]} 字符`,
          severity: streak.length >= 6 ? "high" : "medium",
        });
      }
    }

    if (!sameTool || !differentArgs) {
      streakStart = i;
    }
  }

  return loops;
}
