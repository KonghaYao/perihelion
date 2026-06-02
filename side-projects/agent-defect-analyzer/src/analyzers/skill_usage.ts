//! Skill 使用效能分析器——测量每个 skill 的使用频次、效率、产出质量。
//!
//! 分析维度：
//! 1. **Skill 使用频次 & 活跃度**：每个 skill 被调用了多少次，趋势如何
//! 2. **Skill 效率**：每个 skill 平均消耗的工具调用数、消息数、时长
//! 3. **Skill 产出**：每个 skill 触发后的 Edit 数、SubAgent 数、commit 数
//! 4. **Skill 链**：同一会话中 skill 的组合模式（如 brainstorming → writing-plans → executing-plans）
//! 5. **Skill 对质量的影响**：使用 skill vs 不使用 skill 的会话质量对比

import type { DefectReport } from "../../types.js";
import { DataLoader } from "../utils/data_loader.js";
import { printSection, printMetric, printTable } from "../utils/report.js";

// ── 数据结构 ──

interface SkillUsage {
  name: string;
  /** 用户直接调用次数（/skill-name） */
  directCalls: number;
  /** SubAgent 中调用次数 */
  subAgentCalls: number;
  /** SKILL.md 被 Read 的次数 */
  skillFileReads: number;
  /** 相关会话数 */
  sessionCount: number;
  /** 平均每会话工具调用数 */
  avgToolCalls: number;
  /** 平均每会话 Edit 数 */
  avgEdits: number;
  /** 平均每会话 SubAgent 数 */
  avgSubAgents: number;
  /** 平均每会话消息数 */
  avgMessages: number;
}

interface SkillChain {
  pattern: string;
  count: number;
  exampleTitles: string[];
}

interface SessionSkillInfo {
  threadId: string;
  threadTitle: string;
  threadDate: string;
  skills: string[];
  toolCallCount: number;
  editCount: number;
  subAgentCount: number;
  messageCount: number;
  hasSkillFileRead: boolean;
}

// ── 主分析 ──

export function analyzeSkillUsage(loader: DataLoader): DefectReport[] {
  printSection("Skill 使用效能分析");

  const threads = loader.loadVisibleThreads();
  const skillStats: Record<string, {
    directCalls: number;
    subAgentCalls: number;
    skillFileReads: number;
    sessions: Set<string>;
    toolCalls: number[];
    edits: number[];
    subAgents: number[];
    messages: number[];
  }> = {};

  const sessionInfos: SessionSkillInfo[] = [];
  const skillChains: Record<string, { count: number; titles: string[] }> = {};

  // 已知 skill 名列表
  const knownSkills = new Set([
    "writing-plans", "executing-plans", "brainstorming", "grill-me",
    "issue-create", "issue-archive", "systematic-debugging", "slop-cleaner",
    "code-review", "ultra-batch", "subagent-driven-development",
    "using-git-worktrees", "finishing-a-development-branch",
    "verification-before-completion", "requesting-code-review",
    "receiving-code-review", "dispatching-parallel-agents",
    "skill-creator", "improve-codebase-architecture",
    "claude-md-improver", "frontend-design", "langfuse",
    "llm-log-analyzer", "compact", "sdd-brainstorming", "sdd-writing-plans",
    "interview", "setup",
  ]);

  for (const thread of threads) {
    const messages = loader.loadMessages(thread.id);
    const subAgents = loader.loadSubAgents(thread.id);

    // 提取该会话中的 skill 列表（按首次出现顺序）
    const skillsInSession: string[] = [];
    let toolCallCount = 0;
    let editCount = 0;

    for (const msg of messages) {
      // 从用户消息提取 slash 命令
      if (msg.role === "user") {
        const parsed = DataLoader.parseContent(msg.content);
        if (!parsed) continue;
        const text = extractText(parsed);
        const slashMatch = text.trim().match(/^\/(\w+[-\w]*)/);
        if (slashMatch && knownSkills.has(slashMatch[1])) {
          if (!skillsInSession.includes(slashMatch[1])) {
            skillsInSession.push(slashMatch[1]);
          }
          const sk = slashMatch[1];
          if (!skillStats[sk]) skillStats[sk] = makeEmpty();
          skillStats[sk].directCalls++;
          skillStats[sk].sessions.add(thread.id);
        }
      }

      // 从 assistant 消息统计工具调用
      if (msg.role === "assistant") {
        const parsed = DataLoader.parseContent(msg.content);
        if (!parsed) continue;
        const toolCalls = DataLoader.extractToolCalls(parsed);
        toolCallCount += toolCalls.length;
        for (const tc of toolCalls) {
          if (["Edit", "Write"].includes(tc.name)) editCount++;
          // 统计 SKILL.md 读取
          if (tc.name === "Read" && typeof tc.arguments.file_path === "string") {
            const fp = tc.arguments.file_path;
            if (fp.includes("SKILL.md")) {
              const skillMatch = fp.match(/skills\/([^/]+)\//);
              if (skillMatch) {
                const sk = skillMatch[1];
                if (!skillStats[sk]) skillStats[sk] = makeEmpty();
                skillStats[sk].skillFileReads++;
              }
            }
          }
        }
      }
    }

    // Skill 链（同一会话中使用多个 skill 的顺序）
    if (skillsInSession.length >= 2) {
      const chainKey = skillsInSession.join(" → ");
      if (!skillChains[chainKey]) skillChains[chainKey] = { count: 0, titles: [] };
      skillChains[chainKey].count++;
      if (skillChains[chainKey].titles.length < 3) {
        skillChains[chainKey].titles.push(thread.title?.slice(0, 30) || "");
      }
    }

    // 记录每个 skill 的效率指标
    for (const sk of skillsInSession) {
      if (!skillStats[sk]) skillStats[sk] = makeEmpty();
      skillStats[sk].sessions.add(thread.id);
      skillStats[sk].toolCalls.push(toolCallCount);
      skillStats[sk].edits.push(editCount);
      skillStats[sk].subAgents.push(subAgents.length);
      skillStats[sk].messages.push(messages.length);
    }

    sessionInfos.push({
      threadId: thread.id,
      threadTitle: thread.title || "",
      threadDate: thread.created_at.slice(0, 10),
      skills: skillsInSession,
      toolCallCount,
      editCount,
      subAgentCount: subAgents.length,
      messageCount: messages.length,
      hasSkillFileRead: false,
    });
  }

  // ── 输出报告 ──

  // 1. Skill 使用频次排行
  printSection("Skill 使用频次排行");
  const skillRows = Object.entries(skillStats)
    .sort((a, b) => b[1].directCalls - a[1].directCalls)
    .map(([name, stats]) => [
      "/" + name,
      String(stats.directCalls),
      String(stats.sessions.size),
      String(stats.skillFileReads),
    ]);
  printTable(["Skill", "调用次数", "涉及会话", "SKILL.md读取"], skillRows);

  // 2. Skill 效率对比
  printSection("Skill 效率对比（每会话平均）");
  const efficiencyRows = Object.entries(skillStats)
    .filter(([, stats]) => stats.directCalls >= 3)
    .sort((a, b) => b[1].directCalls - a[1].directCalls)
    .map(([name, stats]) => {
      const n = stats.toolCalls.length || 1;
      return [
        "/" + name,
        (stats.toolCalls.reduce((a, b) => a + b, 0) / n).toFixed(0),
        (stats.edits.reduce((a, b) => a + b, 0) / n).toFixed(1),
        (stats.subAgents.reduce((a, b) => a + b, 0) / n).toFixed(1),
        (stats.messages.reduce((a, b) => a + b, 0) / n).toFixed(0),
      ];
    });
  printTable(["Skill", "平均工具", "平均Edit", "平均SubAgent", "平均消息"], efficiencyRows);

  // 3. Skill 分类效率
  printSection("Skill 按用途分类");
  const categories: Record<string, { skills: string[]; avgTools: number; avgEdits: number; sessions: number }> = {
    "规划类": { skills: ["writing-plans", "executing-plans", "brainstorming", "grill-me", "sdd-brainstorming", "sdd-writing-plans", "interview"], avgTools: 0, avgEdits: 0, sessions: 0 },
    "执行类": { skills: ["subagent-driven-development", "ultra-batch", "dispatching-parallel-agents", "finishing-a-development-branch"], avgTools: 0, avgEdits: 0, sessions: 0 },
    "质量类": { skills: ["code-review", "slop-cleaner", "systematic-debugging", "verification-before-completion", "requesting-code-review", "claude-md-improver"], avgTools: 0, avgEdits: 0, sessions: 0 },
    "流程类": { skills: ["issue-create", "issue-archive", "using-git-worktrees", "langfuse", "llm-log-analyzer"], avgTools: 0, avgEdits: 0, sessions: 0 },
  };

  for (const [cat, info] of Object.entries(categories)) {
    let totalTools = 0, totalEdits = 0, totalSessions = 0, count = 0;
    for (const sk of info.skills) {
      if (skillStats[sk]) {
        const s = skillStats[sk];
        totalTools += s.toolCalls.reduce((a, b) => a + b, 0);
        totalEdits += s.edits.reduce((a, b) => a + b, 0);
        totalSessions += s.sessions.size;
        count += s.toolCalls.length;
      }
    }
    const n = count || 1;
    info.avgTools = totalTools / n;
    info.avgEdits = totalEdits / n;
    info.sessions = totalSessions;
  }

  const catRows = Object.entries(categories).map(([cat, info]) => [
    cat,
    String(info.sessions),
    info.avgTools.toFixed(0),
    info.avgEdits.toFixed(1),
  ]);
  printTable(["类别", "会话数", "平均工具", "平均Edit"], catRows);

  // 4. Skill 链（组合模式）
  printSection("Skill 组合链 Top 15");
  const topChains = Object.entries(skillChains)
    .sort((a, b) => b[1].count - a[1].count)
    .slice(0, 15);
  const chainRows = topChains.map(([chain, data]) => [
    chain.slice(0, 50),
    String(data.count),
    data.titles[0]?.slice(0, 25) || "",
  ]);
  printTable(["Skill 链", "次数", "示例"], chainRows);

  // 5. Skill vs 无 Skill 对比
  printSection("使用 Skill vs 无 Skill 会话对比");
  const withSkill = sessionInfos.filter((s) => s.skills.length > 0);
  const withoutSkill = sessionInfos.filter((s) => s.skills.length === 0);

  const avgOf = (arr: number[]) => arr.length > 0 ? (arr.reduce((a, b) => a + b, 0) / arr.length) : 0;

  const compareRows = [
    ["会话数", String(withSkill.length), String(withoutSkill.length)],
    ["平均工具调用", avgOf(withSkill.map((s) => s.toolCallCount)).toFixed(0), avgOf(withoutSkill.map((s) => s.toolCallCount)).toFixed(0)],
    ["平均 Edit", avgOf(withSkill.map((s) => s.editCount)).toFixed(1), avgOf(withoutSkill.map((s) => s.editCount)).toFixed(1)],
    ["平均 SubAgent", avgOf(withSkill.map((s) => s.subAgentCount)).toFixed(1), avgOf(withoutSkill.map((s) => s.subAgentCount)).toFixed(1)],
    ["平均消息数", avgOf(withSkill.map((s) => s.messageCount)).toFixed(0), avgOf(withoutSkill.map((s) => s.messageCount)).toFixed(0)],
  ];
  printTable(["指标", "有 Skill", "无 Skill"], compareRows);

  // 6. 时间趋势
  printSection("Skill 使用趋势（按周）");
  const weeklySkillUsage: Record<string, { total: number; uniqueSkills: Set<string> }> = {};
  for (const s of sessionInfos) {
    if (s.skills.length === 0) continue;
    const week = s.threadDate.slice(0, 10); // 按日（数据不够按周）
    // 按周
    const date = new Date(s.threadDate);
    const weekNum = Math.floor((date.getDate() - 1) / 7) + 1;
    const weekKey = s.threadDate.slice(0, 7) + "-W" + weekNum;
    if (!weeklySkillUsage[weekKey]) weeklySkillUsage[weekKey] = { total: 0, uniqueSkills: new Set() };
    weeklySkillUsage[weekKey].total++;
    for (const sk of s.skills) weeklySkillUsage[weekKey].uniqueSkills.add(sk);
  }
  const weekRows = Object.entries(weeklySkillUsage)
    .sort(([a], [b]) => a.localeCompare(b))
    .map(([week, data]) => [week, String(data.total), String(data.uniqueSkills.size), [...data.uniqueSkills].slice(0, 4).join(", ")]);
  printTable(["周", "Skill会话", "种类数", "Top Skills"], weekRows);

  // 7. 最重 Skill（SKILL.md 被读取最多）
  printSection("SKILL.md 读取频次 Top 10");
  const topSkillReads = Object.entries(skillStats)
    .filter(([, s]) => s.skillFileReads > 0)
    .sort((a, b) => b[1].skillFileReads - a[1].skillFileReads)
    .slice(0, 10);
  const readRows = topSkillReads.map(([name, stats]) => [
    "/" + name,
    String(stats.skillFileReads),
    String(stats.directCalls),
    (stats.skillFileReads / (stats.directCalls || 1)).toFixed(1),
  ]);
  printTable(["Skill", "读取次数", "调用次数", "读取/调用比"], readRows);

  // ── 缺陷报告 ──
  const reports: DefectReport[] = [];

  // 高读取/调用比（Skill 文件被反复读取但很少调用）
  const highReadRatio = topSkillReads.filter(([, s]) => s.directCalls > 3 && s.skillFileReads / s.directCalls > 5);
  if (highReadRatio.length > 0) {
    reports.push({
      id: "SKILL-001",
      severity: "low",
      category: "Skill 缓存",
      title: "部分 Skill 文件被反复读取但调用不多",
      description: highReadRatio.map(([name, s]) =>
        `/${name}: SKILL.md 被 Read ${s.skillFileReads} 次，但仅被调用 ${s.directCalls} 次（比例 ${s.skillFileReads / s.directCalls}）`
      ).join("；") + "。SubAgent 可能每次都重新读取 Skill 文件，而非缓存内容。",
      evidence: highReadRatio.map(([name, s]) =>
        `/${name}: ${s.skillFileReads}读取 / ${s.directCalls}调用 = ${s.skillFileReads / s.directCalls}x`
      ),
      affectedSessions: [],
      recommendation: "Skill 内容应在首次加载后缓存在 Agent 上下文中。SubAgent 继承父 Agent 的 Skill 知识，避免重复读取。考虑在 SkillPreload middleware 中实现 Skill 内容的去重缓存。",
      confidence: 0.5,
    });
  }

  // Skill 使用集中度
  const topSkill = Object.entries(skillStats).sort((a, b) => b[1].directCalls - a[1].directCalls)[0];
  const totalSkillCalls = Object.values(skillStats).reduce((a, s) => a + s.directCalls, 0);
  if (topSkill && topSkill[1].directCalls / totalSkillCalls > 0.3) {
    reports.push({
      id: "SKILL-002",
      severity: "low",
      category: "Skill 使用分布",
      title: `Skill 使用高度集中于 /${topSkill[0]}`,
      description: `/${topSkill[0]} 占所有 Skill 调用的 ${(topSkill[1].directCalls / totalSkillCalls * 100).toFixed(1)}%（${topSkill[1].directCalls}/${totalSkillCalls}）。其余 ${Object.keys(skillStats).length - 1} 个 Skill 共享 ${(100 - topSkill[1].directCalls / totalSkillCalls * 100).toFixed(1)}%。部分 Skill 可能价值低或发现性差。`,
      evidence: Object.entries(skillStats)
        .sort((a, b) => b[1].directCalls - a[1].directCalls)
        .slice(0, 8)
        .map(([name, s]) => `/${name}: ${s.directCalls}次`),
      affectedSessions: [],
      recommendation: "1. 评估低频 Skill 是否仍需要保留为独立 Skill。2. 改善 Skill 的触发条件描述，提高 LLM 的选择准确度。3. 考虑将相关 Skill 合并（如 issue-create + issue-archive）。",
      confidence: 0.4,
    });
  }

  // 未使用的 Skill
  const unusedSkills = [...knownSkills].filter((sk) => !skillStats[sk] || skillStats[sk].directCalls === 0);
  if (unusedSkills.length > 3) {
    reports.push({
      id: "SKILL-003",
      severity: "low",
      category: "Skill 使用分布",
      title: `${unusedSkills.length} 个已注册 Skill 从未被调用`,
      description: `${unusedSkills.join(", ")}。这些 Skill 虽然已注册但从未通过 slash 命令触发。可能是触发条件描述不够匹配用户意图，或用户不知道这些 Skill 的存在。`,
      evidence: unusedSkills.map((s) => `/${s}: 0 次调用`),
      affectedSessions: [],
      recommendation: "1. 检查这些 Skill 的 description 是否能被 LLM 正确匹配到用户意图。2. 考虑改善 Skill 发现机制（如在帮助信息中列出所有 Skill）。3. 评估是否需要保留。",
      confidence: 0.35,
    });
  }

  return reports;
}

// ── Helpers ──

function makeEmpty() {
  return {
    directCalls: 0,
    subAgentCalls: 0,
    skillFileReads: 0,
    sessions: new Set<string>(),
    toolCalls: [] as number[],
    edits: [] as number[],
    subAgents: [] as number[],
    messages: [] as number[],
  };
}

function extractText(parsed: any): string {
  const content = parsed.content;
  return typeof content === "string" ? content :
    Array.isArray(content) ? content.filter((b: any) => b.type === "text").map((b: any) => b.text || "").join("") : "";
}
