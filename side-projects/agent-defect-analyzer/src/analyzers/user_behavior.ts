//! 用户行为信号分析器。
//!
//! 分析维度：
//! 1. 会话放弃率（用户多久后不再回来？）
//! 2. 重复话题检测（同一 cwd 下反复提问相似问题）
//! 3. 用户消息长度分布（长需求 vs 短指令）
//! 4. 活跃 cwd 分布（哪些项目最常使用 Agent？）
//! 5. Agent 回答效率（用户消息 vs agent 回复的比例）

import type { DefectReport } from "../../types.js";
import { DataLoader } from "../utils/data_loader.js";
import { printSection, printMetric, printTable, printWarning } from "../utils/report.js";

export function analyzeUserBehavior(loader: DataLoader): DefectReport[] {
  printSection("用户行为信号分析");

  const threads = loader.loadVisibleThreads();

  // Step 1: 活跃项目（cwd）分布
  printSection("活跃项目分布（Top 15）");

  const cwdCount = new Map<string, number>();
  for (const thread of threads) {
    const cwd = thread.cwd || "(unknown)";
    // 取最后两级目录
    const shortCwd = cwd.split("/").slice(-2).join("/");
    cwdCount.set(shortCwd, (cwdCount.get(shortCwd) || 0) + 1);
  }

  const cwdRows = [...cwdCount.entries()]
    .sort((a, b) => b[1] - a[1])
    .slice(0, 15)
    .map(([cwd, count]) => [cwd, String(count)]);
  printTable(["项目目录", "会话数"], cwdRows);

  // Step 2: 用户消息长度分布
  printSection("用户消息长度分布");

  const userMsgLengths: number[] = [];
  const userMsgSamples: string[] = [];

  for (const thread of threads) {
    loader.processMessages(thread.id, (msg, idx) => {
      if (msg.role === "user") {
        const parsed = DataLoader.parseContent(msg.content);
        if (parsed && "content" in parsed) {
          let text = "";
          const content = (parsed as any).content;
          if (typeof content === "string") {
            text = content;
          } else if (Array.isArray(content)) {
            text = content
              .filter((b: any) => b.type === "text")
              .map((b: any) => b.text || "")
              .join("");
          }
          userMsgLengths.push(text.length);
          if (userMsgSamples.length < 20 && text.length > 0) {
            userMsgSamples.push(text.slice(0, 60).replace(/\n/g, " "));
          }
        }
      }
    });
  }

  const lengthBuckets = [
    { label: "极短 (1-20字)", min: 1, max: 20 },
    { label: "短 (21-50字)", min: 21, max: 50 },
    { label: "中 (51-150字)", min: 51, max: 150 },
    { label: "长 (151-500字)", min: 151, max: 500 },
    { label: "超长 (>500字)", min: 501, max: Infinity },
  ];

  const lenRows = lengthBuckets.map((bucket) => {
    const count = userMsgLengths.filter(
      (l) => l >= bucket.min && l <= bucket.max
    ).length;
    return [bucket.label, String(count), (count / (userMsgLengths.length || 1) * 100).toFixed(1) + "%"];
  });
  printTable(["长度区间", "消息数", "占比"], lenRows);

  if (userMsgLengths.length > 0) {
    const avgLen = userMsgLengths.reduce((a, b) => a + b, 0) / userMsgLengths.length;
    const medianLen = userMsgLengths.sort((a, b) => a - b)[Math.floor(userMsgLengths.length / 2)];
    printMetric("平均用户消息长度", avgLen.toFixed(0), " 字");
    printMetric("中位数消息长度", medianLen, " 字");
  }

  // Step 3: 用户-Agent 交互比
  printSection("用户-Agent 交互比");

  const ratios: { threadId: string; title: string; userMsgs: number; totalMsgs: number; ratio: number }[] = [];

  for (const thread of threads) {
    const messages = loader.loadMessages(thread.id);
    const userMsgs = messages.filter((m) => m.role === "user").length;
    const toolMsgs = messages.filter((m) => m.role === "tool").length;
    if (userMsgs > 0) {
      // 交互比 = 总非用户消息 / 用户消息
      // 高比值意味着 Agent 做了大量工作才回应用户
      ratios.push({
        threadId: thread.id,
        title: (thread.title || "").slice(0, 40),
        userMsgs,
        totalMsgs: messages.length,
        ratio: messages.length / userMsgs,
      });
    }
  }

  // 按交互比排序
  const sortedByRatio = [...ratios].sort((a, b) => b.ratio - a.ratio);

  printMetric("平均交互比", (ratios.reduce((a, r) => a + r.ratio, 0) / (ratios.length || 1)).toFixed(1), " (总消息/用户消息)");
  printMetric("最高交互比", sortedByRatio[0]?.ratio.toFixed(1) || "N/A");

  // 极高交互比 top 10
  const topRatioRows = sortedByRatio.slice(0, 10).map((r) => [
    r.threadId.slice(0, 12) + "...",
    r.title.slice(0, 30),
    String(r.userMsgs),
    String(r.totalMsgs),
    r.ratio.toFixed(1),
  ]);
  printTable(["Session", "标题", "用户消息", "总消息", "交互比"], topRatioRows);

  // Step 4: 重复模式检测（相似标题的会话）
  printSection("重复话题检测");

  const titleGroups = new Map<string, { count: number; sessions: string[] }>();
  for (const thread of threads) {
    const title = (thread.title || "").toLowerCase().trim();
    if (title.length < 3) continue;
    // 取前 30 字符作为分组键
    const key = title.slice(0, 30);
    if (!titleGroups.has(key)) {
      titleGroups.set(key, { count: 0, sessions: [] });
    }
    const group = titleGroups.get(key)!;
    group.count++;
    if (group.sessions.length < 5) {
      group.sessions.push(thread.id);
    }
  }

  const repeatedTopics = [...titleGroups.entries()]
    .filter(([, group]) => group.count >= 2)
    .sort((a, b) => b[1].count - a[1].count);

  if (repeatedTopics.length > 0) {
    printWarning("发现重复话题", `${repeatedTopics.length} 个话题被多次讨论`);
    const repeatRows = repeatedTopics.slice(0, 10).map(([title, group]) => [
      title.slice(0, 40),
      String(group.count),
    ]);
    printTable(["话题", "重复次数"], repeatRows);
  }

  // Step 5: 会话时段模式
  printSection("每日会话数量趋势");

  const dailyCount = new Map<string, number>();
  for (const thread of threads) {
    const date = thread.created_at.slice(0, 10); // YYYY-MM-DD
    dailyCount.set(date, (dailyCount.get(date) || 0) + 1);
  }

  const dailyRows = [...dailyCount.entries()]
    .sort((a, b) => a[0].localeCompare(b[0]))
    .slice(-14) // 最近 14 天
    .map(([date, count]) => [
      date,
      String(count),
      "█".repeat(Math.min(count, 50)),
    ]);
  printTable(["日期", "会话数", "柱状图"], dailyRows);

  // 生成缺陷报告
  const reports: DefectReport[] = [];

  // 超高交互比会话
  const extremeRatio = ratios.filter((r) => r.ratio > 50);
  if (extremeRatio.length > 0) {
    reports.push({
      id: "UX-001",
      severity: "medium",
      category: "交互效率",
      title: "部分会话交互比极高",
      description: `${extremeRatio.length} 个会话的交互比 >50（每条用户消息触发 50+ 条消息）。Agent 可能在单轮中过度使用工具，缺乏效率。`,
      evidence: extremeRatio.slice(0, 3).map((r) =>
        `${r.title}: ${r.ratio.toFixed(0)}x (${r.totalMsgs} messages / ${r.userMsgs} user)`
      ),
      affectedSessions: extremeRatio.map((r) => r.threadId),
      recommendation: "分析这些会话中是否存在重复工具调用、无效搜索循环。在 agent 策略中增加'单轮工具调用上限'或'进展检测'机制。",
      confidence: 0.6,
    });
  }

  // 重复话题
  if (repeatedTopics.length > 10) {
    reports.push({
      id: "UX-002",
      severity: "low",
      category: "知识管理",
      title: "大量重复话题表明知识积累不足",
      description: `${repeatedTopics.length} 个话题被反复讨论。用户需要反复问 Agent 相似问题，表明 Agent 缺乏跨会话记忆。`,
      evidence: repeatedTopics.slice(0, 5).map(([title, group]) =>
        `"${title}" - ${group.count}次`
      ),
      affectedSessions: repeatedTopics.flatMap(([, g]) => g.sessions),
      recommendation: "实现跨会话记忆机制（如 CLAUDE.md 自动积累）。在会话开始时自动加载相关历史上下文。",
      confidence: 0.4,
    });
  }

  // 极短用户消息比例
  const veryShortMsgs = userMsgLengths.filter((l) => l <= 10).length;
  const veryShortPct = veryShortMsgs / (userMsgLengths.length || 1) * 100;
  if (veryShortPct > 30) {
    reports.push({
      id: "UX-003",
      severity: "low",
      category: "用户交互",
      title: "大量极短用户消息",
      description: `${veryShortPct.toFixed(0)}% 的用户消息 ≤10 字。用户倾向于发短指令而非详细描述需求。`,
      evidence: [`极短消息: ${veryShortMsgs}/${userMsgLengths.length}`],
      affectedSessions: [],
      recommendation: "在 UI 中增加需求引导（如建议描述格式的 placeholder）。优化 Agent 对短指令的理解能力。",
      confidence: 0.35,
    });
  }

  return reports;
}
