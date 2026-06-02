# Peri

Peri is a terminal coding agent, built the **Nobody Coding** way — powered by **DeepSeek-V4-Pro** and **GLM-5.1**. Peri is compatible with Claude Code — your `.claude/` config just works. It grew out of [OpenLangGraphServer](https://github.com/konghayao/open-langgraph-server) and [Zen Code](https://github.com/konghayao/zen-code). Built in Rust, runs on my little RISC-V dev board.

> Recent commits are almost entirely DeepSeek and GLM. Claude was just there in the beginning.

## Why Peri

- **Rust, not Node.js or Bun.** Fast startup, ~50MB memory footprint, no runtime overhead. Won't sneak up to 1GB while you're not looking.
- **Context optimized.** System prompt frozen at session start, dynamic content isolated behind a boundary marker, tool definitions stable across rounds.
  - 95-99% prompt cache hit rate — minimal token waste.
  - No agent memory / auto-dream / extra calls to waste your tokens.
  - Only core tools in every request. Other tools lazy-loaded on demand using Tool Search.
- **Any LLM, not just one.** Anthropic, OpenAI-compatible APIs — DeepSeek, GLM, whatever works for you.
- **Drop-in compatible.** Your `.claude/` config just works. Zero migration.
  - Agents, skills, hooks, and MCP servers.
  - Plugins from the Claude Code ecosystem.
  - Sub-agents with the same `.claude/agents/` definitions.
  - Auto compact — long sessions stay fast and cheap.
- **Streaming Markdown.** Full Markdown rendering as the agent types — code blocks, tables, diffs, all live.
- **IDE-ready via [ACP](https://github.com/Azure/agent-client-protocol).** Connects to [Zed](https://zed.dev) and other ACP clients out of the box. We're also building a "Cloud Code" platform where any ACP-compatible agent can plug in and go.
- **Unchecked but ready.**
  - Built-in LSP.
  - Background agents: fork work to sub-agents that run while you keep going.

## Install

Binaries available for macOS (x86_64 / Apple Silicon), Linux (x86_64 / aarch64 / riscv64), and Windows (x86_64).

```bash
# macOS / Linux
curl -fsSL https://raw.githubusercontent.com/konghayao/peri/main/scripts/install.sh | bash

# Windows (PowerShell)
irm https://raw.githubusercontent.com/konghayao/peri/main/scripts/install.ps1 | iex
```

## How We Built Peri with Nobody Coding

**Nobody Coding** means exactly what it sounds like. No human wrote a single line of Peri — not the architecture, not the TUI, not the harness tuning that makes open-source models reliable in a Agent loop. Humans decide *what*. AI figures out *how*. You're not pair programming — you're product managing an engineer that never sleeps. 99% of Peri was built this way.

A typical pipeline:

| When you... | Pipeline kicks off |
|---|---|
| **Find a bug or piece of tech debt** | `issue-create` → `systematic-debugging` → `writing-plans` → `subagent-driven-development` → `issue-archive` → improve CLAUDE.md |
| **Want to build a new feature** | `grill-me` → `writing-plans` → `subagent-driven-development` |
| **Notice the codebase getting messy** | `slop-cleaner` → `improve-codebase-architecture` → `writing-plans` → `subagent-driven-development` |
| **Need someone to grok the architecture** | `teacher` → assign a task → `teacher` |

## Acknowledgments

- [Claude Code Best](https://github.com/claude-code-best/claude-code) — invaluable community support and feedback
- [Superpowers](https://github.com/obra/superpowers) & [Matt Pocock's Skills](https://github.com/mattpocock/skills) — the skill suites that drive Peri's AI engineering workflow
- [ACP](hhttps://agentclientprotocol.com/) — open protocol for agent-IDE communication
- [rmcp](https://github.com/anthropics/rmcp) — Rust MCP client library
- [Ratatui](https://ratatui.rs) & [Tokio](https://tokio.rs)
- [Langfuse](https://langfuse.com) — LLM observability
- [Zed](https://zed.dev) — first ACP-compatible IDE, proved the protocol works

## License

Apache 2.0
