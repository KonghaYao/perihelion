//! Agent 回答语义质量检测器（启发式代理指标）。
//!
//! 不调用 LLM，通过结构化分析间接推断语义质量：
//! 1. **指令覆盖率**：用户消息中的祈使句/需求 vs Agent 实际执行的工具调用
//! 2. **文件路径幻觉**：Agent 在文本中提到的文件路径，后续是否被验证存在
//! 3. **重复回答**：Agent 多次回复的文本相似度（n-gram Jaccard）
//! 4. **截断/未完成**：Agent 最后一条消息是否暗示"未完成"
//! 5. **用户不满信号**：用户消息中出现否定/修正/重复指令

import type { DefectReport, MessageRow, ToolCallRequest } from "../../types.js";
import { DataLoader } from "../utils/data_loader.js";
import { printSection, printMetric, printTable } from "../utils/report.js";

// ── 质量评分权重常量 ──
const SCORE_WEIGHTS = {
  /** 覆盖率扣分权重 */
  coverage: 30,
  /** 路径验证扣分权重 */
  pathVerify: 10,
  /** 每个不满信号扣分（上限 maxNegPenalty） */
  negSignalPer: 10,
  /** 不满信号扣分上限 */
  negSignalMax: 30,
  /** 截断扣分 */
  truncation: 10,
  /** 重复度扣分权重 */
  repetition: 20,
} as const;

/** n-gram Jaccard 计算的文本截断上限（防止超长文本内存爆炸） */
const JACCARD_TEXT_LIMIT = 2000;

// ── 数据结构 ──

interface AnswerQualityRecord {
  threadId: string;
  threadTitle: string;
  totalTurns: number;
  /** 用户指令中能提取到的具体需求关键词数 */
  userKeywords: number;
  /** 被关键词匹配到的需求数量 */
  coveredKeywords: number;
  /** 指令覆盖率（无关键词时为 0.5 中性值） */
  coverageRate: number;
  /** 是否有明确关键词（覆盖率有意义） */
  hasKeywords: boolean;
  /** Agent 文本中提到的文件路径数 */
  mentionedPaths: number;
  /** 其中有对应 Read/Write/Edit/Glob/Grep 调用的路径数 */
  verifiedPaths: number;
  /** 路径验证率（无路径时为 0.5 中性值） */
  pathVerifyRate: number;
  /** 是否有文件路径（验证率有意义） */
  hasPaths: boolean;
  /** 用户消息中的否定/不满信号次数 */
  userNegSignals: number;
  /** Agent 是否有截断/未完成的最后消息 */
  hasTruncation: boolean;
  /** Agent 回答的重复度（0-1） */
  answerRepetition: number;
  /** 综合质量评分（0-100） */
  qualityScore: number;
}

// ── 非人类用户消息识别 ──

/**
 * 判断消息是否为 cron 模板触发、后台任务通知等非人类消息。
 * 这类消息不应参与关键词提取和不满信号检测。
 */
function isNonHumanMessage(text: string): boolean {
  const trimmed = text.trim();
  // cron 模板：以 ❯ 开头的系统指令
  if (trimmed.startsWith("❯") || trimmed.startsWith("> ")) return true;
  // cron 模板：包含典型 cron 触发指令
  if (/^请根据以下要求/.test(trimmed)) return true;
  // 后台任务结果通知：包含 JSON 格式的 agent 输出
  if (/^\[后台任务.*完成\]/.test(trimmed)) return true;
  // fork directive（agent-to-agent 通信）
  if (trimmed.startsWith("<fork_directive>")) return true;
  // 纯错误日志粘贴（以 error/Error/panic/failed 开头的长文本）
  if (/^(error|Error|panic|failed|thread.*panicked)/i.test(trimmed) && trimmed.length > 200) return true;
  return false;
}

// ── 关键词提取 ──

/**
 * 从用户消息中提取需求关键词。
 *
 * 实际数据中用户消息模式分布（3321 条）：
 * - 中文自然语言 48.1%，英文 27.8%
 * - Slash 命令 12.2%（/writing-plans, /issue-create 等）
 * - HITL 审批回复 11.6%（A/B/1/同意）
 * - @ 引用 0.3%
 *
 * 因此提取逻辑覆盖四层信号：
 * 1. Slash 命令 + 参数（/issue-create xxx → 提取命令名和后续文本中的实体）
 * 2. @ 路径引用（@side-projects/git-graph → 提取路径段）
 * 3. 祈使句 + 目标（实现/修复/重构/写 + 目标名词，支持 CJK）
 * 4. 技术实体（文件路径、引号术语、代码标识符）
 */
function extractNeedsKeywords(text: string): string[] {
  const keywords: string[] = [];
  const trimmed = text.trim();

  // 跳过非人类消息（cron 模板、后台任务通知、错误日志粘贴）
  if (isNonHumanMessage(trimmed)) return [];

  // 跳过无意图消息（HITL 审批、纯打招呼、继续指令）
  if (/^(继续|continue|hello|hi|nice|ok|yes|好|是|对|同意|A|B|C|D|[1-9]\d*)$/i.test(trimmed)) {
    return [];
  }

  // ── 第 1 层：Slash 命令 ──
  const slashMatch = trimmed.match(/^\/(\w+[-\w]*)\s*(.*)/);
  if (slashMatch) {
    const command = slashMatch[1].toLowerCase();
    const args = slashMatch[2] || "";
    // 命令本身作为关键词
    keywords.push(command);
    // 参数中的实体也提取
    if (args.length > 0) {
      keywords.push(...extractEntities(args));
    }
    // 不再继续下面的层（slash 命令的意图由命令名+参数表达）
    return [...new Set(keywords)];
  }

  // ── 第 2 层：@ 路径引用 ──
  const atPattern = /@([a-zA-Z0-9_./-]+)/g;
  let atMatch;
  while ((atMatch = atPattern.exec(trimmed)) !== null) {
    if (atMatch[1] && atMatch[1].length > 2) {
      // 拆分路径段，取最后两段
      const parts = atMatch[1].split("/").filter(Boolean);
      for (const part of parts.slice(-2)) {
        if (part.length > 2) keywords.push(part.toLowerCase());
      }
    }
  }

  // ── 第 3 层：祈使句 + 目标（支持 CJK）──
  // 允许动词和目标之间有"一个"、"一下"、"暂存区"等间隔词
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
      if (match[1] && match[1].length > 1) {
        keywords.push(match[1].toLowerCase());
      }
    }
  }

  // ── 第 4 层：技术实体 ──
  keywords.push(...extractEntities(trimmed));

  return [...new Set(keywords)];
}

/** 从文本中提取技术实体（文件路径、引号术语、代码标识符） */
function extractEntities(text: string): string[] {
  const entities: string[] = [];

  // 文件路径（含扩展名的路径）
  const pathPattern = /["']?([\/\\][\w./-]+\.\w{1,10})["']?/g;
  let pathMatch;
  while ((pathMatch = pathPattern.exec(text)) !== null) {
    if (pathMatch[1]) {
      // 取最后两段作为关键词
      const parts = pathMatch[1].replace(/\\/g, "/").split("/").filter(Boolean);
      for (const part of parts.slice(-2)) {
        if (part.length > 2) entities.push(part.toLowerCase());
      }
    }
  }

  // 带引号的术语（单标识符）
  const quotedPattern = /[`"']([^`"'\s]{2,30})[`"']/g;
  let quotedMatch;
  while ((quotedMatch = quotedPattern.exec(text)) !== null) {
    if (quotedMatch[1]) {
      entities.push(quotedMatch[1].toLowerCase());
    }
  }

  // 代码标识符（camelCase / snake_case / kebab-case）
  const identPattern = /(?:^|\s|["'(])([a-z][a-z0-9_]*(?:_[a-z0-9_]+){1,}|[a-z][a-z0-9]*(?:[A-Z][a-z0-9]*){1,}|[a-z][a-z0-9]*(?:-[a-z0-9]+){1,})(?:$|\s|["')\]],.])/g;
  let identMatch;
  while ((identMatch = identPattern.exec(text)) !== null) {
    if (identMatch[1] && identMatch[1].length > 3 && identMatch[1].length < 40) {
      entities.push(identMatch[1].toLowerCase());
    }
  }

  return [...new Set(entities)];
}

/** 从 Agent 文本中提取文件路径（排除 URL、系统路径、API 路由、枚举、平台名） */
function extractFilePaths(text: string): string[] {
  const paths: string[] = [];
  const pattern = /(?:\/[\w.-]+){2,}(?:\.\w+)?/g;
  let match;
  while ((match = pattern.exec(text)) !== null) {
    const p = match[0];
    if (p.length <= 5) continue;
    // 排除 URL
    const before = text.slice(Math.max(0, match.index - 10), match.index);
    if (/https?:/.test(before)) continue;
    // 排除纯系统路径
    if (/^\/usr\/|^\/etc\/|^\/var\/|^\/tmp\/|^\/opt\//.test(p)) continue;
    // 排除 API 路由（常见 HTTP 路径段，无文件扩展名）
    if (/^\/(v\d+|api|web|rest|graphql|health|status|auth|login|logout|sessions|users|items|assets)\b/i.test(p) && !/\.\w{1,10}$/.test(p)) continue;
    // 排除平台名（/linux/macos, /windows, /android, /ios 等）
    if (/^\/(linux|macos|windows|android|ios|unix|darwin|freebsd)(\/|$)/i.test(p)) continue;
    // 排除枚举列表上下文（如 "Snip/Sleep/CtxInspect" 或 "Read/Write/Edit"）
    const contextBefore = text.slice(Math.max(0, match.index - 30), match.index);
    const contextAfter = text.slice(match.index + p.length, match.index + p.length + 30);
    if (/(?:\/[\w.-]+){2,}/.test(contextBefore) || /(?:\/[\w.-]+){2,}/.test(contextAfter)) {
      // 被其他 /segment 包围，可能是枚举而非路径——只有含扩展名的才算文件路径
      if (!/\.\w{1,10}$/.test(p)) continue;
    }
    paths.push(p);
  }
  return [...new Set(paths)];
}

/** 从工具调用参数中提取文件路径（排除 glob pattern） */
function extractToolPaths(toolCalls: ToolCallRequest[]): string[] {
  const paths: string[] = [];
  for (const tc of toolCalls) {
    const args = tc.arguments;
    if (args.file_path && typeof args.file_path === "string") {
      paths.push(args.file_path);
    }
    if (args.path && typeof args.path === "string") {
      paths.push(args.path);
    }
    // pattern 可能是 glob，只有不含通配符的才算路径
    if (args.pattern && typeof args.pattern === "string" && !/[*?\[]/.test(args.pattern)) {
      paths.push(args.pattern);
    }
    if (args.command && typeof args.command === "string") {
      const cmdPaths = extractFilePaths(args.command);
      paths.push(...cmdPaths);
    }
  }
  return [...new Set(paths)];
}

/** 检测用户消息中的否定/不满信号（精确化模式，排除非人类消息） */
function detectUserNegSignals(text: string): string[] {
  const trimmed = text.trim();

  // 跳过非人类消息（cron 模板触发的不应算不满）
  if (isNonHumanMessage(trimmed)) return [];

  // 跳过纯错误日志粘贴（用户贴的错误日志中含"错误"/"failed"是引用不是不满）
  if (/^(error|Error|panic|failed|thread.*panicked)/i.test(trimmed) && trimmed.length > 100) return [];

  const signals: string[] = [];
  const patterns = [
    // "不对" 作为独立否定词（前后不能是"这不对"这种中性表述）
    { re: /(?<![这那])不对(?![吗吧])/u, label: "否定" },
    // "错误" 较少在闲聊中出现
    { re: /错误|搞错了|失败了|failed|wrong|incorrect/i, label: "否定" },
    // "不行" 通常是不满
    { re: /不行|不可以|行不通/i, label: "否定" },
    // "没有" + 负面结果
    { re: /没有(?:效果|用|成功|解决|完成|生效|变化)/i, label: "否定" },
    // 重试要求
    { re: /再试|重试|redo|retry|再来一次|again/i, label: "重试要求" },
    // 纠正指令
    { re: /不要这样|别这样|stop|don't do that/i, label: "纠正" },
    // 质疑（需要较长上下文才算不满，要求"为什么"后跟负面词）
    { re: /为什么.{0,5}(不行|错误|失败|不对|没用)/i, label: "质疑" },
    // 困惑+不满组合
    { re: /不对劲|怎么还是|还是(不对|不行|错误|失败)/i, label: "困惑" },
  ];

  for (const { re, label } of patterns) {
    if (re.test(text)) {
      signals.push(label);
    }
  }
  return signals;
}

/** 计算两个文本的 n-gram Jaccard 相似度（文本截断到 JACCARD_TEXT_LIMIT 防止内存爆炸） */
function ngramJaccard(textA: string, textB: string, n = 3): number {
  const normalize = (s: string) => s.toLowerCase().replace(/\s+/g, " ").trim().slice(0, JACCARD_TEXT_LIMIT);
  const a = normalize(textA);
  const b = normalize(textB);

  if (a.length < n || b.length < n) return 0;

  const ngrams = (s: string) => {
    const set = new Set<string>();
    for (let i = 0; i <= s.length - n; i++) {
      set.add(s.slice(i, i + n));
    }
    return set;
  };

  const setA = ngrams(a);
  const setB = ngrams(b);

  if (setA.size === 0 && setB.size === 0) return 0;

  let intersection = 0;
  for (const ng of setA) {
    if (setB.has(ng)) intersection++;
  }

  const union = setA.size + setB.size - intersection;
  return union === 0 ? 0 : intersection / union;
}

/** 检测截断/未完成信号（仅匹配明确的截断，降低误报） */
function detectTruncation(text: string): boolean {
  // 只检测明确的截断信号
  const truncationSignals = [
    /output was truncated/i,
    /response was (cut off|truncated)/i,
    /\[truncated\]/i,
  ];

  return truncationSignals.some((re) => re.test(text));
}

// ── 路径匹配 ──

/** 按路径分段尾部匹配（取最后 2 段做精确比较） */
function pathSegmentsMatch(agentPath: string, toolPath: string): boolean {
  const getTailSegments = (p: string) => {
    const parts = p.split("/").filter(Boolean);
    return parts.slice(-2).join("/");
  };
  const agentTail = getTailSegments(agentPath);
  const toolTail = getTailSegments(toolPath);
  // 要求尾部完全相等，或一个包含另一个（处理细微差异）
  return agentTail === toolTail || agentTail.endsWith("/" + toolTail) || toolTail.endsWith("/" + agentTail);
}

// ── 主分析 ──

export function analyzeAnswerQuality(loader: DataLoader): DefectReport[] {
  printSection("Agent 回答语义质量分析");

  const threads = loader.loadVisibleThreads();
  const records: AnswerQualityRecord[] = [];

  for (const thread of threads) {
    const messages = loader.loadMessages(thread.id);

    // 只分析有多轮交互的会话
    const userMsgs = messages.filter((m) => m.role === "user");
    if (userMsgs.length < 1) continue;

    const record = analyzeSession(thread.id, thread.title || "", messages);
    records.push(record);
  }

  // ── 输出报告 ──

  printMetric("分析会话数", records.length);

  // 质量评分分布（互斥半开区间）
  printSection("质量评分分布");
  const scoreBins: { label: string; min: number; max: number }[] = [
    { label: "优秀 (80-100)", min: 80, max: Infinity },
    { label: "良好 (60-80)", min: 60, max: 80 },
    { label: "一般 (40-60)", min: 40, max: 60 },
    { label: "较差 (20-40)", min: 20, max: 40 },
    { label: "很差 (0-20)", min: 0, max: 20 },
  ];

  const scoreRows = scoreBins.map((bin) => {
    const count = records.filter((r) => r.qualityScore >= bin.min && r.qualityScore < bin.max).length;
    return [bin.label, String(count), (count / records.length * 100).toFixed(1) + "%"];
  });
  printTable(["质量等级", "会话数", "占比"], scoreRows);

  // 指令覆盖率分布
  printSection("用户指令覆盖率");
  const withKw = records.filter((r) => r.hasKeywords);
  const noKw = records.filter((r) => !r.hasKeywords);
  printMetric("有明确指令", withKw.length, " 会话");
  printMetric("无明确指令（不可评估）", noKw.length, " 会话");
  if (withKw.length > 0) {
    const highCov = withKw.filter((r) => r.coverageRate >= 0.8).length;
    const midCov = withKw.filter((r) => r.coverageRate >= 0.4 && r.coverageRate < 0.8).length;
    const lowCov = withKw.filter((r) => r.coverageRate < 0.4).length;
    printMetric("  高覆盖 (≥80%)", highCov, ` (${(highCov / withKw.length * 100).toFixed(1)}%)`);
    printMetric("  中覆盖 (40-80%)", midCov, ` (${(midCov / withKw.length * 100).toFixed(1)}%)`);
    printMetric("  低覆盖 (<40%)", lowCov, ` (${(lowCov / withKw.length * 100).toFixed(1)}%)`);
  }

  // 文件路径幻觉
  printSection("文件路径验证率");
  const withPaths = records.filter((r) => r.hasPaths);
  if (withPaths.length > 0) {
    const avgVerifyRate = withPaths.reduce((a, r) => a + r.pathVerifyRate, 0) / withPaths.length;
    printMetric("提及路径的会话", withPaths.length);
    printMetric("平均路径验证率", (avgVerifyRate * 100).toFixed(1) + "%");
    const unverified = withPaths.filter((r) => r.pathVerifyRate < 0.5);
    printMetric("路径验证率 <50%", unverified.length, " 会话");
  } else {
    printMetric("提及路径的会话", 0);
  }

  // 用户不满信号
  printSection("用户不满信号");
  const withNegSignals = records.filter((r) => r.userNegSignals > 0);
  printMetric("含不满信号的会话", withNegSignals.length, ` (${(withNegSignals.length / records.length * 100).toFixed(1)}%)`);
  if (withNegSignals.length > 0) {
    const avgNeg = withNegSignals.reduce((a, r) => a + r.userNegSignals, 0) / withNegSignals.length;
    printMetric("平均每不满会话信号数", avgNeg.toFixed(1));
  }

  // 截断/未完成
  printSection("截断/未完成检测");
  const truncated = records.filter((r) => r.hasTruncation);
  printMetric("含截断信号的会话", truncated.length);

  // 回答重复度（互斥分桶）
  printSection("Agent 回答重复度");
  const highRep = records.filter((r) => r.answerRepetition >= 0.3).length;
  const midRep = records.filter((r) => r.answerRepetition >= 0.1 && r.answerRepetition < 0.3).length;
  const lowRep = records.filter((r) => r.answerRepetition < 0.1).length;
  printMetric("高重复 (≥0.3)", highRep, " 会话");
  printMetric("中等 (0.1-0.3)", midRep, " 会话");
  printMetric("低重复 (<0.1)", lowRep, " 会话");

  // 质量最差的 Top 10
  printSection("质量评分最低的会话 (Top 10)");
  const worstSessions = [...records]
    .sort((a, b) => a.qualityScore - b.qualityScore)
    .slice(0, 10);

  const worstRows = worstSessions.map((r) => [
    r.threadId.slice(0, 12) + "...",
    r.threadTitle.slice(0, 30),
    String(r.qualityScore),
    r.hasKeywords ? (r.coverageRate * 100).toFixed(0) + "%" : "N/A",
    r.userNegSignals > 0 ? String(r.userNegSignals) : "-",
    r.hasTruncation ? "Y" : "-",
  ]);
  printTable(["Session", "标题", "质量分", "覆盖率", "不满", "截断"], worstRows);

  // ── 缺陷报告 ──

  const reports: DefectReport[] = [];

  // 低覆盖率报告
  const lowCoverage = records.filter((r) => r.hasKeywords && r.userKeywords >= 2 && r.coverageRate < 0.4);
  if (lowCoverage.length > 3) {
    reports.push({
      id: "SEM-001",
      severity: "medium",
      category: "指令覆盖",
      title: "部分会话 Agent 未完全执行用户指令",
      description: `${lowCoverage.length} 个会话中用户有 ≥2 个明确需求关键词，但 Agent 的工具调用覆盖率 <40%。Agent 可能遗漏了部分任务。`,
      evidence: lowCoverage.slice(0, 5).map((r) =>
        `${r.threadTitle.slice(0, 30)}: ${r.userKeywords}个需求, 覆盖${(r.coverageRate * 100).toFixed(0)}%`
      ),
      affectedSessions: lowCoverage.map((r) => r.threadId),
      recommendation: "在 Agent 系统提示中增加'逐条检查用户需求是否已执行'的指令。在 TodoWrite 中自动解析用户需求为 checklist。",
      confidence: 0.55,
    });
  }

  // 路径幻觉报告
  const pathHallucination = withPaths.filter((r) => r.mentionedPaths >= 3 && r.pathVerifyRate < 0.3);
  if (pathHallucination.length > 2) {
    reports.push({
      id: "SEM-002",
      severity: "medium",
      category: "路径幻觉",
      title: "Agent 提到的文件路径多数未验证",
      description: `${pathHallucination.length} 个会话中 Agent 在文本中提到 ≥3 个文件路径，但 <30% 的路径有对应的工具调用验证。Agent 可能编造了不存在的文件路径。`,
      evidence: pathHallucination.slice(0, 5).map((r) =>
        `${r.threadTitle.slice(0, 30)}: ${r.mentionedPaths}路径, 验证${(r.pathVerifyRate * 100).toFixed(0)}%`
      ),
      affectedSessions: pathHallucination.map((r) => r.threadId),
      recommendation: "Agent 应在提到文件路径后，先用 Read/Glob 验证文件是否存在。在系统提示中强调'不要编造文件路径'。",
      confidence: 0.45,
    });
  }

  // 高用户不满信号报告
  const highNeg = records.filter((r) => r.userNegSignals >= 3);
  if (highNeg.length > 2) {
    reports.push({
      id: "SEM-003",
      severity: "high",
      category: "用户不满",
      title: "部分会话用户频繁表达不满",
      description: `${highNeg.length} 个会话中用户有 ≥3 次否定/纠正/质疑信号。Agent 的回答可能未达到用户期望。`,
      evidence: highNeg.slice(0, 5).map((r) =>
        `${r.threadTitle.slice(0, 30)}: ${r.userNegSignals}次不满信号, 质量分${r.qualityScore}`
      ),
      affectedSessions: highNeg.map((r) => r.threadId),
      recommendation: "分析这些会话中 Agent 的具体失败模式。在检测到用户不满时，Agent 应主动请求澄清而非盲目继续。",
      confidence: 0.55,
    });
  }

  // 截断报告
  if (truncated.length > 3) {
    reports.push({
      id: "SEM-004",
      severity: "low",
      category: "回答截断",
      title: "部分会话 Agent 回答被截断",
      description: `${truncated.length} 个会话中 Agent 的最后一条消息包含截断信号，但后续没有继续。用户可能收到了不完整的回答。`,
      evidence: truncated.slice(0, 5).map((r) =>
        `${r.threadTitle.slice(0, 30)}: 质量分${r.qualityScore}`
      ),
      affectedSessions: truncated.map((r) => r.threadId),
      recommendation: "检查 max_tokens 设置是否合理。在 Agent 回复被截断时，应自动续写而非停止。",
      confidence: 0.7,
    });
  }

  return reports;
}

// ── 会话级分析 ──

function analyzeSession(
  threadId: string,
  threadTitle: string,
  messages: MessageRow[]
): AnswerQualityRecord {
  const allUserKeywords: string[] = [];
  const allAgentTexts: string[] = [];
  const allToolPaths: string[] = [];
  const allAgentPaths: string[] = [];
  let userNegSignals = 0;
  let hasTruncation = false;

  for (const msg of messages) {
    const parsed = DataLoader.parseContent(msg.content);
    if (!parsed) continue;

    if (msg.role === "user") {
      const content = (parsed as any).content;
      const text = typeof content === "string" ? content :
        Array.isArray(content) ? content.filter((b: any) => b.type === "text").map((b: any) => b.text || "").join("") : "";
      const keywords = extractNeedsKeywords(text);
      allUserKeywords.push(...keywords);

      userNegSignals += detectUserNegSignals(text).length;

    } else if (msg.role === "assistant") {
      const ai = parsed as any;
      const blocks = Array.isArray(ai.content) ? ai.content : [];

      let agentText = "";
      for (const block of blocks) {
        if (block.type === "text" && block.text) {
          agentText += block.text + " ";
          allAgentPaths.push(...extractFilePaths(block.text));
        }
      }
      allAgentTexts.push(agentText.trim());

      const toolCalls = DataLoader.extractToolCalls(parsed);
      allToolPaths.push(...extractToolPaths(toolCalls));
    }
  }

  // 计算指令覆盖率
  const uniqueKeywords = [...new Set(allUserKeywords)];
  const hasKeywords = uniqueKeywords.length > 0;
  let coveredCount = 0;
  if (hasKeywords) {
    const allText = allAgentTexts.join(" ").toLowerCase();
    const allPathsLower = allToolPaths.map((p) => p.toLowerCase());
    for (const kw of uniqueKeywords) {
      if (allText.includes(kw) || allPathsLower.some((p) => p.includes(kw))) {
        coveredCount++;
      }
    }
  }
  // 无关键词时用中性值 0.5，避免无指令会话质量分虚高
  const coverageRate = hasKeywords ? coveredCount / uniqueKeywords.length : 0.5;

  // 计算路径验证率（按路径分段尾部匹配）
  const uniqueAgentPaths = [...new Set(allAgentPaths.map((p) => p.toLowerCase()))];
  const hasPaths = uniqueAgentPaths.length > 0;
  let verifiedPaths = 0;
  if (hasPaths) {
    const normalizedToolPaths = allToolPaths.map((p) => p.toLowerCase());
    for (const ap of uniqueAgentPaths) {
      if (normalizedToolPaths.some((tp) => pathSegmentsMatch(ap, tp))) {
        verifiedPaths++;
      }
    }
  }
  // 无路径时用中性值
  const pathVerifyRate = hasPaths ? verifiedPaths / uniqueAgentPaths.length : 0.5;

  // 检测截断（仅检查最后一条 Agent 消息）
  const lastAgentText = allAgentTexts.length > 0 ? allAgentTexts[allAgentTexts.length - 1] : "";
  hasTruncation = detectTruncation(lastAgentText);

  // 计算回答重复度（文本已截断到 JACCARD_TEXT_LIMIT）
  let totalSimilarity = 0;
  let pairs = 0;
  for (let i = 0; i < allAgentTexts.length; i++) {
    for (let j = i + 1; j < Math.min(i + 5, allAgentTexts.length); j++) {
      totalSimilarity += ngramJaccard(allAgentTexts[i], allAgentTexts[j]);
      pairs++;
    }
  }
  const answerRepetition = pairs > 0 ? totalSimilarity / pairs : 0;

  // 综合质量评分
  let score = 100;
  score -= (1 - coverageRate) * SCORE_WEIGHTS.coverage;
  score -= (1 - pathVerifyRate) * SCORE_WEIGHTS.pathVerify;
  score -= Math.min(userNegSignals * SCORE_WEIGHTS.negSignalPer, SCORE_WEIGHTS.negSignalMax);
  score -= hasTruncation ? SCORE_WEIGHTS.truncation : 0;
  score -= answerRepetition * SCORE_WEIGHTS.repetition;
  score = Math.max(0, Math.min(100, Math.round(score)));

  return {
    threadId,
    threadTitle,
    totalTurns: messages.filter((m) => m.role === "user").length,
    userKeywords: uniqueKeywords.length,
    coveredKeywords: coveredCount,
    coverageRate,
    hasKeywords,
    mentionedPaths: uniqueAgentPaths.length,
    verifiedPaths,
    pathVerifyRate,
    hasPaths,
    userNegSignals,
    hasTruncation,
    answerRepetition,
    qualityScore: score,
  };
}
