# Design Review Progress

## 2026-04-29 第1轮

修复4个UX问题：thread_browser和login_panel的'd'键删除功能缺失（帮助栏提示但未实现）、Welcome Card缺少全局快捷键提示、所有配置保存点从静默忽略改为检查错误并显示反馈。772个测试全部通过。

## 2026-04-29 第2轮

修复2个UX问题：cron面板'd'键删除未连接（同第1轮同类问题），thread_browser删除后增加反馈消息显示被删对话标题。772个测试全部通过。

## 2026-04-29 第3轮

全面排查并修复单字母快捷键违规：HITL弹窗移除y/n/t改为Space+Enter；Thread/Cron删除改Ctrl+D；Login编辑/新建/删除改Enter/Ctrl+N/Ctrl+D。同步更新所有面板提示文字和状态栏。新增3个headless测试验证合规性。241测试通过。
