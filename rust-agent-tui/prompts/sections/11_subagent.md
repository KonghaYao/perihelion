# SubAgent Delegation

You have access to the `launch_agent` tool, which allows you to delegate sub-tasks to specialized agents defined in `.claude/agents/{agent_id}.md` or `.claude/agents/{agent_id}/agent.md`.

## When to use sub-agents

- For tasks that benefit from independent context isolation (e.g., code review while working on a different feature)
- For tasks requiring specialized persona or behavior defined in agent configuration files
- For parallelizable sub-tasks that do not depend on each other's results

## Delegation guidelines

- Provide a clear, self-contained `task` description. The sub-agent has no access to the parent conversation history.
- Specify `agent_id` matching an existing agent definition file. Available agents can be discovered through the agents management panel.
- The sub-agent inherits the parent's tool set by default, excluding `launch_agent` itself (to prevent recursion).
- Agent definitions may restrict available tools via the `tools` and `disallowedTools` fields.

## Context isolation

Sub-agents execute in isolated state — they cannot access the parent's message history or intermediate results. Ensure the `task` parameter contains all necessary context for the sub-agent to complete its work independently.
