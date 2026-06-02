//! Skill 链深度分析器——测量链的完成率、断裂点、最优路径和效能对比。
//!
//! 核心洞察：
//! 1. **链完成率**：用户启动了链但没走完的比例（如 brainstorming 但没执行类 Skill）
//! 2. **链效能对比**：完整链 vs 部分链 vs 无链的 Edit 产出和工具效率
//! 3. **断裂点分析**：链在哪里断了？为什么断？
//! 4. **最优路径**：哪条链的 Edit/工具比最高？
//! 5. **执行类 Skill 多样性**：writing-plans 后用户实际选择了什么执行方式
//!
//! 注意：issue-archive 是跨会话的定期维护，不算在单会话链内。

import type { DefectReport } from "../../types.js";
import { DataLoader } from "../utils/data_loader.js";
import { printSection, printMetric, printTable } from "../utils/report.js";

// ── 执行类 Skill（writing-plans 后可能选择的任何一种执行方式）──
const EXECUTION_SKILLS = new Set([
  "executing-plans",
  "subagent-driven-development",
  "ultra-batch",
  "dispatching-parallel-agents",
]);

// ── 已知 Skill 工作流模板 ──

const KNOWN_WORKFLOWS: { name: string; chain: string[]; description: string }[] = [
  {
    name: "规划执行流",
    chain: ["brainstorming", "writing-plans", "__EXEC__"],
    description: "brainstorm → plan → 任一执行类 Skill（executing-plans/subagent-driven/ultra-batch）",
  },
  {
    name: "调试修复流",
    chain: ["issue-create", "systematic-debugging"],
    description: "记录 bug → 系统化调试",
  },
  {
    name: "Issue 规划执行流",
    chain: ["issue-create", "writing-plans", "__EXEC__"],
    description: "创建 issue → 规划 → 任一执行类 Skill",
  },
  {
    name: "质量审查流",
    chain: ["slop-cleaner", "improve-codebase-architecture"],
    description: "代码扫描 → 架构改进",
  },
  {
    name: "设计验证流",
    chain: ["grill-me", "writing-plans"],
    description: "质疑设计 → 输出计划",
  },
  {
    name: "并行执行流",
    chain: ["ultra-batch", "subagent-driven-development"],
    description: "拆分任务 → 并行执行",
  },
  {
    name: "分支管理流",
    chain: ["using-git-worktrees", "finishing-a-development-branch"],
    description: "隔离工作区 → 完成分支",
  },
];

// ── 数据结构 ──

interface SessionChainInfo {
  threadId: string;
  threadTitle: string;
  threadDate: string;
  /** 有序 skill 列表 */
  skills: string[];
  /** 每步之间的消息数（skills[i] 到 skills[i+1] 的间隔） */
  stepsBetween: number[];
  toolCallCount: number;
  editCount: number;
  subAgentCount: number;
  messageCount: number;
  /** 匹配到的已知工作流 */
  matchedWorkflows: { name: string; completionRate: number; completedSteps: number; totalSteps: number }[];
}

// ── 主分析 ──

export function analyzeSkillChains(loader: DataLoader): DefectReport[] {
  printSection("Skill 链深度分析");

  const threads = loader.loadVisibleThreads();
  const knownSkills = new Set(KNOWN_WORKFLOWS.flatMap((w) => w.chain).concat([
    "grill-me", "brainstorming", "issue-create", "writing-plans", "executing-plans",
    "ultra-batch", "subagent-driven-development", "systematic-debugging", "slop-cleaner",
    "claude-md-improver", "using-git-worktrees", "finishing-a-development-branch",
    "verification-before-completion", "requesting-code-review", "issue-archive",
    "skill-creator", "improve-codebase-architecture", "langfuse", "interview",
    "sdd-brainstorming", "sdd-writing-plans", "code-review", "frontend-design",
    "compact", "setup", "llm-log-analyzer", "dispatching-parallel-agents",
  ]));

  const chainSessions: SessionChainInfo[] = [];
  const singleSkillSessions: { skill: string; tools: number; edits: number; msgs: number }[] = [];
  const noSkillData: { tools: number; edits: number; msgs: number }[] = [];

  for (const thread of threads) {
    const messages = loader.loadMessages(thread.id);
    const subAgents = loader.loadSubAgents(thread.id);

    // 提取有序 skill 列表（按首次出现）
    const skillsWithIdx: { name: string; msgIdx: number }[] = [];
    let toolCallCount = 0;
    let editCount = 0;

    for (let msgIdx = 0; msgIdx < messages.length; msgIdx++) {
      const msg = messages[msgIdx];
      if (msg.role === "user") {
        const parsed = DataLoader.parseContent(msg.content);
        if (!parsed) continue;
        const text = extractText(parsed);
        const slashMatch = text.trim().match(/^\/(\w+[-\w]*)/);
        if (slashMatch && knownSkills.has(slashMatch[1])) {
          if (!skillsWithIdx.find((s) => s.name === slashMatch[1])) {
            skillsWithIdx.push({ name: slashMatch[1], msgIdx });
          }
        }
      }
      if (msg.role === "assistant") {
        const parsed = DataLoader.parseContent(msg.content);
        if (!parsed) continue;
        const toolCalls = DataLoader.extractToolCalls(parsed);
        toolCallCount += toolCalls.length;
        for (const tc of toolCalls) {
          if (["Edit", "Write"].includes(tc.name)) editCount++;
        }
      }
    }

    if (skillsWithIdx.length === 0) continue;

    // 计算步间消息数
    const stepsBetween: number[] = [];
    for (let i = 0; i < skillsWithIdx.length - 1; i++) {
      stepsBetween.push(skillsWithIdx[i + 1].msgIdx - skillsWithIdx[i].msgIdx);
    }

    const skills = skillsWithIdx.map((s) => s.name);

    // 匹配已知工作流（支持 __EXEC__ 通配符匹配任一执行类 Skill）
    // 严格顺序匹配：每步必须在上一步之后的紧邻位置按序出现，跳步则终止
    const matchedWorkflows = KNOWN_WORKFLOWS
      .map((wf) => {
        let completedSteps = 0;
        let lastIdx = -1;
        let broken = false;
        for (const step of wf.chain) {
          if (broken) break;
          if (step === "__EXEC__") {
            const execIdx = skills.findIndex((s, i) => i > lastIdx && EXECUTION_SKILLS.has(s));
            if (execIdx > lastIdx) {
              completedSteps++;
              lastIdx = execIdx;
            } else {
              broken = true;
            }
          } else {
            const idx = skills.indexOf(step, lastIdx + 1);
            if (idx > lastIdx) {
              completedSteps++;
              lastIdx = idx;
            } else {
              broken = true;
            }
          }
        }
        const completionRate = completedSteps / wf.chain.length;
        if (completedSteps >= 2 || (completedSteps >= 1 && wf.chain.length === 1)) {
          return { name: wf.name, completionRate, completedSteps, totalSteps: wf.chain.length };
        }
        return null;
      })
      .filter((m): m is NonNullable<typeof m> => m !== null && m.completionRate > 0);

    const info: SessionChainInfo = {
      threadId: thread.id,
      threadTitle: thread.title || "",
      threadDate: thread.created_at.slice(0, 10),
      skills,
      stepsBetween,
      toolCallCount,
      editCount,
      subAgentCount: subAgents.length,
      messageCount: messages.length,
      matchedWorkflows,
    };

    if (skills.length >= 2) {
      chainSessions.push(info);
    } else if (skills.length === 1) {
      singleSkillSessions.push({ skill: skills[0], tools: toolCallCount, edits: editCount, msgs: messages.length });
    } else {
      noSkillData.push({ tools: toolCallCount, edits: editCount, msgs: messages.length });
    }
  }

  // ── 输出报告 ──

  // 1. 工作流完成率
  printSection("已知工作流完成率");
  const avgOf = (arr: number[]) => arr.length > 0 ? arr.reduce((a, b) => a + b, 0) / arr.length : 0;

  const wfRows = KNOWN_WORKFLOWS.map((wf) => {
    // 找出所有至少触发了该工作流第一步的会话
    const triggered = chainSessions.filter((s) =>
      s.matchedWorkflows.find((m) => m.name === wf.name && m.completedSteps >= 1)
    );
    // 找出完成了整个工作流的会话
    const completed = chainSessions.filter((s) =>
      s.matchedWorkflows.find((m) => m.name === wf.name && m.completionRate >= 1)
    );
    // 找出完成了一半以上的
    const halfDone = chainSessions.filter((s) =>
      s.matchedWorkflows.find((m) => m.name === wf.name && m.completionRate >= 0.5 && m.completionRate < 1)
    );
    const abandoned = triggered.length - completed.length - halfDone.length;

    return [
      wf.name,
      wf.chain.join(" → ").slice(0, 40),
      String(triggered.length),
      String(completed.length),
      String(halfDone.length),
      String(abandoned),
      triggered.length > 0 ? (completed.length / triggered.length * 100).toFixed(0) + "%" : "-",
    ];
  }).filter((row) => row[2] !== "0");
  printTable(["工作流", "链", "触发", "完成", "半完成", "中断", "完成率"], wfRows);

  // 2. 完整链 vs 部分链 vs 单 Skill 效能对比
  printSection("链完成度 vs 产出效能");
  const avgOfGroup = (arr: { tools: number; edits: number; msgs: number }[]) => ({
    tools: avgOf(arr.map((x) => x.tools)),
    edits: avgOf(arr.map((x) => x.edits)),
    msgs: avgOf(arr.map((x) => x.msgs)),
  });

  // 找出有匹配工作流的会话
  const withFullChain = chainSessions.filter((s) => s.matchedWorkflows.some((m) => m.completionRate >= 1));
  const withPartialChain = chainSessions.filter((s) =>
    s.matchedWorkflows.some((m) => m.completionRate > 0 && m.completionRate < 1) &&
    !s.matchedWorkflows.some((m) => m.completionRate >= 1)
  );

  const fullChainStats = avgOfGroup(withFullChain.map((s) => ({ tools: s.toolCallCount, edits: s.editCount, msgs: s.messageCount })));
  const partialChainStats = avgOfGroup(withPartialChain.map((s) => ({ tools: s.toolCallCount, edits: s.editCount, msgs: s.messageCount })));
  const singleStats = avgOfGroup(singleSkillSessions);
  const noSkillSessions = noSkillData.length;
  const noSkillStats = avgOfGroup(noSkillData);

  const compareRows = [
    ["完整链 (≥1 工作流完成)", String(withFullChain.length), fullChainStats.tools.toFixed(0), fullChainStats.edits.toFixed(1), fullChainStats.msgs.toFixed(0), (fullChainStats.edits / (fullChainStats.tools || 1) * 100).toFixed(1) + "%"],
    ["部分链 (未完成)", String(withPartialChain.length), partialChainStats.tools.toFixed(0), partialChainStats.edits.toFixed(1), partialChainStats.msgs.toFixed(0), (partialChainStats.edits / (partialChainStats.tools || 1) * 100).toFixed(1) + "%"],
    ["单 Skill", String(singleSkillSessions.length), singleStats.tools.toFixed(0), singleStats.edits.toFixed(1), singleStats.msgs.toFixed(0), (singleStats.edits / (singleStats.tools || 1) * 100).toFixed(1) + "%"],
    ["无 Skill", String(noSkillSessions), noSkillStats.tools.toFixed(0), noSkillStats.edits.toFixed(1), noSkillStats.msgs.toFixed(0), (noSkillStats.edits / (noSkillStats.tools || 1) * 100).toFixed(1) + "%"],
  ];
  printTable(["类型", "会话数", "平均工具", "平均Edit", "平均消息", "Edit/工具"], compareRows);

  // 3. 最优工作流排行（Edit 产出效率）
  printSection("工作流效能排行（按 Edit/工具比）");
  const wfEfficiency = KNOWN_WORKFLOWS
    .map((wf) => {
      const completed = chainSessions.filter((s) =>
        s.matchedWorkflows.find((m) => m.name === wf.name && m.completionRate >= 1)
      );
      if (completed.length === 0) return null;
      const avgTools = avgOf(completed.map((s) => s.toolCallCount));
      const avgEdits = avgOf(completed.map((s) => s.editCount));
      const editPerTool = avgEdits / (avgTools || 1);
      return { name: wf.name, chain: wf.chain, count: completed.length, avgTools, avgEdits, editPerTool };
    })
    .filter((x): x is NonNullable<typeof x> => x !== null)
    .sort((a, b) => b.editPerTool - a.editPerTool);

  const effRows = wfEfficiency.map((wf) => [
    wf.name,
    String(wf.count),
    wf.avgTools.toFixed(0),
    wf.avgEdits.toFixed(1),
    (wf.editPerTool * 100).toFixed(1) + "%",
  ]);
  printTable(["工作流", "完成次数", "平均工具", "平均Edit", "Edit/工具"], effRows);

  // 4. 断裂点分析
  printSection("链断裂点分析");
  for (const wf of KNOWN_WORKFLOWS) {
    const triggered = chainSessions.filter((s) =>
      s.matchedWorkflows.find((m) => m.name === wf.name && m.completedSteps >= 1)
    );
    if (triggered.length < 3) continue;

    // 每一步的完成率
    const stepCompletions = wf.chain.map((step) => {
      const reached = triggered.filter((s) => s.skills.includes(step)).length;
      return { step, reached, rate: reached / triggered.length };
    });

    // 找断裂点（完成率下降最大的步骤）
    const breakPoints: string[] = [];
    for (let i = 1; i < stepCompletions.length; i++) {
      const drop = stepCompletions[i - 1].rate - stepCompletions[i].rate;
      if (drop > 0.3) {
        breakPoints.push(`${stepCompletions[i - 1].step} → ${stepCompletions[i].step} (${(drop * 100).toFixed(0)}% 流失)`);
      }
    }

    if (breakPoints.length > 0 || stepCompletions.some((s) => s.rate < 0.5)) {
      const stepRow = stepCompletions.map((s) => `${s.step}: ${s.reached}/${triggered.length} (${(s.rate * 100).toFixed(0)}%)`);
      console.log(`  ${wf.name}: ${stepRow.join(" → ")}`);
      if (breakPoints.length > 0) {
        console.log(`    断裂: ${breakPoints.join(", ")}`);
      }
    }
  }

  // 5. writing-plans 之后用户实际选择了什么
  printSection("writing-plans 后续选择分布");
  const afterPlan: Record<string, number> = { "(无后续 Skill)": 0 };
  for (const s of chainSessions) {
    const planIdx = s.skills.indexOf("writing-plans");
    if (planIdx < 0) continue;
    if (planIdx + 1 < s.skills.length) {
      const next = s.skills[planIdx + 1];
      afterPlan[next] = (afterPlan[next] || 0) + 1;
    } else {
      afterPlan["(无后续 Skill)"]++;
    }
  }
  // 也统计单 Skill 会话中的 writing-plans
  for (const s of singleSkillSessions) {
    if (s.skill === "writing-plans") {
      afterPlan["(无后续 Skill)"]++;
    }
  }
  const afterPlanTotal = Object.values(afterPlan).reduce((a, b) => a + b, 0);
  const afterPlanRows = Object.entries(afterPlan)
    .sort((a, b) => b[1] - a[1])
    .map(([name, count]) => [name, String(count), (count / afterPlanTotal * 100).toFixed(1) + "%"]);
  printTable(["writing-plans 后续", "次数", "占比"], afterPlanRows);

  // 6. 执行类 Skill 对比（executing-plans vs subagent-driven vs ultra-batch）
  printSection("执行类 Skill 效能对比");
  const execSkillStats: Record<string, { count: number; tools: number; edits: number; subagents: number }> = {};
  for (const sk of EXECUTION_SKILLS) {
    const sessions = [...chainSessions, ...singleSkillSessions.filter((s) => s.skill === sk).map((s) => ({
      threadId: "", threadTitle: "", threadDate: "", skills: [s.skill], stepsBetween: [] as number[],
      toolCallCount: s.tools, editCount: s.edits, subAgentCount: 0, messageCount: s.msgs,
      matchedWorkflows: [] as any[],
    }))]
      .filter((s) => s.skills.includes(sk));
    if (sessions.length > 0) {
      execSkillStats[sk] = {
        count: sessions.length,
        tools: sessions.reduce((a, s) => a + s.toolCallCount, 0) / sessions.length,
        edits: sessions.reduce((a, s) => a + s.editCount, 0) / sessions.length,
        subagents: sessions.reduce((a, s) => a + s.subAgentCount, 0) / sessions.length,
      };
    }
  }
  if (Object.keys(execSkillStats).length > 0) {
    const execRows = Object.entries(execSkillStats)
      .sort((a, b) => b[1].count - a[1].count)
      .map(([name, s]) => [
        "/" + name,
        String(s.count),
        s.tools.toFixed(0),
        s.edits.toFixed(1),
        s.subagents.toFixed(1),
        (s.edits / (s.tools || 1) * 100).toFixed(1) + "%",
      ]);
    printTable(["执行类 Skill", "会话数", "平均工具", "平均Edit", "平均SubAgent", "Edit/工具"], execRows);
  }

  // 7. Skill 步间间隔分析
  printSection("Skill 步间消息间隔");
  const allIntervals: { from: string; to: string; msgs: number }[] = [];
  for (const s of chainSessions) {
    for (let i = 0; i < s.skills.length - 1; i++) {
      allIntervals.push({
        from: s.skills[i],
        to: s.skills[i + 1],
        msgs: s.stepsBetween[i] || 0,
      });
    }
  }

  // 按转移对聚合
  const transitionStats: Record<string, { count: number; avgMsgs: number; totalMsgs: number }> = {};
  for (const interval of allIntervals) {
    const key = `${interval.from} → ${interval.to}`;
    if (!transitionStats[key]) transitionStats[key] = { count: 0, avgMsgs: 0, totalMsgs: 0 };
    transitionStats[key].count++;
    transitionStats[key].totalMsgs += interval.msgs;
  }
  for (const v of Object.values(transitionStats)) {
    v.avgMsgs = v.totalMsgs / v.count;
  }

  const topTransitions = Object.entries(transitionStats)
    .sort((a, b) => b[1].count - a[1].count)
    .slice(0, 15);
  const transRows = topTransitions.map(([key, stats]) => [
    key.slice(0, 40),
    String(stats.count),
    stats.avgMsgs.toFixed(0),
  ]);
  printTable(["转移", "次数", "平均间隔消息"], transRows);

  // 6. 推荐：最高效的 Skill 组合
  printSection("最高效的 Skill 组合（Edit/工具比 Top 10）");
  const comboStats: Record<string, { tools: number[]; edits: number[] }> = {};
  for (const s of chainSessions) {
    const key = s.skills.join(" → ");
    if (!comboStats[key]) comboStats[key] = { tools: [], edits: [] };
    comboStats[key].tools.push(s.toolCallCount);
    comboStats[key].edits.push(s.editCount);
  }

  const topCombos = Object.entries(comboStats)
    .filter(([, d]) => d.edits.length >= 2)
    .map(([key, data]) => ({
      combo: key,
      count: data.edits.length,
      avgTools: avgOf(data.tools),
      avgEdits: avgOf(data.edits),
      editPerTool: avgOf(data.edits) / (avgOf(data.tools) || 1),
    }))
    .sort((a, b) => b.editPerTool - a.editPerTool)
    .slice(0, 10);

  const comboRows = topCombos.map((c) => [
    c.combo.slice(0, 45),
    String(c.count),
    c.avgTools.toFixed(0),
    c.avgEdits.toFixed(1),
    (c.editPerTool * 100).toFixed(1) + "%",
  ]);
  printTable(["Skill 组合", "次数", "平均工具", "平均Edit", "Edit/工具"], comboRows);

  // ── 缺陷报告 ──
  const reports: DefectReport[] = [];

  // 高断裂率的工作流
  for (const wf of KNOWN_WORKFLOWS) {
    const triggered = chainSessions.filter((s) =>
      s.matchedWorkflows.find((m) => m.name === wf.name && m.completedSteps >= 1)
    );
    const completed = chainSessions.filter((s) =>
      s.matchedWorkflows.find((m) => m.name === wf.name && m.completionRate >= 1)
    );
    if (triggered.length >= 5 && completed.length / triggered.length < 0.3) {
      reports.push({
        id: `CHAIN-${reports.length + 1}`.padStart(3, "0").replace(" ", ""),
        severity: "medium",
        category: "工作流断裂",
        title: `"${wf.name}" 工作流完成率低 (${(completed.length / triggered.length * 100).toFixed(0)}%)`,
        description: `${triggered.length} 个会话触发了该工作流，但仅 ${completed.length} 个（${(completed.length / triggered.length * 100).toFixed(0)}%）走完了完整链。用户经常在中间某步放弃或跳到其他 Skill。`,
        evidence: [
          `触发: ${triggered.length}, 完成: ${completed.length}`,
          `链: ${wf.chain.join(" → ")}`,
        ],
        affectedSessions: triggered.map((s) => s.threadId),
        recommendation: `1. 检查断裂点——是 Skill 本身设计问题还是用户需求变了？2. 考虑在工作流中间步骤增加自动引导（如 systematic-debugging 完成后提示用户可以用 writing-plans）。3. 评估断裂点之后的步骤是否可以自动化。`,
        confidence: 0.5,
      });
    }
  }

  // 完整链效能显著优于部分链
  if (withFullChain.length >= 5 && withPartialChain.length >= 5) {
    const fullRatio = fullChainStats.edits / (fullChainStats.tools || 1);
    const partialRatio = partialChainStats.edits / (partialChainStats.tools || 1);
    if (fullRatio > partialRatio * 1.5) {
      reports.push({
        id: "CHAIN-EFF",
        severity: "low",
        category: "工作流效能",
        title: "完整 Skill 链的 Edit 产出效率显著高于部分链",
        description: `完整链平均 Edit/工具比 ${(fullRatio * 100).toFixed(1)}%，部分链仅 ${(partialRatio * 100).toFixed(1)}%。完成整个工作流的会话能更高效地产出代码修改。`,
        evidence: [
          `完整链: ${withFullChain.length}会话, ${(fullRatio * 100).toFixed(1)}% Edit/工具`,
          `部分链: ${withPartialChain.length}会话, ${(partialRatio * 100).toFixed(1)}% Edit/工具`,
        ],
        affectedSessions: [],
        recommendation: "在 Skill 完成后主动提示用户下一步 Skill（如 writing-plans 完成后提示 executing-plans），提高链完成率。可在 Skill 输出末尾加入推荐。",
        confidence: 0.45,
      });
    }
  }

  return reports;
}

function extractText(parsed: any): string {
  const content = parsed.content;
  return typeof content === "string" ? content :
    Array.isArray(content) ? content.filter((b: any) => b.type === "text").map((b: any) => b.text || "").join("") : "";
}
