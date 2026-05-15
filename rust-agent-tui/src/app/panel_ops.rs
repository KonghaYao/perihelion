use super::*;

impl App {
    // ─── Model 面板操作 ───────────────────────────────────────────────────────

    /// 打开 /model 面板
    pub fn open_model_panel(&mut self) {
        let cfg = self
            .services
            .peri_config
            .get_or_insert_with(PeriConfig::default);
        let panel = ModelPanel::from_config(cfg);
        self.open_panel(PanelState::Model(panel));
    }

    /// 关闭 /model 面板（不保存）
    pub fn close_model_panel(&mut self) {
        self.session_mgr.sessions[self.session_mgr.active]
            .session_panels
            .close_if(PanelKind::Model);
    }

    /// 确认选择并保存（Enter 键）：写入 active_alias + effort，更新状态栏
    pub fn model_panel_confirm(&mut self) {
        let alias_label;
        let effort;
        {
            let Some(panel) = self.session_mgr.sessions[self.session_mgr.active]
                .session_panels
                .get::<ModelPanel>()
            else {
                return;
            };
            alias_label = panel.active_tab.label().to_string();
            effort = panel.buf_thinking_effort.clone();
            let Some(cfg) = self.services.peri_config.as_mut() else {
                return;
            };
            panel.apply_to_config(cfg);
        }
        let effort_display = match effort.as_str() {
            "low" => "Low",
            "high" => "High",
            "xhigh" => "XHigh",
            "max" => "Max",
            _ => "Medium",
        };
        self.session_mgr.sessions[self.session_mgr.active]
            .messages
            .push_system_note(self.services.lc.tr_args(
                "app-model-switched",
                &[
                    ("alias".into(), alias_label.into()),
                    ("effort".into(), effort_display.into()),
                ],
            ));
        {
            let cfg = self.services.peri_config.as_ref().unwrap();
            if let Err(e) = Self::save_config(cfg, self.services.config_path_override.as_deref()) {
                self.session_mgr.sessions[self.session_mgr.active]
                    .messages
                    .push_system_note(self.services.lc.tr_args(
                        "app-config-save-failed",
                        &[("error".into(), e.to_string().into())],
                    ));
            }
            if let Some(p) = agent::LlmProvider::from_config(cfg) {
                self.services.provider_name = p.display_name().to_string();
                self.services.model_name = p.model_name().to_string();
            }
        }
        self.session_mgr.sessions[self.session_mgr.active]
            .session_panels
            .close_if(PanelKind::Model);
    }

    // ─── Login 面板操作 ───────────────────────────────────────────────────────

    /// 打开 /login 面板
    pub fn open_login_panel(&mut self) {
        let cfg = self
            .services
            .peri_config
            .get_or_insert_with(PeriConfig::default);
        let panel = login_panel::LoginPanel::from_config(cfg);
        self.open_panel(PanelState::Login(panel));
    }

    /// 关闭 /login 面板（不保存）
    pub fn close_login_panel(&mut self) {
        self.session_mgr.sessions[self.session_mgr.active]
            .session_panels
            .close_if(PanelKind::Login);
    }

    /// 选中（激活）光标处的 Provider
    pub fn login_panel_select_provider(&mut self) {
        let Some(panel) = self.session_mgr.sessions[self.session_mgr.active]
            .session_panels
            .get_mut::<login_panel::LoginPanel>()
        else {
            return;
        };
        let selected_name = panel
            .providers
            .get(panel.cursor())
            .map(|p| p.display_name().to_string())
            .unwrap_or_default();
        let Some(cfg) = self.services.peri_config.as_mut() else {
            return;
        };
        panel.select_provider(cfg);
        if !selected_name.is_empty() {
            self.session_mgr.sessions[self.session_mgr.active]
                .messages
                .push_system_note(self.services.lc.tr_args(
                    "app-provider-activated",
                    &[("name".into(), selected_name.into())],
                ));
        }
        if let Err(e) = Self::save_config(cfg, self.services.config_path_override.as_deref()) {
            self.session_mgr.sessions[self.session_mgr.active]
                .messages
                .push_system_note(self.services.lc.tr_args(
                    "app-config-save-failed",
                    &[("error".into(), e.to_string().into())],
                ));
        }
        if let Some(p) = agent::LlmProvider::from_config(cfg) {
            self.services.provider_name = p.display_name().to_string();
            self.services.model_name = p.model_name().to_string();
        }
        self.close_login_panel();
    }

    /// 保存 Login 面板的编辑/新建内容到 PeriConfig，自动激活并关闭面板
    pub fn login_panel_apply_edit(&mut self) {
        let Some(panel) = self.session_mgr.sessions[self.session_mgr.active]
            .session_panels
            .get_mut::<login_panel::LoginPanel>()
        else {
            return;
        };
        let edit_name = panel.buf_name.clone();
        let is_new = matches!(panel.mode, login_panel::LoginPanelMode::New);
        let Some(cfg) = self.services.peri_config.as_mut() else {
            return;
        };
        if !panel.apply_edit(cfg) {
            self.session_mgr.sessions[self.session_mgr.active]
                .messages
                .view_messages
                .push(MessageViewModel::system(
                    self.services.lc.tr("app-provider-name-empty"),
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
        let key = if is_new {
            "app-provider-created"
        } else {
            "app-provider-saved"
        };
        self.session_mgr.sessions[self.session_mgr.active]
            .messages
            .view_messages
            .push(MessageViewModel::system(
                self.services
                    .lc
                    .tr_args(key, &[("name".into(), display.into())]),
            ));
        if let Err(e) = Self::save_config(cfg, self.services.config_path_override.as_deref()) {
            self.session_mgr.sessions[self.session_mgr.active]
                .messages
                .view_messages
                .push(MessageViewModel::system(self.services.lc.tr_args(
                    "app-config-save-failed",
                    &[("error".into(), e.to_string().into())],
                )));
        }
        if let Some(p) = agent::LlmProvider::from_config(cfg) {
            self.services.provider_name = p.display_name().to_string();
            self.services.model_name = p.model_name().to_string();
        }
        self.close_login_panel();
    }

    /// 确认删除光标处的 Provider
    pub fn login_panel_confirm_delete(&mut self) {
        let Some(panel) = self.session_mgr.sessions[self.session_mgr.active]
            .session_panels
            .get_mut::<login_panel::LoginPanel>()
        else {
            return;
        };
        let Some(cfg) = self.services.peri_config.as_mut() else {
            return;
        };
        let deleted_name = panel
            .providers
            .get(panel.cursor())
            .map(|p| p.display_name().to_string())
            .unwrap_or_default();
        panel.confirm_delete(cfg);
        if !deleted_name.is_empty() {
            self.session_mgr.sessions[self.session_mgr.active]
                .messages
                .view_messages
                .push(MessageViewModel::system(self.services.lc.tr_args(
                    "app-provider-deleted",
                    &[("name".into(), deleted_name.into())],
                )));
        }
        if let Err(e) = Self::save_config(cfg, self.services.config_path_override.as_deref()) {
            self.session_mgr.sessions[self.session_mgr.active]
                .messages
                .view_messages
                .push(MessageViewModel::system(self.services.lc.tr_args(
                    "app-config-save-failed",
                    &[("error".into(), e.to_string().into())],
                )));
        }
        if let Some(p) = agent::LlmProvider::from_config(cfg) {
            self.services.provider_name = p.display_name().to_string();
            self.services.model_name = p.model_name().to_string();
        }
    }

    // ─── Config 面板操作 ───────────────────────────────────────────────────────

    /// 打开 /config 面板
    pub fn open_config_panel(&mut self) {
        let cfg = self
            .services
            .peri_config
            .get_or_insert_with(PeriConfig::default);
        let panel = config_panel::ConfigPanel::from_config(cfg);
        self.open_panel(PanelState::Config(panel));
    }

    /// 关闭 /config 面板
    pub fn close_config_panel(&mut self) {
        self.session_mgr.sessions[self.session_mgr.active]
            .session_panels
            .close_if(PanelKind::Config);
    }

    /// 保存 Config 面板编辑并关闭
    pub fn config_panel_apply(&mut self) {
        let Some(panel) = self.session_mgr.sessions[self.session_mgr.active]
            .session_panels
            .get_mut::<config_panel::ConfigPanel>()
        else {
            return;
        };
        let Some(cfg) = self.services.peri_config.as_mut() else {
            return;
        };
        if let Err(err_msg) = panel.apply_edit(cfg, &self.services.lc) {
            self.session_mgr.sessions[self.session_mgr.active]
                .messages
                .view_messages
                .push(MessageViewModel::system(err_msg));
            return;
        }
        if let Some(ref lang) = cfg.config.language {
            let _ = self.services.lc.switch(lang);
        }
        if let Err(e) = Self::save_config(cfg, self.services.config_path_override.as_deref()) {
            self.session_mgr.sessions[self.session_mgr.active]
                .messages
                .view_messages
                .push(MessageViewModel::system(self.services.lc.tr_args(
                    "app-config-save-failed",
                    &[("error".into(), e.to_string().into())],
                )));
        } else {
            self.session_mgr.sessions[self.session_mgr.active]
                .messages
                .view_messages
                .push(MessageViewModel::system(
                    self.services.lc.tr("app-config-saved"),
                ));
        }
        self.session_mgr.sessions[self.session_mgr.active]
            .session_panels
            .close_if(PanelKind::Config);
    }

    // ─── Status 面板操作 ───────────────────────────────────────────────────────

    /// 打开状态面板并激活指定 Tab
    pub fn open_status_panel(&mut self, tab: usize) {
        let panel = status_panel::StatusPanel::new(tab);
        self.open_panel(PanelState::Status(panel));
    }

    /// 关闭状态面板
    pub fn close_status_panel(&mut self) {
        self.global_panels.close_if(PanelKind::Status);
    }

    // ─── Memory 面板操作 ───────────────────────────────────────────────────────

    /// 打开 /memory 面板
    pub fn open_memory_panel(&mut self) {
        let home_dir = dirs_next::home_dir();
        let mut panel = crate::app::memory_panel::MemoryPanel::new(&self.services.cwd, home_dir);
        panel.refresh_exists();
        self.open_panel(PanelState::Memory(panel));
    }

    /// 关闭 /memory 面板
    pub fn close_memory_panel(&mut self) {
        self.global_panels.close_if(PanelKind::Memory);
    }

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
            self.session_mgr.sessions[self.session_mgr.active]
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
            self.session_mgr.sessions[self.session_mgr.active]
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
            let name = MarketplaceManager::extract_name(&km.source);

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
        let install_counts = rust_agent_middlewares::plugin::load_install_counts();
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

        let mut panel = crate::app::plugin_panel::PluginPanel::new(entries);
        panel.discover_plugins = discover_plugins;
        panel.marketplace_entries = marketplace_view_entries;

        self.open_panel(PanelState::Plugin(Box::new(panel)));

        let _ = cache_base;
        let _ = claude_dir;

        // 缓存不存在或过期时，后台刷新安装量数据
        if !rust_agent_middlewares::plugin::is_install_counts_cache_valid() {
            let tx = self.services.bg_event_tx.clone();
            tokio::spawn(async move {
                let result = rust_agent_middlewares::plugin::fetch_install_counts().await;
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
    }

    pub fn close_plugin_panel(&mut self) {
        self.global_panels.close_if(PanelKind::Plugin);
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
        self.session_mgr.sessions[self.session_mgr.active]
            .messages
            .view_messages
            .push(crate::app::MessageViewModel::system(format!(
                "Marketplace 已添加: {} (正在获取内容...)",
                name
            )));

        // 刷新面板以显示新添加的 marketplace
        self.open_plugin_panel();

        // 启动后台任务获取内容并更新 installLocation
        let name_clone = name.clone();
        let tx = self.services.bg_event_tx.clone();
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
        self.session_mgr.sessions[self.session_mgr.active]
            .messages
            .view_messages
            .push(crate::app::MessageViewModel::system(format!(
                "Marketplace 已移除: {}",
                name
            )));

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

    /// 打开外部编辑器编辑选中的 memory 文件
    pub fn memory_panel_open_editor(&mut self) -> anyhow::Result<()> {
        let entry = self
            .global_panels
            .get::<crate::app::memory_panel::MemoryPanel>()
            .and_then(|p| p.entries.get(p.cursor()))
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
            if let Some(ref mut panel) = self
                .global_panels
                .get_mut::<crate::app::memory_panel::MemoryPanel>()
            {
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
        let panel = AgentPanel::new(
            agents,
            self.session_mgr.sessions[self.session_mgr.active]
                .agent
                .agent_id
                .clone(),
        );
        self.open_panel(PanelState::Agent(panel));
    }

    /// 关闭 /agents 面板（不选择任何 agent）
    pub fn close_agent_panel(&mut self) {
        self.session_mgr.sessions[self.session_mgr.active]
            .session_panels
            .close_if(PanelKind::Agent);
    }

    /// 确认选择当前 agent，关闭面板，设置 agent_id
    pub fn agent_panel_confirm(&mut self) {
        // 先取出 selection，避免同时借用 panel 和 agent_id
        let (is_none, agent_id, agent_name) = {
            let panel = match self.session_mgr.sessions[self.session_mgr.active]
                .session_panels
                .get_mut::<AgentPanel>()
            {
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
            self.session_mgr.sessions[self.session_mgr.active]
                .messages
                .view_messages
                .push(MessageViewModel::system(
                    self.services.lc.tr("app-agent-reset"),
                ));
        } else if let Some(id) = agent_id {
            self.set_agent_id(Some(id.clone()));
            let name = agent_name.unwrap_or_else(|| id.clone());
            self.session_mgr.sessions[self.session_mgr.active]
                .messages
                .view_messages
                .push(MessageViewModel::system(self.services.lc.tr_args(
                    "app-agent-switched",
                    &[("name".into(), name.into()), ("id".into(), id.into())],
                )));
        }
        self.session_mgr.sessions[self.session_mgr.active]
            .session_panels
            .close_if(PanelKind::Agent);
    }

    // ─── Hooks 面板操作 ───────────────────────────────────────────────────────

    /// 打开 /hooks 面板（只读）
    pub fn open_hooks_panel(&mut self) {
        let mut hooks = self
            .services
            .plugin_data
            .as_ref()
            .map(|pd| pd.all_hooks.clone())
            .unwrap_or_default();
        // 合并 settings.local.json 中的 hooks
        let local_hooks =
            rust_agent_middlewares::hooks::loader::load_settings_local_hooks(&self.services.cwd);
        hooks.extend(local_hooks);
        let panel = HooksPanel::new(hooks);
        self.open_panel(PanelState::Hooks(panel));
    }

    /// 关闭 /hooks 面板
    pub fn close_hooks_panel(&mut self) {
        self.session_mgr.sessions[self.session_mgr.active]
            .session_panels
            .close_if(PanelKind::Hooks);
    }
}

// ─── 测试辅助方法（仅在 cfg(any(test, feature = "headless")) 下编译）──────────

#[cfg(any(test, feature = "headless"))]
impl App {
    /// 向事件队列注入 AgentEvent（测试用）
    pub fn push_agent_event(&mut self, event: AgentEvent) {
        self.session_mgr.sessions[self.session_mgr.active]
            .agent
            .agent_event_queue
            .push(event);
    }

    /// 强制从 pipeline 规范状态重建 view_messages 并发送 RenderEvent。
    /// 用于 headless 测试：确保流式缓冲区内容（throttle 未触发的 chunk）也被渲染。
    pub fn flush_rebuild(&mut self) {
        let prefix_len = self.session_mgr.sessions[self.session_mgr.active]
            .messages
            .round_start_vm_idx;
        let action = self.session_mgr.sessions[self.session_mgr.active]
            .messages
            .pipeline
            .build_rebuild_all(prefix_len);
        self.apply_pipeline_action(action);
    }

    /// 批量处理队列中所有待处理事件，复用 handle_agent_event 逻辑
    pub fn process_pending_events(&mut self) {
        let events: Vec<AgentEvent> = std::mem::take(
            &mut self.session_mgr.sessions[self.session_mgr.active]
                .agent
                .agent_event_queue,
        );
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

        // 将配置路径重定向到临时目录，防止测试污染全局 ~/.peri/settings.json
        let test_config_path = std::env::temp_dir().join(format!(
            "zen-config-test-{}/settings.json",
            uuid::Uuid::now_v7()
        ));

        let (bg_event_tx, bg_event_rx) = tokio::sync::mpsc::channel(32);

        let lc = crate::i18n::LcRegistry::default();
        let commands =
            super::CommandSystem::new(crate::command::default_registry(), Vec::new(), &lc);

        let session = super::ChatSession {
            ui: super::UiState::new(super::build_textarea(false)),
            messages: super::MessageState::new(
                "/tmp".to_string(),
                render_tx.clone(),
                std::sync::Arc::clone(&render_cache),
                std::sync::Arc::clone(&render_notify),
            ),
            session_panels: super::panel_manager::PanelManager::new(),
            commands,
            metadata: super::SessionMetadata::new(),
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
            session_mgr: super::SessionManager::new(session),
            services: super::ServiceRegistry {
                peri_config: None,
                cwd: "/tmp".to_string(),
                provider_name: "test".to_string(),
                model_name: "test-model".to_string(),
                permission_mode: rust_agent_middlewares::prelude::SharedPermissionMode::new(
                    rust_agent_middlewares::prelude::PermissionMode::Bypass,
                ),
                thread_store,
                mcp_pool: None,
                mcp_init_rx: None,
                cron: super::CronState::default(),
                plugin_data: None,
                bg_event_tx,
                bg_event_rx: Some(bg_event_rx),
                config_path_override: Some(test_config_path),
                claude_settings_override: Some(std::env::temp_dir().join(format!(
                    "claude-settings-test-{}.json",
                    uuid::Uuid::now_v7()
                ))),
                resource_monitor: parking_lot::Mutex::new(
                    super::service_registry::ProcessResourceMonitor::new(),
                ),
                lc: crate::i18n::LcRegistry::default(),
            },
            global_panels: PanelManager::new(),
            global_ui: super::GlobalUiState::new(),
            focused: true,
        };

        let handle = crate::ui::headless::HeadlessHandle {
            terminal,
            render_notify,
        };

        (app, handle)
    }
}
