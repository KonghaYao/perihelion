mod executor;
mod loader;
pub mod template;

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;

use anyhow::Context;
use sqlx::SqlitePool;
use tokio::sync::Semaphore;

use crate::db::{NodeRun, WorkflowRun};
use crate::runner::template::TemplateContext;
use crate::schema::{NodeDef, Workflow};

pub use loader::load_workflow;
pub use loader::load_workflow_from_content;

const DEFAULT_MAX_CONCURRENT_NODES: usize = 16;

/// Read max concurrent nodes from env `ACPX_MAX_CONCURRENT`, fallback to default.
fn max_concurrent_nodes() -> usize {
    std::env::var("ACPX_MAX_CONCURRENT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_MAX_CONCURRENT_NODES)
        .max(1)
}

/// Run a workflow to completion asynchronously.
/// This spawns the actual execution so the caller (HTTP handler) can return immediately.
pub async fn run_workflow(
    pool: Arc<SqlitePool>,
    run_id: String,
    workflow: Workflow,
    inputs: HashMap<String, String>,
) {
    tokio::spawn(async move {
        if let Err(e) = execute_dag(pool.clone(), &run_id, &workflow, &inputs).await {
            tracing::error!(run_id = %run_id, error = %e, "workflow execution failed");
            let _ =
                WorkflowRun::update_status(&pool, &run_id, "failed", Some(&e.to_string())).await;
        }
    });
}

/// Execute the full DAG: schedule → run nodes → finalize.
async fn execute_dag(
    pool: Arc<SqlitePool>,
    run_id: &str,
    wf: &Workflow,
    inputs: &HashMap<String, String>,
) -> anyhow::Result<()> {
    let nodes = &wf.nodes;

    WorkflowRun::set_started(&pool, run_id).await?;

    let levels = topological_sort(nodes)?;

    let node_index: HashMap<&str, usize> = nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (node_id(n), i))
        .collect();

    let semaphore = Arc::new(Semaphore::new(max_concurrent_nodes()));

    let mut completed: HashSet<usize> = HashSet::new();
    let mut failed: HashSet<usize> = HashSet::new();

    // In-memory output tracking: node_id -> outputs
    let mut completed_outputs: HashMap<String, HashMap<String, String>> = HashMap::new();

    for level in &levels {
        let mut tasks = Vec::new();

        for &idx in level {
            let node = &nodes[idx];

            // Check dependency status: a node should only run if all deps succeeded
            // or if deps have continue_on_error and completed (even if failed)
            let deps_satisfied = node_depends(node).iter().all(|dep| {
                node_index
                    .get(dep.as_str())
                    .is_some_and(|&di| completed.contains(&di))
            });

            let deps_any_finished = node_depends(node).iter().all(|dep| {
                node_index
                    .get(dep.as_str())
                    .is_some_and(|&di| completed.contains(&di) || failed.contains(&di))
            });

            if !deps_satisfied {
                if !deps_any_finished {
                    tracing::warn!(node_id = %node_id(node), "dependencies not met, skipping");
                    continue;
                }
                // All deps finished but some failed — skip this node
                tracing::warn!(node_id = %node_id(node), "dependency failed, skipping");
                continue;
            }

            // Build template context for this node
            let ctx = build_template_context(
                node,
                inputs,
                &wf.reference_inputs,
                &wf.env,
                &completed_outputs,
            );

            let pool = pool.clone();
            let semaphore = semaphore.clone();
            let run_id = run_id.to_string();
            let node = node.clone();

            let task = tokio::spawn(async move {
                let _permit = semaphore.acquire().await.unwrap();
                executor::execute_node(&pool, &run_id, &node, &ctx).await
            });

            tasks.push((idx, task));
        }

        for (idx, task) in tasks {
            match task.await {
                Ok(Ok(outputs)) => {
                    let nid = node_id(&nodes[idx]).to_string();
                    if !outputs.is_empty() {
                        completed_outputs.insert(nid, outputs);
                    }
                    completed.insert(idx);
                }
                Ok(Err(e)) => {
                    tracing::error!(node_idx = idx, error = %e, "node failed");
                    failed.insert(idx);
                    // For continue_on_error nodes, we still treat them as "completed"
                    // so downstream nodes can access their (possibly partial) outputs
                    if node_continue_on_error(&nodes[idx]) {
                        completed.insert(idx);
                    }
                }
                Err(e) => {
                    tracing::error!(node_idx = idx, error = %e, "node task panicked");
                    failed.insert(idx);
                    if node_continue_on_error(&nodes[idx]) {
                        completed.insert(idx);
                    }
                }
            }
        }

        // Check if any hard-failed nodes (not continue_on_error) exist
        let has_hard_failure = failed.iter().any(|&fi| !node_continue_on_error(&nodes[fi]));

        if has_hard_failure {
            // Find the first hard-failed node for the error message
            let first_hard_fail = failed
                .iter()
                .find(|&&fi| !node_continue_on_error(&nodes[fi]));
            if let Some(&fi) = first_hard_fail {
                let node = &nodes[fi];
                WorkflowRun::update_status(
                    &pool,
                    run_id,
                    "failed",
                    Some(&format!("node '{}' failed", node_id(node))),
                )
                .await?;
                let _ = NodeRun::mark_run_pending_as_skipped(&pool, run_id).await;
                return Err(anyhow::anyhow!("node '{}' failed", node_id(node)));
            }
        }
    }

    WorkflowRun::update_status(&pool, run_id, "success", None).await?;
    tracing::info!(run_id = %run_id, "workflow completed successfully");
    Ok(())
}

/// Build the template context for a node.
fn build_template_context(
    node: &NodeDef,
    root_inputs: &HashMap<String, String>,
    reference_inputs: &HashMap<String, HashMap<String, String>>,
    global_env: &HashMap<String, String>,
    completed_outputs: &HashMap<String, HashMap<String, String>>,
) -> TemplateContext {
    let nid = node_id(node);

    // Determine effective inputs: if node ID has a prefix (e.g. "do-build/checkout"),
    // look up reference_inputs for that prefix.
    let effective_inputs = if let Some(slash_pos) = nid.find('/') {
        let prefix = &nid[..slash_pos];
        reference_inputs
            .get(prefix)
            .cloned()
            .unwrap_or_else(|| root_inputs.clone())
    } else {
        root_inputs.clone()
    };

    // Build env: start with global, then interpolate and merge node env
    let node_env = get_node_env(node);
    let mut env = global_env.clone();
    // Interpolate node env with global-only context first (avoid circularity)
    let pre_ctx = TemplateContext {
        inputs: effective_inputs.clone(),
        needs_outputs: completed_outputs.clone(),
        env: global_env.clone(),
    };
    let resolved_node_env = crate::runner::template::interpolate_map(&node_env, &pre_ctx);
    env.extend(resolved_node_env);

    TemplateContext {
        inputs: effective_inputs,
        needs_outputs: completed_outputs.clone(),
        env,
    }
}

fn get_node_env(node: &NodeDef) -> HashMap<String, String> {
    match node {
        NodeDef::Shell(n) => n.env.clone(),
        NodeDef::Agent(n) => n.env.clone(),
        NodeDef::Reference(_) => HashMap::new(),
    }
}

/// Topological sort returning levels of node indices that can run in parallel.
fn topological_sort(nodes: &[NodeDef]) -> anyhow::Result<Vec<Vec<usize>>> {
    let n = nodes.len();

    // Detect duplicate node IDs
    let mut seen_ids: HashMap<&str, usize> = HashMap::new();
    for (i, node) in nodes.iter().enumerate() {
        let id = node_id(node);
        if let Some(&prev) = seen_ids.get(id) {
            anyhow::bail!(
                "duplicate node id '{}' found at indices {} and {}",
                id,
                prev,
                i
            );
        }
        seen_ids.insert(id, i);
    }

    let id_to_idx: HashMap<&str, usize> = nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (node_id(n), i))
        .collect();

    let mut adj: Vec<Vec<usize>> = vec![vec![]; n];
    let mut in_degree = vec![0u32; n];

    for (i, node) in nodes.iter().enumerate() {
        for dep in node_depends(node) {
            let j = id_to_idx.get(dep.as_str()).with_context(|| {
                format!("node '{}' depends on unknown node '{}'", node_id(node), dep)
            })?;
            adj[*j].push(i);
            in_degree[i] += 1;
        }
    }

    let mut queue: VecDeque<usize> = (0..n).filter(|&i| in_degree[i] == 0).collect();
    let mut levels: Vec<Vec<usize>> = Vec::new();

    while !queue.is_empty() {
        let current_level: Vec<usize> = queue.drain(..).collect();
        levels.push(current_level.clone());

        let mut next_queue = VecDeque::new();
        for &node in &current_level {
            for &neighbor in &adj[node] {
                in_degree[neighbor] -= 1;
                if in_degree[neighbor] == 0 {
                    next_queue.push_back(neighbor);
                }
            }
        }
        queue = next_queue;
    }

    if levels.iter().map(|l| l.len()).sum::<usize>() != n {
        anyhow::bail!("workflow contains a cycle");
    }

    Ok(levels)
}

// ─── Node Helpers ─────────────────────────────────────────────────

pub fn node_id(node: &NodeDef) -> &str {
    match node {
        NodeDef::Shell(n) => &n.id,
        NodeDef::Agent(n) => &n.id,
        NodeDef::Reference(n) => &n.id,
    }
}

pub fn node_depends(node: &NodeDef) -> &[String] {
    match node {
        NodeDef::Shell(n) => &n.depends,
        NodeDef::Agent(n) => &n.depends,
        NodeDef::Reference(n) => &n.depends,
    }
}

pub fn node_type_name(node: &NodeDef) -> &str {
    match node {
        NodeDef::Shell(_) => "shell",
        NodeDef::Agent(_) => "agent",
        NodeDef::Reference(_) => "reference",
    }
}

fn node_continue_on_error(node: &NodeDef) -> bool {
    match node {
        NodeDef::Shell(n) => n.continue_on_error,
        NodeDef::Agent(n) => n.continue_on_error,
        NodeDef::Reference(n) => n.continue_on_error,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{ExecConfig, ShellNode};

    fn make_shell_node(id: &str, depends: Vec<String>, continue_on_error: bool) -> NodeDef {
        NodeDef::Shell(ShellNode {
            id: id.to_string(),
            run: crate::schema::ScriptSource::Inline("echo".to_string()),
            depends,
            outputs: Default::default(),
            env: Default::default(),
            continue_on_error,
            exec: ExecConfig {
                timeout: None,
                retry: None,
                shell: None,
            },
        })
    }

    #[test]
    fn test_topological_sort_simple() {
        let nodes = vec![
            make_shell_node("a", vec![], false),
            make_shell_node("b", vec!["a".to_string()], false),
            make_shell_node("c", vec!["b".to_string()], false),
        ];
        let levels = topological_sort(&nodes).unwrap();
        assert_eq!(levels.len(), 3);
        assert_eq!(levels[0], vec![0]); // a
        assert_eq!(levels[1], vec![1]); // b
        assert_eq!(levels[2], vec![2]); // c
    }

    #[test]
    fn test_topological_sort_parallel() {
        let nodes = vec![
            make_shell_node("a", vec![], false),
            make_shell_node("b", vec![], false),
            make_shell_node("c", vec!["a".to_string(), "b".to_string()], false),
        ];
        let levels = topological_sort(&nodes).unwrap();
        assert_eq!(levels.len(), 2);
        assert_eq!(levels[0].len(), 2); // a, b in parallel
        assert_eq!(levels[1], vec![2]); // c
    }

    #[test]
    fn test_topological_sort_cycle() {
        let nodes = vec![
            make_shell_node("a", vec!["b".to_string()], false),
            make_shell_node("b", vec!["a".to_string()], false),
        ];
        assert!(topological_sort(&nodes).is_err());
    }

    #[test]
    fn test_topological_sort_unknown_dep() {
        let nodes = vec![make_shell_node("a", vec!["nonexistent".to_string()], false)];
        assert!(topological_sort(&nodes).is_err());
    }

    #[test]
    fn test_topological_sort_duplicate_id() {
        let nodes = vec![
            make_shell_node("a", vec![], false),
            make_shell_node("a", vec![], false), // duplicate
        ];
        let err = topological_sort(&nodes).unwrap_err();
        assert!(err.to_string().contains("duplicate node id 'a'"));
    }

    #[test]
    fn test_topological_sort_empty() {
        let levels = topological_sort(&[]).unwrap();
        assert!(levels.is_empty());
    }

    #[test]
    fn test_topological_sort_single() {
        let nodes = vec![make_shell_node("only", vec![], false)];
        let levels = topological_sort(&nodes).unwrap();
        assert_eq!(levels.len(), 1);
        assert_eq!(levels[0], vec![0]);
    }

    #[test]
    fn test_node_continue_on_error() {
        let node = make_shell_node("x", vec![], true);
        assert!(node_continue_on_error(&node));
        let node2 = make_shell_node("y", vec![], false);
        assert!(!node_continue_on_error(&node2));
    }

    #[test]
    fn test_max_concurrent_nodes_default() {
        std::env::remove_var("ACPX_MAX_CONCURRENT");
        assert_eq!(max_concurrent_nodes(), 16);
    }

    #[test]
    fn test_max_concurrent_nodes_custom() {
        // Use a unique key to avoid race with other tests
        let key = "ACPX_MAX_CONCURRENT";
        let prev = std::env::var(key).ok();
        std::env::set_var(key, "32");
        assert_eq!(max_concurrent_nodes(), 32);
        match prev {
            Some(v) => std::env::set_var(key, v),
            None => std::env::remove_var(key),
        }
    }

    #[test]
    fn test_max_concurrent_nodes_minimum() {
        let key = "ACPX_MAX_CONCURRENT";
        let prev = std::env::var(key).ok();
        std::env::set_var(key, "0");
        assert_eq!(max_concurrent_nodes(), 1);
        match prev {
            Some(v) => std::env::set_var(key, v),
            None => std::env::remove_var(key),
        }
    }

    #[test]
    fn test_build_template_context_root_inputs() {
        let node = make_shell_node("build", vec![], false);
        let mut root_inputs = HashMap::new();
        root_inputs.insert("env".to_string(), "production".to_string());
        let reference_inputs = HashMap::new();
        let global_env = HashMap::new();
        let completed_outputs = HashMap::new();

        let ctx = build_template_context(
            &node,
            &root_inputs,
            &reference_inputs,
            &global_env,
            &completed_outputs,
        );
        assert_eq!(ctx.inputs.get("env").unwrap(), "production");
    }

    #[test]
    fn test_build_template_context_prefixed_inputs() {
        // Node with prefixed ID (from reference expansion)
        let node = make_shell_node("do-build/checkout", vec![], false);
        let mut root_inputs = HashMap::new();
        root_inputs.insert("env".to_string(), "production".to_string());

        let mut ref_inputs = HashMap::new();
        let mut build_inputs = HashMap::new();
        build_inputs.insert("repo".to_string(), "myrepo".to_string());
        ref_inputs.insert("do-build".to_string(), build_inputs);

        let global_env = HashMap::new();
        let completed_outputs = HashMap::new();

        let ctx = build_template_context(
            &node,
            &root_inputs,
            &ref_inputs,
            &global_env,
            &completed_outputs,
        );
        // Should use reference inputs for prefixed node
        assert_eq!(ctx.inputs.get("repo").unwrap(), "myrepo");
        assert!(!ctx.inputs.contains_key("env"));
    }

    #[test]
    fn test_get_node_env() {
        let node = make_shell_node("x", vec![], false);
        assert!(get_node_env(&node).is_empty());

        let mut env = HashMap::new();
        env.insert("KEY".to_string(), "VAL".to_string());
        let node_with_env = NodeDef::Shell(ShellNode {
            id: "y".to_string(),
            run: crate::schema::ScriptSource::Inline("echo".to_string()),
            depends: vec![],
            outputs: Default::default(),
            env,
            continue_on_error: false,
            exec: ExecConfig {
                timeout: None,
                retry: None,
                shell: None,
            },
        });
        assert_eq!(get_node_env(&node_with_env).get("KEY").unwrap(), "VAL");
    }

    #[test]
    fn test_node_type_name() {
        let shell = make_shell_node("s", vec![], false);
        assert_eq!(node_type_name(&shell), "shell");

        let agent = NodeDef::Agent(crate::schema::AgentNode {
            id: "a".to_string(),
            prompt: crate::schema::PromptSource::Inline("prompt".to_string()),
            agent: None,
            model: None,
            cwd: None,
            depends: vec![],
            outputs: Default::default(),
            env: Default::default(),
            continue_on_error: false,
            exec: ExecConfig {
                timeout: None,
                retry: None,
                shell: None,
            },
        });
        assert_eq!(node_type_name(&agent), "agent");
    }
}
