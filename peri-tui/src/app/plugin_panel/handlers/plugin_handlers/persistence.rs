use crate::app::{
    panel_manager::PanelContext,
    plugin_panel::{MarketplaceViewEntry, MarketplaceViewStatus, PluginPanel},
    AgentEvent,
};

impl PluginPanel {
    /// 持久化 enabled 状态到 Claude settings
    pub(super) fn persist_enabled_state(
        &self,
        claude_settings_override: Option<&std::path::PathBuf>,
    ) {
        let states: Vec<(String, bool)> = self
            .entries
            .iter()
            .map(|e| (e.id.clone(), e.enabled))
            .collect();
        if let Err(e) = peri_middlewares::plugin::save_claude_settings_enabled_plugins(
            &states,
            claude_settings_override.map(|p| p.as_path()),
        ) {
            tracing::warn!(error = %e, "\u{4fdd}\u{5b58} enabledPlugins \u{5931}\u{8d25}");
        }
    }

    /// 持久化删除 marketplace
    pub(super) fn persist_marketplace_delete(&self, name: &str) -> anyhow::Result<()> {
        use peri_middlewares::plugin::{
            load_known_marketplaces, save_known_marketplaces, MarketplaceSource,
        };
        let marketplaces = load_known_marketplaces(None).unwrap_or_default();
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
        save_known_marketplaces(&filtered, None)?;
        Ok(())
    }

    /// 持久化添加 marketplace
    pub(super) fn persist_marketplace_add(
        &mut self,
        input: &str,
        ctx: &mut PanelContext<'_>,
    ) -> anyhow::Result<()> {
        use peri_middlewares::plugin::{
            load_known_marketplaces, parse_marketplace_input, save_known_marketplaces,
            KnownMarketplace, MarketplaceManager,
        };
        let source = parse_marketplace_input(input)
            .map_err(|e| anyhow::anyhow!("\u{89e3}\u{6790}\u{5931}\u{8d25}: {}", e))?;
        let mut marketplaces = load_known_marketplaces(None).unwrap_or_default();
        for existing in &marketplaces {
            if existing.source == source {
                anyhow::bail!("Marketplace \u{5df2}\u{5b58}\u{5728}");
            }
        }
        let name = MarketplaceManager::extract_name(&source);
        let new_entry = KnownMarketplace {
            source: source.clone(),
            install_location: String::new(),
            auto_update: false,
            last_updated: String::new(),
        };
        marketplaces.push(new_entry);
        save_known_marketplaces(&marketplaces, None)?;

        ctx.session_mgr.current_mut().messages.push_system_note(
            ctx.services
                .lc
                .tr_args("app-plugin-added", &[("name".into(), name.clone().into())]),
        );

        // Add placeholder entry to marketplace_entries
        self.marketplace_entries.push(MarketplaceViewEntry {
            name: name.clone(),
            source: source.clone(),
            source_label: format!("{:?}", source),
            plugin_count: 0,
            installed_count: 0,
            status: MarketplaceViewStatus::Fetching,
            last_updated: None,
            auto_update: false,
        });

        // Spawn background refresh
        let name_clone = name.clone();
        let tx = ctx.services.bg_event_tx.clone();
        tokio::spawn(async move {
            use peri_middlewares::plugin::marketplace::refresh_marketplace;
            match refresh_marketplace(&source, &name_clone).await {
                Ok((_manifest, install_location)) => {
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
                        .send(AgentEvent::PluginActionCompleted {
                            plugin_id: name_clone.clone(),
                            action: "add".to_string(),
                            success: true,
                            message: format!(
                                "Marketplace '{}' \u{5185}\u{5bb9}\u{5df2}\u{83b7}\u{53d6}",
                                name_clone
                            ),
                        })
                        .await;
                }
                Err(e) => {
                    let _ = tx
                        .send(AgentEvent::PluginActionCompleted {
                            plugin_id: name_clone.clone(),
                            action: "add".to_string(),
                            success: false,
                            message: format!(
                                "\u{83b7}\u{53d6}\u{5185}\u{5bb9}\u{5931}\u{8d25}: {}",
                                e
                            ),
                        })
                        .await;
                }
            }
        });

        Ok(())
    }
}
