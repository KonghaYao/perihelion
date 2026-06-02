//! 会话效率分析器。
//!
//! 分析维度：
//! 1. 会话生存曲线（消息数分布 → 多少会话在早期就结束？）
//! 2. 超长会话特征（什么类型的任务需要 500+ 消息？）
//! 3. 会话效率指标（工具调用密度、错误密度、每轮信息产出）
//! 4. SubAgent 使用模式

import type { SessionProfile, DefectReport } from "../../types.js";
import { DataLoader } from "../utils/data_loader.js";
import { printSection, printMetric, printTable, printFinding, printWarning } from "../utils/report.js";

export function analyzeSessionEfficiency(loader: DataLoader): DefectReport[] {
  printSection("会话效率分析");

  const threads = loader.loadVisibleThreads();
  const profiles: SessionProfile[] = [];

  for (const thread of threads) {
    const profile = buildSessionProfile(loader, thread.id, thread);
    profiles.push(profile);
  }

  // Step 1: 消息数分布（会话生存曲线）
  printSection("会话消息数分布（生存曲线）");

  const bins = [
    { label: "1-2", min: 1, max: 2 },
    { label: "3-5", min: 3, max: 5 },
    { label: "6-10", min: 6, max: 10 },
    { label: "11-20", min: 11, max: 20 },
    { label: "21-50", min: 21, max: 50 },
    { label: "51-100", min: 51, max: 100 },
    { label: "101-200", min: 101, max: 200 },
    { label: "201-500", min: 201, max: 500 },
    { label: "500+", min: 501, max: Infinity },
  ];

  const distribution = bins.map((bin) => {
    const count = profiles.filter(
      (p) => p.totalMessages >= bin.min && p.totalMessages <= bin.max
    ).length;
    return { label: bin.label, count, pct: (count / profiles.length * 100) };
  });

  const distRows = distribution.map((d) => [
    d.label,
    String(d.count),
    d.pct.toFixed(1) + "%",
    "█".repeat(Math.round(d.pct / 2)) || "░",
  ]);
  printTable(["消息数", "会话数", "占比", "分布"], distRows);

  // Step 2: 超短会话分析（可能是测试 / 放弃 / 快速问答）
  printSection("超短会话分析（≤2 条消息）");

  const shortSessions = profiles.filter((p) => p.totalMessages <= 2);
  const shortPct = (shortSessions.length / profiles.length * 100).toFixed(1);
  printMetric("超短会话数", shortSessions.length, ` (${shortPct}%)`);

  // 分析超短会话标题模式
  const shortTitlePatterns = new Map<string, number>();
  for (const s of shortSessions) {
    const title = s.title?.toLowerCase() || "";
    if (title.startsWith("hello") || title.startsWith("hi") || title.startsWith("test")) {
      shortTitlePatterns.set("测试/打招呼", (shortTitlePatterns.get("测试/打招呼") || 0) + 1);
    } else if (title.startsWith("/")) {
      shortTitlePatterns.set("slash 命令", (shortTitlePatterns.get("slash 命令") || 0) + 1);
    } else {
      shortTitlePatterns.set("正常提问", (shortTitlePatterns.get("正常提问") || 0) + 1);
    }
  }
  if (shortTitlePatterns.size > 0) {
    const shortRows = [...shortTitlePatterns.entries()]
      .sort((a, b) => b[1] - a[1])
      .map(([pattern, count]) => [pattern, String(count)]);
    printTable(["超短会话类型", "数量"], shortRows);
  }

  // Step 3: 超长会话分析
  printSection("超长会话分析（>200 条消息）");

  const longSessions = profiles
    .filter((p) => p.totalMessages > 200)
    .sort((a, b) => b.totalMessages - a.totalMessages);

  printMetric("超长会话数", longSessions.length);
  if (longSessions.length > 0) {
    const longRows = longSessions.slice(0, 10).map((s) => [
      s.threadId.slice(0, 12) + "...",
      (s.title || "").slice(0, 40),
      String(s.totalMessages),
      String(s.toolErrors),
      s.toolErrorRate.toFixed(2),
      String(s.subAgentCount),
      `${s.durationMinutes.toFixed(0)}min`,
    ]);
    printTable(
      ["Session", "标题", "消息数", "工具错误", "错误率", "子Agent", "时长"],
      longRows
    );

    // 超长会话的共同特征
    const avgErrors = longSessions.reduce((a, p) => a + p.toolErrors, 0) / longSessions.length;
    const avgSubAgents = longSessions.reduce((a, p) => a + p.subAgentCount, 0) / longSessions.length;
    printMetric("超长会话平均错误数", avgErrors.toFixed(1));
    printMetric("超长会话平均子 Agent 数", avgSubAgents.toFixed(1));
  }

  // Step 4: 工具调用密度分析
  printSection("工具调用密度分析");

  const toolDensityBuckets = [
    { label: "低 (<2/轮)", test: (p: SessionProfile) => p.avgToolsPerTurn < 2 },
    { label: "中 (2-4/轮)", test: (p: SessionProfile) => p.avgToolsPerTurn >= 2 && p.avgToolsPerTurn < 4 },
    { label: "高 (4-8/轮)", test: (p: SessionProfile) => p.avgToolsPerTurn >= 4 && p.avgToolsPerTurn < 8 },
    { label: "极高 (>8/轮)", test: (p: SessionProfile) => p.avgToolsPerTurn >= 8 },
  ];

  for (const bucket of toolDensityBuckets) {
    const count = profiles.filter(bucket.test).length;
    printMetric(bucket.label, count, ` 会话`);
  }

  // 高密度会话的 top 工具
  const highDensity = profiles.filter((p) => p.avgToolsPerTurn >= 8);
  if (highDensity.length > 0) {
    const allTools = new Map<string, number>();
    for (const p of highDensity) {
      for (const [tool, count] of Object.entries(p.toolFrequency)) {
        allTools.set(tool, (allTools.get(tool) || 0) + count);
      }
    }
    const topTools = [...allTools.entries()]
      .sort((a, b) => b[1] - a[1])
      .slice(0, 5)
      .map(([tool, count]) => [tool, String(count)]);
    printTable(["高频工具", "调用次数"], topTools);
  }

  // Step 5: SubAgent 使用统计
  printSection("SubAgent 使用模式");

  const sessionsWithSubAgents = profiles.filter((p) => p.subAgentCount > 0);
  printMetric("使用 SubAgent 的会话", sessionsWithSubAgents.length, ` (${(sessionsWithSubAgents.length / profiles.length * 100).toFixed(1)}%)`);

  if (sessionsWithSubAgents.length > 0) {
    const subAgentCounts = sessionsWithSubAgents.map((p) => p.subAgentCount);
    printMetric("平均 SubAgent 数", (subAgentCounts.reduce((a, b) => a + b, 0) / subAgentCounts.length).toFixed(1));
    printMetric("最多 SubAgent 数", Math.max(...subAgentCounts));
  }

  // Step 6: 活跃时间段分析
  printSection("会话活跃时间段");

  const hourDistribution = new Map<number, number>();
  for (const p of profiles) {
    const hour = p.createdAt.getUTCHours();
    // 转换为 UTC+8
    const localHour = (hour + 8) % 24;
    hourDistribution.set(localHour, (hourDistribution.get(localHour) || 0) + 1);
  }

  const hourRows = [...hourDistribution.entries()]
    .sort((a, b) => a[0] - b[0])
    .map(([hour, count]) => [
      `${hour}:00-${hour}:59`,
      String(count),
      "█".repeat(Math.round(count / profiles.length * 100)) || "░",
    ]);
  printTable(["时间段", "会话数", "密度"], hourRows);

  // 生成报告
  const reports: DefectReport[] = [];

  // 超短会话比例过高
  if (shortSessions.length > profiles.length * 0.3) {
    reports.push({
      id: "EFF-001",
      severity: "medium",
      category: "会话效率",
      title: "超短会话比例过高",
      description: `${shortPct}% 的会话只有 ≤2 条消息。大量会话可能是在测试或因初始化问题被放弃。`,
      evidence: [`超短会话: ${shortSessions.length}/${profiles.length}`],
      affectedSessions: shortSessions.map((s) => s.threadId),
      recommendation: "在欢迎页面增加更明确的使用引导。检查初始化流程是否存在阻塞点。",
      confidence: 0.5,
    });
  }

  // 超长会话的上下文管理问题
  if (longSessions.length > 0) {
    const highErrorLong = longSessions.filter((p) => p.toolErrorRate > 0.1);
    if (highErrorLong.length > 0) {
      reports.push({
        id: "EFF-002",
        severity: "high",
        category: "上下文管理",
        title: "超长会话工具错误率飙升",
        description: `${highErrorLong.length}/${longSessions.length} 个超长会话的工具错误率 >10%。上下文膨胀可能导致 Agent 判断力下降。`,
        evidence: highErrorLong.slice(0, 3).map((p) =>
          `${p.threadId.slice(0, 12)}: ${p.totalMessages}条消息, 错误率${(p.toolErrorRate * 100).toFixed(0)}%`
        ),
        affectedSessions: highErrorLong.map((p) => p.threadId),
        recommendation: "优化 compact 策略的触发阈值。对长会话增加自动摘要频率。考虑在错误率上升时主动提醒用户开始新会话。",
        confidence: 0.75,
      });
    }
  }

  return reports;
}

// ── Helpers ──

function buildSessionProfile(
  loader: DataLoader,
  threadId: string,
  thread: any
): SessionProfile {
  const messages = loader.loadMessages(threadId);
  const subAgents = loader.loadSubAgents(threadId);

  let userMsgs = 0;
  let assistantMsgs = 0;
  let toolMsgs = 0;
  let systemMsgs = 0;
  let toolErrors = 0;
  let totalToolCalls = 0;
  let totalReasoningChars = 0;
  const toolFrequency: Record<string, number> = {};

  // 连续错误检测
  let consecutiveErrors = 0;
  let maxConsecutiveErrors = 0;

  for (const msg of messages) {
    switch (msg.role) {
      case "user":
        userMsgs++;
        break;
      case "assistant": {
        assistantMsgs++;
        const parsed = DataLoader.parseContent(msg.content);
        const toolCalls = DataLoader.extractToolCalls(parsed);
        totalToolCalls += toolCalls.length;
        for (const tc of toolCalls) {
          toolFrequency[tc.name] = (toolFrequency[tc.name] || 0) + 1;
        }
        // 提取 reasoning
        if (parsed && "content" in parsed && Array.isArray((parsed as any).content)) {
          for (const block of (parsed as any).content) {
            if ((block.type === "reasoning" || block.type === "thinking") && block.text) {
              totalReasoningChars += block.text.length;
            }
          }
        }
        break;
      }
      case "tool": {
        toolMsgs++;
        const parsed = DataLoader.parseContent(msg.content);
        const errInfo = DataLoader.parseToolError(parsed);
        if (errInfo?.isError) {
          toolErrors++;
          consecutiveErrors++;
          maxConsecutiveErrors = Math.max(maxConsecutiveErrors, consecutiveErrors);
        } else {
          consecutiveErrors = 0;
        }
        break;
      }
      case "system":
        systemMsgs++;
        break;
    }
  }

  const createdAt = new Date(thread.created_at);
  const updatedAt = new Date(thread.updated_at);
  const durationMinutes = (updatedAt.getTime() - createdAt.getTime()) / 60000;
  const turns = Math.max(userMsgs, 1);

  return {
    threadId,
    title: thread.title,
    cwd: thread.cwd,
    createdAt,
    updatedAt,
    durationMinutes,
    totalMessages: messages.length,
    userMessages: userMsgs,
    assistantMessages: assistantMsgs,
    toolMessages: toolMsgs,
    systemMessages: systemMsgs,
    toolErrors,
    toolErrorRate: toolErrors / (toolMsgs || 1),
    subAgentCount: subAgents.length,
    avgToolsPerTurn: totalToolCalls / turns,
    maxConsecutiveErrors,
    agentStatus: thread.agent_status,
    toolFrequency,
    totalReasoningChars,
  };
}
