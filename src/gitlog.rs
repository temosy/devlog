use chrono::{DateTime, Utc};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug)]
pub struct Commit {
    pub repo: String,
    pub sha: String,
    pub author: String,
    pub ts: DateTime<Utc>,
    pub message: String,
}

/// Resolve session cwds and configured repos into a deduplicated set of
/// git worktree roots.
pub fn discover_repos(cwds: &[PathBuf], config_repos: &[PathBuf]) -> Vec<PathBuf> {
    let mut roots = BTreeSet::new();
    for path in cwds.iter().chain(config_repos.iter()) {
        if let Some(root) = git_toplevel(path) {
            roots.insert(root);
        }
    }
    roots.into_iter().collect()
}

fn git_toplevel(path: &Path) -> Option<PathBuf> {
    if !path.is_dir() {
        return None;
    }
    let output = Command::new("git")
        .arg("-C")
        .arg(path)
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let root = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if root.is_empty() {
        None
    } else {
        Some(PathBuf::from(root))
    }
}

/// Collect commits across all repos within [start, end), newest last.
pub fn collect(repos: &[PathBuf], start: DateTime<Utc>, end: DateTime<Utc>) -> Vec<Commit> {
    let mut commits = Vec::new();
    for repo in repos {
        let repo_name = repo
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| repo.display().to_string());
        let output = Command::new("git")
            .arg("-C")
            .arg(repo)
            .args([
                "log",
                "--all",
                "--no-merges",
                &format!("--since={}", start.to_rfc3339()),
                &format!("--until={}", end.to_rfc3339()),
                "--pretty=format:%h%x09%an%x09%aI%x09%s",
            ])
            .output();
        let Ok(output) = output else { continue };
        if !output.status.success() {
            continue;
        }
        for line in String::from_utf8_lossy(&output.stdout).lines() {
            let parts: Vec<&str> = line.splitn(4, '\t').collect();
            if parts.len() != 4 {
                continue;
            }
            let Ok(ts) = DateTime::parse_from_rfc3339(parts[2]) else {
                continue;
            };
            let ts = ts.with_timezone(&Utc);
            // git --since/--until filter by commit date; author date can
            // drift outside the range (e.g. rebases), so re-check here.
            if ts < start || ts >= end {
                continue;
            }
            commits.push(Commit {
                repo: repo_name.clone(),
                sha: parts[0].to_string(),
                author: parts[1].to_string(),
                ts,
                message: parts[3].to_string(),
            });
        }
    }
    commits.sort_by_key(|c| c.ts);
    commits
}
