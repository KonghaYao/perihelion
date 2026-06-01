//! 控制台报告渲染工具。
//!
//! 提供 Markdown 风格的报告输出，支持颜色高亮和表格渲染。

import chalk from "chalk";
import Table from "cli-table3";

const SEPARATOR = "─".repeat(80);

export function printHeader(title: string) {
  console.log("\n" + chalk.bold.cyan(`═${SEPARATOR}═`));
  console.log(chalk.bold.cyan(`  ${title}`));
  console.log(chalk.bold.cyan(`═${SEPARATOR}═\n`));
}

export function printSection(title: string) {
  console.log(chalk.bold.yellow(`\n▸ ${title}`));
  console.log(chalk.yellow(`  ${"─".repeat(60)}`));
}

export function printMetric(label: string, value: string | number, unit?: string) {
  const val = typeof value === "number" ? value.toLocaleString() : value;
  console.log(`  ${chalk.gray("•")} ${chalk.white(label)}: ${chalk.bold.green(val)}${unit ? chalk.gray(unit) : ""}`);
}

export function printWarning(label: string, detail: string) {
  console.log(`  ${chalk.yellow("⚠")} ${chalk.yellow(label)}: ${detail}`);
}

export function printFinding(severity: "critical" | "high" | "medium" | "low", title: string, detail: string) {
  const icons: Record<string, string> = {
    critical: chalk.bgRed.white.bold(" CRIT "),
    high: chalk.red.bold(" HIGH "),
    medium: chalk.yellow.bold(" MED  "),
    low: chalk.blue.bold(" LOW  "),
  };
  console.log(`  ${icons[severity]} ${chalk.bold(title)}`);
  console.log(`          ${chalk.gray(detail)}`);
}

export function printTable(headers: string[], rows: string[][]) {
  const table = new Table({
    head: headers.map((h) => chalk.bold.cyan(h)),
    style: { head: [], border: ["gray"] },
    chars: {
      top: "─", "top-mid": "┬", "top-left": "┌", "top-right": "┐",
      bottom: "─", "bottom-mid": "┴", "bottom-left": "└", "bottom-right": "┘",
      left: "│", "left-mid": "├", mid: "─", "mid-mid": "┼",
      right: "│", "right-mid": "┤", middle: "│",
    },
  });
  table.push(...rows);
  console.log(table.toString());
}

export function printProgressBar(label: string, ratio: number, width: number = 40) {
  const filled = Math.round(ratio * width);
  const empty = width - filled;
  const color = ratio > 0.7 ? chalk.red : ratio > 0.4 ? chalk.yellow : chalk.green;
  console.log(`  ${label} ${color("█".repeat(filled))}${chalk.gray("░".repeat(empty))} ${(ratio * 100).toFixed(1)}%`);
}

export function printCodeBlock(code: string) {
  console.log(chalk.gray("  ┌─────────────────────────────────────────"));
  for (const line of code.split("\n")) {
    console.log(chalk.gray("  │ ") + chalk.white(line));
  }
  console.log(chalk.gray("  └─────────────────────────────────────────"));
}
