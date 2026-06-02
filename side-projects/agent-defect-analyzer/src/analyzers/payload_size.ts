//! 超大工具入参/出参检测器。
//!
//! 检测异常大的工具输入参数和输出结果，识别以下问题：
//! 1. **超大入参**：Agent 向工具传入了异常大的参数（如把整个文件内容作为参数）
//! 2. **超大出参**：工具返回了异常大的结果（可能触发上下文膨胀）
//! 3. **入参/出参膨胀趋势**：随着会话进行，工具参数/结果越来越大
//! 4. **文件内容泄露**：Agent 把大段文件内容作为参数传递而非使用文件路径

import type { DefectReport, MessageRow } from "../../types.js";
import { DataLoader } from "../utils/data_loader.js";
import { printSection, printMetric, printTable, printWarning } from "../utils/report.js";

// ── 数据结构 ──

interface ToolPayloadRecord {
  threadId: string;
  callId: string;
  toolName: string;
  /** 入参 JSON 大小（字节） */
  inputSize: number;
  /** 入参预览 */
  inputPreview: string;
  /** 出参内容大小（字节） */
  outputSize: number;
  /** 是否为错误结果 */
  isError: boolean;
  /** 在会话中的消息序号 */
  messageIndex: number;
}

interface PayloadStats {
  toolName: string;
  count: number;
  avgInputSize: number;
  maxInputSize: number;
  avgOutputSize: number;
  maxOutputSize: number;
  /** 入参 >10KB 的次数 */
  oversizedInputCount: number;
  /** 出参 >50KB 的次数 */
  oversizedOutputCount: number;
  /** 入参包含文件内容的次数（启发式检测） */
  fileContentInInputCount: number;
}

// ── 阈值 ──

/** 入参超过此值视为"大" */
const LARGE_INPUT_THRESHOLD = 5_000; // 5KB
/** 入参超过此值视为"超大" */
const OVERSIZED_INPUT_THRESHOLD = 50_000; // 50KB
/** 出参超过此值视为"大" */
const LARGE_OUTPUT_THRESHOLD = 20_000; // 20KB
/** 出参超过此值视为"超大"（会严重膨胀上下文） */
const OVERSIZED_OUTPUT_THRESHOLD = 100_000; // 100KB

// ── 主分析 ──

export function analyzePayloadSize(loader: DataLoader): DefectReport[] {
  printSection("超大工具入参/出参检测");

  const threads = loader.loadVisibleThreads();
  const records: ToolPayloadRecord[] = [];

  // 建立全局入参/出参大小分布
  const inputSizeBuckets = [
    { label: "<1KB", max: 1000 },
    { label: "1-5KB", max: 5000 },
    { label: "5-20KB", max: 20000 },
    { label: "20-50KB", max: 50000 },
    { label: "50-100KB", max: 100000 },
    { label: ">100KB", max: Infinity },
  ];

  const outputSizeBuckets = [
    { label: "<1KB", max: 1000 },
    { label: "1-5KB", max: 5000 },
    { label: "5-20KB", max: 20000 },
    { label: "20-50KB", max: 50000 },
    { label: "50-100KB", max: 100000 },
    { label: ">100KB", max: Infinity },
  ];

  const inputDistribution = new Map<string, number>();
  const outputDistribution = new Map<string, number>();
  for (const b of [...inputSizeBuckets, ...outputSizeBuckets]) {
    inputDistribution.set(b.label, 0);
    outputDistribution.set(b.label, 0);
  }

  // Step 1: 提取所有工具调用的入参/出参大小
  for (const thread of threads) {
    const messages = loader.loadMessages(thread.id);

    // 建立 callId → outputSize 映射
    const callOutputs = new Map<string, { size: number; isError: boolean }>();
    for (const msg of messages) {
      if (msg.role === "tool") {
        const parsed = DataLoader.parseContent(msg.content);
        if (parsed && "tool_call_id" in parsed) {
          const tc = parsed as any;
          callOutputs.set(tc.tool_call_id, {
            size: msg.content.length,
            isError: tc.is_error || false,
          });
        }
      }
    }

    // 从 assistant 消息提取入参大小
    for (let i = 0; i < messages.length; i++) {
      const msg = messages[i];
      if (msg.role !== "assistant") continue;

      const parsed = DataLoader.parseContent(msg.content);
      if (!parsed || parsed.role !== "assistant") continue;

      const ai = parsed as any;
      const blocks = Array.isArray(ai.content) ? ai.content : [];

      for (const block of blocks) {
        if (block.type === "tool_use") {
          const inputJson = JSON.stringify(block.input || {});
          const output = callOutputs.get(block.id);

          const record: ToolPayloadRecord = {
            threadId: thread.id,
            callId: block.id,
            toolName: block.name,
            inputSize: Buffer.byteLength(inputJson, "utf8"),
            inputPreview: inputJson.slice(0, 80),
            outputSize: output?.size || 0,
            isError: output?.isError || false,
            messageIndex: i,
          };
          records.push(record);

          // 统计分布
          for (const b of inputSizeBuckets) {
            if (record.inputSize < b.max) {
              inputDistribution.set(b.label, (inputDistribution.get(b.label) || 0) + 1);
              break;
            }
          }
          for (const b of outputSizeBuckets) {
            if (record.outputSize < b.max) {
              outputDistribution.set(b.label, (outputDistribution.get(b.label) || 0) + 1);
              break;
            }
          }
        }
      }
    }
  }

  // Step 2: 入参大小分布
  printSection("工具入参大小分布");
  const inputRows = inputSizeBuckets.map((b) => {
    const count = inputDistribution.get(b.label) || 0;
    const total = records.length || 1;
    return [b.label, String(count), (count / total * 100).toFixed(1) + "%"];
  });
  printTable(["大小", "调用数", "占比"], inputRows);

  // Step 3: 出参大小分布
  printSection("工具出参大小分布");
  const outputRows = outputSizeBuckets.map((b) => {
    const count = outputDistribution.get(b.label) || 0;
    const total = records.length || 1;
    return [b.label, String(count), (count / total * 100).toFixed(1) + "%"];
  });
  printTable(["大小", "调用数", "占比"], outputRows);

  // Step 4: 按工具统计
  printSection("按工具的入参/出参统计");
  const toolStats = new Map<string, PayloadStats>();

  for (const rec of records) {
    if (!toolStats.has(rec.toolName)) {
      toolStats.set(rec.toolName, {
        toolName: rec.toolName,
        count: 0,
        avgInputSize: 0,
        maxInputSize: 0,
        avgOutputSize: 0,
        maxOutputSize: 0,
        oversizedInputCount: 0,
        oversizedOutputCount: 0,
        fileContentInInputCount: 0,
      });
    }
    const stats = toolStats.get(rec.toolName)!;
    stats.count++;
    stats.avgInputSize += rec.inputSize;
    stats.maxInputSize = Math.max(stats.maxInputSize, rec.inputSize);
    stats.avgOutputSize += rec.outputSize;
    stats.maxOutputSize = Math.max(stats.maxOutputSize, rec.outputSize);
    if (rec.inputSize > OVERSIZED_INPUT_THRESHOLD) stats.oversizedInputCount++;
    if (rec.outputSize > OVERSIZED_OUTPUT_THRESHOLD) stats.oversizedOutputCount++;
    // 启发式：入参中包含多行内容 = 可能是文件内容
    if (rec.inputSize > LARGE_INPUT_THRESHOLD && rec.inputPreview.includes("\\n")) {
      stats.fileContentInInputCount++;
    }
  }

  // 计算平均值
  for (const stats of toolStats.values()) {
    stats.avgInputSize = Math.round(stats.avgInputSize / stats.count);
    stats.avgOutputSize = Math.round(stats.avgOutputSize / stats.count);
  }

  const statsRows = [...toolStats.values()]
    .sort((a, b) => b.maxOutputSize - a.maxOutputSize)
    .slice(0, 15)
    .map((s) => [
      s.toolName,
      String(s.count),
      formatSize(s.avgInputSize),
      formatSize(s.maxInputSize),
      formatSize(s.avgOutputSize),
      formatSize(s.maxOutputSize),
      s.oversizedOutputCount > 0 ? String(s.oversizedOutputCount) : "-",
    ]);
  printTable(
    ["工具", "调用数", "平均入参", "最大入参", "平均出参", "最大出参", "超大出参"],
    statsRows
  );

  // Step 5: 超大出参 Top 榜
  printSection("超大出参 Top 15（>100KB）");

  const oversizedOutputs = records
    .filter((r) => r.outputSize > OVERSIZED_OUTPUT_THRESHOLD)
    .sort((a, b) => b.outputSize - a.outputSize)
    .slice(0, 15);

  if (oversizedOutputs.length > 0) {
    printMetric("超大出参总数", oversizedOutputs.length);
    const bigRows = oversizedOutputs.map((r) => [
      r.threadId.slice(0, 12) + "...",
      r.toolName,
      formatSize(r.outputSize),
      r.inputPreview.slice(0, 40),
    ]);
    printTable(["Session", "工具", "出参大小", "入参预览"], bigRows);
  } else {
    console.log("  ✅ 未发现超过 100KB 的工具出参");
  }

  // Step 6: 超大入参 Top 榜
  printSection("超大入参 Top 15（>50KB）");

  const oversizedInputs = records
    .filter((r) => r.inputSize > OVERSIZED_INPUT_THRESHOLD)
    .sort((a, b) => b.inputSize - a.inputSize)
    .slice(0, 15);

  if (oversizedInputs.length > 0) {
    printMetric("超大入参总数", oversizedInputs.length);
    const bigInputRows = oversizedInputs.map((r) => [
      r.threadId.slice(0, 12) + "...",
      r.toolName,
      formatSize(r.inputSize),
      detectPayloadType(r.inputPreview),
    ]);
    printTable(["Session", "工具", "入参大小", "内容类型"], bigInputRows);
  } else {
    console.log("  ✅ 未发现超过 50KB 的工具入参");
  }

  // Step 7: 上下文膨胀风险会话
  printSection("上下文膨胀风险会话");

  // 按会话聚合出参总量
  const sessionOutputTotal = new Map<string, { total: number; oversized: number; tools: string[] }>();
  for (const rec of records) {
    if (!sessionOutputTotal.has(rec.threadId)) {
      sessionOutputTotal.set(rec.threadId, { total: 0, oversized: 0, tools: [] });
    }
    const entry = sessionOutputTotal.get(rec.threadId)!;
    entry.total += rec.outputSize;
    if (rec.outputSize > LARGE_OUTPUT_THRESHOLD) {
      entry.oversized++;
      if (!entry.tools.includes(rec.toolName)) entry.tools.push(rec.toolName);
    }
  }

  const inflationRisk = [...sessionOutputTotal.entries()]
    .filter(([, v]) => v.total > 500_000) // 500KB 总出参
    .sort((a, b) => b[1].total - a[1].total)
    .slice(0, 10);

  if (inflationRisk.length > 0) {
    printMetric("高膨胀风险会话", inflationRisk.length);
    const riskRows = inflationRisk.map(([tid, v]) => [
      tid.slice(0, 12) + "...",
      formatSize(v.total),
      String(v.oversized),
      v.tools.slice(0, 3).join(", "),
    ]);
    printTable(["Session", "总出参", "大出参次数", "涉及工具"], riskRows);
  }

  // ── 缺陷报告 ──

  const reports: DefectReport[] = [];

  // 超大出参报告
  const totalOversizedOutput = records.filter((r) => r.outputSize > OVERSIZED_OUTPUT_THRESHOLD).length;
  const totalLargeOutput = records.filter((r) => r.outputSize > LARGE_OUTPUT_THRESHOLD).length;

  if (totalOversizedOutput > 0 || totalLargeOutput > 10) {
    reports.push({
      id: "SIZE-001",
      severity: totalOversizedOutput > 5 ? "high" : "medium",
      category: "上下文膨胀",
      title: "工具返回超大结果导致上下文膨胀",
      description: `${totalOversizedOutput} 次工具调用返回超过 100KB 的结果，${totalLargeOutput} 次超过 20KB。大出参会快速消耗上下文窗口，触发频繁 compact，降低 Agent 效率。`,
      evidence: oversizedOutputs.slice(0, 5).map((r) =>
        `${r.toolName}: ${formatSize(r.outputSize)} — ${r.inputPreview.slice(0, 40)}`
      ),
      affectedSessions: [...new Set(oversizedOutputs.map((r) => r.threadId))],
      recommendation: "对 Read/Grep/Bash 等工具增加输出截断逻辑。超过 20KB 的结果应截断并提示'结果过长，已截断至前 N 行'。Agent 应该用更精确的搜索条件缩小结果范围。",
      confidence: 0.85,
    });
  }

  // 超大入参报告
  const totalOversizedInput = records.filter((r) => r.inputSize > OVERSIZED_INPUT_THRESHOLD).length;
  if (totalOversizedInput > 0) {
    reports.push({
      id: "SIZE-002",
      severity: "medium",
      category: "参数膨胀",
      title: "工具入参异常大",
      description: `${totalOversizedInput} 次工具调用的入参超过 50KB。Agent 可能把整个文件内容作为参数传递，而非使用文件路径或更精确的参数。`,
      evidence: oversizedInputs.slice(0, 5).map((r) =>
        `${r.toolName}: ${formatSize(r.inputSize)} — ${detectPayloadType(r.inputPreview)}`
      ),
      affectedSessions: [...new Set(oversizedInputs.map((r) => r.threadId))],
      recommendation: "检查是否是 LLM 把文件内容直接序列化到 tool_use 的 input 字段中。如果是，应在系统提示中强调'使用文件路径而非文件内容作为参数'。对工具参数大小做运行时校验。",
      confidence: 0.7,
    });
  }

  return reports;
}

// ── Helpers ──

function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes}B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)}KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)}MB`;
}

/** 启发式判断入参内容类型 */
function detectPayloadType(preview: string): string {
  if (preview.includes("file_path") || preview.includes("path")) return "文件路径参数";
  if (preview.includes("content") && preview.length > 200) return "可能含文件内容";
  if (preview.includes("pattern") || preview.includes("query")) return "搜索/查询参数";
  if (preview.includes("command")) return "命令参数";
  return "其他";
}
