use super::*;

impl App {
    // ─── Model 面板操作 ───────────────────────────────────────────────────────

    /// 打开 /model 面板
    pub fn open_model_panel(&mut self) {
        let cfg = self.zen_config.get_or_insert_with(ZenConfig::default);
        self.sessions[self.active].core.model_panel = Some(ModelPanel::from_config(cfg));
        // 互斥：关闭其他面板
        self.sessions[self.active].core.login_panel = None;
        self.sessions[self.active].core.config_panel = None;
        self.status_panel = None;
        self.memory_panel = None;
    }

    /// 关闭 /model 面板（不保存）
    pub fn close_model_panel(&mut self) {
        self.sessions[self.active].core.model_panel = None;
    }

    /// 确认选择并保存（Enter 键）：写入 active_alias + effort，更新状态栏
    pub fn model_panel_confirm(&mut self) {
        let Some(panel) = self.sessions[self.active].core.model_panel.as_ref() else {
            return;
        };
        let alias_label = panel.active_tab.label().to_string();
        let effort = panel.buf_thinking_effort.clone();
        let Some(cfg) = self.zen_config.as_mut() else {
            return;
        };
        panel.apply_to_config(cfg);
        let effort_display = match effort.as_str() {
            "low" => "Low",
            "high" => "High",
            _ => "Medium",
        };
        self.sessions[self.active]
            .core
            .view_messages
            .push(MessageViewModel::system(format!(
                "模型已切换为: {} ({} effort)",
                alias_label, effort_display
            )));
        if let Err(e) = Self::save_config(cfg, self.config_path_override.as_deref()) {
            self.sessions[self.active]
                .core
                .view_messages
                .push(MessageViewModel::system(format!("配置保存失败: {}", e)));
        }
        if let Some(p) = agent::LlmProvider::from_config(cfg) {
            self.provider_name = p.display_name().to_string();
            self.model_name = p.model_name().to_string();
        }
        self.sessions[self.active].core.model_panel = None;
    }

    // ─── Login 面板操作 ───────────────────────────────────────────────────────

    /// 打开 /login 面板（同时关闭 model 面板，实现互斥）
    pub fn open_login_panel(&mut self) {
        let cfg = self.zen_config.get_or_insert_with(ZenConfig::default);
        self.sessions[self.active].core.login_panel =
            Some(login_panel::LoginPanel::from_config(cfg));
        // 互斥：关闭其他面板
        self.sessions[self.active].core.model_panel = None;
        self.sessions[self.active].core.config_panel = None;
        self.status_panel = None;
        self.memory_panel = None;
    }

    /// 关闭 /login 面板（不保存）
    pub fn close_login_panel(&mut self) {
        self.sessions[self.active].core.login_panel = None;
    }

    /// 选中（激活）光标处的 Provider
    pub fn login_panel_select_provider(&mut self) {
        let Some(panel) = self.sessions[self.active].core.login_panel.as_mut() else {
            return;
        };
        let selected_name = panel
            .providers
            .get(panel.cursor)
            .map(|p| p.display_name().to_string())
            .unwrap_or_default();
        let Some(cfg) = self.zen_config.as_mut() else {
            return;
        };
        panel.select_provider(cfg);
        if !selected_name.is_empty() {
            self.sessions[self.active]
                .core
                .view_messages
                .push(MessageViewModel::system(format!(
                    "已激活 Provider: {}",
                    selected_name
                )));
        }
        if let Err(e) = Self::save_config(cfg, self.config_path_override.as_deref()) {
            self.sessions[self.active]
                .core
                .view_messages
                .push(MessageViewModel::system(format!("配置保存失败: {}", e)));
        }
        if let Some(p) = agent::LlmProvider::from_config(cfg) {
            self.provider_name = p.display_name().to_string();
            self.model_name = p.model_name().to_string();
        }
        self.close_login_panel();
    }

    /// 保存 Login 面板的编辑/新建内容到 ZenConfig，自动激活并关闭面板
    pub fn login_panel_apply_edit(&mut self) {
        let Some(panel) = self.sessions[self.active].core.login_panel.as_mut() else {
            return;
        };
        let edit_name = panel.buf_name.clone();
        let is_new = matches!(panel.mode, login_panel::LoginPanelMode::New);
        let Some(cfg) = self.zen_config.as_mut() else {
            return;
        };
        if !panel.apply_edit(cfg) {
            self.sessions[self.active]
                .core
                .view_messages
                .push(MessageViewModel::system(
                    "保存失败：Provider 名称不能为空".to_string(),
                ));
            return;
        }
        let display = if edit_name.is_empty() {
            "Provider".to_string()
        } else {
            edit_name
        };
        // 自动激活保存的 provider
        panel.select_provider(cfg);
        self.sessions[self.active]
            .core
            .view_messages
            .push(MessageViewModel::system(format!(
                "已{}并激活 Provider: {}",
                if is_new { "新建" } else { "保存" },
                display
            )));
        if let Err(e) = Self::save_config(cfg, self.config_path_override.as_deref()) {
            self.sessions[self.active]
                .core
                .view_messages
                .push(MessageViewModel::system(format!("配置保存失败: {}", e)));
        }
        if let Some(p) = agent::LlmProvider::from_config(cfg) {
            self.provider_name = p.display_name().to_string();
            self.model_name = p.model_name().to_string();
        }
        self.close_login_panel();
    }

    /// 确认删除光标处的 Provider
    pub fn login_panel_confirm_delete(&mut self) {
        let Some(panel) = self.sessions[self.active].core.login_panel.as_mut() else {
            return;
        };
        let Some(cfg) = self.zen_config.as_mut() else {
            return;
        };
        let deleted_name = panel
            .providers
            .get(panel.cursor)
            .map(|p| p.display_name().to_string())
            .unwrap_or_default();
        panel.confirm_delete(cfg);
        if !deleted_name.is_empty() {
            self.sessions[self.active]
                .core
                .view_messages
                .push(MessageViewModel::system(format!(
                    "已删除 Provider: {}",
                    deleted_name
                )));
        }
        if let Err(e) = Self::save_config(cfg, self.config_path_override.as_deref()) {
            self.sessions[self.active]
                .core
                .view_messages
                .push(MessageViewModel::system(format!("配置保存失败: {}", e)));
        }
        if let Some(p) = agent::LlmProvider::from_config(cfg) {
            self.provider_name = p.display_name().to_string();
            self.model_name = p.model_name().to_string();
        }
    }

    // ─── Config 面板操作 ───────────────────────────────────────────────────────

    /// 打开 /config 面板
    pub fn open_config_panel(&mut self) {
        let cfg = self.zen_config.get_or_insert_with(ZenConfig::default);
        self.sessions[self.active].core.config_panel =
            Some(config_panel::ConfigPanel::from_config(cfg));
        // 互斥：关闭其他面板
        self.sessions[self.active].core.login_panel = None;
        self.sessions[self.active].core.model_panel = None;
    }

    /// 关闭 /config 面板
    pub fn close_config_panel(&mut self) {
        self.sessions[self.active].core.config_panel = None;
    }

    /// 保存 Config 面板编辑并关闭
    pub fn config_panel_apply(&mut self) {
        let Some(panel) = self.sessions[self.active].core.config_panel.as_mut() else {
            return;
        };
        let Some(cfg) = self.zen_config.as_mut() else {
            return;
        };
        panel.apply_edit(cfg);
        if let Err(e) = Self::save_config(cfg, self.config_path_override.as_deref()) {
            self.sessions[self.active]
                .core
                .view_messages
                .push(MessageViewModel::system(format!("配置保存失败: {}", e)));
        } else {
            self.sessions[self.active]
                .core
                .view_messages
                .push(MessageViewModel::system("配置已保存".to_string()));
        }
        self.sessions[self.active].core.config_panel = None;
    }

    // ─── Status 面板操作 ───────────────────────────────────────────────────────

    /// 打开状态面板并激活指定 Tab
    pub fn open_status_panel(&mut self, tab: usize) {
        self.status_panel = Some(status_panel::StatusPanel::new(tab));
        // 互斥
        self.sessions[self.active].core.config_panel = None;
        self.sessions[self.active].core.login_panel = None;
        self.sessions[self.active].core.model_panel = None;
    }

    /// 关闭状态面板
    pub fn close_status_panel(&mut self) {
        self.status_panel = None;
    }

    // ─── Memory 面板操作 ───────────────────────────────────────────────────────

    /// 打开 /memory 面板
    pub fn open_memory_panel(&mut self) {
        let home_dir = dirs_next::home_dir();
        let mut panel = crate::app::memory_panel::MemoryPanel::new(&self.cwd, home_dir);
        panel.refresh_exists();
        self.memory_panel = Some(panel);
        // 互斥
        self.sessions[self.active].core.config_panel = None;
        self.sessions[self.active].core.login_panel = None;
        self.sessions[self.active].core.model_panel = None;
        self.status_panel = None;
    }

    /// 关闭 /memory 面板
    pub fn close_memory_panel(&mut self) {
        self.memory_panel = None;
    }

    pub fn open_plugin_panel(&mut self) {
        use crate::app::plugin_panel::{
            DiscoverPlugin, MarketplaceViewEntry, MarketplaceViewStatus, PluginEntry,
            PluginItemType,
        };
        use rust_agent_middlewares::plugin::{
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
                                .filter_map(|cmd| {
                                    cmd.name.clone().or_else(|| {
                                        std::path::Path::new(&cmd.path)
                                            .file_stem()
                                            .and_then(|s| s.to_str().map(String::from))
                                    })
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
            let scope_order = |s: &rust_agent_middlewares::plugin::InstallScope| match s {
                rust_agent_middlewares::plugin::InstallScope::Project => 0,
                rust_agent_middlewares::plugin::InstallScope::Local => 1,
                rust_agent_middlewares::plugin::InstallScope::User => 2,
            };
            scope_order(&a.scope).cmp(&scope_order(&b.scope))
        });

        // --- 加载 Discover 数据 ---
        let cache_base = marketplaces_cache_dir();
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
                all_known.push(rust_agent_middlewares::plugin::KnownMarketplace::from(
                    extra.clone(),
                ));
            }
        }

        // 确保 official marketplace 已注册
        use rust_agent_middlewares::plugin::MarketplaceSource;
        let has_official = all_known.iter().any(|km| match &km.source {
            MarketplaceSource::GitHub { repo } => repo == "anthropics/claude-plugins-official",
            _ => false,
        });
        if !has_official {
            all_known.push(rust_agent_middlewares::plugin::KnownMarketplace {
                source: MarketplaceSource::GitHub {
                    repo: "anthropics/claude-plugins-official".into(),
                },
                install_location: String::new(), // 占位符，实际安装时会更新
                auto_update: true,
                last_updated: String::new(), // 占位符，实际安装时会更新
            });
        }

        for km in &all_known {
            let name = MarketplaceManager::extract_name_wrapper(&km.source);

            // 优先从 install_location 加载，如果不存在则使用默认路径
            // 注意：Url 类型的 install_location 指向 .json 文件，其他类型指向目录
            let cached_manifest = if !km.install_location.is_empty() {
                use rust_agent_middlewares::plugin::marketplace::{
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
                mgr.try_load_cache_wrapper(&km.source, &name)
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

        // discover 按名称排序
        discover_plugins.sort_by(|a, b| a.name.cmp(&b.name));

        let mut panel = crate::app::plugin_panel::PluginPanel::new(entries);
        panel.discover_plugins = discover_plugins;
        panel.marketplace_entries = marketplace_view_entries;

        self.plugin_panel = Some(panel);
        // 互斥：关闭其他面板
        self.sessions[self.active].core.login_panel = None;
        self.sessions[self.active].core.model_panel = None;
        self.sessions[self.active].core.config_panel = None;
        self.status_panel = None;
        self.memory_panel = None;
        self.mcp_panel = None;

        let _ = cache_base;
        let _ = claude_dir;
    }

    pub fn close_plugin_panel(&mut self) {
        self.plugin_panel = None;
    }

    /// 添加并保存 marketplace
    ///
    /// 这个方法是同步的，但会启动后台任务获取内容
    pub fn marketplace_add_and_save(&mut self, input: &str) -> anyhow::Result<()> {
        use rust_agent_middlewares::plugin::{
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
        let name = MarketplaceManager::extract_name_wrapper(&source);

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
        self.sessions[self.active]
            .core
            .view_messages
            .push(crate::app::MessageViewModel::system(format!(
                "Marketplace 已添加: {} (正在获取内容...)",
                name
            )));

        // 刷新面板以显示新添加的 marketplace
        self.open_plugin_panel();

        // 启动后台任务获取内容并更新 installLocation
        let name_clone = name.clone();
        let tx = self.bg_event_tx.clone();
        tokio::spawn(async move {
            use rust_agent_middlewares::plugin::marketplace::refresh_marketplace;
            match refresh_marketplace(&source, &name_clone).await {
                Ok((_manifest, install_location)) => {
                    // 更新 installLocation 和 lastUpdated
                    if let Ok(mut mkt_places) =
                        rust_agent_middlewares::plugin::load_known_marketplaces(None)
                    {
                        if let Some(entry) = mkt_places.iter_mut().find(|km| km.source == source) {
                            entry.install_location = install_location;
                            entry.last_updated = chrono::Utc::now().to_rfc3339();
                            let _ = rust_agent_middlewares::plugin::save_known_marketplaces(
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
        use rust_agent_middlewares::plugin::{
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
                        repo.split('/').last().unwrap_or(repo).to_string()
                    }
                    MarketplaceSource::Git { url } => url
                        .split('/')
                        .last()
                        .and_then(|s| s.strip_suffix(".git"))
                        .unwrap_or("marketplace")
                        .to_string(),
                    MarketplaceSource::Url { url } => {
                        let last = url.split('/').last().unwrap_or("marketplace");
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
        self.sessions[self.active]
            .core
            .view_messages
            .push(crate::app::MessageViewModel::system(format!(
                "Marketplace 已移除: {}",
                name
            )));

        // 刷新面板
        self.open_plugin_panel();

        Ok(())
    }

    /// 打开外部编辑器编辑选中的 memory 文件
    pub fn memory_panel_open_editor(&mut self) -> anyhow::Result<()> {
        let entry = self
            .memory_panel
            .as_ref()
            .and_then(|p| p.entries.get(p.cursor))
            .cloned();
        let Some(entry) = entry else {
            return Ok(());
        };

        // 文件不存在时创建空文件
        if !entry.path.exists() {
            if let Some(parent) = entry.path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::File::create(&entry.path)?;
            // 刷新面板中的 exists 状态
            if let Some(ref mut panel) = self.memory_panel {
                panel.refresh_exists();
            }
        }

        let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());
        tracing::info!("Opening memory file with {}: {:?}", editor, entry.path);

        // 挂起 TUI: 离开 alternate screen + 恢复 raw mode
        ratatui::crossterm::execute!(
            std::io::stdout(),
            ratatui::crossterm::terminal::LeaveAlternateScreen
        )?;
        ratatui::crossterm::terminal::disable_raw_mode()?;

        // 启动编辑器
        let status = std::process::Command::new(&editor)
            .arg(&entry.path)
            .status();

        // 恢复 TUI: 重新进入 alternate screen + raw mode
        ratatui::crossterm::terminal::enable_raw_mode()?;
        ratatui::crossterm::execute!(
            std::io::stdout(),
            ratatui::crossterm::terminal::EnterAlternateScreen
        )?;

        match status {
            Ok(s) if s.success() => {
                tracing::info!("Editor exited successfully");
            }
            Ok(s) => {
                tracing::warn!("Editor exited with status: {}", s);
            }
            Err(e) => {
                tracing::error!("Failed to launch editor: {}", e);
            }
        }

        Ok(())
    }

    // ─── Agent 面板操作 ───────────────────────────────────────────────────────

    /// 打开 /agents 面板（传入扫描到的 agent 列表）
    pub fn open_agent_panel(&mut self, agents: Vec<AgentItem>) {
        self.sessions[self.active].core.agent_panel = Some(AgentPanel::new(
            agents,
            self.sessions[self.active].agent.agent_id.clone(),
        ));
    }

    /// 关闭 /agents 面板（不选择任何 agent）
    pub fn close_agent_panel(&mut self) {
        self.sessions[self.active].core.agent_panel = None;
    }

    /// 在 agent 面板中上移光标
    pub fn agent_panel_move_up(&mut self) {
        if let Some(panel) = self.sessions[self.active].core.agent_panel.as_mut() {
            panel.move_cursor(-1);
            panel.scroll_offset =
                ensure_cursor_visible(panel.cursor as u16, panel.scroll_offset, 10);
        }
    }

    /// 在 agent 面板中下移光标
    pub fn agent_panel_move_down(&mut self) {
        if let Some(panel) = self.sessions[self.active].core.agent_panel.as_mut() {
            panel.move_cursor(1);
            panel.scroll_offset =
                ensure_cursor_visible(panel.cursor as u16, panel.scroll_offset, 10);
        }
    }

    /// 确认选择当前 agent，关闭面板，设置 agent_id
    pub fn agent_panel_confirm(&mut self) {
        // 先取出 selection，避免同时借用 panel 和 agent_id
        let (is_none, agent_id, agent_name) = {
            let panel = match self.sessions[self.active].core.agent_panel.as_mut() {
                Some(p) => p,
                None => return,
            };
            let (is_none, agent_id) = panel.get_selection();
            let agent_name = if is_none {
                None
            } else {
                agent_id
                    .as_ref()
                    .and_then(|_id| panel.current_agent().map(|a| a.name.clone()))
            };
            (is_none, agent_id, agent_name)
        };

        if is_none {
            self.set_agent_id(None);
            self.sessions[self.active]
                .core
                .view_messages
                .push(MessageViewModel::system(
                    "Agent 已重置（未设置 agent_id）".to_string(),
                ));
        } else if let Some(id) = agent_id {
            self.set_agent_id(Some(id.clone()));
            let name = agent_name.unwrap_or_else(|| id.clone());
            self.sessions[self.active]
                .core
                .view_messages
                .push(MessageViewModel::system(format!(
                    "Agent 已切换为: {} ({})",
                    name, id
                )));
        }
        self.sessions[self.active].core.agent_panel = None;
    }

    /// 取消选择（不改变当前 agent_id），关闭面板
    #[allow(dead_code)]
    pub fn agent_panel_clear(&mut self) {
        self.sessions[self.active].core.agent_panel = None;
    }
}

// ─── 测试辅助方法（仅在 cfg(any(test, feature = "headless")) 下编译）──────────

#[cfg(any(test, feature = "headless"))]
impl App {
    /// 向事件队列注入 AgentEvent（测试用）
    pub fn push_agent_event(&mut self, event: AgentEvent) {
        self.sessions[self.active]
            .agent
            .agent_event_queue
            .push(event);
    }

    /// 批量处理队列中所有待处理事件，复用 handle_agent_event 逻辑
    pub fn process_pending_events(&mut self) {
        let events: Vec<AgentEvent> =
            std::mem::take(&mut self.sessions[self.active].agent.agent_event_queue);
        for event in events {
            let (_updated, should_break, should_return) = self.handle_agent_event(event);
            if should_return || should_break {
                break;
            }
        }
    }

    /// 构造 Headless 测试用 App，使用 ratatui TestBackend 替代真实终端
    pub async fn new_headless(
        width: u16,
        height: u16,
    ) -> (App, crate::ui::headless::HeadlessHandle) {
        use crate::thread::SqliteThreadStore;
        use ratatui::{backend::TestBackend, Terminal};

        let backend = TestBackend::new(width, height);
        let terminal = Terminal::new(backend).expect("TestBackend should never fail");

        // 启动渲染线程
        let (render_tx, render_cache, render_notify) =
            crate::ui::render_thread::spawn_render_thread(width);

        // 使用唯一临时 SQLite 存储，避免测试并发时文件锁冲突
        let db_name = format!("zen-threads-test-{}.db", uuid::Uuid::now_v7());
        let thread_store: Arc<dyn ThreadStore> = Arc::new(
            SqliteThreadStore::new(std::env::temp_dir().join(db_name))
                .await
                .expect("无法创建测试用 SQLite 数据库"),
        );

        // 将配置路径重定向到临时目录，防止测试污染全局 ~/.zen-code/settings.json
        let test_config_path = std::env::temp_dir().join(format!(
            "zen-config-test-{}/settings.json",
            uuid::Uuid::now_v7()
        ));

        let core = super::AppCore::new(
            "/tmp".to_string(),
            render_tx,
            render_cache,
            Arc::clone(&render_notify),
            crate::command::default_registry(),
            Vec::new(),
        );

        let (bg_event_tx, bg_event_rx) = tokio::sync::mpsc::channel(32);

        let session = super::ChatSession {
            core,
            agent: super::AgentComm::default(),
            langfuse: super::LangfuseState::default(),
            current_thread_id: None,
            todo_items: Vec::new(),
            background_task_count: 0,
            spinner_state: perihelion_widgets::SpinnerState::new(
                perihelion_widgets::SpinnerMode::Idle,
            ),
        };

        let app = App {
            sessions: vec![session],
            active: 0,
            session_areas: Vec::new(),
            cwd: "/tmp".to_string(),
            provider_name: "test".to_string(),
            model_name: "test-model".to_string(),
            zen_config: None,
            thread_store,
            cron: super::CronState::default(),
            setup_wizard: None,
            permission_mode: rust_agent_middlewares::prelude::SharedPermissionMode::new(
                rust_agent_middlewares::prelude::PermissionMode::Bypass,
            ),
            mode_highlight_until: None,
            model_highlight_until: None,
            config_path_override: Some(test_config_path),
            mcp_pool: None,
            mcp_init_rx: None,
            mcp_panel: None,
            mcp_ready_shown_until: std::cell::Cell::new(None),
            status_panel: None,
            memory_panel: None,
            plugin_data: None,
            plugin_panel: None,
            oauth_prompt: None,
            bg_event_tx,
            bg_event_rx: Some(bg_event_rx),
            quit_pending_since: None,
        };

        let handle = crate::ui::headless::HeadlessHandle {
            terminal,
            render_notify,
        };

        (app, handle)
    }
}
