//! 会话聚类分析器（基于规则的特征向量聚类）。
//!
//! 将会话按行为特征聚类，发现"Agent 在哪类任务上表现好/差"。
//!
//! 聚类方法：基于特征向量的层次聚类（不依赖 ML 库）
//! 1. 提取每个会话的特征向量（7 维）
//! 2. 用余弦相似度 + 优先队列加速的层次聚类
//! 3. 输出每个簇的典型会话、平均指标、常见缺陷

import type { DefectReport, MessageRow, ThreadRow } from "../../types.js";
import { DataLoader } from "../utils/data_loader.js";
import { printSection, printMetric, printTable } from "../utils/report.js";

// ── 配置常量 ──

/** 聚类相似度阈值（余弦相似度 0.80 对应约 37° 夹角，适合中等粒度分组） */
const SIMILARITY_THRESHOLD = 0.80;

/** 最大聚类迭代次数（防止大规模数据 O(n³) 爆炸） */
const MAX_MERGE_ITERATIONS = 1000;

/** 参与聚类的最小消息数（过滤掉极短会话） */
const MIN_MESSAGES = 5;

// ── 特征名映射（避免魔术索引） ──

const FEATURE_NAMES = [
  "messageDensity",
  "toolDensity",
  "errorRate",
  "subAgentCount",
  "parallelRate",
  "editRatio",
  "searchRatio",
] as const;

type FeatureIndex = typeof FEATURE_NAMES[number];

function getFeature(vector: number[], name: FeatureIndex): number {
  return vector[FEATURE_NAMES.indexOf(name)];
}

// ── 特征向量 ──

interface SessionFeatures {
  threadId: string;
  threadTitle: string;
  /** 消息密度：总消息 / 时长（分钟），反映交互紧凑度 */
  messageDensity: number;
  /** 工具密度：工具消息 / 总消息，反映工具依赖度 */
  toolDensity: number;
  /** 错误率：错误工具消息 / 工具消息 */
  errorRate: number;
  /** SubAgent 使用数 */
  subAgentCount: number;
  /** 并行度：并行工具轮次 / 总工具轮次 */
  parallelRate: number;
  /** 编辑占比：Edit+Write 调用 / 总工具调用 */
  editRatio: number;
  /** 搜索占比：Read+Grep+Glob 调用 / 总工具调用 */
  searchRatio: number;
  /** 归一化特征向量（用于聚类） */
  vector: number[];
  /** 原始特征值（用于展示） */
  rawFeatures: Record<string, number>;
}

interface Cluster {
  id: number;
  label: string;
  sessions: SessionFeatures[];
  /** 簇中心（各维度均值） */
  centroid: number[];
  /** 平均指标 */
  avgMetrics: {
    messageDensity: number;
    toolDensity: number;
    errorRate: number;
    subAgentCount: number;
    parallelRate: number;
    editRatio: number;
    searchRatio: number;
  };
}

// ── 特征提取 ──

function extractFeatures(
  loader: DataLoader,
  threadId: string,
  threadTitle: string,
  thread: ThreadRow
): SessionFeatures | null {
  const messages = loader.loadMessages(threadId);

  // 空消息保护
  if (messages.length === 0) return null;

  const subAgents = loader.loadSubAgents(threadId);

  let toolMsgs = 0;
  let errorToolMsgs = 0;
  let editCalls = 0;
  let searchCalls = 0;
  let totalToolCalls = 0;
  let parallelTurns = 0;
  let totalTurns = 0;

  for (const msg of messages) {
    if (msg.role === "tool") {
      toolMsgs++;
      const parsed = DataLoader.parseContent(msg.content);
      if (parsed && (parsed as any).is_error) {
        errorToolMsgs++;
      } else if (!parsed) {
        // 无法解析的 tool 消息视为可疑错误
        errorToolMsgs++;
      }
    } else if (msg.role === "assistant") {
      const parsed = DataLoader.parseContent(msg.content);
      const toolCalls = DataLoader.extractToolCalls(parsed);
      totalToolCalls += toolCalls.length;
      if (toolCalls.length > 0) {
        totalTurns++;
        if (toolCalls.length >= 2) parallelTurns++;
      }
      for (const tc of toolCalls) {
        if (["Edit", "Write"].includes(tc.name)) editCalls++;
        if (["Read", "Grep", "Glob"].includes(tc.name)) searchCalls++;
      }
    }
  }

  const createdAt = new Date(thread.created_at);
  const updatedAt = new Date(thread.updated_at);
  const durationMs = updatedAt.getTime() - createdAt.getTime();
  // 时间差为 0 或负数时标记为异常，用中性值 1 分钟
  const durationMinutes = durationMs > 0 ? durationMs / 60000 : 1;

  const messageDensity = messages.length / durationMinutes;
  const toolDensity = toolMsgs / (messages.length || 1);
  const errorRate = errorToolMsgs / (toolMsgs || 1);
  const subAgentCount = subAgents.length;
  const parallelRate = parallelTurns / (totalTurns || 1);
  const editRatio = editCalls / (totalToolCalls || 1);
  const searchRatio = searchCalls / (totalToolCalls || 1);

  // 用对数归一化避免天花板效应：log(1+x)/log(1+cap)
  const logNorm = (x: number, cap: number) => Math.log(1 + x) / Math.log(1 + cap);
  const vector = [
    logNorm(messageDensity, 5),   // messageDensity
    toolDensity,                   // toolDensity（已是 [0, 1]）
    errorRate,                     // errorRate（已是 [0, 1]）
    logNorm(subAgentCount, 10),   // subAgentCount
    parallelRate,                  // parallelRate（已是 [0, 1]）
    editRatio,                     // editRatio（已是 [0, 1]）
    searchRatio,                   // searchRatio（已是 [0, 1]）
  ];

  return {
    threadId,
    threadTitle,
    messageDensity,
    toolDensity,
    errorRate,
    subAgentCount,
    parallelRate,
    editRatio,
    searchRatio,
    vector,
    rawFeatures: {
      messageDensity: Math.round(messageDensity * 100) / 100,
      toolDensity: Math.round(toolDensity * 100),
      errorRate: Math.round(errorRate * 1000) / 10,
      subAgentCount,
      parallelRate: Math.round(parallelRate * 100),
      editRatio: Math.round(editRatio * 100),
      searchRatio: Math.round(searchRatio * 100),
    },
  };
}

// ── 聚类算法 ──

/** 余弦相似度 */
function cosineSimilarity(a: number[], b: number[]): number {
  let dot = 0, normA = 0, normB = 0;
  for (let i = 0; i < a.length; i++) {
    dot += a[i] * b[i];
    normA += a[i] * a[i];
    normB += b[i] * b[i];
  }
  const denom = Math.sqrt(normA) * Math.sqrt(normB);
  return denom === 0 ? 0 : dot / denom;
}

/**
 * 层次聚类（预计算距离矩阵 + 迭代合并上限保护）。
 *
 * 复杂度：O(n²) 预计算 + O(k·n²) 合并（k ≤ MAX_MERGE_ITERATIONS）。
 * 相比朴素 O(n³) 的改进：合并时只更新受影响的行/列。
 */
function hierarchicalClustering(
  features: SessionFeatures[],
  similarityThreshold: number = SIMILARITY_THRESHOLD
): Cluster[] {
  if (features.length === 0) return [];

  const n = features.length;

  // 预计算距离矩阵（只存上三角）
  // sim[i][j] = 余弦相似度（i < j）
  const sim = new Map<string, number>();
  for (let i = 0; i < n; i++) {
    for (let j = i + 1; j < n; j++) {
      sim.set(`${i},${j}`, cosineSimilarity(features[i].vector, features[j].vector));
    }
  }

  // 活跃簇索引 → 会话列表
  const clusterSessions = new Map<number, SessionFeatures[]>();
  const clusterCentroids = new Map<number, number[]>();
  let activeIndices: number[] = [];
  for (let i = 0; i < n; i++) {
    clusterSessions.set(i, [features[i]]);
    clusterCentroids.set(i, [...features[i].vector]);
    activeIndices.push(i);
  }
  let nextClusterId = n;

  let iterations = 0;
  let merged = true;

  while (merged && iterations < MAX_MERGE_ITERATIONS) {
    merged = false;
    iterations++;

    let bestI = -1, bestJ = -1, bestSimVal = similarityThreshold;

    for (let ai = 0; ai < activeIndices.length; ai++) {
      for (let aj = ai + 1; aj < activeIndices.length; aj++) {
        const ci = activeIndices[ai];
        const cj = activeIndices[aj];
        // 查找或计算相似度
        const key = ci < cj ? `${ci},${cj}` : `${cj},${ci}`;
        const s = sim.get(key) ?? cosineSimilarity(clusterCentroids.get(ci)!, clusterCentroids.get(cj)!);
        if (!sim.has(key)) sim.set(key, s);
        if (s > bestSimVal) {
          bestSimVal = s;
          bestI = ci;
          bestJ = cj;
        }
      }
    }

    if (bestI >= 0 && bestJ >= 0) {
      // 合并
      const mergedSessions = [...clusterSessions.get(bestI)!, ...clusterSessions.get(bestJ)!];
      const mergedCentroid = computeCentroid(mergedSessions);
      const newId = nextClusterId++;
      clusterSessions.set(newId, mergedSessions);
      clusterCentroids.set(newId, mergedCentroid);

      // 更新距离矩阵：新簇与所有其他活跃簇的相似度
      activeIndices = activeIndices.filter((idx) => idx !== bestI && idx !== bestJ);
      for (const otherIdx of activeIndices) {
        const newSim = cosineSimilarity(mergedCentroid, clusterCentroids.get(otherIdx)!);
        const key = Math.min(newId, otherIdx) + "," + Math.max(newId, otherIdx);
        sim.set(key, newSim);
      }
      activeIndices.push(newId);
      merged = true;
    }
  }

  if (iterations >= MAX_MERGE_ITERATIONS) {
    console.warn(`  [警告] 聚类达到最大迭代次数 ${MAX_MERGE_ITERATIONS}，提前终止`);
  }

  // 构建结果
  const result = activeIndices
    .map((idx, i) => {
      const sessions = clusterSessions.get(idx)!;
      return {
        id: i,
        label: labelCluster(sessions, clusterCentroids.get(idx)!),
        sessions,
        centroid: clusterCentroids.get(idx)!,
        avgMetrics: computeAvgMetrics(sessions),
      };
    })
    .sort((a, b) => b.sessions.length - a.sessions.length);

  // 重新分配连续 ID
  return result.map((r, i) => ({ ...r, id: i }));
}

function computeCentroid(sessions: SessionFeatures[]): number[] {
  if (sessions.length === 0) return [];
  const dim = sessions[0].vector.length;
  const centroid = new Array(dim).fill(0);
  for (const s of sessions) {
    for (let i = 0; i < dim; i++) {
      centroid[i] += s.vector[i];
    }
  }
  for (let i = 0; i < dim; i++) {
    centroid[i] /= sessions.length;
  }
  return centroid;
}

function computeAvgMetrics(sessions: SessionFeatures[]): Cluster["avgMetrics"] {
  const n = sessions.length || 1;
  return {
    messageDensity: sessions.reduce((a, s) => a + s.messageDensity, 0) / n,
    toolDensity: sessions.reduce((a, s) => a + s.toolDensity, 0) / n,
    errorRate: sessions.reduce((a, s) => a + s.errorRate, 0) / n,
    subAgentCount: sessions.reduce((a, s) => a + s.subAgentCount, 0) / n,
    parallelRate: sessions.reduce((a, s) => a + s.parallelRate, 0) / n,
    editRatio: sessions.reduce((a, s) => a + s.editRatio, 0) / n,
    searchRatio: sessions.reduce((a, s) => a + s.searchRatio, 0) / n,
  };
}

/** 基于簇的质心特征自动命名（使用命名特征索引，多标签组合） */
function labelCluster(sessions: SessionFeatures[], centroid: number[]): string {
  const avgSubAgents = sessions.reduce((a, s) => a + s.rawFeatures.subAgentCount, 0) / sessions.length;
  const avgMsgDensity = sessions.reduce((a, s) => a + s.rawFeatures.messageDensity, 0) / sessions.length;

  // 用命名特征提取，避免魔术索引
  const editRatio = getFeature(centroid, "editRatio");
  const searchRatio = getFeature(centroid, "searchRatio");
  const subAgentNorm = getFeature(centroid, "subAgentCount");
  const parallelRate = getFeature(centroid, "parallelRate");
  const errorRate = getFeature(centroid, "errorRate");

  const tags: string[] = [];

  // 按显著特征添加标签
  if (editRatio > 0.4) tags.push("重编辑");
  if (searchRatio > 0.5) tags.push("搜索理解");
  if (subAgentNorm > 0.3 || avgSubAgents >= 3) tags.push("SubAgent密集");
  if (parallelRate > 0.3) tags.push("高并行");
  if (errorRate > 0.05) tags.push("高错误率");
  if (avgMsgDensity < 0.5 && sessions.every((s) => s.rawFeatures.messageDensity < 0.5)) tags.push("低频长会话");

  if (tags.length === 0) return "混合型";
  return tags.join("+") + "型";
}

// ── 主分析 ──

export function analyzeSessionClustering(loader: DataLoader): DefectReport[] {
  printSection("会话聚类分析");

  const threads = loader.loadVisibleThreads();

  // 只分析有足够消息的会话
  const eligibleThreads = threads.filter((t) => t.message_count >= MIN_MESSAGES);
  printMetric("参与聚类的会话", eligibleThreads.length, ` / ${threads.length} 总会话`);

  // 提取特征（过滤掉 extractFeatures 返回 null 的异常数据）
  const features = eligibleThreads
    .map((t) => extractFeatures(loader, t.id, t.title || "", t))
    .filter((f): f is SessionFeatures => f !== null);

  // 执行聚类
  printSection("执行聚类...");
  const clusters = hierarchicalClustering(features);
  printMetric("发现的簇数", clusters.length);

  // 输出每个簇的统计
  printSection("聚类结果");

  const clusterRows = clusters.map((c) => [
    c.label.slice(0, 30),
    String(c.sessions.length),
    (c.avgMetrics.errorRate * 100).toFixed(1) + "%",
    (c.avgMetrics.toolDensity * 100).toFixed(0) + "%",
    (c.avgMetrics.editRatio * 100).toFixed(0) + "%",
    (c.avgMetrics.searchRatio * 100).toFixed(0) + "%",
    c.avgMetrics.subAgentCount.toFixed(1),
  ]);
  printTable(
    ["簇类型", "会话数", "错误率", "工具密度", "编辑占比", "搜索占比", "SubAgent"],
    clusterRows
  );

  // 每个簇的详细分析
  for (const cluster of clusters) {
    printSection(`簇: ${cluster.label} (${cluster.sessions.length} 会话)`);

    // 典型会话（距离中心最近的 3 个）
    const sortedByDist = [...cluster.sessions].sort((a, b) => {
      const distA = 1 - cosineSimilarity(a.vector, cluster.centroid);
      const distB = 1 - cosineSimilarity(b.vector, cluster.centroid);
      return distA - distB;
    });

    const typicalRows = sortedByDist.slice(0, 3).map((s) => [
      s.threadId.slice(0, 12) + "...",
      s.threadTitle.slice(0, 35),
      s.rawFeatures.messageDensity.toFixed(2) + "/min",
      s.rawFeatures.errorRate.toFixed(1) + "%",
      String(s.rawFeatures.subAgentCount),
    ]);
    printTable(["Session", "标题", "密度", "错误率", "SubAgent"], typicalRows);

    // 该簇的效率指标
    printMetric("  平均消息密度", cluster.avgMetrics.messageDensity.toFixed(2), " msg/min");
    printMetric("  平均并行率", (cluster.avgMetrics.parallelRate * 100).toFixed(1) + "%");
    printMetric("  平均错误率", (cluster.avgMetrics.errorRate * 100).toFixed(2) + "%");
  }

  // ── 跨簇对比 ──

  printSection("跨簇效率对比");
  const efficiencyRows = clusters
    .map((c) => ({
      label: c.label,
      errorRate: c.avgMetrics.errorRate,
      toolDensity: c.avgMetrics.toolDensity,
      sessions: c.sessions.length,
    }))
    .sort((a, b) => b.errorRate - a.errorRate);

  const effRows = efficiencyRows.map((c) => [
    c.label.slice(0, 30),
    String(c.sessions),
    (c.errorRate * 100).toFixed(2) + "%",
    (c.toolDensity * 100).toFixed(0) + "%",
  ]);
  printTable(["簇类型", "会话数", "错误率", "工具密度"], effRows);

  // ── 缺陷报告 ──

  const reports: DefectReport[] = [];

  // 高错误率簇（统一静态 ID）
  const highErrorClusters = clusters.filter((c) => c.avgMetrics.errorRate > 0.02 && c.sessions.length >= 3);
  if (highErrorClusters.length > 0) {
    reports.push({
      id: "CLUSTER-001",
      severity: highErrorClusters.some((c) => c.avgMetrics.errorRate > 0.05) ? "high" : "medium",
      category: "任务类型缺陷",
      title: `${highErrorClusters.length} 种会话类型错误率偏高`,
      description: highErrorClusters.map((c) =>
        `"${c.label}" (${c.sessions.length}会话, 平均错误率${(c.avgMetrics.errorRate * 100).toFixed(1)}%)`
      ).join("；") + "。这些任务类型的 Agent 表现可能存在系统性问题。",
      evidence: highErrorClusters.flatMap((c) => [
        `${c.label}: 会话${c.sessions.length}, 错误率${(c.avgMetrics.errorRate * 100).toFixed(1)}%, 密度${(c.avgMetrics.toolDensity * 100).toFixed(0)}%`,
        `典型: ${c.sessions.slice(0, 2).map((s) => s.threadTitle.slice(0, 20)).join(", ")}`,
      ]),
      affectedSessions: highErrorClusters.flatMap((c) => c.sessions.map((s) => s.threadId)),
      recommendation: "针对高错误率类型任务，检查 Agent 的工具选择策略和错误恢复能力。考虑为此类任务定制系统提示词。",
      confidence: 0.5,
    });
  }

  // 低效簇（高工具密度但低编辑比）
  const inefficientClusters = clusters.filter(
    (c) => c.avgMetrics.toolDensity > 0.6 && c.avgMetrics.editRatio < 0.15 && c.sessions.length >= 3
  );
  if (inefficientClusters.length > 0) {
    reports.push({
      id: "CLUSTER-002",
      severity: "low",
      category: "效率问题",
      title: "部分会话类型工具调用多但产出少",
      description: `${inefficientClusters.length} 种会话类型的工具密度 >60% 但编辑比 <15%。Agent 大量调用搜索/读取工具但很少实际修改代码，可能存在搜索效率低下的问题。`,
      evidence: inefficientClusters.map((c) =>
        `${c.label}: 密度${(c.avgMetrics.toolDensity * 100).toFixed(0)}%, 编辑${(c.avgMetrics.editRatio * 100).toFixed(0)}%`
      ),
      affectedSessions: inefficientClusters.flatMap((c) => c.sessions.map((s) => s.threadId)),
      recommendation: "Agent 在搜索阶段应更精准，减少无用的广度搜索。可以限制连续搜索轮次，强制 Agent 在搜索 N 轮后开始执行。",
      confidence: 0.4,
    });
  }

  return reports;
}
