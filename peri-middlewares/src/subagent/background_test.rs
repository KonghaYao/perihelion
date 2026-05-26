    fn make_registry() -> (
        BackgroundTaskRegistry,
        tokio::sync::mpsc::UnboundedReceiver<BackgroundTaskResult>,
    ) {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        (BackgroundTaskRegistry::new(tx), rx)
    }

    fn make_task(id: &str) -> BackgroundTask {
        BackgroundTask {
            id: id.to_string(),
            agent_name: "test-agent".to_string(),
            prompt_summary: "test task".to_string(),
            status: BackgroundTaskStatus::Running,
            started_at: std::time::Instant::now(),
            abort_handle: tokio::runtime::Handle::current().spawn(async {}),
        }
    }

    #[tokio::test]
    async fn test_register_and_active_count() {
        let (registry, _rx) = make_registry();
        assert_eq!(registry.active_count(), 0);

        registry.register(make_task("bg-1")).unwrap();
        assert_eq!(registry.active_count(), 1);
    }

    #[tokio::test]
    async fn test_max_concurrent_limit() {
        let (registry, _rx) = make_registry();

        registry.register(make_task("bg-1")).unwrap();
        registry.register(make_task("bg-2")).unwrap();
        registry.register(make_task("bg-3")).unwrap();

        let result = registry.register(make_task("bg-4"));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Maximum 3"));
    }

    #[tokio::test]
    async fn test_complete_sends_notification() {
        let (registry, mut rx) = make_registry();

        registry.register(make_task("bg-1")).unwrap();
        assert_eq!(registry.active_count(), 1);

        let result = BackgroundTaskResult {
            task_id: "bg-1".to_string(),
            agent_name: "test-agent".to_string(),
            prompt_summary: "test".to_string(),
            success: true,
            output: "done".to_string(),
            tool_calls_count: 2,
            duration_ms: 100,
            child_thread_id: None,
        };

        registry.complete("bg-1", result);

        // 已完成任务应被立即清理，list_tasks 不再返回
        let tasks = registry.list_tasks();
        assert_eq!(tasks.len(), 0, "completed tasks should be cleaned up immediately");
        assert_eq!(registry.active_count(), 0);

        // 通知应已发送
        let received = rx.try_recv().unwrap();
        assert_eq!(received.task_id, "bg-1");
        assert!(received.success);
    }

    #[tokio::test]
    async fn test_cancel_removes_task() {
        let (registry, _rx) = make_registry();

        registry.register(make_task("bg-1")).unwrap();
        registry.register(make_task("bg-2")).unwrap();

        registry.cancel("bg-1").unwrap();
        let tasks = registry.list_tasks();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].0, "bg-2");

        // 取消不存在的任务返回 Err
        let result = registry.cancel("nonexistent");
        assert!(result.is_err());
    }
