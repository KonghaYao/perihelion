use anyhow::Context;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::schema::{parse_workflow, NodeDef, Workflow};

/// Load a workflow from a file path or URL, fully expanding all references.
///
/// - Resolves `references` recursively
/// - Detects circular references via canonical path tracking
/// - Resolves relative paths against the declaring file's directory
/// - Wires boundary dependencies (entry/exit nodes)
/// - Stores `with` bindings in `workflow.reference_inputs`
pub async fn load_workflow(
    path_or_url: &str,
    inputs: HashMap<String, String>,
) -> anyhow::Result<Workflow> {
    let base_dir = compute_base_dir(path_or_url);
    let mut visited = HashSet::new();

    // Read top-level file directly — path is relative to CWD, not base_dir
    let content = if is_remote_url(path_or_url) {
        fetch_remote(path_or_url).await?
    } else {
        let canonical = std::fs::canonicalize(path_or_url)
            .with_context(|| format!("cannot resolve workflow path: {path_or_url}"))?
            .to_string_lossy()
            .to_string();
        visited.insert(canonical);
        std::fs::read_to_string(path_or_url)
            .with_context(|| format!("failed to read workflow file: {path_or_url}"))?
    };

    let mut wf = parse_workflow(&content)?;

    // Expand references — reference paths resolve against the declaring file's directory
    if !wf.references.is_empty() {
        wf = expand_references(wf, &base_dir, &mut visited).await?;
    }

    wf.reference_inputs.insert("__root__".to_string(), inputs);
    Ok(wf)
}

/// Load a workflow from raw YAML content (no file path).
/// Uses current working directory as base for relative path resolution.
pub async fn load_workflow_from_content(
    yaml: &str,
    inputs: HashMap<String, String>,
) -> anyhow::Result<Workflow> {
    let mut wf = parse_workflow(yaml)?;
    if !wf.references.is_empty() {
        // Check for reference nodes that need expansion
        wf = expand_references(wf, &std::env::current_dir()?, &mut HashSet::new()).await?;
    }
    wf.reference_inputs.insert("__root__".to_string(), inputs);
    Ok(wf)
}

fn load_workflow_inner<'a>(
    path_or_url: &'a str,
    base_dir: &'a Path,
    visited: &'a mut HashSet<String>,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<Workflow>> + Send + 'a>> {
    Box::pin(async move {
        // Cycle detection for local files
        if !is_remote_url(path_or_url) {
            let abs = resolve_path(path_or_url, base_dir);
            let canonical = std::fs::canonicalize(&abs)
                .with_context(|| format!("cannot resolve workflow path: {abs}"))?
                .to_string_lossy()
                .to_string();
            if visited.contains(&canonical) {
                anyhow::bail!("circular reference detected: {}", canonical);
            }
            visited.insert(canonical);
        }

        let content = fetch_content(path_or_url, base_dir).await?;
        let mut wf = parse_workflow(&content)?;

        // Compute child base dir
        let child_base = compute_base_dir(path_or_url);

        // Expand references if any
        if !wf.references.is_empty() {
            wf = expand_references(wf, &child_base, visited).await?;
        }

        Ok(wf)
    })
}

fn expand_references<'a>(
    mut wf: Workflow,
    base_dir: &'a Path,
    visited: &'a mut HashSet<String>,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<Workflow>> + Send + 'a>> {
    Box::pin(async move {
        let mut expanded_nodes = Vec::new();

        for node in wf.nodes.into_iter() {
            match node {
                NodeDef::Reference(ref_node) => {
                    let ref_path = wf.references.get(&ref_node.r#ref).with_context(|| {
                        format!(
                            "reference '{}' not found in workflow references",
                            ref_node.r#ref
                        )
                    })?;

                    let abs_path = if is_remote_url(ref_path) {
                        ref_path.to_string()
                    } else {
                        resolve_path(ref_path, base_dir)
                    };

                    // Extract with parameters
                    let with_map = with_value_to_map(&ref_node.with);
                    let prefix = format!("{}/", ref_node.id);

                    // Load child workflow
                    let child_wf = load_workflow_inner(&abs_path, base_dir, visited).await?;

                    // Find exit nodes before rewriting
                    let exit_node_ids = find_exit_nodes(&child_wf.nodes);

                    // Prefix and rewire child nodes
                    let mut inlined_nodes = Vec::new();
                    for sub_node in child_wf.nodes {
                        let mut n = prefix_id(sub_node, &prefix);
                        n = prefix_depends(n, &prefix);

                        // Entry node wiring: if no internal deps, add reference node's depends
                        if internal_depends_empty(&n) {
                            add_depends(&mut n, &ref_node.depends);
                        }

                        inlined_nodes.push(n);
                    }

                    // Store with bindings for this reference
                    wf.reference_inputs.insert(ref_node.id.clone(), with_map);

                    // Merge child's reference_inputs with prefixed keys
                    for (child_key, child_inputs) in child_wf.reference_inputs {
                        let prefixed_key = format!("{prefix}{child_key}");
                        wf.reference_inputs.insert(prefixed_key, child_inputs);
                    }

                    // Wire exit nodes: in OTHER parent nodes, replace ref_node.id with exit_node_ids
                    let exit_ids: Vec<String> = exit_node_ids
                        .into_iter()
                        .map(|id| format!("{prefix}{id}"))
                        .collect();

                    // We can't modify nodes already in expanded_nodes or inlined_nodes yet,
                    // so defer: rewire after all nodes collected
                    expanded_nodes.push((inlined_nodes, ref_node.id.clone(), exit_ids));
                }
                other => {
                    expanded_nodes.push((vec![other], String::new(), vec![]));
                }
            }
        }

        // Collect replacement map: ref_node_id -> exit_node_ids
        let mut replacements: HashMap<String, Vec<String>> = HashMap::new();
        let mut final_nodes = Vec::new();
        for (nodes, ref_id, exit_ids) in expanded_nodes {
            if !ref_id.is_empty() {
                replacements.insert(ref_id, exit_ids);
            }
            final_nodes.extend(nodes);
        }

        // Rewire depends in all nodes: replace ref IDs with exit IDs
        let final_nodes = final_nodes
            .into_iter()
            .map(|mut n| {
                rewire_depends(&mut n, &replacements);
                n
            })
            .collect();

        wf.nodes = final_nodes;
        Ok(wf)
    })
}

// ─── Path Helpers ─────────────────────────────────────────────────

fn is_remote_url(path: &str) -> bool {
    path.starts_with("http://") || path.starts_with("https://")
}

fn resolve_path(path: &str, base_dir: &Path) -> String {
    if Path::new(path).is_absolute() || is_remote_url(path) {
        return path.to_string();
    }
    // If the path already contains a directory separator (e.g. "./dir/file.yaml"),
    // treat it as already resolved — don't join with base_dir again.
    let p = Path::new(path);
    if p.parent()
        .is_some_and(|parent| parent != Path::new("") && parent != Path::new("."))
    {
        return path.to_string();
    }
    base_dir.join(path).to_string_lossy().to_string()
}

fn compute_base_dir(path_or_url: &str) -> PathBuf {
    if is_remote_url(path_or_url) {
        // For URLs, use the URL's directory as base
        if let Some(slash) = path_or_url.rfind('/') {
            PathBuf::from(&path_or_url[..slash])
        } else {
            PathBuf::from(".")
        }
    } else {
        PathBuf::from(path_or_url)
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."))
    }
}

async fn fetch_remote(url: &str) -> anyhow::Result<String> {
    let client = reqwest::Client::new();
    let resp = client
        .get(url)
        .send()
        .await
        .with_context(|| format!("failed to fetch remote workflow: {url}"))?;
    let status = resp.status();
    if !status.is_success() {
        anyhow::bail!("failed to fetch remote workflow {}: HTTP {}", url, status);
    }
    let body = resp
        .text()
        .await
        .with_context(|| format!("failed to read response body from: {url}"))?;
    Ok(body)
}

async fn fetch_content(path_or_url: &str, base_dir: &Path) -> anyhow::Result<String> {
    if is_remote_url(path_or_url) {
        fetch_remote(path_or_url).await
    } else {
        let abs = resolve_path(path_or_url, base_dir);
        std::fs::read_to_string(&abs)
            .with_context(|| format!("failed to read workflow file: {abs}"))
    }
}

// ─── With Value Conversion ────────────────────────────────────────

/// Convert `with` serde_yaml::Value to HashMap<String, String>.
fn with_value_to_map(value: &serde_yaml::Value) -> HashMap<String, String> {
    let mut map = HashMap::new();
    if let serde_yaml::Value::Mapping(m) = value {
        for (k, v) in m {
            if let serde_yaml::Value::String(key) = k {
                let val = match v {
                    serde_yaml::Value::String(s) => s.clone(),
                    serde_yaml::Value::Number(n) => n.to_string(),
                    serde_yaml::Value::Bool(b) => b.to_string(),
                    serde_yaml::Value::Null => String::new(),
                    other => serde_yaml::to_string(other)
                        .unwrap_or_default()
                        .trim()
                        .to_string(),
                };
                map.insert(key.clone(), val);
            }
        }
    }
    map
}

// ─── Node Helpers ─────────────────────────────────────────────────

/// Find exit nodes: nodes not depended upon by any other node.
fn find_exit_nodes(nodes: &[NodeDef]) -> Vec<String> {
    let mut depended = HashSet::new();
    for node in nodes {
        for dep in node_depends(node) {
            depended.insert(dep.clone());
        }
    }
    nodes
        .iter()
        .map(node_id)
        .filter(|id| !depended.contains(*id))
        .map(|s| s.to_string())
        .collect()
}

fn node_id(node: &NodeDef) -> &str {
    match node {
        NodeDef::Shell(n) => &n.id,
        NodeDef::Agent(n) => &n.id,
        NodeDef::Reference(n) => &n.id,
    }
}

fn node_depends(node: &NodeDef) -> &[String] {
    match node {
        NodeDef::Shell(n) => &n.depends,
        NodeDef::Agent(n) => &n.depends,
        NodeDef::Reference(n) => &n.depends,
    }
}

fn internal_depends_empty(node: &NodeDef) -> bool {
    node_depends(node).is_empty()
}

fn prefix_id(node: NodeDef, prefix: &str) -> NodeDef {
    match node {
        NodeDef::Shell(mut n) => {
            n.id = format!("{prefix}{}", n.id);
            NodeDef::Shell(n)
        }
        NodeDef::Agent(mut n) => {
            n.id = format!("{prefix}{}", n.id);
            NodeDef::Agent(n)
        }
        NodeDef::Reference(mut n) => {
            n.id = format!("{prefix}{}", n.id);
            NodeDef::Reference(n)
        }
    }
}

fn prefix_depends(node: NodeDef, prefix: &str) -> NodeDef {
    match node {
        NodeDef::Shell(mut n) => {
            n.depends = n
                .depends
                .into_iter()
                .map(|d| format!("{prefix}{d}"))
                .collect();
            NodeDef::Shell(n)
        }
        NodeDef::Agent(mut n) => {
            n.depends = n
                .depends
                .into_iter()
                .map(|d| format!("{prefix}{d}"))
                .collect();
            NodeDef::Agent(n)
        }
        NodeDef::Reference(mut n) => {
            n.depends = n
                .depends
                .into_iter()
                .map(|d| format!("{prefix}{d}"))
                .collect();
            NodeDef::Reference(n)
        }
    }
}

fn add_depends(node: &mut NodeDef, deps: &[String]) {
    match node {
        NodeDef::Shell(n) => n.depends.extend(deps.iter().cloned()),
        NodeDef::Agent(n) => n.depends.extend(deps.iter().cloned()),
        NodeDef::Reference(n) => n.depends.extend(deps.iter().cloned()),
    }
}

/// In a node's depends list, replace any ref_node_id with the corresponding exit node IDs.
fn rewire_depends(node: &mut NodeDef, replacements: &HashMap<String, Vec<String>>) {
    let current = match node {
        NodeDef::Shell(n) => &mut n.depends,
        NodeDef::Agent(n) => &mut n.depends,
        NodeDef::Reference(n) => &mut n.depends,
    };

    let mut new_depends = Vec::new();
    for dep in current.drain(..) {
        if let Some(exit_ids) = replacements.get(&dep) {
            new_depends.extend(exit_ids.iter().cloned());
        } else {
            new_depends.push(dep);
        }
    }
    *current = new_depends;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_with_value_to_map() {
        let yaml = r###"
channel: "#deploy"
message: "Build done"
level: "info"
"###;
        let value: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let map = with_value_to_map(&value);
        assert_eq!(map.get("channel").unwrap(), "#deploy");
        assert_eq!(map.get("message").unwrap(), "Build done");
        assert_eq!(map.get("level").unwrap(), "info");
    }

    #[test]
    fn test_with_value_to_map_empty() {
        let map = with_value_to_map(&serde_yaml::Value::Null);
        assert!(map.is_empty());
    }

    #[test]
    fn test_compute_base_dir_local() {
        let dir = compute_base_dir("/workflows/ci.yaml");
        assert_eq!(dir, PathBuf::from("/workflows"));
    }

    #[test]
    fn test_compute_base_dir_url() {
        let dir = compute_base_dir("https://example.com/workflows/ci.yaml");
        assert_eq!(dir, PathBuf::from("https://example.com/workflows"));
    }

    #[test]
    fn test_resolve_path_relative() {
        let base = Path::new("/workflows");
        let result = resolve_path("./notify.yaml", base);
        assert_eq!(result, "/workflows/./notify.yaml");
    }

    #[test]
    fn test_resolve_path_absolute() {
        let base = Path::new("/other");
        let result = resolve_path("/abs/path.yaml", base);
        assert_eq!(result, "/abs/path.yaml");
    }

    #[test]
    fn test_resolve_path_url() {
        let base = Path::new("/other");
        let result = resolve_path("https://example.com/wf.yaml", base);
        assert_eq!(result, "https://example.com/wf.yaml");
    }

    #[test]
    fn test_find_exit_nodes() {
        use crate::schema::{ExecConfig, ShellNode};
        let nodes = vec![
            NodeDef::Shell(ShellNode {
                id: "a".into(),
                run: crate::schema::ScriptSource::Inline("echo a".into()),
                depends: vec![],
                outputs: Default::default(),
                env: Default::default(),
                continue_on_error: false,
                exec: ExecConfig {
                    timeout: None,
                    retry: None,
                    shell: None,
                },
            }),
            NodeDef::Shell(ShellNode {
                id: "b".into(),
                run: crate::schema::ScriptSource::Inline("echo b".into()),
                depends: vec!["a".into()],
                outputs: Default::default(),
                env: Default::default(),
                continue_on_error: false,
                exec: ExecConfig {
                    timeout: None,
                    retry: None,
                    shell: None,
                },
            }),
        ];
        let exits = find_exit_nodes(&nodes);
        assert_eq!(exits, vec!["b"]);
    }

    #[test]
    fn test_find_exit_nodes_parallel() {
        use crate::schema::{ExecConfig, ShellNode};
        let nodes = vec![
            NodeDef::Shell(ShellNode {
                id: "x".into(),
                run: crate::schema::ScriptSource::Inline("echo".into()),
                depends: vec![],
                outputs: Default::default(),
                env: Default::default(),
                continue_on_error: false,
                exec: ExecConfig {
                    timeout: None,
                    retry: None,
                    shell: None,
                },
            }),
            NodeDef::Shell(ShellNode {
                id: "y".into(),
                run: crate::schema::ScriptSource::Inline("echo".into()),
                depends: vec![],
                outputs: Default::default(),
                env: Default::default(),
                continue_on_error: false,
                exec: ExecConfig {
                    timeout: None,
                    retry: None,
                    shell: None,
                },
            }),
        ];
        let mut exits = find_exit_nodes(&nodes);
        exits.sort();
        assert_eq!(exits, vec!["x", "y"]);
    }

    #[test]
    fn test_prefix_id() {
        use crate::schema::{ExecConfig, ShellNode};
        let node = NodeDef::Shell(ShellNode {
            id: "build".into(),
            run: crate::schema::ScriptSource::Inline("echo".into()),
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
        let result = prefix_id(node, "ci/");
        assert_eq!(node_id(&result), "ci/build");
    }

    #[test]
    fn test_rewire_depends() {
        use crate::schema::{ExecConfig, ShellNode};
        let mut node = NodeDef::Shell(ShellNode {
            id: "deploy".into(),
            run: crate::schema::ScriptSource::Inline("echo".into()),
            depends: vec!["ref1".into()],
            outputs: Default::default(),
            env: Default::default(),
            continue_on_error: false,
            exec: ExecConfig {
                timeout: None,
                retry: None,
                shell: None,
            },
        });
        let mut replacements = HashMap::new();
        replacements.insert("ref1".into(), vec!["ci/build".into(), "ci/test".into()]);
        rewire_depends(&mut node, &replacements);
        assert_eq!(node_depends(&node), &["ci/build", "ci/test"]);
    }

    #[test]
    fn test_rewire_depends_no_match() {
        use crate::schema::{ExecConfig, ShellNode};
        let mut node = NodeDef::Shell(ShellNode {
            id: "x".into(),
            run: crate::schema::ScriptSource::Inline("echo".into()),
            depends: vec!["a".into()],
            outputs: Default::default(),
            env: Default::default(),
            continue_on_error: false,
            exec: ExecConfig {
                timeout: None,
                retry: None,
                shell: None,
            },
        });
        rewire_depends(&mut node, &HashMap::new());
        assert_eq!(node_depends(&node), &["a"]);
    }

    #[test]
    fn test_with_value_to_map_number_and_bool() {
        let yaml = "count: 42\nflag: true";
        let value: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let map = with_value_to_map(&value);
        assert_eq!(map.get("count").unwrap(), "42");
        assert_eq!(map.get("flag").unwrap(), "true");
    }
}
