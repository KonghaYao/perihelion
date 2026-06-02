//! 工具使用模式分析器——深入探索工具间协作关系和效率模式。
//!
//! 分析维度：
//! 1. **Grep→Read 转化链**：Grep 找到文件后 Read 的命中率，Grep pattern 质量
//! 2. **工具序列模式**：常见工具调用序列（Read→Edit、Grep→Grep→Read 等）
//! 3. **重复读取检测**：同一文件被多次 Read 的比例（缓存效率）
//! 4. **Grep Pattern 质量**：高/低效 pattern 特征
//! 5. **搜索→编辑效率**：从开始搜索到最终 Edit 经过了多少步骤

import type { DefectReport, MessageRow, ToolCallRequest } from "../../types.js";
import { DataLoader } from "../utils/data_loader.js";
import { printSection, printMetric, printTable } from "../utils/report.js";

// ── 数据结构 ──

interface ToolSequenceRecord {
  threadId: string;
  threadTitle: string;
  /** 工具序列（按时间顺序） */
  sequence: { name: string; args: Record<string, string> }[];
}

interface FileReadStats {
  file: string;
  readCount: number;
  /** 被多少个不同会话读取 */
  sessionCount: number;
}

// ── 主分析 ──

export function analyzeToolPatterns(loader: DataLoader): DefectReport[] {
  printSection("工具使用模式分析");

  const threads = loader.loadVisibleThreads();
  const allSequences: ToolSequenceRecord[] = [];
  const readFileCounts: Record<string, { count: number; sessions: Set<string> }> = {};
  const grepPatterns: { pattern: string; path: string; hitRead: boolean; threadId: string }[] = [];
  const toolPairs: Record<string, number> = {};

  // ── 2-gram 工具序列统计 ──
  const bigramCounts: Record<string, number> = {};

  for (const thread of threads) {
    const messages = loader.loadMessages(thread.id);
    const sequence: { name: string; args: Record<string, string> }[] = [];

    for (const msg of messages) {
      if (msg.role !== "assistant") continue;
      const parsed = DataLoader.parseContent(msg.content);
      if (!parsed) continue;
      const toolCalls = DataLoader.extractToolCalls(parsed);

      for (const tc of toolCalls) {
        const args: Record<string, string> = {};
        if (tc.arguments.file_path && typeof tc.arguments.file_path === "string") {
          args.file_path = tc.arguments.file_path;
        }
        if (tc.arguments.pattern && typeof tc.arguments.pattern === "string") {
          args.pattern = tc.arguments.pattern;
        }
        if (tc.arguments.path && typeof tc.arguments.path === "string") {
          args.path = tc.arguments.path;
        }
        if (tc.arguments.command && typeof tc.arguments.command === "string") {
          args.command = tc.arguments.command;
        }

        sequence.push({ name: tc.name, args });

        // Read 计数
        if (tc.name === "Read" && args.file_path) {
          const key = args.file_path;
          if (!readFileCounts[key]) readFileCounts[key] = { count: 0, sessions: new Set() };
          readFileCounts[key].count++;
          readFileCounts[key].sessions.add(thread.id);
        }
      }
    }

    if (sequence.length >= 2) {
      allSequences.push({ threadId: thread.id, threadTitle: thread.title || "", sequence });

      // bigram
      for (let i = 0; i < sequence.length - 1; i++) {
        const pair = `${sequence[i].name}→${sequence[i + 1].name}`;
        bigramCounts[pair] = (bigramCounts[pair] || 0) + 1;
      }
    }

    // Grep→Read 链
    for (let i = 0; i < sequence.length; i++) {
      if (sequence[i].name === "Grep" && sequence[i].args.pattern) {
        let hitRead = false;
        // 后续 5 步内是否有 Read 且路径匹配
        for (let j = i + 1; j < Math.min(i + 6, sequence.length); j++) {
          if (sequence[j].name === "Read" && sequence[j].args.file_path) {
            hitRead = true;
            break;
          }
        }
        grepPatterns.push({
          pattern: sequence[i].args.pattern,
          path: sequence[i].args.path || "",
          hitRead,
          threadId: thread.id,
        });
      }
    }
  }

  // ── 输出报告 ──

  // 1. 工具调用频次
  printSection("工具调用频次");
  const toolCounts: Record<string, number> = {};
  for (const seq of allSequences) {
    for (const t of seq.sequence) {
      toolCounts[t.name] = (toolCounts[t.name] || 0) + 1;
    }
  }
  const totalCalls = Object.values(toolCounts).reduce((a, b) => a + b, 0);
  const toolRows = Object.entries(toolCounts)
    .sort((a, b) => b[1] - a[1])
    .slice(0, 12)
    .map(([name, count]) => [name, String(count), (count / totalCalls * 100).toFixed(1) + "%"]);
  printTable(["工具", "调用次数", "占比"], toolRows);

  // 2. 工具序列 Bigram Top 20
  printSection("工具序列 (2-gram) Top 20");
  const topBigrams = Object.entries(bigramCounts)
    .sort((a, b) => b[1] - a[1])
    .slice(0, 20);
  const bigramRows = topBigrams.map(([pair, count]) => {
    const [from, to] = pair.split("→");
    return [from, "→", to, String(count)];
  });
  printTable(["工具 A", "", "工具 B", "次数"], bigramRows);

  // 3. Grep→Read 转化率
  printSection("Grep→Read 转化率");
  const grepWithRead = grepPatterns.filter((g) => g.hitRead).length;
  const totalGrep = grepPatterns.length;
  printMetric("Grep 总调用", totalGrep);
  printMetric("后续 5 步内 Read", grepWithRead);
  printMetric("转化率", (grepWithRead / (totalGrep || 1) * 100).toFixed(1) + "%");

  // Grep pattern 无后续 Read 的比例
  const grepNoRead = totalGrep - grepWithRead;
  printMetric("无后续 Read", grepNoRead, ` (${(grepNoRead / (totalGrep || 1) * 100).toFixed(1)}%)`);

  // 4. Grep Pattern 频次 Top 20
  printSection("Grep Pattern 频次 Top 20");
  const patternCounts: Record<string, { count: number; hitRead: number }> = {};
  for (const g of grepPatterns) {
    const normalized = g.pattern.replace(/["']/g, "").trim().toLowerCase();
    if (!normalized) continue;
    if (!patternCounts[normalized]) patternCounts[normalized] = { count: 0, hitRead: 0 };
    patternCounts[normalized].count++;
    if (g.hitRead) patternCounts[normalized].hitRead++;
  }
  const topPatterns = Object.entries(patternCounts)
    .sort((a, b) => b[1].count - a[1].count)
    .slice(0, 20);
  const patternRows = topPatterns.map(([p, { count, hitRead }]) => [
    p.slice(0, 40),
    String(count),
    `${hitRead}/${count}`,
    (hitRead / count * 100).toFixed(0) + "%",
  ]);
  printTable(["Pattern", "次数", "命中Read", "转化率"], patternRows);

  // 5. 重复读取 Top 20
  printSection("重复读取 Top 20（同文件多次 Read）");
  const topReReads = Object.entries(readFileCounts)
    .filter(([, v]) => v.count > 2)
    .sort((a, b) => b[1].count - a[1].count)
    .slice(0, 20);
  const reReadRows = topReReads.map(([file, { count, sessions }]) => {
    // 取相对路径
    const short = file.replace(/^\/Users\/[\w]+\//, "~/");
    return [short.slice(-45), String(count), String(sessions.size)];
  });
  printTable(["文件", "Read 次数", "会话数"], reReadRows);

  // 6. 搜索→编辑效率
  printSection("搜索→编辑效率（每个会话的 Grep+Glob 数 vs Edit+Write 数）");
  let totalSearch = 0;
  let totalEdit = 0;
  let highSearchLowEdit = 0;
  for (const seq of allSequences) {
    const searchCount = seq.sequence.filter((t) => ["Grep", "Glob", "Read"].includes(t.name)).length;
    const editCount = seq.sequence.filter((t) => ["Edit", "Write"].includes(t.name)).length;
    totalSearch += searchCount;
    totalEdit += editCount;
    if (searchCount >= 10 && editCount <= 2) highSearchLowEdit++;
  }
  printMetric("总搜索调用 (Grep+Glob+Read)", totalSearch);
  printMetric("总编辑调用 (Edit+Write)", totalEdit);
  printMetric("搜索/编辑比", (totalSearch / (totalEdit || 1)).toFixed(1) + ":1");
  printMetric("高搜索低编辑会话 (≥10搜索, ≤2编辑)", highSearchLowEdit);

  // 7. Read 文件类型分布
  printSection("Read 文件类型分布");
  const extCounts: Record<string, number> = {};
  for (const [, { count }] of Object.entries(readFileCounts)) {
    // 用 file key 的后缀
  }
  // 重新统计
  const extMap: Record<string, number> = {};
  for (const file of Object.keys(readFileCounts)) {
    const ext = file.split(".").pop()?.toLowerCase() || "(no ext)";
    extMap[ext] = (extMap[ext] || 0) + readFileCounts[file].count;
  }
  const topExt = Object.entries(extMap).sort((a, b) => b[1] - a[1]).slice(0, 12);
  const extRows = topExt.map(([ext, count]) => [`.${ext}`, String(count), (count / (totalSearch || 1) * 100).toFixed(1) + "%"]);
  printTable(["扩展名", "Read 次数", "占比"], extRows);

  // 8. 工具序列长度分布
  printSection("工具序列长度分布");
  const seqLenBins = [
    { label: "1-5", min: 1, max: 6 },
    { label: "6-10", min: 6, max: 11 },
    { label: "11-20", min: 11, max: 21 },
    { label: "21-50", min: 21, max: 51 },
    { label: "50+", min: 51, max: Infinity },
  ];
  const seqLenRows = seqLenBins.map((bin) => {
    const count = allSequences.filter((s) => s.sequence.length >= bin.min && s.sequence.length < bin.max).length;
    return [bin.label, String(count), (count / allSequences.length * 100).toFixed(1) + "%"];
  });
  printTable(["工具数", "会话数", "占比"], seqLenRows);

  // ── 缺陷报告 ──
  const reports: DefectReport[] = [];

  // Grep→Read 转化率过低
  const grepHitRate = grepWithRead / (totalGrep || 1);
  if (grepHitRate < 0.4) {
    reports.push({
      id: "TOOL-001",
      severity: "medium",
      category: "搜索效率",
      title: "Grep→Read 转化率偏低",
      description: `${totalGrep} 次 Grep 调用中仅 ${grepWithRead} 次（${(grepHitRate * 100).toFixed(1)}%）在后续 5 步内触发了 Read。部分 Grep 可能是"盲搜"——搜索了但没有跟进阅读匹配结果。`,
      evidence: [
        `转化率: ${(grepHitRate * 100).toFixed(1)}%`,
        `无后续 Read: ${grepNoRead} 次`,
        `高 Pattern Top 3: ${topPatterns.slice(0, 3).map(([p, d]) => `"${p}" ${d.count}x`).join(", ")}`,
      ],
      affectedSessions: grepPatterns.filter((g) => !g.hitRead).map((g) => g.threadId).filter((v, i, a) => a.indexOf(v) === i),
      recommendation: "Agent 应在 Grep 返回结果后优先 Read 匹配文件，而非继续发起更多 Grep。系统提示可加入'Grep 后至少 Read 一个匹配文件'的约束。",
      confidence: 0.5,
    });
  }

  // 高搜索低编辑
  if (highSearchLowEdit > 5) {
    reports.push({
      id: "TOOL-002",
      severity: "low",
      category: "搜索效率",
      title: "部分会话大量搜索但几乎不编辑",
      description: `${highSearchLowEdit} 个会话中搜索类调用 (Grep+Glob+Read) ≥10 次，但编辑 (Edit+Write) ≤2 次。Agent 在"搜索模式"中停留过久，可能是因为不确定目标或搜索策略低效。`,
      evidence: [
        `高搜索低编辑会话: ${highSearchLowEdit}`,
        `全局搜索/编辑比: ${(totalSearch / (totalEdit || 1)).toFixed(1)}:1`,
      ],
      affectedSessions: [],
      recommendation: "限制连续搜索轮次（如 5 轮后强制切换到编辑模式）。Agent 应在搜索前明确'我在找什么'，找到后立即行动。",
      confidence: 0.45,
    });
  }

  // 重复读取
  const multiReadFiles = Object.entries(readFileCounts).filter(([, v]) => v.count >= 5);
  if (multiReadFiles.length > 10) {
    reports.push({
      id: "TOOL-003",
      severity: "low",
      category: "缓存效率",
      title: "大量文件被重复读取 5 次以上",
      description: `${multiReadFiles.length} 个文件被 Read ≥5 次。Agent 可能在不同轮次中重复读取同一文件，浪费 token。理想情况下，Agent 应在首次 Read 后记住文件内容。`,
      evidence: multiReadFiles.sort((a, b) => b[1].count - a[1].count).slice(0, 5).map(([f, v]) =>
        `${f.split("/").slice(-2).join("/")}: ${v.count}次, ${v.sessions.size}会话`
      ),
      affectedSessions: [],
      recommendation: "考虑在 Agent 上下文中缓存已读文件内容（或摘要），避免重复 Read。系统提示可加入'已读取的文件无需再次 Read'的指令。",
      confidence: 0.4,
    });
  }

  return reports;
}
