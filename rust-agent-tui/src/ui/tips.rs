/// Random tips shown below the loading spinner.
const TIPS: &[&str] = &[
    "使用 # 前缀快速搜索可用 Skills",
    "按 / 输入命令，如 /login 配置 Provider",
    "按 Ctrl+C 中断正在运行的 Agent",
    "按 Tab 在 Skills 或命令提示中补全",
    "使用 /model 切换 LLM 模型",
    "将文件拖入终端可自动添加为附件",
    "使用 /history 浏览历史对话记录",
    "按 /agents 管理 SubAgent 定义",
    "使用 /loop 创建定时任务",
    "按 Esc 关闭面板，按 Enter 确认选择",
];

/// Pick a tip based on a tick counter. Tip changes every ~180 ticks (roughly every 3 seconds at 60fps).
pub fn pick_tip(tick: u64) -> &'static str {
    TIPS[((tick / 180) as usize) % TIPS.len()]
}
