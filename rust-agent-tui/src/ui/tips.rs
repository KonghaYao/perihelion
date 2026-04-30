/// Random tips shown below the loading spinner, inspired by Claude Code.
const TIPS: &[&str] = &[
    "输入 / 前缀搜索可用命令和 Skills",
    "按 / 输入命令，如 /login 配置 Provider",
    "按 Ctrl+C 中断正在运行的 Agent",
    "按 Tab 在命令或 Skills 提示中补全",
    "使用 /model 切换 LLM 模型",
    "将文件拖入终端可自动添加为附件",
    "使用 /history 浏览历史对话记录",
    "按 /agents 管理 SubAgent 定义",
    "使用 /loop 创建定时任务",
    "按 Esc 关闭面板，按 Enter 确认选择",
    "按 ↑/↓ 浏览历史对话上下文",
    "使用 /clear 清空当前对话消息",
    "按 /help 查看所有可用命令",
    "使用 /compact 压缩上下文节省 token",
    "在 .claude/skills/ 中添加自定义 Skills",
    "在 .claude/agents/ 中添加自定义 SubAgent",
    "对复杂任务可以让 Claude 先制定计划",
    "拖拽图片到终端可自动附加到消息",
    "按 Alt+Enter 在输入框中换行",
    "长按 Ctrl+V 粘贴剪贴板图片",
    "让 Claude 使用子 Agent 并行工作",
];

/// Pick a tip based on a tick counter. Tip changes every ~180 ticks (roughly every 3 seconds at 60fps).
pub fn pick_tip(tick: u64) -> &'static str {
    TIPS[((tick / 180) as usize) % TIPS.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tips_no_hash_prefix_for_skills() {
        for tip in TIPS {
            assert!(!tip.contains("# 前缀"), "tips 不应包含 '# 前缀': {}", tip);
            assert!(!tip.contains("#skill"), "tips 不应包含 '#skill': {}", tip);
            assert!(!tip.contains("#Skill"), "tips 不应包含 '#Skill': {}", tip);
        }
    }

    #[test]
    fn test_tips_contains_slash_skills_hint() {
        let has_merged = TIPS.iter().any(|t| t.contains("命令和 Skills"));
        assert!(
            has_merged,
            "tips 应包含合并后的 '/ 前缀搜索命令和 Skills' 提示"
        );
    }

    #[test]
    fn test_tips_tab_hint_order() {
        let has_ordered = TIPS.iter().any(|t| t.contains("命令或 Skills 提示中补全"));
        assert!(has_ordered, "tips 应包含 '命令或 Skills 提示中补全'");
    }

    #[test]
    fn test_tips_only_reference_existing_commands() {
        // tips 中引用的 /xxx 命令必须存在于 command registry
        let existing_commands = [
            "login", "model", "history", "agents", "loop", "clear", "help", "compact", "cron",
        ];
        for tip in TIPS {
            // 提取 tip 中的 /xxx 命令引用
            for word in tip.split_whitespace() {
                if word.starts_with('/')
                    && word.len() > 1
                    && word.chars().nth(1).map_or(false, |c| c.is_alphabetic())
                {
                    let cmd_name: String = word[1..]
                        .chars()
                        .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '-')
                        .collect();
                    if !cmd_name.is_empty() {
                        assert!(
                            existing_commands.contains(&cmd_name.as_str()),
                            "tip 引用了不存在的命令 /{}: {}",
                            cmd_name,
                            tip
                        );
                    }
                }
            }
        }
    }
}
