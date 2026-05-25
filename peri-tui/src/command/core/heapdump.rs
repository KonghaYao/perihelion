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

        // ── 2. Allocator info (mimalloc) ──
        #[cfg(not(target_os = "windows"))]
        let current_commit: usize;
        #[cfg(target_os = "windows")]
        let current_commit: usize = 0;
        #[cfg(not(target_os = "windows"))]
        {
            let mut elapsed_msecs: usize = 0;
            let mut user_msecs: usize = 0;
            let mut system_msecs: usize = 0;
            let mut mi_current_rss: usize = 0;
            let mut peak_rss: usize = 0;
            let mut mi_current_commit: usize = 0;
            let mut peak_commit: usize = 0;
            let mut page_faults: usize = 0;

            unsafe {
                libmimalloc_sys::mi_process_info(
                    &mut elapsed_msecs,
                    &mut user_msecs,
                    &mut system_msecs,
                    &mut mi_current_rss,
                    &mut peak_rss,
                    &mut mi_current_commit,
                    &mut peak_commit,
                    &mut page_faults,
                );
            }
            current_commit = mi_current_commit;

            let mi_rss_mb = mi_current_rss as f64 / (1024.0 * 1024.0);
            let peak_rss_mb = peak_rss as f64 / (1024.0 * 1024.0);
            let commit_mb = mi_current_commit as f64 / (1024.0 * 1024.0);
            let peak_commit_mb = peak_commit as f64 / (1024.0 * 1024.0);
            let rss_overhead_mb = rss_mb - mi_rss_mb;

            let _ = writeln!(buf, "=== MIMALLOC SUMMARY ===");
            let _ = writeln!(
                buf,
                "  elapsed:          {:.1}s",
                elapsed_msecs as f64 / 1000.0
            );
            let _ = writeln!(
                buf,
                "  user_time:        {:.1}s",
                user_msecs as f64 / 1000.0
            );
            let _ = writeln!(
                buf,
                "  system_time:      {:.1}s",
                system_msecs as f64 / 1000.0
            );
            let _ = writeln!(
                buf,
                "  current_rss:      {:.0}MB  (ps RSS: {:.0}MB)",
                mi_rss_mb, rss_mb
            );
            let _ = writeln!(buf, "  peak_rss:         {:.0}MB", peak_rss_mb);
            let _ = writeln!(buf, "  current_committed:{:.0}MB", commit_mb);
            let _ = writeln!(buf, "  peak_committed:   {:.0}MB", peak_commit_mb);
            let _ = writeln!(buf, "  page_faults:      {page_faults}");
            let _ = writeln!(buf, "  RSS-overhead:     {:.0}MB", rss_overhead_mb);
            let _ = writeln!(buf);

            // Detailed mimalloc stats via callback
            let mut stats_buf: Vec<u8> = Vec::new();
            unsafe extern "C" fn write_to_vec(
                msg: *const std::os::raw::c_char,
                arg: *mut std::os::raw::c_void,
            ) {
                if msg.is_null() {
                    return;
                }
                let cstr = std::ffi::CStr::from_ptr(msg);
                let bytes = cstr.to_bytes();
                if !bytes.is_empty() {
                    let vec = &mut *(arg as *mut Vec<u8>);
                    vec.extend_from_slice(bytes);
                }
            }
            unsafe {
                libmimalloc_sys::mi_stats_print_out(
                    Some(write_to_vec),
                    &mut stats_buf as *mut Vec<u8> as *mut std::os::raw::c_void,
                );
            }
            buf.extend_from_slice(&stats_buf);
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
                let commit_str = format!("{:.0}MB", current_commit as f64 / (1024.0 * 1024.0));
                #[cfg(target_os = "windows")]
                let commit_str = "N/A".to_string();
                format!("Heapdump -> {filename}\nRSS: {rss_mb:.0}MB | committed: {commit_str}")
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
    } else if cfg!(target_os = "linux") {
        // /proc/self/statm 第 2 列 = resident pages，乘以 page_size 得字节
        std::fs::read_to_string("/proc/self/statm")
            .ok()
            .and_then(|s| s.split_whitespace().nth(1)?.parse::<f64>().ok())
            .map(|pages| pages * 4096.0 / (1024.0 * 1024.0))
            .unwrap_or(-1.0)
    } else {
        -1.0
    }
}
