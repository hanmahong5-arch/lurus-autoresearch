//! `resman tree -t <tag>` — render lineage tree via `parent_commit` links.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use serde_json::json;

use crate::cli::OutputFormat;
use crate::error::Result;
use crate::model::{Experiment, RunLog};
use crate::store::load_run_or_suggest;

// ---------------------------------------------------------------------------
// Forest data structures (pub(crate) for tests)
// ---------------------------------------------------------------------------

pub(crate) struct TreeNode<'a> {
    pub exp: &'a Experiment,
    pub children: Vec<TreeNode<'a>>,
    pub on_best_lineage: bool,
}

pub(crate) struct Forest<'a> {
    pub roots: Vec<TreeNode<'a>>,
    pub best_commit: Option<String>,
}

// ---------------------------------------------------------------------------
// Forest builder
// ---------------------------------------------------------------------------

/// Build the forest for a run.  Exposed `pub(crate)` so unit tests can call it.
pub(crate) fn build_forest(run: &RunLog) -> Forest<'_> {
    // commit -> index in run.experiments (first occurrence wins)
    let mut commit_idx: HashMap<&str, usize> = HashMap::new();
    for (i, exp) in run.experiments.iter().enumerate() {
        commit_idx.entry(exp.commit.as_str()).or_insert(i);
    }

    // children[i] = list of child indices (experiments whose parent_commit == experiments[i].commit)
    let n = run.experiments.len();
    let mut children: Vec<Vec<usize>> = vec![vec![]; n];
    let mut has_known_parent = vec![false; n];

    for (i, exp) in run.experiments.iter().enumerate() {
        if let Some(ref pc) = exp.parent_commit
            && let Some(&pi) = commit_idx.get(pc.as_str())
            && pi != i
        {
            children[pi].push(i);
            has_known_parent[i] = true;
        }
    }

    // Root indices: experiments with no known parent in this run
    let root_indices: Vec<usize> = (0..n).filter(|&i| !has_known_parent[i]).collect();

    // Best-lineage set: walk parent chain from best up to root
    let best_commit = run.best().map(|e| e.commit.clone());
    let mut best_lineage: HashSet<String> = HashSet::new();
    if let Some(ref bc) = best_commit {
        let mut cur = bc.clone();
        let mut visited: HashSet<String> = HashSet::new();
        loop {
            if !visited.insert(cur.clone()) {
                break; // cycle
            }
            best_lineage.insert(cur.clone());
            // Find the experiment with this commit
            if let Some(&idx) = commit_idx.get(cur.as_str()) {
                match &run.experiments[idx].parent_commit {
                    Some(pc) => cur = pc.clone(),
                    None => break,
                }
            } else {
                break;
            }
        }
    }

    // Recursively build nodes with cycle detection
    fn build_node<'a>(
        idx: usize,
        exps: &'a [Experiment],
        children: &[Vec<usize>],
        best_lineage: &HashSet<String>,
        visited: &mut HashSet<usize>,
    ) -> TreeNode<'a> {
        let exp = &exps[idx];
        let on_best = best_lineage.contains(&exp.commit);
        let mut node = TreeNode {
            exp,
            children: vec![],
            on_best_lineage: on_best,
        };

        if !visited.insert(idx) {
            // Cycle — return node without children
            return node;
        }

        let mut child_indices = children[idx].clone();
        // Sort children by timestamp (stable; empty timestamps preserve array order)
        child_indices.sort_by(|&a, &b| exps[a].timestamp.cmp(&exps[b].timestamp));

        for ci in child_indices {
            node.children
                .push(build_node(ci, exps, children, best_lineage, visited));
        }

        visited.remove(&idx);
        node
    }

    let roots = root_indices
        .into_iter()
        .map(|ri| {
            let mut visited = HashSet::new();
            build_node(ri, &run.experiments, &children, &best_lineage, &mut visited)
        })
        .collect();

    Forest { roots, best_commit }
}

// ---------------------------------------------------------------------------
// Rendering helpers
// ---------------------------------------------------------------------------

fn render_table_node(
    node: &TreeNode<'_>,
    prefix: &str,
    is_last: bool,
    best_commit: &Option<String>,
    highlight_best: bool,
    buf: &mut String,
) {
    let connector = if is_last { "└── " } else { "├── " };
    let continuation = if is_last { "    " } else { "│   " };

    // Only include if highlight_best is off, or this node is on best lineage
    if highlight_best && !node.on_best_lineage {
        return;
    }

    let is_best = best_commit
        .as_deref()
        .map(|bc| bc == node.exp.commit)
        .unwrap_or(false);
    let star = if node.on_best_lineage { "★ " } else { "  " };
    let best_label = if is_best { "  (best)" } else { "" };

    let short_commit = if node.exp.commit.len() >= 7 {
        &node.exp.commit[..7]
    } else {
        &node.exp.commit
    };

    buf.push_str(&format!(
        "{}{}{:<8}  {:<7.4}  {:<8}  {}{}  {}{}\n",
        prefix,
        connector,
        short_commit,
        node.exp.val_bpb,
        node.exp.status.as_str(),
        star,
        node.exp.description,
        best_label,
        "",
    ));

    let child_prefix = format!("{}{}", prefix, continuation);
    let n = node.children.len();
    for (i, child) in node.children.iter().enumerate() {
        render_table_node(
            child,
            &child_prefix,
            i + 1 == n,
            best_commit,
            highlight_best,
            buf,
        );
    }
}

fn render_root_table(
    node: &TreeNode<'_>,
    best_commit: &Option<String>,
    highlight_best: bool,
    buf: &mut String,
) {
    if highlight_best && !node.on_best_lineage {
        return;
    }

    let is_best = best_commit
        .as_deref()
        .map(|bc| bc == node.exp.commit)
        .unwrap_or(false);
    let star = if node.on_best_lineage { "★ " } else { "  " };
    let best_label = if is_best { "  (best)" } else { "" };

    let short_commit = if node.exp.commit.len() >= 7 {
        &node.exp.commit[..7]
    } else {
        &node.exp.commit
    };

    buf.push_str(&format!(
        "{:<8}  {:<7.4}  {:<8}  {}{}  {}{}\n",
        short_commit,
        node.exp.val_bpb,
        node.exp.status.as_str(),
        star,
        node.exp.description,
        best_label,
        "",
    ));

    let n = node.children.len();
    for (i, child) in node.children.iter().enumerate() {
        render_table_node(child, "", i + 1 == n, best_commit, highlight_best, buf);
    }
}

// ---------------------------------------------------------------------------
// Public text-summary helper (for MCP tool)
// ---------------------------------------------------------------------------

pub fn tree_text(data_dir: &Path, tag: &str, highlight_best: bool) -> Result<String> {
    let run = load_run_or_suggest(data_dir, tag)?;
    let forest = build_forest(&run);
    let total = run.experiments.len();
    let n_roots = forest.roots.len();

    let mut buf = String::new();
    buf.push_str(&format!(
        "{tag}: {total} experiment(s), {n_roots} root(s)\n\n"
    ));

    for root in &forest.roots {
        render_root_table(root, &forest.best_commit, highlight_best, &mut buf);
    }

    Ok(buf)
}

// ---------------------------------------------------------------------------
// Public command entry point
// ---------------------------------------------------------------------------

pub fn cmd_tree(
    data_dir: &Path,
    tag: &str,
    highlight_best: bool,
    format: &OutputFormat,
) -> Result<()> {
    let run = load_run_or_suggest(data_dir, tag)?;
    let forest = build_forest(&run);

    match format {
        OutputFormat::Table => {
            let text = tree_text(data_dir, tag, highlight_best)?;
            print!("{text}");
        }
        OutputFormat::Json => {
            fn node_to_json(node: &TreeNode<'_>) -> serde_json::Value {
                let short_commit = if node.exp.commit.len() >= 7 {
                    &node.exp.commit[..7]
                } else {
                    &node.exp.commit
                };
                let children: Vec<_> = node.children.iter().map(node_to_json).collect();
                json!({
                    "commit": short_commit,
                    "val_bpb": node.exp.val_bpb,
                    "status": node.exp.status.as_str(),
                    "description": node.exp.description,
                    "is_on_best_lineage": node.on_best_lineage,
                    "children": children,
                })
            }

            let roots: Vec<_> = forest
                .roots
                .iter()
                .filter(|r| !highlight_best || r.on_best_lineage)
                .map(|r| node_to_json(r))
                .collect();

            let out = json!({ "tag": tag, "roots": roots });
            println!("{}", serde_json::to_string_pretty(&out)?);
        }
        OutputFormat::Tsv => {
            println!("depth\tcommit\tval_bpb\tstatus\tdescription\ton_best_lineage");

            fn print_tsv(node: &TreeNode<'_>, depth: usize, highlight_best: bool) {
                if highlight_best && !node.on_best_lineage {
                    return;
                }
                let short_commit = if node.exp.commit.len() >= 7 {
                    &node.exp.commit[..7]
                } else {
                    &node.exp.commit
                };
                println!(
                    "{}\t{}\t{:.6}\t{}\t{}\t{}",
                    depth,
                    short_commit,
                    node.exp.val_bpb,
                    node.exp.status.as_str(),
                    node.exp.description,
                    node.on_best_lineage,
                );
                for child in &node.children {
                    print_tsv(child, depth + 1, highlight_best);
                }
            }

            for root in &forest.roots {
                print_tsv(root, 0, highlight_best);
            }
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Experiment, RunLog, Status};
    use std::collections::HashMap;

    fn make_exp(commit: &str, val_bpb: f64, status: Status, parent: Option<&str>) -> Experiment {
        Experiment {
            commit: commit.to_string(),
            val_bpb,
            memory_gb: 0.0,
            status,
            description: String::new(),
            timestamp: String::new(),
            params: HashMap::new(),
            parent_commit: parent.map(|s| s.to_string()),
            crash_excerpt: None,
            metric_name: None,
            metric_direction: None,
            signals: Vec::new(),
        }
    }

    fn make_run(tag: &str, exps: Vec<Experiment>) -> RunLog {
        RunLog {
            run_tag: tag.to_string(),
            created_at: String::new(),
            experiments: exps,
            metric_name: None,
            metric_direction: None,
        }
    }

    #[test]
    fn roots_detected_by_missing_parent() {
        // A: no parent (root)
        // B: parent = A (child)
        // C: parent = "external" not in run (root)
        let run = make_run(
            "t",
            vec![
                make_exp("aaa", 0.99, Status::Keep, None),
                make_exp("bbb", 0.98, Status::Keep, Some("aaa")),
                make_exp("ccc", 0.97, Status::Keep, Some("zzz")),
            ],
        );
        let forest = build_forest(&run);
        assert_eq!(forest.roots.len(), 2, "expected 2 roots (aaa and ccc)");
    }

    #[test]
    fn cycle_detection_terminates() {
        // ccc is a root with no parent.
        // aaa -> parent bbb, bbb -> parent aaa  (mutual cycle).
        // The cycle means aaa and bbb both end up as roots (each others' parent is in-run
        // but forms a loop); build_forest must not stack-overflow.
        let run = make_run(
            "t",
            vec![
                make_exp("ccc", 0.97, Status::Keep, None), // standalone root
                make_exp("aaa", 0.99, Status::Keep, Some("bbb")), // cycle
                make_exp("bbb", 0.98, Status::Keep, Some("aaa")), // cycle
            ],
        );

        // This must not stack-overflow or panic
        let forest = build_forest(&run);
        assert!(!forest.roots.is_empty());
    }

    #[test]
    fn best_lineage_computed() {
        // root -> mid -> leaf (leaf is best with lowest val_bpb)
        let run = make_run(
            "t",
            vec![
                make_exp("root", 0.99, Status::Keep, None),
                make_exp("mid", 0.985, Status::Keep, Some("root")),
                make_exp("leaf", 0.98, Status::Keep, Some("mid")),
            ],
        );
        let forest = build_forest(&run);
        // Best is "leaf"; lineage should include root, mid, leaf
        assert_eq!(
            forest.best_commit.as_deref(),
            Some("leaf"),
            "best should be leaf"
        );

        // Walk the tree and collect on_best_lineage flags
        fn collect_lineage<'a>(node: &'a TreeNode<'a>, acc: &mut Vec<(String, bool)>) {
            acc.push((node.exp.commit.clone(), node.on_best_lineage));
            for child in &node.children {
                collect_lineage(child, acc);
            }
        }
        let mut flags = vec![];
        for root in &forest.roots {
            collect_lineage(root, &mut flags);
        }

        let on_lineage: Vec<&str> = flags
            .iter()
            .filter(|(_, f)| *f)
            .map(|(c, _)| c.as_str())
            .collect();

        assert!(on_lineage.contains(&"root"), "root must be on lineage");
        assert!(on_lineage.contains(&"mid"), "mid must be on lineage");
        assert!(on_lineage.contains(&"leaf"), "leaf must be on lineage");
    }
}
