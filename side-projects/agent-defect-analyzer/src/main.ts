//! Agent 缺陷分析主入口。
//!
//! 用法:
//!   bun run src/main.ts                  # 全量分析
//!   bun run src/main.ts --focus errors   # 仅工具失败分析
//!   bun run src/main.ts --focus efficiency  # 仅会话效率
//!   bun run src/main.ts --focus strategy # 仅策略质量
//!   bun run src/main.ts --focus loops     # 仅死循环检测
//!   bun run src/main.ts --focus payload   # 仅超大入参/出参

import chalk from "chalk";
import { DataLoader } from "./utils/data_loader.js";
import { printHeader, printSection, printMetric, printFinding } from "./utils/report.js";
import { analyzeToolErrors } from "./analyzers/tool_errors.js";
import { analyzeSessionEfficiency } from "./analyzers/session_efficiency.js";
import { analyzeStrategyQuality } from "./analyzers/strategy_quality.js";
import { analyzeUserBehavior } from "./analyzers/user_behavior.js";
import { analyzeDeathLoops } from "./analyzers/death_loops.js";
import { analyzePayloadSize } from "./analyzers/payload_size.js";
import type { DefectReport } from "./types.js";

// ── CLI 参数解析 ──

const args = process.argv.slice(2);
const focusIdx = args.indexOf("--focus");
const focus = focusIdx >= 0 ? args[focusIdx + 1] : "all";

const VALID_FOCUSES = ["all", "errors", "efficiency", "strategy", "ux", "loops", "payload"];
if (!VALID_FOCUSES.includes(focus)) {
  console.error(chalk.red(`无效的 --focus 值: ${focus}`));
  console.error(chalk.gray(`可选: ${VALID_FOCUSES.join(", ")}`));
  process.exit(1);
}

// ── 主流程 ──

const loader = new DataLoader();

try {
  // 数据概览
  printHeader("Peri Agent 缺陷分析报告");
  const stats = loader.getStats();
  printMetric("总会话数", stats.totalThreads);
  printMetric("可见会话", stats.visibleThreads);
  printMetric("总消息数", stats.totalMessages.toLocaleString());
  printMetric("角色分布", Object.entries(stats.roleDistribution).map(([r, c]) => `${r}:${c}`).join(" / "));
  printMetric("工具错误总数", stats.totalToolErrors);

  const allReports: DefectReport[] = [];

  // 执行分析模块
  if (focus === "all" || focus === "errors") {
    console.time("  工具失败分析耗时");
    allReports.push(...analyzeToolErrors(loader));
    console.timeEnd("  工具失败分析耗时");
  }

  if (focus === "all" || focus === "efficiency") {
    console.time("  会话效率分析耗时");
    allReports.push(...analyzeSessionEfficiency(loader));
    console.timeEnd("  会话效率分析耗时");
  }

  if (focus === "all" || focus === "strategy") {
    console.time("  策略质量分析耗时");
    allReports.push(...analyzeStrategyQuality(loader));
    console.timeEnd("  策略质量分析耗时");
  }

  if (focus === "all" || focus === "ux") {
    console.time("  用户行为分析耗时");
    allReports.push(...analyzeUserBehavior(loader));
    console.timeEnd("  用户行为分析耗时");
  }

  if (focus === "all" || focus === "loops") {
    console.time("  死循环检测耗时");
    allReports.push(...analyzeDeathLoops(loader));
    console.timeEnd("  死循环检测耗时");
  }

  if (focus === "all" || focus === "payload") {
    console.time("  入参/出参检测耗时");
    allReports.push(...analyzePayloadSize(loader));
    console.timeEnd("  入参/出参检测耗时");
  }

  // 综合报告
  printHeader("综合缺陷报告");
  printMetric("发现缺陷总数", allReports.length);

  // 按严重性排序
  const severityOrder: Record<string, number> = { critical: 0, high: 1, medium: 2, low: 3 };
  const sortedReports = [...allReports].sort(
    (a, b) => severityOrder[a.severity] - severityOrder[b.severity]
  );

  for (const report of sortedReports) {
    printFinding(report.severity, `[${report.id}] ${report.title}`, report.description);
    console.log(chalk.gray(`          建议: ${report.recommendation}`));
    console.log(chalk.gray(`          置信度: ${(report.confidence * 100).toFixed(0)}% | 影响会话: ${report.affectedSessions.length}`));
    if (report.evidence.length > 0) {
      console.log(chalk.gray(`          证据: ${report.evidence.slice(0, 3).join(" | ")}`));
    }
    console.log();
  }

  // 优先修复建议
  printSection("优先修复建议");

  const highConfidence = sortedReports.filter((r) => r.confidence >= 0.7 && severityOrder[r.severity] <= 1);
  if (highConfidence.length > 0) {
    console.log(chalk.bold("  🔴 应立即修复 (高置信 + 高严重度):"));
    for (const report of highConfidence) {
      console.log(chalk.white(`     ${report.id}: ${report.title}`));
      console.log(chalk.gray(`       → ${report.recommendation}`));
    }
  }

  const mediumPriority = sortedReports.filter((r) => r.confidence >= 0.5 && severityOrder[r.severity] <= 2 && !highConfidence.includes(r));
  if (mediumPriority.length > 0) {
    console.log(chalk.bold("\n  🟡 建议近期优化 (中等优先级):"));
    for (const report of mediumPriority) {
      console.log(chalk.white(`     ${report.id}: ${report.title}`));
      console.log(chalk.gray(`       → ${report.recommendation}`));
    }
  }

  console.log(chalk.bold.cyan(`\n${"═".repeat(80)}`));
  console.log(chalk.bold.cyan("  分析完成。以上建议基于历史数据统计推断，请结合实际验证。"));
  console.log(chalk.bold.cyan(`${"═".repeat(80)}\n`));

} finally {
  loader.close();
}
