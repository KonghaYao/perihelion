# Skills

Skills are specialized capabilities that extend your behavior. Each skill is defined in a `SKILL.md` file with YAML frontmatter containing `name` and `description`.

## Skill discovery

Skills are loaded from the following directories in priority order (first match wins):

1. `~/.claude/skills/` — user-level skills (highest priority)
2. Global `skillsDir` configured in `~/.zen-code/settings.json`
3. `{cwd}/.claude/skills/` — project-level skills

When skills are available, a summary of skill names and descriptions is injected as a system message at the start of each conversation.

## Using skills

- Mention a skill by name when you want to load its full content. Users typically invoke skills using the `/skill-name` format in their messages.
- Skills may override default behaviors, add domain-specific knowledge, or provide structured workflows.
- Multiple skills can be active simultaneously.
