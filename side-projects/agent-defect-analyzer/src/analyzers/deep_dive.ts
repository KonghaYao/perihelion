//! 质量最差会话深度下钻分析脚本。
//!
//! 三类问题会话的逐条检查：
//!   SEM-001: 低覆盖率 — 质量分最低的 10 个会话
//!   SEM-002: 路径幻觉 — 路径验证率最低的 5 个会话
//!   SEM-003: 高不满信号 — 不满信号最多的 5 个会话
//!
//! 用法: bun run src/analyzers/deep_dive.ts

import chalk from "chalk";
import { DataLoader } from "../utils/data_loader.js";
import { printHeader, printSection } from "../utils/report.js";
import type { MessageRow, ToolCallRequest, ParsedMessage, AiContent } from "../types.js";

// ── 复用 answer_quality 的关键词/路径提取逻辑 ──

function extractNeedsKeywords(text: string): string[] {
  const keywords: string[] = [];
  const trimmed = text.trim();
  if (/^(继续|continue|hello|hi|nice|ok|yes|好|是|对|同意|A|B|C|D|[1-9]\d*)$/i.test(trimmed)) return [];

  // Slash 命令
  const slashMatch = trimmed.match(/^\/(\w+[-\w]*)\s*(.*)/);
  if (slashMatch) {
    keywords.push(slashMatch[1].toLowerCase());
    if (slashMatch[2]?.length > 0) keywords.push(...extractEntities(slashMatch[2]));
    return [...new Set(keywords)];
  }

  // @ 路径引用
  const atPattern = /@([a-zA-Z0-9_./-]+)/g;
  let atMatch;
  while ((atMatch = atPattern.exec(trimmed)) !== null) {
    if (atMatch[1] && atMatch[1].length > 2) {
      const parts = atMatch[1].split("/").filter(Boolean);
      for (const part of parts.slice(-2)) {
        if (part.length > 2) keywords.push(part.toLowerCase());
      }
    }
  }

  // 祈使句
  const gap = "(?:一个|一下|一些|暂存区|所有|整个|这个|那个|那几|这几)?\\s*";
  const cjkTarget = "[^\\s，。！？；：\"''）\\]）]{2,20}";
  const imperativePatterns = [
    new RegExp(`(?:实现|implement|add|create|build|写|添加|新增|开发|生成)${gap}["']?(\\w+|${cjkTarget})["']?`, "gi"),
    new RegExp(`(?:修复|fix|solve|解决|debug|修补)${gap}["']?(\\w+|${cjkTarget})["']?`, "gi"),
    new RegExp(`(?:重构|refactor|重写|rewrite|改造)${gap}["']?(\\w+|${cjkTarget})["']?`, "gi"),
    new RegExp(`(?:测试|test|验证|跑)${gap}["']?(\\w+|${cjkTarget})["']?`, "gi"),
    new RegExp(`(?:删除|remove|delete|移除|清理)${gap}["']?(\\w+|${cjkTarget})["']?`, "gi"),
    new RegExp(`(?:检查|check|分析|analyze|查看|看看|review|审核|扫)${gap}["']?@?(${cjkTarget})["']?`, "gi"),
    new RegExp(`(?:优化|optimize|改进|improve|提升)${gap}["']?(\\w+|${cjkTarget})["']?`, "gi"),
    new RegExp(`(?:搜索|search|查找|find|找)${gap}["']?(\\w+|${cjkTarget})["']?`, "gi"),
    new RegExp(`(?:提交|commit|push|合并|merge)${gap}["']?(\\w+|${cjkTarget})["']?`, "gi"),
    new RegExp(`(?:安装|install|配置|config|设置|setup)${gap}["']?(\\w+|${cjkTarget})["']?`, "gi"),
    new RegExp(`(?:改|change|修改|modify|更新|update|替换|replace)${gap}["']?(\\w+|${cjkTarget})["']?`, "gi"),
    new RegExp(`(?:运行|run|执行|execute|启动|start)${gap}["']?(\\w+|${cjkTarget})["']?`, "gi"),
  ];
  for (const pattern of imperativePatterns) {
    let match;
    while ((match = pattern.exec(trimmed)) !== null) {
      if (match[1] && match[1].length > 1) keywords.push(match[1].toLowerCase());
    }
  }

  keywords.push(...extractEntities(trimmed));
  return [...new Set(keywords)];
}

function extractEntities(text: string): string[] {
  const entities: string[] = [];
  const pathPattern = /["']?([\/\\][\w./-]+\.\w{1,10})["']?/g;
  let m;
  while ((m = pathPattern.exec(text)) !== null) {
    if (m[1]) {
      const parts = m[1].replace(/\\/g, "/").split("/").filter(Boolean);
      for (const part of parts.slice(-2)) {
        if (part.length > 2) entities.push(part.toLowerCase());
      }
    }
  }
  const quotedPattern = /[`"']([^`"'\s]{2,30})[`"']/g;
  while ((m = quotedPattern.exec(text)) !== null) {
    if (m[1]) entities.push(m[1].toLowerCase());
  }
  const identPattern = /(?:^|\s|["'(])([a-z][a-z0-9_]*(?:_[a-z0-9_]+){1,}|[a-z][a-z0-9]*(?:[A-Z][a-z0-9]*){1,}|[a-z][a-z0-9]*(?:-[a-z0-9]+){1,})(?:$|\s|["')\]],.])/g;
  while ((m = identPattern.exec(text)) !== null) {
    if (m[1] && m[1].length > 3 && m[1].length < 40) entities.push(m[1].toLowerCase());
  }
  return [...new Set(entities)];
}

function extractFilePaths(text: string): string[] {
  const paths: string[] = [];
  const pattern = /(?:\/[\w.-]+){2,}(?:\.\w+)?/g;
  let match;
  while ((match = pattern.exec(text)) !== null) {
    const p = match[0];
    if (p.length <= 5) continue;
    const before = text.slice(Math.max(0, match.index - 10), match.index);
    if (/https?:/.test(before)) continue;
    if (/^\/usr\/|^\/etc\/|^\/var\/|^\/tmp\/|^\/opt\//.test(p)) continue;
    paths.push(p);
  }
  return [...new Set(paths)];
}

function extractToolPaths(toolCalls: ToolCallRequest[]): string[] {
  const paths: string[] = [];
  for (const tc of toolCalls) {
    const args = tc.arguments;
    if (args.file_path && typeof args.file_path === "string") paths.push(args.file_path);
    if (args.path && typeof args.path === "string") paths.push(args.path);
    if (args.pattern && typeof args.pattern === "string" && !/[*?\[]/.test(args.pattern)) paths.push(args.pattern);
    if (args.command && typeof args.command === "string") paths.push(...extractFilePaths(args.command));
  }
  return [...new Set(paths)];
}

function pathSegmentsMatch(agentPath: string, toolPath: string): boolean {
  const getTail = (p: string) => p.split("/").filter(Boolean).slice(-2).join("/");
  const a = getTail(agentPath);
  const t = getTail(toolPath);
  return a === t || a.endsWith("/" + t) || t.endsWith("/" + a);
}

function detectUserNegSignals(text: string): string[] {
  const signals: string[] = [];
  const patterns = [
    { re: /(?<![这那])不对(?![吗吧])/u, label: "否定(不对)" },
    { re: /错误|搞错了|失败了|failed|wrong|incorrect/i, label: "否定(错误)" },
    { re: /不行|不可以|行不通/i, label: "否定(不行)" },
    { re: /没有(?:效果|用|成功|解决|完成|生效|变化)/i, label: "否定(没有效果)" },
    { re: /再试|重试|redo|retry|再来一次|again/i, label: "重试要求" },
    { re: /不要这样|别这样|stop|don't do that/i, label: "纠正" },
    { re: /为什么.{0,5}(不行|错误|失败|不对|没用)/i, label: "质疑" },
    { re: /不对劲|怎么还是|还是(不对|不行|错误|失败)/i, label: "困惑" },
  ];
  for (const { re, label } of patterns) {
    if (re.test(text)) signals.push(label);
  }
  return signals;
}

// ── 辅助函数 ──

function getUserText(parsed: ParsedMessage | null): string {
  if (!parsed || parsed.role !== "user") return "";
  const content = (parsed as any).content;
  return typeof content === "string"
    ? content
    : Array.isArray(content)
      ? content.filter((b: any) => b.type === "text").map((b: any) => b.text || "").join("")
      : "";
}

function getAssistantText(parsed: ParsedMessage | null): string {
  if (!parsed || parsed.role !== "assistant") return "";
  const ai = parsed as AiContent;
  const blocks = Array.isArray(ai.content) ? ai.content : [];
  return blocks.filter((b: any) => b.type === "text").map((b: any) => b.text || "").join("\n");
}

function summarizeArgs(args: Record<string, unknown>, maxLen = 80): string {
  const entries = Object.entries(args);
  if (entries.length === 0) return "{}";
  return entries.map(([k, v]) => {
    const vs = typeof v === "string" ? (v.length > maxLen ? v.slice(0, maxLen) + "..." : v) : JSON.stringify(v)?.slice(0, maxLen);
    return `${k}=${vs}`;
  }).join(", ");
}

// ── 数据结构 ──

interface SessionDigest {
  threadId: string;
  threadTitle: string;
  qualityScore: number;
  coverageRate: number;
  hasKeywords: boolean;
  userKeywords: string[];
  coveredKeywords: string[];
  uncoveredKeywords: string[];
  pathVerifyRate: number;
  mentionedPaths: string[];
  verifiedPaths: string[];
  unverifiedPaths: string[];
  userNegSignals: number;
  // 逐轮详情
  turns: TurnDetail[];
}

interface TurnDetail {
  userText: string;
  userKeywords: string[];
  negSignals: string[];
  agentToolCalls: { name: string; argsSummary: string }[];
  agentTextSnippet: string; // 前 200 字
  agentMentionedPaths: string[];
}

// ── 会话级深度分析 ──

function deepAnalyzeSession(
  threadId: string,
  threadTitle: string,
  messages: MessageRow[]
): SessionDigest {
  const allKeywords: string[] = [];
  const allToolPaths: string[] = [];
  const allAgentPaths: string[] = [];
  const allAgentTexts: string[] = [];
  let negSignals = 0;
  const turns: TurnDetail[] = [];

  // 按轮次分组：user → assistant(s) → tool(s) → assistant → ...
  let currentTurn: Partial<TurnDetail> | null = null;

  for (const msg of messages) {
    const parsed = DataLoader.parseContent(msg.content);
    if (!parsed) continue;

    if (msg.role === "user") {
      // 先保存上一轮
      if (currentTurn && currentTurn.userText !== undefined) {
        turns.push(currentTurn as TurnDetail);
      }
      const text = getUserText(parsed);
      const keywords = extractNeedsKeywords(text);
      const negs = detectUserNegSignals(text);
      allKeywords.push(...keywords);
      negSignals += negs.length;
      currentTurn = {
        userText: text,
        userKeywords: keywords,
        negSignals: negs,
        agentToolCalls: [],
        agentTextSnippet: "",
        agentMentionedPaths: [],
      };
    } else if (msg.role === "assistant" && currentTurn) {
      const ai = parsed as AiContent;
      const blocks = Array.isArray(ai.content) ? ai.content : [];
      let text = "";
      for (const block of blocks) {
        if (block.type === "text" && (block as any).text) {
          text += (block as any).text + " ";
          allAgentPaths.push(...extractFilePaths((block as any).text));
        }
      }
      allAgentTexts.push(text.trim());
      if (currentTurn) {
        currentTurn.agentTextSnippet = (currentTurn.agentTextSnippet || "") + text.trim() + "\n";
        currentTurn.agentMentionedPaths!.push(...extractFilePaths(text));
      }

      const toolCalls = DataLoader.extractToolCalls(parsed);
      allToolPaths.push(...extractToolPaths(toolCalls));
      for (const tc of toolCalls) {
        currentTurn.agentToolCalls!.push({
          name: tc.name,
          argsSummary: summarizeArgs(tc.arguments),
        });
      }
    }
  }
  // 最后一轮
  if (currentTurn && currentTurn.userText !== undefined) {
    turns.push(currentTurn as TurnDetail);
  }

  // 覆盖率计算
  const uniqueKeywords = [...new Set(allKeywords)];
  const hasKeywords = uniqueKeywords.length > 0;
  const coveredKeywords: string[] = [];
  const uncoveredKeywords: string[] = [];
  if (hasKeywords) {
    const allText = allAgentTexts.join(" ").toLowerCase();
    const pathsLower = allToolPaths.map(p => p.toLowerCase());
    for (const kw of uniqueKeywords) {
      if (allText.includes(kw) || pathsLower.some(p => p.includes(kw))) {
        coveredKeywords.push(kw);
      } else {
        uncoveredKeywords.push(kw);
      }
    }
  }
  const coverageRate = hasKeywords ? coveredKeywords.length / uniqueKeywords.length : 0.5;

  // 路径验证率
  const uniqueAgentPaths = [...new Set(allAgentPaths.map(p => p.toLowerCase()))];
  const hasPaths = uniqueAgentPaths.length > 0;
  const verifiedPaths: string[] = [];
  const unverifiedPaths: string[] = [];
  if (hasPaths) {
    const normalizedToolPaths = allToolPaths.map(p => p.toLowerCase());
    for (const ap of uniqueAgentPaths) {
      if (normalizedToolPaths.some(tp => pathSegmentsMatch(ap, tp))) {
        verifiedPaths.push(ap);
      } else {
        unverifiedPaths.push(ap);
      }
    }
  }
  const pathVerifyRate = hasPaths ? verifiedPaths.length / uniqueAgentPaths.length : 0.5;

  // 质量分（复用 answer_quality 的评分公式）
  const SCORE_WEIGHTS = { coverage: 30, pathVerify: 10, negSignalPer: 10, negSignalMax: 30, truncation: 10, repetition: 20 };
  let score = 100;
  score -= (1 - coverageRate) * SCORE_WEIGHTS.coverage;
  score -= (1 - pathVerifyRate) * SCORE_WEIGHTS.pathVerify;
  score -= Math.min(negSignals * SCORE_WEIGHTS.negSignalPer, SCORE_WEIGHTS.negSignalMax);
  // 简化：不计算截断和重复度
  score = Math.max(0, Math.min(100, Math.round(score)));

  return {
    threadId,
    threadTitle,
    qualityScore: score,
    coverageRate,
    hasKeywords,
    userKeywords: uniqueKeywords,
    coveredKeywords,
    uncoveredKeywords,
    pathVerifyRate,
    mentionedPaths: uniqueAgentPaths,
    verifiedPaths,
    unverifiedPaths,
    userNegSignals: negSignals,
    turns,
  };
}

// ── 输出格式化 ──

function printDigest(d: SessionDigest) {
  console.log(chalk.bold.cyan(`  会话: ${d.threadTitle || "(无标题)"}`));
  console.log(chalk.gray(`  ID: ${d.threadId}`));
  console.log(chalk.gray(`  质量分: ${d.qualityScore} | 覆盖率: ${(d.coverageRate * 100).toFixed(0)}% | 路径验证率: ${(d.pathVerifyRate * 100).toFixed(0)}% | 不满信号: ${d.userNegSignals}`));
}

function truncateText(text: string, maxLen = 150): string {
  if (text.length <= maxLen) return text;
  return text.slice(0, maxLen) + "...";
}

// ── SEM-001: 低覆盖率会话 ──

function analyzeLowCoverage(sessions: SessionDigest[]) {
  printHeader("SEM-001: 低覆盖率会话深度分析");

  const sorted = [...sessions]
    .filter(s => s.hasKeywords && s.userKeywords.length >= 1)
    .sort((a, b) => a.qualityScore - b.qualityScore)
    .slice(0, 10);

  console.log(chalk.gray(`  筛选出 ${sorted.length} 个有关键词的最低质量会话\n`));

  for (let i = 0; i < sorted.length; i++) {
    const s = sorted[i];
    printDigest(s);
    console.log();

    // 逐轮分析
    for (let t = 0; t < s.turns.length; t++) {
      const turn = s.turns[t];
      if (!turn.userText || turn.userText.trim().length === 0) continue;

      console.log(chalk.white(`    ── 轮次 ${t + 1} ──`));
      console.log(chalk.yellow(`    用户: ${chalk.reset(truncateText(turn.userText.replace(/\n/g, " "), 200))}`));

      if (turn.userKeywords.length > 0) {
        console.log(chalk.gray(`    提取关键词: [${turn.userKeywords.join(", ")}]`));
      }

      if (turn.negSignals.length > 0) {
        console.log(chalk.red(`    不满信号: ${turn.negSignals.join(", ")}`));
      }

      if (turn.agentToolCalls.length > 0) {
        console.log(chalk.green(`    Agent 工具调用:`));
        for (const tc of turn.agentToolCalls) {
          console.log(chalk.green(`      - ${tc.name}(${truncateText(tc.argsSummary, 100)})`));
        }
      } else {
        console.log(chalk.gray(`    Agent 工具调用: (无)`));
      }

      if (turn.agentTextSnippet.trim()) {
        console.log(chalk.blue(`    Agent 回复摘要: ${chalk.reset(truncateText(turn.agentTextSnippet.replace(/\n/g, " "), 150))}`));
      }
      console.log();
    }

    // 覆盖分析总结
    console.log(chalk.bold(`    [覆盖分析]`));
    if (s.coveredKeywords.length > 0) {
      console.log(chalk.green(`    已覆盖关键词: [${s.coveredKeywords.join(", ")}]`));
    }
    if (s.uncoveredKeywords.length > 0) {
      console.log(chalk.red(`    未覆盖关键词: [${s.uncoveredKeywords.join(", ")}]`));
    }

    // 判断原因
    console.log(chalk.bold(`    [根因判断]`));
    if (s.uncoveredKeywords.length === 0) {
      console.log(chalk.green(`    所有关键词已被覆盖，质量低可能因为路径/不满等其他因素`));
    } else if (s.turns.every(t => t.agentToolCalls.length === 0)) {
      console.log(chalk.red(`    Agent 完全没有调用工具 → Agent 可能没有理解需要执行任务`));
    } else {
      const toolNames = s.turns.flatMap(t => t.agentToolCalls.map(tc => tc.name));
      const uniqueTools = [...new Set(toolNames)];
      if (uniqueTools.length <= 2 && s.userKeywords.length >= 3) {
        console.log(chalk.red(`    Agent 只用了 ${uniqueTools.join("/")} 少量工具，但用户有 ${s.userKeywords.length} 个需求 → Agent 工具使用不够充分`));
      } else {
        console.log(chalk.yellow(`    Agent 用了 ${uniqueTools.join("/")} 共 ${toolNames.length} 次调用，但仍有 ${s.uncoveredKeywords.length} 个关键词未覆盖 → 可能是关键词提取不准，也可能是 Agent 确实遗漏了`));
        // 进一步检查：未覆盖关键词是否在 agent 文本中出现
        const agentAllText = s.turns.map(t => t.agentTextSnippet).join(" ").toLowerCase();
        const textMentioned = s.uncoveredKeywords.filter(kw => agentAllText.includes(kw));
        const completelyMissed = s.uncoveredKeywords.filter(kw => !agentAllText.includes(kw));
        if (textMentioned.length > 0) {
          console.log(chalk.yellow(`    文本中提到但无工具验证: [${textMentioned.join(", ")}] → 可能是关键词提取粒度问题`));
        }
        if (completelyMissed.length > 0) {
          console.log(chalk.red(`    文本和工具中完全未出现: [${completelyMissed.join(", ")}] → Agent 确实遗漏了这些任务`));
        }
      }
    }
    console.log("\n" + chalk.gray("  " + "─".repeat(70)) + "\n");
  }
}

// ── SEM-002: 路径幻觉会话 ──

function analyzePathHallucination(sessions: SessionDigest[]) {
  printHeader("SEM-002: 路径幻觉会话深度分析");

  const sorted = [...sessions]
    .filter(s => s.mentionedPaths.length >= 2)
    .sort((a, b) => a.pathVerifyRate - b.pathVerifyRate)
    .slice(0, 5);

  console.log(chalk.gray(`  筛选出 ${sorted.length} 个提及路径但验证率最低的会话\n`));

  for (const s of sorted) {
    printDigest(s);
    console.log();

    console.log(chalk.bold(`    [路径验证详情]`));
    console.log(chalk.green(`    已验证路径 (${s.verifiedPaths.length}):`));
    for (const p of s.verifiedPaths.slice(0, 10)) {
      console.log(chalk.green(`      ✓ ${p}`));
    }

    console.log(chalk.red(`    未验证路径 (${s.unverifiedPaths.length}):`));
    for (const p of s.unverifiedPaths.slice(0, 15)) {
      console.log(chalk.red(`      ✗ ${p}`));
    }
    console.log();

    // 分类未验证路径
    console.log(chalk.bold(`    [路径分类判断]`));
    for (const p of s.unverifiedPaths.slice(0, 10)) {
      const ctx = findPathContext(s, p);
      if (ctx.isReference) {
        console.log(chalk.yellow(`      "${p}" → Agent 仅引用/提到，非实际操作目标 (上下文: "${ctx.snippet}")`));
      } else if (ctx.hasToolCall) {
        console.log(chalk.yellow(`      "${p}" → 有工具调用但路径不精确匹配 (工具: ${ctx.toolName})`));
      } else {
        console.log(chalk.red(`      "${p}" → Agent 提到路径但无任何工具验证，可能是幻觉 (上下文: "${ctx.snippet}")`));
      }
    }
    console.log("\n" + chalk.gray("  " + "─".repeat(70)) + "\n");
  }
}

function findPathContext(s: SessionDigest, path: string): {
  isReference: boolean;
  hasToolCall: boolean;
  toolName?: string;
  snippet: string;
} {
  const pathLower = path.toLowerCase();

  // 检查是否有工具调用了相近路径
  for (const turn of s.turns) {
    for (const tc of turn.agentToolCalls) {
      if (tc.argsSummary.toLowerCase().includes(pathLower.split("/").pop() || "")) {
        return { isReference: false, hasToolCall: true, toolName: tc.name, snippet: tc.argsSummary.slice(0, 80) };
      }
    }
  }

  // 检查上下文判断是引用还是声称
  for (const turn of s.turns) {
    const snippet = turn.agentTextSnippet.toLowerCase();
    if (snippet.includes(pathLower)) {
      // 引用模式：前面有 "看到"、"已经"、"你修改了" 等
      const idx = snippet.indexOf(pathLower);
      const prefix = snippet.slice(Math.max(0, idx - 30), idx);
      const refPatterns = /已经|看到|我发现|注意到|你.*修改|你.*编辑|已经.*在|above|previous|earlier|you.*changed|you.*modified/i;
      if (refPatterns.test(prefix)) {
        const rawSnippet = turn.agentTextSnippet.replace(/\n/g, " ").slice(Math.max(0, idx - 40), idx + pathLower.length + 20);
        return { isReference: true, hasToolCall: false, snippet: rawSnippet };
      }
      // 声称存在的模式
      const claimPatterns = /存在|在.*中|文件.*是|the file|contains|located at/i;
      if (claimPatterns.test(prefix)) {
        const rawSnippet = turn.agentTextSnippet.replace(/\n/g, " ").slice(Math.max(0, idx - 40), idx + pathLower.length + 20);
        return { isReference: false, hasToolCall: false, snippet: rawSnippet };
      }
    }
  }

  // 默认
  for (const turn of s.turns) {
    if (turn.agentTextSnippet.toLowerCase().includes(pathLower)) {
      const idx = turn.agentTextSnippet.toLowerCase().indexOf(pathLower);
      const rawSnippet = turn.agentTextSnippet.replace(/\n/g, " ").slice(Math.max(0, idx - 30), idx + pathLower.length + 30);
      return { isReference: false, hasToolCall: false, snippet: rawSnippet };
    }
  }

  return { isReference: false, hasToolCall: false, snippet: "(未找到上下文)" };
}

// ── SEM-003: 高不满信号会话 ──

function analyzeHighDissatisfaction(sessions: SessionDigest[]) {
  printHeader("SEM-003: 高不满信号会话深度分析");

  const sorted = [...sessions]
    .filter(s => s.userNegSignals >= 1)
    .sort((a, b) => b.userNegSignals - a.userNegSignals)
    .slice(0, 5);

  console.log(chalk.gray(`  筛选出 ${sorted.length} 个不满信号最多的会话\n`));

  for (const s of sorted) {
    printDigest(s);
    console.log();

    // 逐轮展示
    for (let t = 0; t < s.turns.length; t++) {
      const turn = s.turns[t];
      if (!turn.userText || turn.userText.trim().length === 0) continue;

      const hasNeg = turn.negSignals.length > 0;
      const marker = hasNeg ? chalk.bgRed.white(" ⚠不满 ") : "       ";

      console.log(`    ${marker} 轮次 ${t + 1}`);
      console.log(chalk.cyan(`    用户: ${chalk.reset(truncateText(turn.userText.replace(/\n/g, " "), 200))}`));

      if (hasNeg) {
        console.log(chalk.red(`    匹配模式: [${turn.negSignals.join(", ")}]`));
      }

      if (turn.agentToolCalls.length > 0) {
        const tools = turn.agentToolCalls.map(tc => tc.name).join(", ");
        console.log(chalk.green(`    Agent 工具: ${tools}`));
      }

      if (turn.agentTextSnippet.trim()) {
        console.log(chalk.blue(`    Agent 回复: ${chalk.reset(truncateText(turn.agentTextSnippet.replace(/\n/g, " "), 200))}`));
      }

      // 判断是真不满还是误报
      if (hasNeg) {
        console.log(chalk.bold(`    [判断] ${classifyNegSignal(turn)}`));
      }
      console.log();
    }
    console.log(chalk.gray("  " + "─".repeat(70)) + "\n");
  }
}

function classifyNegSignal(turn: TurnDetail): string {
  const text = turn.userText;
  const hasTool = turn.agentToolCalls.length > 0;

  // 技术讨论中常见的否定句式（不算真正不满）
  const technicalDiscussion = /如果是.*错误|假设.*错误|如果.*失败|当.*失败时|error.*when|fail.*if/i.test(text);
  if (technicalDiscussion) {
    return chalk.green("可能误报 → 用户在讨论技术场景中的错误/失败，非对 Agent 的不满");
  }

  // 用户提出新的修复要求
  const newRequest = /修复|fix|改成|换成|更新|update|改一下|换/i.test(text);
  if (newRequest && !/还是.*不对|又.*错误|怎么还是/i.test(text)) {
    return chalk.yellow("可能误报 → 用户在提出新的修改要求，未必是对上轮结果不满");
  }

  // 重试/重做 明确不满
  const explicitRetry = /再试|重试|redo|retry|再来一次|again|重新/i.test(text);
  if (explicitRetry) {
    return chalk.red("真不满 → 用户明确要求重做，说明上轮结果不可接受");
  }

  // "还是不对" 类连续不满
  const continuedDissatisfaction = /还是|仍然|怎么还是|又/i.test(text);
  if (continuedDissatisfaction) {
    return chalk.red("真不满 → 用户连续表达不满，Agent 反复出错");
  }

  // 如果用户给了否定信号后又给了新指令
  if (hasTool) {
    return chalk.yellow("中性 → 用户虽表达不满但给出了新指令，Agent 已继续执行");
  }

  return chalk.red("真不满 → 用户表达了不满，且无后续操作指令");
}

// ── 主入口 ──

const loader = new DataLoader();

try {
  printHeader("Peri Agent 问题会话深度下钻");

  const threads = loader.loadVisibleThreads();
  console.log(chalk.gray(`  加载 ${threads.length} 个可见会话，开始深度分析...\n`));

  const sessions: SessionDigest[] = [];
  for (const thread of threads) {
    const messages = loader.loadMessages(thread.id);
    const userMsgs = messages.filter(m => m.role === "user");
    if (userMsgs.length < 1) continue;
    sessions.push(deepAnalyzeSession(thread.id, thread.title || "", messages));
  }

  console.log(chalk.gray(`  有效会话: ${sessions.length}\n`));

  // 三类分析
  analyzeLowCoverage(sessions);
  analyzePathHallucination(sessions);
  analyzeHighDissatisfaction(sessions);

} finally {
  loader.close();
}
