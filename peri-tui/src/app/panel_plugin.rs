use super::*;

impl App {
    /// 打开 MCP 面板
    pub fn open_mcp_panel(&mut self) {
        let infos = self
            .services
            .mcp_pool
            .as_ref()
            .map(|p| p.all_server_infos())
            .unwrap_or_default();
        if infos.is_empty() {
            let vm = crate::ui::message_view::MessageViewModel::system(
                self.services.lc.tr("app-no-mcp-configured"),
            );
            self.session_mgr
                .current_mut()
                .messages
                .view_messages
                .push(vm);
            self.render_rebuild();
            return;
        }
        let panel = McpPanel::new(infos);
        self.open_panel(PanelState::Mcp(panel));
    }

    /// 打开 Cron 面板
    pub fn open_cron_panel(&mut self) {
        let tasks: Vec<_> = self
            .services
            .cron
            .scheduler
            .lock()
            .list_tasks()
            .into_iter()
            .cloned()
            .collect();
        if tasks.is_empty() {
            let vm = crate::ui::message_view::MessageViewModel::system(
                self.services.lc.tr("app-no-cron-tasks"),
            );
            self.session_mgr
                .current_mut()
                .messages
                .view_messages
                .push(vm);
            self.render_rebuild();
            return;
        }
        let panel = CronPanel::new(tasks);
        self.open_panel(PanelState::Cron(panel));
    }

    pub fn open_plugin_panel(&mut self) {
        use crate::app::plugin_panel::{
            DiscoverPlugin, MarketplaceViewEntry, MarketplaceViewStatus, PluginEntry,
            PluginItemType,
        };
        use peri_middlewares::plugin::{
            load_claude_settings, load_installed_plugins, load_known_marketplaces,
            load_plugin_manifest, marketplaces_cache_dir, MarketplaceManager,
        };

        let claude_dir = dirs_next::home_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join(".claude");

        let installed = load_installed_plugins(None).unwrap_or_default();
        let settings = load_claude_settings(None).unwrap_or_default();
        let enabled_ids: std::collections::HashSet<&str> = settings
            .enabled_plugins
            .iter()
            .map(|s| s.as_str())
            .collect();

        // 已安装插件 ID 集合（用于 Discover 标记 installed）
        let installed_ids: std::collections::HashSet<String> =
            installed.plugins.iter().map(|p| p.id.clone()).collect();

        let mut entries: Vec<PluginEntry> = Vec::new();
        for p in &installed.plugins {
            let enabled = enabled_ids.contains(p.id.as_str());

            let manifest_result = load_plugin_manifest(&p.install_path);
            let (
                plugin_type,
                load_error,
                description,
                author,
                commands,
                skills,
                agents,
                mcp_servers,
            ) = match &manifest_result {
                Ok(m) => {
                    // 统一显示为 Plugin 类型
                    let ptype = PluginItemType::Plugin;
                    let desc = m.description.clone();
                    let auth = m.author.as_ref().map(|a| a.name.clone());
                    let cmds = m
                        .commands
                        .as_ref()
                        .map(|c| {
                            c.iter()
                                .filter_map(|cmd| match cmd {
                                    peri_middlewares::plugin::PluginCommandEntry::Full(fc) => {
                                        fc.name.clone().or_else(|| {
                                            std::path::Path::new(&fc.path)
                                                .file_stem()
                                                .and_then(|s| s.to_str().map(String::from))
                                        })
                                    }
                                    peri_middlewares::plugin::PluginCommandEntry::Path(p) => {
                                        std::path::Path::new(p)
                                            .file_stem()
                                            .and_then(|s| s.to_str().map(String::from))
                                    }
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    let sks = m.skills.clone().unwrap_or_default();
                    let ags = m
                        .agents
                        .as_ref()
                        .map(|a| a.iter().map(|ag| ag.name.clone()).collect())
                        .unwrap_or_default();
                    let mcps = m
                        .mcp_servers
                        .as_ref()
                        .map(|s| s.keys().cloned().collect())
                        .unwrap_or_default();
                    (ptype, None, desc, auth, cmds, sks, ags, mcps)
                }
                Err(e) => (
                    PluginItemType::Plugin,
                    Some(e.to_string()),
                    String::new(),
                    None,
                    vec![],
                    vec![],
                    vec![],
                    vec![],
                ),
            };

            entries.push(PluginEntry {
                id: p.id.clone(),
                name: p.name.clone(),
                plugin_type,
                marketplace: p.marketplace.clone(),
                enabled,
                scope: p.scope,
                version: p.version.clone(),
                install_path: p.install_path.clone(),
                project_path: p.project_path.clone(),
                load_error,
                description,
                author,
                commands,
                skills,
                agents,
                mcp_servers,
            });
        }

        // 按 scope 排序: Project 在前, User 在后
        entries.sort_by(|a, b| {
            let scope_order = |s: &peri_middlewares::plugin::InstallScope| match s {
                peri_middlewares::plugin::InstallScope::Project => 0,
                peri_middlewares::plugin::InstallScope::Local => 1,
                peri_middlewares::plugin::InstallScope::User => 2,
            };
            scope_order(&a.scope).cmp(&scope_order(&b.scope))
        });

        // --- 加载 Discover 数据 ---
        let cache_base = marketplaces_cache_dir();
        // 确保缓存目录存在（首次运行时 ~/.claude/ 可能不存在）
        let _ = std::fs::create_dir_all(&cache_base);
        let mgr = MarketplaceManager::new(None);
        let known = load_known_marketplaces(None).unwrap_or_default();

        // 构建 discover_plugins：从已缓存的 marketplace manifest 中提取
        let mut discover_plugins: Vec<DiscoverPlugin> = Vec::new();
        let mut marketplace_view_entries: Vec<MarketplaceViewEntry> = Vec::new();

        // 合并 extraKnownMarketplaces
        let mut all_known = known;
        for extra in &settings.extra_known_marketplaces {
            let extra_json = serde_json::to_string(&extra.source).unwrap_or_default();
            let already_exists = all_known
                .iter()
                .any(|km| serde_json::to_string(&km.source).unwrap_or_default() == extra_json);
            if !already_exists {
                // 将 DeclaredMarketplace 转换为 KnownMarketplace
                all_known.push(peri_middlewares::plugin::KnownMarketplace::from(
                    extra.clone(),
                ));
            }
        }

        // 确保 official marketplace 已注册
        use peri_middlewares::plugin::MarketplaceSource;
        let has_official = all_known.iter().any(|km| match &km.source {
            MarketplaceSource::GitHub { repo } => repo == "anthropics/claude-plugins-official",
            _ => false,
        });
        if !has_official {
            all_known.push(peri_middlewares::plugin::KnownMarketplace {
                source: MarketplaceSource::GitHub {
                    repo: "anthropics/claude-plugins-official".into(),
                },
                install_location: String::new(), // 占位符，实际安装时会更新
                auto_update: true,
                last_updated: String::new(), // 占位符，实际安装时会更新
            });
        }

        for km in &all_known {
            let name = MarketplaceManager::extract_name(&km.source);

            // 优先从 install_location 加载，如果不存在则使用默认路径
            // 注意：Url 类型的 install_location 指向 .json 文件，其他类型指向目录
            let cached_manifest = if !km.install_location.is_empty() {
                use peri_middlewares::plugin::marketplace::{
                    find_marketplace_json, read_manifest_from_path,
                };
                let cache_path = std::path::Path::new(&km.install_location);

                // 判断是文件还是目录
                if cache_path.is_file() {
                    // 直接是 .json 文件（Url 类型）
                    read_manifest_from_path(cache_path).ok()
                } else {
                    // 是目录，需要查找 marketplace.json
                    find_marketplace_json(cache_path).and_then(|p| read_manifest_from_path(&p).ok())
                }
            } else {
                mgr.try_load_cache(&km.source, &name)
            };

            let (status, plugin_count) = if let Some(ref manifest) = cached_manifest {
                let count = manifest.plugins.len();
                (MarketplaceViewStatus::Cached, count)
            } else {
                (MarketplaceViewStatus::Stale, 0)
            };

            // 构建 discover 列表
            if let Some(ref manifest) = cached_manifest {
                for p in &manifest.plugins {
                    let plugin_id = format!("{}@{}", p.name, name);
                    let is_installed = installed_ids.contains(&plugin_id);
                    discover_plugins.push(DiscoverPlugin {
                        name: p.name.clone(),
                        description: p.description.clone(),
                        marketplace: name.clone(),
                        version: p.version.clone(),
                        author: p.author.as_ref().map(|a| a.name.clone()),
                        installed: is_installed,
                        plugin_id,
                        install_count: None,
                    });
                }
            }

            // source label
            let source_label = match &km.source {
                MarketplaceSource::GitHub { repo } => format!("github:{}", repo),
                MarketplaceSource::Git { url } => format!("git:{}", url),
                MarketplaceSource::Url { url } => format!("url:{}", url),
                MarketplaceSource::File { path } => format!("file:{}", path),
                MarketplaceSource::Directory { path } => format!("dir:{}", path),
                MarketplaceSource::Npm { package } => format!("npm:{}", package),
            };

            // 统计该 marketplace 的已安装插件数
            let installed_count = installed_ids
                .iter()
                .filter(|id| id.ends_with(&format!("@{}", name)))
                .count();

            marketplace_view_entries.push(MarketplaceViewEntry {
                name: name.clone(),
                source: km.source.clone(),
                source_label,
                plugin_count,
                installed_count,
                status,
                last_updated: if km.last_updated.is_empty() {
                    None
                } else {
                    Some(km.last_updated.clone())
                },
                auto_update: km.auto_update,
            });
        }

        // 注入安装量数据并排序
        let install_counts = peri_middlewares::plugin::load_install_counts();
        if let Some(ref counts) = install_counts {
            for dp in &mut discover_plugins {
                // 远程数据 key 格式为 "plugin-name@marketplace-name"，与 plugin_id 一致
                dp.install_count = counts.get(&dp.plugin_id).copied();
            }
            // 安装量降序 -> 同安装量按字母序
            discover_plugins.sort_by(|a, b| {
                let ca = a.install_count.unwrap_or(0);
                let cb = b.install_count.unwrap_or(0);
                cb.cmp(&ca).then_with(|| a.name.cmp(&b.name))
            });
        } else {
            // 无安装量数据，按字母排序
            discover_plugins.sort_by(|a, b| a.name.cmp(&b.name));
        }

        let discover_was_empty = discover_plugins.is_empty();

        let mut panel = crate::app::plugin_panel::PluginPanel::new(entries);
        panel.discover_plugins = discover_plugins;
        panel.marketplace_entries = marketplace_view_entries;
        panel.sync_marketplace_list_items();

        self.open_panel(PanelState::Plugin(Box::new(panel)));

        let _ = cache_base;
        let _ = claude_dir;

        // 缓存不存在或过期时，后台刷新安装量数据
        if !peri_middlewares::plugin::is_install_counts_cache_valid() {
            let tx = self.services.bg_event_tx.clone();
            tokio::spawn(async move {
                let result = peri_middlewares::plugin::fetch_install_counts().await;
                if result.is_some() {
                    let _ = tx
                        .send(crate::app::AgentEvent::PluginActionCompleted {
                            plugin_id: "__install_counts__".to_string(),
                            action: "install_counts_refresh".to_string(),
                            success: true,
                            message: String::new(),
                        })
                        .await;
                }
            });
        }

        // 首次无缓存时，后台刷新 official marketplace
        if discover_was_empty {
            // 标记面板加载中状态，避免显示"No plugins available"
            if let Some(ref mut p) = self
                .global_panels
                .get_mut::<crate::app::plugin_panel::PluginPanel>()
            {
                p.discover_loading = true;
            }
            let tx = self.services.bg_event_tx.clone();
            let official_source = MarketplaceSource::GitHub {
                repo: "anthropics/claude-plugins-official".into(),
            };
            let official_name = MarketplaceManager::extract_name(&official_source);
            tokio::spawn(async move {
                use peri_middlewares::plugin::marketplace::refresh_marketplace;
                match refresh_marketplace(&official_source, &official_name).await {
                    Ok((_manifest, _install_location)) => {
                        // 同步到 known_marketplaces 以记录 install_location
                        if let Ok(mut marketplaces) =
                            peri_middlewares::plugin::load_known_marketplaces(None)
                        {
                            if let Some(km) = marketplaces
                                .iter_mut()
                                .find(|km| km.source == official_source)
                            {
                                km.install_location = _install_location;
                                km.last_updated = chrono::Utc::now().to_rfc3339();
                                let _ = peri_middlewares::plugin::save_known_marketplaces(
                                    &marketplaces,
                                    None,
                                );
                            }
                        }
                        let _ = tx
                            .send(crate::app::AgentEvent::PluginActionCompleted {
                                plugin_id: official_name,
                                action: "refresh".to_string(),
                                success: true,
                                message: String::new(),
                            })
                            .await;
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "official marketplace \u{521d}\u{59cb}\u{5237}\u{65b0}\u{5931}\u{8d25}");
                    }
                }
            });
        }
    }

    pub fn close_plugin_panel(&mut self) {
        self.global_panels.close_if(PanelKind::Plugin);
    }

    /// 打开 Tasks 面板（Agent Threads + Cron Tasks 双标签页）
    pub fn open_tasks_panel(&mut self) {
        use super::tasks_panel::TasksPanel;

        // Load agent threads from ThreadStore (subagent threads with parent_thread_id)
        let agents = self.load_agent_thread_entries();

        // Load cron tasks
        let cron_tasks: Vec<_> = self
            .services
            .cron
            .scheduler
            .lock()
            .list_tasks()
            .into_iter()
            .cloned()
            .collect();

        let panel = TasksPanel::new(agents, cron_tasks);
        self.open_panel(PanelState::Tasks(panel));
    }

    /// 从 ThreadStore 加载 agent 线程列表
    fn load_agent_thread_entries(&self) -> Vec<super::tasks_panel::AgentThreadEntry> {
        use super::tasks_panel::AgentThreadEntry;

        let store = self.services.thread_store.clone();
        let threads = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(store.list_threads())
                .unwrap_or_default()
        });

        // Filter to threads matching current cwd
        let cwd = &self.services.cwd;
        let mut entries: Vec<AgentThreadEntry> = threads
            .into_iter()
            .filter(|t| t.cwd == *cwd)
            .map(|t| {
                let title = t.title.clone().unwrap_or_else(|| "(untitled)".to_string());
                AgentThreadEntry {
                    thread_id: t.id.clone(),
                    title,
                    status: "done".to_string(),
                    is_active: false,
                    message_count: t.message_count,
                }
            })
            .collect();

        // Sort: mark current thread as active if we have one
        if let Some(ref current_id) = self.session_mgr.current().current_thread_id {
            for entry in &mut entries {
                if entry.thread_id == *current_id {
                    entry.is_active = true;
                    entry.status = "active".to_string();
                }
            }
            // Sort active first
            entries.sort_by_key(|b| std::cmp::Reverse(b.is_active));
        }

        entries
    }

    /// 添加并保存 marketplace
    ///
    /// 这个方法是同步的，但会启动后台任务获取内容
    pub fn marketplace_add_and_save(&mut self, input: &str) -> anyhow::Result<()> {
        use peri_middlewares::plugin::{
            load_known_marketplaces, parse_marketplace_input, save_known_marketplaces,
            KnownMarketplace, MarketplaceManager,
        };

        // 解析输入
        let source =
            parse_marketplace_input(input).map_err(|e| anyhow::anyhow!("解析失败: {}", e))?;

        // 加载现有的 marketplaces
        let mut marketplaces = load_known_marketplaces(None).unwrap_or_default();

        // 检查是否已存在
        for existing in &marketplaces {
            if existing.source == source {
                anyhow::bail!("Marketplace 已存在");
            }
        }

        // 提取名称
        let name = MarketplaceManager::extract_name(&source);

        // 创建新条目（初始状态：install_location 和 last_updated 为空）
        let new_entry = KnownMarketplace {
            source: source.clone(),
            install_location: String::new(),
            auto_update: false,
            last_updated: String::new(),
        };

        marketplaces.push(new_entry);

        // 保存配置
        save_known_marketplaces(&marketplaces, None)?;

        // 显示成功消息
        self.session_mgr.current_mut().messages.view_messages.push(
            crate::app::MessageViewModel::system(format!(
                "Marketplace 已添加: {} (正在获取内容...)",
                name
            )),
        );

        // 刷新面板以显示新添加的 marketplace
        self.open_plugin_panel();

        // 启动后台任务获取内容并更新 installLocation
        let name_clone = name.clone();
        let tx = self.services.bg_event_tx.clone();
        tokio::spawn(async move {
            use peri_middlewares::plugin::marketplace::refresh_marketplace;
            match refresh_marketplace(&source, &name_clone).await {
                Ok((_manifest, install_location)) => {
                    // 更新 installLocation 和 lastUpdated
                    if let Ok(mut mkt_places) =
                        peri_middlewares::plugin::load_known_marketplaces(None)
                    {
                        if let Some(entry) = mkt_places.iter_mut().find(|km| km.source == source) {
                            entry.install_location = install_location;
                            entry.last_updated = chrono::Utc::now().to_rfc3339();
                            let _ = peri_middlewares::plugin::save_known_marketplaces(
                                &mkt_places,
                                None,
                            );
                        }
                    }
                    let _ = tx
                        .send(crate::app::AgentEvent::PluginActionCompleted {
                            plugin_id: name_clone.clone(),
                            action: "add".to_string(),
                            success: true,
                            message: format!("Marketplace '{}' 内容已获取", name_clone),
                        })
                        .await;
                }
                Err(e) => {
                    let _ = tx
                        .send(crate::app::AgentEvent::PluginActionCompleted {
                            plugin_id: name_clone.clone(),
                            action: "add".to_string(),
                            success: false,
                            message: format!("获取内容失败: {}", e),
                        })
                        .await;
                }
            }
        });

        Ok(())
    }

    /// 删除并保存 marketplace
    pub fn marketplace_delete_and_save(&mut self, name: &str) -> anyhow::Result<()> {
        use peri_middlewares::plugin::{
            load_known_marketplaces, save_known_marketplaces, MarketplaceSource,
        };

        // 加载现有的 marketplaces
        let marketplaces = load_known_marketplaces(None).unwrap_or_default();

        // 过滤掉要删除的 marketplace（通过名称匹配）
        let filtered: Vec<_> = marketplaces
            .into_iter()
            .filter(|km| {
                let km_name = match &km.source {
                    MarketplaceSource::GitHub { repo } => {
                        repo.split('/').next_back().unwrap_or(repo).to_string()
                    }
                    MarketplaceSource::Git { url } => url
                        .split('/')
                        .next_back()
                        .and_then(|s| s.strip_suffix(".git"))
                        .unwrap_or("marketplace")
                        .to_string(),
                    MarketplaceSource::Url { url } => {
                        let last = url.split('/').next_back().unwrap_or("marketplace");
                        last.strip_suffix(".json").unwrap_or(last).to_string()
                    }
                    MarketplaceSource::File { path } => std::path::Path::new(path)
                        .file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or("marketplace")
                        .to_string(),
                    MarketplaceSource::Directory { path } => std::path::Path::new(path)
                        .file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or("marketplace")
                        .to_string(),
                    MarketplaceSource::Npm { package } => {
                        package.split('@').next().unwrap_or(package).to_string()
                    }
                };
                km_name != name
            })
            .collect();

        // 保存
        save_known_marketplaces(&filtered, None)?;

        // 显示成功消息
        self.session_mgr.current_mut().messages.view_messages.push(
            crate::app::MessageViewModel::system(format!("Marketplace 已移除: {}", name)),
        );

        // 刷新面板并恢复到 Marketplaces 视图
        self.open_plugin_panel();
        if let Some(ref mut p) = self.global_panels.get_mut::<plugin_panel::PluginPanel>() {
            p.view = crate::app::plugin_panel::PluginPanelView::Marketplaces;
            // 确保 cursor 不越界
            let max = p.marketplace_entries.len();
            if p.marketplace_list.cursor() > max {
                p.marketplace_list.move_cursor_to(max);
            }
        }

        Ok(())
    }
}
