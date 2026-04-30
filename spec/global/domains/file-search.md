# 文件搜索 领域

## 领域综述

文件搜索领域负责代码库中的文件内容搜索功能，使用 ripgrep 底层 crate 实现进程内搜索，替代外部 rg 进程调用。

核心职责：
- grep + grep-regex crate 实现正则搜索
- 复用 ignore crate 的 WalkBuilder 做目录遍历
- WalkParallel + crossbeam channel 实现多线程并行
- tokio::spawn_blocking 避免阻塞 async runtime

## 核心流程

### 搜索流程

```
search_files_rg(pattern, path, glob, type, case_insensitive, whole_word, context, head_limit)
  → RegexMatcherBuilder 构建 matcher
  → WalkBuilder 配置目录遍历（自动尊重 .gitignore）
  → WalkParallel + num_cpus 线程并行搜索
  → SearchSink 收集结果（content/files_with_matches/count 三种模式）
  → 15 秒超时 + 500 行上限
  → 输出格式与原 rg 工具保持一致
```

## 技术方案总结

| 维度 | 选型 |
|------|------|
| 搜索库 | grep 0.4 + grep-regex（ripgrep 底层子 crate） |
| 目录遍历 | ignore crate WalkBuilder + WalkParallel |
| 并行模型 | crossbeam channel + num_cpus 线程 |
| 异步桥接 | tokio::task::spawn_blocking + 15s timeout |
| 接口兼容 | 工具名、参数 schema、description、输出格式保持不变 |

## Feature 附录

### feature_20260430_F003_replace-grep-with-ripgrep
**摘要:** 用 grep+grep-regex crate 替换外部 rg 进程调用实现进程内搜索
**关键决策:**
- 使用 grep + grep-regex crate（ripgrep 底层子 crate）替代 tokio::process::Command 调用 rg
- 复用已有的 ignore crate 的 WalkBuilder 做目录遍历
- 使用 WalkParallel + crossbeam channel 实现多线程并行搜索
- 通过 tokio::task::spawn_blocking 避免阻塞 async runtime，15 秒超时控制
- 工具名、参数 schema、description、输出格式保持不变，LLM 侧无感知
**归档:** [链接](../../archive/feature_20260430_F003_replace-grep-with-ripgrep/)
**归档日期:** 2026-04-30

---

## 相关 Feature
- → [agent.md](./agent.md) — FilesystemMiddleware 工具注册
