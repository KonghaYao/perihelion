use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use sqlx::SqlitePool;

use crate::api::{TemplateNodeInfo, WorkflowTemplate};
use crate::runner;
use crate::schema::parse_workflow;

/// Track the latest seen version for each workflow name.
#[derive(Default)]
struct VersionTracker {
    /// workflow_name → (workflow_version, content_hash)
    entries: HashMap<String, (String, u64)>,
    /// True until first scan completes (only tracks versions, no submission).
    is_first_scan: bool,
}

/// Simple FNV-1a hash for content change detection.
fn fnv1a_hash(data: &str) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in data.bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

/// Periodically scan a directory for .yaml/.yml workflow files.
/// First scan only tracks versions (no submission).
/// Subsequent scans submit new runs only for workflows whose version has changed.
/// The shared `templates` list is refreshed on every scan for the API.
pub async fn watch_directory(
    pool: Arc<SqlitePool>,
    templates: Arc<RwLock<Vec<WorkflowTemplate>>>,
    cancellation_tokens: crate::runner::CancelRegistry,
    dir_path: String,
) {
    let mut tracker = VersionTracker {
        entries: HashMap::new(),
        is_first_scan: true,
    };
    let mut interval = tokio::time::interval(Duration::from_secs(10));

    // Immediate first scan (track only, no submits)
    run_scan(
        &pool,
        &templates,
        &cancellation_tokens,
        &dir_path,
        &mut tracker,
    )
    .await;

    loop {
        interval.tick().await;
        run_scan(
            &pool,
            &templates,
            &cancellation_tokens,
            &dir_path,
            &mut tracker,
        )
        .await;
    }
}

async fn run_scan(
    pool: &Arc<SqlitePool>,
    templates: &Arc<RwLock<Vec<WorkflowTemplate>>>,
    cancellation_tokens: &crate::runner::CancelRegistry,
    dir_path: &str,
    tracker: &mut VersionTracker,
) {
    let dir = Path::new(dir_path);
    if !dir.is_dir() {
        tracing::warn!(dir = %dir_path, "workflow watch directory does not exist");
        return;
    }

    let mut scanned_files = 0;
    let mut new_runs = 0u32;
    let first = tracker.is_first_scan;
    let mut template_list = Vec::new();

    match scan_yaml_files(dir) {
        Ok(files) => {
            for file_path in files {
                scanned_files += 1;
                let content = match std::fs::read_to_string(&file_path) {
                    Ok(c) => c,
                    Err(e) => {
                        tracing::warn!(path = %file_path, error = %e, "failed to read workflow file");
                        continue;
                    }
                };

                let wf = match parse_workflow(&content) {
                    Ok(w) => w,
                    Err(e) => {
                        tracing::warn!(path = %file_path, error = %e, "failed to parse workflow file");
                        continue;
                    }
                };

                let name = wf.name.clone();
                let version = wf.version.clone();

                // Always add to template list
                let tpl_nodes: Vec<TemplateNodeInfo> = wf
                    .nodes
                    .iter()
                    .map(|n| TemplateNodeInfo {
                        id: runner::node_id(n).to_string(),
                        node_type: runner::node_type_name(n).to_string(),
                        depends: runner::node_depends(n).to_vec(),
                    })
                    .collect();

                let tpl_inputs: std::collections::HashMap<String, crate::api::TemplateInputDef> =
                    wf.inputs
                        .iter()
                        .map(|(k, def)| {
                            (
                                k.clone(),
                                crate::api::TemplateInputDef {
                                    input_type: match def.input_type {
                                        crate::schema::InputType::String => "string".to_string(),
                                        crate::schema::InputType::Number => "number".to_string(),
                                        crate::schema::InputType::Boolean => "boolean".to_string(),
                                    },
                                    default: def.default.clone(),
                                    required: def.required,
                                },
                            )
                        })
                        .collect();

                template_list.push(WorkflowTemplate {
                    name: name.clone(),
                    version,
                    description: wf.description.clone(),
                    node_count: wf.nodes.len(),
                    file_path: file_path.clone(),
                    nodes: tpl_nodes,
                    inputs: tpl_inputs,
                });

                if first {
                    // First scan: only track versions, don't submit
                    tracker
                        .entries
                        .insert(name, (wf.version, fnv1a_hash(&content)));
                } else {
                    // Subsequent scans: submit on version change, content change, or new workflow
                    let content_hash = fnv1a_hash(&content);
                    let should_submit = match tracker.entries.get(&name) {
                        Some((prev_version, prev_hash)) => {
                            prev_version != &wf.version || prev_hash != &content_hash
                        }
                        None => true,
                    };

                    if should_submit {
                        if let Err(e) = submit_workflow_from_file(
                            pool,
                            cancellation_tokens,
                            &content,
                            &file_path,
                            &wf,
                        )
                        .await
                        {
                            tracing::error!(name = %name, error = %e, "failed to submit workflow");
                            continue;
                        }
                        tracker.entries.insert(name, (wf.version, content_hash));
                        new_runs += 1;
                    }
                }
            }
        }
        Err(e) => {
            tracing::error!(dir = %dir_path, error = %e, "failed to scan workflow directory");
            return;
        }
    }

    tracker.is_first_scan = false;

    // Refresh shared template list
    if let Ok(mut lock) = templates.write() {
        *lock = template_list;
    }

    if first {
        tracing::info!(
            dir = %dir_path,
            scanned = scanned_files,
            "workflow directory initial scan complete"
        );
    } else if new_runs > 0 {
        tracing::info!(
            dir = %dir_path,
            new_runs = new_runs,
            "workflow directory detected changes"
        );
    }
}

/// Recursively scan a directory for .yaml and .yml files.
fn scan_yaml_files(dir: &Path) -> anyhow::Result<Vec<String>> {
    let mut files = Vec::new();
    scan_dir(dir, &mut files)?;
    Ok(files)
}

fn scan_dir(dir: &Path, files: &mut Vec<String>) -> anyhow::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            scan_dir(&path, files)?;
        } else if let Some(ext) = path.extension() {
            if ext == "yaml" || ext == "yml" {
                files.push(path.to_string_lossy().to_string());
            }
        }
    }
    Ok(())
}

/// Submit a workflow parsed from a file.
/// Expands references via the loader before persisting and executing.
async fn submit_workflow_from_file(
    pool: &SqlitePool,
    cancellation_tokens: &crate::runner::CancelRegistry,
    yaml_content: &str,
    file_path: &str,
    wf: &crate::schema::Workflow,
) -> anyhow::Result<()> {
    // Expand references using the loader
    let expanded_wf = runner::load_workflow(file_path, std::collections::HashMap::new()).await?;

    let run_id = crate::api::create_and_start_run(
        pool,
        cancellation_tokens,
        wf,
        expanded_wf,
        yaml_content.to_string(),
    )
    .await?;

    tracing::info!(
        name = %wf.name,
        version = %wf.version,
        run_id = %run_id,
        "submitted workflow from file"
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fnv1a_hash_deterministic() {
        let h1 = fnv1a_hash("hello world");
        let h2 = fnv1a_hash("hello world");
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_fnv1a_hash_different_inputs() {
        let h1 = fnv1a_hash("version: 1.0");
        let h2 = fnv1a_hash("version: 2.0");
        assert_ne!(h1, h2);
    }

    #[test]
    fn test_fnv1a_hash_empty() {
        let h = fnv1a_hash("");
        assert_ne!(h, 0);
    }

    #[test]
    fn test_scan_yaml_files_nonexistent_dir() {
        let result = scan_yaml_files(Path::new("/nonexistent/path/that/does/not/exist"));
        assert!(result.is_err());
    }

    #[test]
    fn test_scan_yaml_files_empty_dir() {
        let dir = std::env::temp_dir().join("acpx_g_test_empty");
        std::fs::create_dir_all(&dir).unwrap();
        let files = scan_yaml_files(&dir).unwrap();
        assert!(files.is_empty());
        std::fs::remove_dir(&dir).unwrap();
    }

    #[test]
    fn test_scan_yaml_files_filters_extension() {
        let dir = std::env::temp_dir().join("acpx_g_test_scan");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("workflow.yaml"), "test").unwrap();
        std::fs::write(dir.join("workflow.yml"), "test").unwrap();
        std::fs::write(dir.join("readme.md"), "test").unwrap();
        std::fs::write(dir.join("config.json"), "test").unwrap();
        let files = scan_yaml_files(&dir).unwrap();
        assert_eq!(files.len(), 2);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
