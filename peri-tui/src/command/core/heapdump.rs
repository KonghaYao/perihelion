use std::io::Write;

use crate::app::App;
use crate::command::Command;

pub struct HeapdumpCommand;

impl Command for HeapdumpCommand {
    fn name(&self) -> &str {
        "heapdump"
    }

    fn description(&self, _lc: &crate::i18n::LcRegistry) -> String {
        "Dump heap memory profile to .tmp/heapdump-*.txt".to_string()
    }

    fn execute(&self, app: &mut App, _args: &str) {
        let now = chrono::Local::now();
        let filename = format!(".tmp/heapdump-{}.txt", now.format("%Y%m%d-%H%M%S"));

        let mut buf: Vec<u8> = Vec::new();

        // ── 1. RSS ──
        let rss_mb = read_rss_mb();
        let _ = writeln!(buf, "=== HEAPDUMP {} ===", now.format("%Y-%m-%d %H:%M:%S"));
        let _ = writeln!(buf, "RSS: {:.1} MB\n", rss_mb);

        // ── 2. jemalloc summary + detailed ──
        #[cfg(not(target_os = "windows"))]
        {
            let _ = tikv_jemalloc_ctl::epoch::advance();
            let allocated = tikv_jemalloc_ctl::stats::allocated::read().unwrap_or(0);
            let active = tikv_jemalloc_ctl::stats::active::read().unwrap_or(0);
            let mapped = tikv_jemalloc_ctl::stats::mapped::read().unwrap_or(0);
            let resident = tikv_jemalloc_ctl::stats::resident::read().unwrap_or(0);
            let retained = tikv_jemalloc_ctl::stats::retained::read().unwrap_or(0);
            let huge: usize =
                unsafe { tikv_jemalloc_ctl::raw::read(b"stats.huge.allocated\x00") }.unwrap_or(0);
            let mb = |v: usize| v as f64 / (1024.0 * 1024.0);

            let _ = writeln!(buf, "=== JEMALLOC SUMMARY ===");
            let _ = writeln!(buf, "  allocated:    {:.1} MB", mb(allocated));
            let _ = writeln!(buf, "  active:       {:.1} MB", mb(active));
            let _ = writeln!(buf, "  resident:     {:.1} MB", mb(resident));
            let _ = writeln!(buf, "  mapped:       {:.1} MB", mb(mapped));
            let _ = writeln!(buf, "  retained:     {:.1} MB", mb(retained));
            let _ = writeln!(buf, "  huge:         {:.1} MB", mb(huge));
            let _ = writeln!(
                buf,
                "  non_arena:    {:.1} MB (mapped-active)",
                mb(mapped.saturating_sub(active))
            );
            let _ = writeln!(
                buf,
                "  RSS-overhead: {:.1} MB (RSS-resident)\n",
                rss_mb - mb(resident)
            );

            let _ = writeln!(buf, "=== JEMALLOC ARENAS ===");
            let mut jemalloc_buf = Vec::new();
            let mut opts = tikv_jemalloc_ctl::stats_print::Options::default();
            opts.skip_constants = true;
            opts.skip_per_arena = true;
            opts.skip_bin_size_classes = true;
            opts.skip_mutex_statistics = true;
            let _ = tikv_jemalloc_ctl::stats_print::stats_print(&mut jemalloc_buf, opts);
            buf.extend_from_slice(&jemalloc_buf);
        }

        // ── 3. TUI components ──
        {
            let s = &app.session_mgr.sessions[app.session_mgr.active];
            let agent_bytes: usize = s
                .agent
                .agent_state_messages
                .iter()
                .map(|m| m.content().len())
                .sum();
            let pipeline_bytes: usize = s
                .messages
                .pipeline
                .completed_messages()
                .iter()
                .map(|m| m.content().len())
                .sum();

            let _ = writeln!(buf, "\n=== TUI COMPONENTS ===");
            let _ = writeln!(
                buf,
                "  agent_state_messages: count={}, bytes={:.1}MB",
                s.agent.agent_state_messages.len(),
                agent_bytes as f64 / (1024.0 * 1024.0)
            );
            let _ = writeln!(
                buf,
                "  pipeline_completed:   count={}, bytes={:.1}MB",
                s.messages.pipeline.completed_messages().len(),
                pipeline_bytes as f64 / (1024.0 * 1024.0)
            );
            let _ = writeln!(
                buf,
                "  view_messages:        count={}",
                s.messages.view_messages.len()
            );
            let _ = writeln!(
                buf,
                "  pending_messages:     count={}",
                s.messages.pending_messages.len()
            );
            let _ = writeln!(
                buf,
                "  ephemeral_notes:      count={}",
                s.messages.ephemeral_notes.len()
            );
            let _ = writeln!(buf, "  todo_items:           count={}", s.todo_items.len());
            let _ = writeln!(
                buf,
                "  background_tasks:     count={}",
                app.session_mgr.sessions[app.session_mgr.active].background_task_count
            );
        }

        // ── 4. All sessions ──
        {
            let _ = writeln!(buf, "\n=== SESSIONS ===");
            for (i, sess) in app.session_mgr.sessions.iter().enumerate() {
                let _ = writeln!(
                    buf,
                    "  [{}]: agent_msgs={}, view_vms={}, loading={}",
                    i,
                    sess.agent.agent_state_messages.len(),
                    sess.messages.view_messages.len(),
                    sess.ui.loading,
                );
            }
        }

        // Write file
        let full_path = std::path::Path::new(&filename);
        if let Some(parent) = full_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        let msg = match std::fs::write(full_path, &buf) {
            Ok(()) => {
                #[cfg(not(target_os = "windows"))]
                let mapped_str = format!(
                    "{:.0}MB",
                    tikv_jemalloc_ctl::stats::mapped::read().unwrap_or(0) as f64
                        / (1024.0 * 1024.0)
                );
                #[cfg(target_os = "windows")]
                let mapped_str = "N/A".to_string();
                format!("Heapdump -> {filename}\nRSS: {rss_mb:.0}MB | mapped: {mapped_str}")
            }
            Err(e) => format!("heapdump failed: {e}"),
        };
        app.session_mgr.sessions[app.session_mgr.active]
            .messages
            .view_messages
            .push(crate::app::MessageViewModel::system(msg));
    }
}

fn read_rss_mb() -> f64 {
    if cfg!(target_os = "macos") {
        std::process::Command::new("ps")
            .args(["-o", "rss=", "-p", &std::process::id().to_string()])
            .output()
            .ok()
            .and_then(|o| {
                String::from_utf8_lossy(&o.stdout)
                    .trim()
                    .parse::<f64>()
                    .ok()
                    .map(|kb| kb / 1024.0)
            })
            .unwrap_or(-1.0)
    } else {
        -1.0
    }
}
