//! Grep 搜索效能分析器——测量 Grep 对会话任务完成的实际贡献。
//!
//! 核心指标：
//! 1. **Grep→Read→Edit 闭环率**：Grep 后 → Read 匹配文件 → Edit 该文件（完整搜索-理解-修改链）
//! 2. **文件发现率**：最终被 Edit 的文件中，有多少是通过 Grep 发现的（vs Agent 直接 Read 已知文件）
//! 3. **重复搜索率**：同一 pattern 在同一会话中被 Grep 多次的频率（暗示首次搜索未命中目标）
//! 4. **Grep 精确度**：宽泛 pattern vs 精确 pattern 的转化率对比
//! 5. **时间趋势**：随着项目增长，Grep 效能是否在下降

import type { DefectReport } from "../../types.js";
import { DataLoader } from "../utils/data_loader.js";
import { printSection, printMetric, printTable } from "../utils/report.js";

// ── 数据结构 ──

interface GrepCall {
  pattern: string;
  path: string;
  threadId: string;
  timestamp: string;
  /** 后续 10 步内 Read 的文件集合 */
  subsequentReads: string[];
  /** 后续 20 步内 Edit 的文件集合 */
  subsequentEdits: string[];
  /** 该 Grep 是否通过后续 Read→Edit 形成闭环 */
  formedClosure: boolean;
}

interface FileEditOrigin {
  file: string;
  /** 首次发现方式：grep-read（通过 Grep 发现）或 direct-read（直接 Read）或 unknown */
  discoveryMethod: "grep-read" | "direct-read" | "unknown";
}

interface SessionGrepStats {
  threadId: string;
  threadTitle: string;
  threadDate: string;
  grepCount: number;
  closureCount: number;
  closureRate: number;
  repeatGrepCount: number;
  editedFiles: number;
  filesDiscoveredViaGrep: number;
  grepDiscoveryRate: number;
  precisePatternCount: number;
  vaguePatternCount: number;
}

// ── Pattern 精确度分类 ──

/** 判断 Grep pattern 是精确还是宽泛 */
function classifyPattern(pattern: string): "precise" | "vague" | "structural" {
  const p = pattern.trim().toLowerCase();

  // 结构性搜索：搜索函数定义、类型声明等（中等精确度）
  if (/^(fn |pub fn |async fn |pub async fn |struct |enum |impl |trait |mod |use |type )/.test(p)) {
    return "structural";
  }

  // 宽泛模式：纯通用词或正则通配
  const vaguePatterns = [
    /^(todo|fixme|hack|xxx|bug|error|test|config|util|helper|main|mod|setup)$/i,
    /^(pub fn|fn |async|impl|struct|enum|use )$/i,
    /^\.\*$/,
    /^\^?$/,
    /^\w{1,3}$/, // 太短的词
  ];
  if (vaguePatterns.some((re) => re.test(p))) return "vague";

  // 正则元字符多 = 更宽泛
  const metaChars = (p.match(/[|*+?{}()\[\]\\]/g) || []).length;
  if (metaChars >= 3) return "vague";

  // 精确模式：具体标识符、路径、函数名
  if (/^[\w._:-]+$/.test(p) && p.length > 3) return "precise";
  if (/^[\w._:-]+(\.[\w._:-]+)*$/.test(p)) return "precise";

  // 默认中等
  return "structural";
}

// ── 主分析 ──

export function analyzeGrepEffectiveness(loader: DataLoader): DefectReport[] {
  printSection("Grep 搜索效能分析");

  const threads = loader.loadVisibleThreads();
  const sessionStats: SessionGrepStats[] = [];
  const allGreps: GrepCall[] = [];

  for (const thread of threads) {
    const messages = loader.loadMessages(thread.id);
    const toolSequence: { name: string; args: Record<string, string>; msgIdx: number }[] = [];

    // 构建工具序列
    for (let msgIdx = 0; msgIdx < messages.length; msgIdx++) {
      const msg = messages[msgIdx];
      if (msg.role !== "assistant") continue;
      const parsed = DataLoader.parseContent(msg.content);
      if (!parsed) continue;
      const toolCalls = DataLoader.extractToolCalls(parsed);
      for (const tc of toolCalls) {
        const args: Record<string, string> = {};
        if (typeof tc.arguments.file_path === "string") args.file_path = tc.arguments.file_path;
        if (typeof tc.arguments.pattern === "string") args.pattern = tc.arguments.pattern;
        if (typeof tc.arguments.path === "string") args.path = tc.arguments.path;
        toolSequence.push({ name: tc.name, args, msgIdx });
      }
    }

    // 分析每个 Grep 调用
    const greps: GrepCall[] = [];
    const editedFilesSet = new Set<string>();
    const readBeforeEdit = new Map<string, string>(); // file → discovery method

    for (let i = 0; i < toolSequence.length; i++) {
      const tc = toolSequence[i];

      // 记录 Edit 目标文件
      if (tc.name === "Edit" && tc.args.file_path) {
        editedFilesSet.add(tc.args.file_path);
      }

      if (tc.name !== "Grep" || !tc.args.pattern) continue;

      const subsequentReads: string[] = [];
      const subsequentEdits: string[] = [];
      let formedClosure = false;

      // 检查后续 20 步
      for (let j = i + 1; j < Math.min(i + 21, toolSequence.length); j++) {
        const next = toolSequence[j];
        if (next.name === "Read" && next.args.file_path) {
          subsequentReads.push(next.args.file_path);
          readBeforeEdit.set(next.args.file_path, "grep-read");
        }
        if (next.name === "Edit" && next.args.file_path) {
          subsequentEdits.push(next.args.file_path);
          // 检查该 Edit 的文件是否在之前的 Read 中（即 Grep→Read→Edit 闭环）
          if (subsequentReads.includes(next.args.file_path)) {
            formedClosure = true;
          }
        }
      }

      // 对于没有后续 Read 的，标记 direct-read
      for (let j = i + 1; j < Math.min(i + 21, toolSequence.length); j++) {
        const next = toolSequence[j];
        if (next.name === "Read" && next.args.file_path && !readBeforeEdit.has(next.args.file_path)) {
          readBeforeEdit.set(next.args.file_path, "direct-read");
        }
      }

      const grep: GrepCall = {
        pattern: tc.args.pattern,
        path: tc.args.path || "",
        threadId: thread.id,
        timestamp: thread.created_at,
        subsequentReads,
        subsequentEdits,
        formedClosure,
      };
      greps.push(grep);
      allGreps.push(grep);
    }

    // 计算重复搜索
    const patternInSession: Record<string, number> = {};
    for (const g of greps) {
      const normalized = g.pattern.replace(/["']/g, "").trim().toLowerCase();
      patternInSession[normalized] = (patternInSession[normalized] || 0) + 1;
    }
    const repeatGrepCount = Object.values(patternInSession).filter((c) => c > 1).reduce((a, c) => a + c, 0);

    // 文件发现率
    const editedFiles = [...editedFilesSet];
    let filesDiscoveredViaGrep = 0;
    for (const f of editedFiles) {
      if (readBeforeEdit.get(f) === "grep-read") filesDiscoveredViaGrep++;
    }

    // Pattern 精确度
    let preciseCount = 0;
    let vagueCount = 0;
    for (const g of greps) {
      const cls = classifyPattern(g.pattern);
      if (cls === "precise") preciseCount++;
      if (cls === "vague") vagueCount++;
    }

    const closureCount = greps.filter((g) => g.formedClosure).length;

    if (greps.length > 0) {
      sessionStats.push({
        threadId: thread.id,
        threadTitle: thread.title || "",
        threadDate: thread.created_at.slice(0, 10),
        grepCount: greps.length,
        closureCount,
        closureRate: closureCount / greps.length,
        repeatGrepCount,
        editedFiles: editedFiles.length,
        filesDiscoveredViaGrep,
        grepDiscoveryRate: editedFiles.length > 0 ? filesDiscoveredViaGrep / editedFiles.length : 0,
        precisePatternCount: preciseCount,
        vaguePatternCount: vagueCount,
      });
    }
  }

  // ── 输出报告 ──

  // 1. 总体 Grep 效能
  printSection("Grep→Read→Edit 闭环率");
  const totalGreps = allGreps.length;
  const totalClosure = allGreps.filter((g) => g.formedClosure).length;
  const totalWithRead = allGreps.filter((g) => g.subsequentReads.length > 0).length;
  const totalWithEdit = allGreps.filter((g) => g.subsequentEdits.length > 0).length;
  printMetric("Grep 总调用", totalGreps);
  printMetric("后续有 Read", totalWithRead, ` (${(totalWithRead / totalGreps * 100).toFixed(1)}%)`);
  printMetric("后续有 Edit", totalWithEdit, ` (${(totalWithEdit / totalGreps * 100).toFixed(1)}%)`);
  printMetric("完整闭环 (Grep→Read→Edit)", totalClosure, ` (${(totalClosure / totalGreps * 100).toFixed(1)}%)`);

  // 2. 文件发现率
  printSection("文件发现率");
  const sessionsWithEdits = sessionStats.filter((s) => s.editedFiles > 0);
  if (sessionsWithEdits.length > 0) {
    const avgDiscoveryRate = sessionsWithEdits.reduce((a, s) => a + s.grepDiscoveryRate, 0) / sessionsWithEdits.length;
    const totalFilesEdited = sessionsWithEdits.reduce((a, s) => a + s.editedFiles, 0);
    const totalDiscovered = sessionsWithEdits.reduce((a, s) => a + s.filesDiscoveredViaGrep, 0);
    printMetric("有编辑的会话", sessionsWithEdits.length);
    printMetric("总编辑文件数", totalFilesEdited);
    printMetric("通过 Grep 发现的文件", totalDiscovered, ` (${(totalDiscovered / totalFilesEdited * 100).toFixed(1)}%)`);
    printMetric("平均文件发现率", (avgDiscoveryRate * 100).toFixed(1) + "%");

    const discoveryBins = [
      { label: "高发现率 (≥60%)", test: (s: SessionGrepStats) => s.grepDiscoveryRate >= 0.6 },
      { label: "中发现率 (20-60%)", test: (s: SessionGrepStats) => s.grepDiscoveryRate >= 0.2 && s.grepDiscoveryRate < 0.6 },
      { label: "低发现率 (<20%)", test: (s: SessionGrepStats) => s.grepDiscoveryRate > 0 && s.grepDiscoveryRate < 0.2 },
      { label: "无需发现 (0%)", test: (s: SessionGrepStats) => s.grepDiscoveryRate === 0 },
    ];
    for (const bin of discoveryBins) {
      const count = sessionsWithEdits.filter(bin.test).length;
      printMetric(`  ${bin.label}`, count, ` (${(count / sessionsWithEdits.length * 100).toFixed(1)}%)`);
    }
  }

  // 3. 重复搜索率
  printSection("重复搜索率");
  const totalRepeat = sessionStats.reduce((a, s) => a + s.repeatGrepCount, 0);
  printMetric("重复搜索次数", totalRepeat, ` / ${totalGreps} 总搜索 (${(totalRepeat / totalGreps * 100).toFixed(1)}%)`);

  // 4. Pattern 精确度 vs 转化率
  printSection("Pattern 精确度 vs 效能");
  const preciseGreps = allGreps.filter((g) => classifyPattern(g.pattern) === "precise");
  const vagueGreps = allGreps.filter((g) => classifyPattern(g.pattern) === "vague");
  const structuralGreps = allGreps.filter((g) => classifyPattern(g.pattern) === "structural");

  const precisionRows = [
    ["精确 (具体标识符)", String(preciseGreps.length), preciseGreps.length > 0 ? (preciseGreps.filter((g) => g.formedClosure).length / preciseGreps.length * 100).toFixed(1) + "%" : "-", preciseGreps.length > 0 ? (preciseGreps.filter((g) => g.subsequentReads.length > 0).length / preciseGreps.length * 100).toFixed(1) + "%" : "-"],
    ["结构 (fn/struct/mod)", String(structuralGreps.length), structuralGreps.length > 0 ? (structuralGreps.filter((g) => g.formedClosure).length / structuralGreps.length * 100).toFixed(1) + "%" : "-", structuralGreps.length > 0 ? (structuralGreps.filter((g) => g.subsequentReads.length > 0).length / structuralGreps.length * 100).toFixed(1) + "%" : "-"],
    ["宽泛 (通配/短词)", String(vagueGreps.length), vagueGreps.length > 0 ? (vagueGreps.filter((g) => g.formedClosure).length / vagueGreps.length * 100).toFixed(1) + "%" : "-", vagueGreps.length > 0 ? (vagueGreps.filter((g) => g.subsequentReads.length > 0).length / vagueGreps.length * 100).toFixed(1) + "%" : "-"],
  ];
  printTable(["类型", "调用数", "闭环率", "Read率"], precisionRows);

  // 5. 时间趋势——Grep 效能是否随时间下降
  printSection("时间趋势（按月）");
  const monthStats: Record<string, { greps: number; closures: number; reads: number; sessions: number }> = {};
  for (const g of allGreps) {
    // 通过 sessionStats 找日期
    const session = sessionStats.find((s) => s.threadId === g.threadId);
    const month = session ? session.threadDate.slice(0, 7) : "unknown";
    if (!monthStats[month]) monthStats[month] = { greps: 0, closures: 0, reads: 0, sessions: 0 };
    monthStats[month].greps++;
    if (g.formedClosure) monthStats[month].closures++;
    if (g.subsequentReads.length > 0) monthStats[month].reads++;
  }
  // 按月统计会话数
  for (const s of sessionStats) {
    const month = s.threadDate.slice(0, 7);
    if (!monthStats[month]) monthStats[month] = { greps: 0, closures: 0, reads: 0, sessions: 0 };
    monthStats[month].sessions++;
  }

  const monthRows = Object.entries(monthStats)
    .sort(([a], [b]) => a.localeCompare(b))
    .map(([month, data]) => [
      month,
      String(data.sessions),
      String(data.greps),
      (data.closures / (data.greps || 1) * 100).toFixed(1) + "%",
      (data.reads / (data.greps || 1) * 100).toFixed(1) + "%",
    ]);
  printTable(["月份", "会话", "Grep", "闭环率", "Read率"], monthRows);

  // 6. 低效会话 Top 10
  printSection("低效 Grep 会话 (Grep 多但闭环率低)");
  const lowEffSessions = sessionStats
    .filter((s) => s.grepCount >= 5)
    .sort((a, b) => a.closureRate - b.closureRate)
    .slice(0, 10);

  const lowEffRows = lowEffSessions.map((s) => [
    s.threadDate,
    s.threadTitle.slice(0, 30),
    String(s.grepCount),
    (s.closureRate * 100).toFixed(0) + "%",
    (s.grepDiscoveryRate * 100).toFixed(0) + "%",
    String(s.repeatGrepCount),
  ]);
  printTable(["日期", "标题", "Grep数", "闭环率", "发现率", "重复搜索"], lowEffRows);

  // ── 缺陷报告 ──
  const reports: DefectReport[] = [];

  // 闭环率过低
  if (totalClosure / totalGreps < 0.3) {
    reports.push({
      id: "GREP-001",
      severity: "medium",
      category: "搜索效能",
      title: "Grep→Read→Edit 闭环率偏低",
      description: `${totalGreps} 次 Grep 中仅 ${totalClosure} 次（${(totalClosure / totalGreps * 100).toFixed(1)}%）形成了完整的 搜索→阅读→修改 闭环。大量 Grep 搜索后虽然读了文件但没有实际修改，说明搜索命中了不相关的内容或 Agent 对代码库的结构理解不足。`,
      evidence: [
        `闭环率: ${(totalClosure / totalGreps * 100).toFixed(1)}%`,
        `有 Read 无 Edit: ${totalWithRead - totalClosure} 次`,
        `无任何后续: ${totalGreps - totalWithRead} 次`,
      ],
      affectedSessions: sessionStats.filter((s) => s.grepCount >= 5 && s.closureRate < 0.2).map((s) => s.threadId),
      recommendation: "1. Agent 应在 Grep 前明确搜索意图（'我在找什么'）。2. Grep 结果应结合文件路径判断相关性，优先 Read 高置信度匹配。3. 系统提示可加入'先 Grep 定位，再 Read 理解，最后 Edit 修改'的工作流引导。",
      confidence: 0.6,
    });
  }

  // 文件发现率过低
  if (sessionsWithEdits.length > 10) {
    const lowDiscoverySessions = sessionsWithEdits.filter((s) => s.editedFiles >= 3 && s.grepDiscoveryRate < 0.2);
    if (lowDiscoverySessions.length > 5) {
      reports.push({
        id: "GREP-002",
        severity: "medium",
        category: "搜索效能",
        title: "Grep 对文件发现的贡献率低",
        description: `${lowDiscoverySessions.length} 个会话中编辑了 ≥3 个文件，但通过 Grep 发现的文件占比 <20%。Agent 主要依赖已知文件路径直接 Read，Grep 的文件发现能力未被充分利用。随着项目增长，Agent 不可能记住所有文件路径，Grep 将变得更关键。`,
        evidence: lowDiscoverySessions.slice(0, 5).map((s) =>
          `${s.threadTitle.slice(0, 25)}: ${s.grepCount}次Grep, 发现${s.filesDiscoveredViaGrep}/${s.editedFiles}文件`
        ),
        affectedSessions: lowDiscoverySessions.map((s) => s.threadId),
        recommendation: "1. Agent 应在不确定文件位置时主动 Grep，而非猜测路径。2. 可在系统提示中注入项目文件结构摘要，减少盲目搜索。3. 对大项目考虑建立文件索引（如 ctags）。",
        confidence: 0.55,
      });
    }
  }

  // 时间趋势下降
  const monthEntries = Object.entries(monthStats).sort(([a], [b]) => a.localeCompare(b));
  if (monthEntries.length >= 3) {
    const firstMonth = monthEntries[0];
    const lastMonth = monthEntries[monthEntries.length - 1];
    const firstRate = firstMonth[1].closures / (firstMonth[1].greps || 1);
    const lastRate = lastMonth[1].closures / (lastMonth[1].greps || 1);
    if (lastRate < firstRate * 0.7 && lastMonth[1].greps > 100) {
      reports.push({
        id: "GREP-003",
        severity: "high",
        category: "搜索效能趋势",
        title: "Grep 闭环率随项目增长呈下降趋势",
        description: `闭环率从 ${firstMonth[0]} 的 ${(firstRate * 100).toFixed(1)}% 降至 ${lastMonth[0]} 的 ${(lastRate * 100).toFixed(1)}%，下降 ${((1 - lastRate / firstRate) * 100).toFixed(0)}%。随着代码库膨胀，Agent 的搜索效率在下降。`,
        evidence: monthEntries.map(([m, d]) => `${m}: ${(d.closures / (d.greps || 1) * 100).toFixed(1)}% (${d.greps}次)`),
        affectedSessions: [],
        recommendation: "1. 考虑为项目建立语义索引（如 code search embedding），替代纯文本 Grep。2. 在系统提示中注入项目架构概览，减少搜索范围。3. 优先使用 Glob 定位文件再用 Grep 搜索内容。",
        confidence: 0.5,
      });
    }
  }

  // 宽泛 pattern 占比高
  const vagueRatio = vagueGreps.length / totalGreps;
  if (vagueRatio > 0.3) {
    reports.push({
      id: "GREP-004",
      severity: "low",
      category: "搜索策略",
      title: "宽泛 Grep pattern 占比偏高",
      description: `${(vagueRatio * 100).toFixed(1)}% 的 Grep 调用使用了宽泛 pattern（如通配符、极短关键词），这些 pattern 命中大量无关文件，降低搜索效率。`,
      evidence: [
        `宽泛 pattern: ${vagueGreps.length} 次`,
        `精确 pattern: ${preciseGreps.length} 次`,
        `结构 pattern: ${structuralGreps.length} 次`,
      ],
      affectedSessions: [],
      recommendation: "Agent 应优先使用具体标识符（函数名、类型名）搜索，而非泛泛搜索。系统提示可加入'优先搜索具体标识符'的策略引导。",
      confidence: 0.4,
    });
  }

  return reports;
}
