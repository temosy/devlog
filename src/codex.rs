use crate::activity::{
    clean_prompt, truncate, SessionActivity, Source, MAX_PROMPTS_PER_SESSION,
};
use anyhow::Result;
use chrono::{DateTime, Utc};
use serde_json::Value;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// Collect Codex CLI sessions within [start, end).
///
/// Codex stores one rollout file per session under a date-nested tree:
/// `<sessions_dir>/YYYY/MM/DD/rollout-*.jsonl`.
pub fn collect(
    sessions_dir: &Path,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> Result<Vec<SessionActivity>> {
    let mut files = Vec::new();
    walk_jsonl(sessions_dir, start, &mut files);

    let mut result: Vec<SessionActivity> = files
        .iter()
        .filter_map(|path| parse_file(path, start, end))
        .filter(|s| !s.is_empty())
        .collect();
    result.sort_by_key(|s| s.first_ts);
    Ok(result)
}

/// Recursively gather `*.jsonl` files whose mtime is >= `start` (a file last
/// modified before the range cannot contain in-range entries).
fn walk_jsonl(dir: &Path, start: DateTime<Utc>, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk_jsonl(&path, start, out);
            continue;
        }
        if path.extension().is_none_or(|e| e != "jsonl") {
            continue;
        }
        if let Ok(meta) = entry.metadata() {
            if let Ok(mtime) = meta.modified() {
                if DateTime::<Utc>::from(mtime) < start {
                    continue;
                }
            }
        }
        out.push(path);
    }
}

/// Parse one rollout file into a single session, keeping only activity whose
/// timestamp lies in [start, end). Returns None if the file has no in-range
/// activity or can't be read.
fn parse_file(path: &Path, start: DateTime<Utc>, end: DateTime<Utc>) -> Option<SessionActivity> {
    let text = std::fs::read_to_string(path).ok()?;

    let mut cwd: Option<PathBuf> = None;
    let mut first_ts: Option<DateTime<Utc>> = None;
    let mut last_ts: Option<DateTime<Utc>> = None;
    let mut prompts: Vec<String> = Vec::new();
    let mut files_touched: BTreeSet<String> = BTreeSet::new();

    for line in text.lines() {
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let entry_type = value["type"].as_str().unwrap_or("");
        let payload = &value["payload"];

        // cwd comes from session metadata / per-turn context and carries no
        // per-entry timestamp; capture the first one we see regardless of range.
        if cwd.is_none() && (entry_type == "session_meta" || entry_type == "turn_context") {
            if let Some(dir) = payload["cwd"].as_str() {
                cwd = Some(PathBuf::from(dir));
            }
        }

        let Some(ts) = value["timestamp"]
            .as_str()
            .and_then(|t| DateTime::parse_from_rfc3339(t).ok())
            .map(|t| t.with_timezone(&Utc))
        else {
            continue;
        };
        if ts < start || ts >= end {
            continue;
        }

        if entry_type != "event_msg" {
            continue;
        }
        match payload["type"].as_str().unwrap_or("") {
            "user_message" => {
                if let Some(msg) = payload["message"].as_str() {
                    if let Some(prompt) = clean_prompt(msg) {
                        if prompts.len() < MAX_PROMPTS_PER_SESSION {
                            prompts.push(prompt);
                        }
                        touch_range(&mut first_ts, &mut last_ts, ts);
                    }
                }
            }
            "patch_apply_end" => {
                for file in parse_patched_files(payload["stdout"].as_str().unwrap_or("")) {
                    files_touched.insert(file);
                }
                touch_range(&mut first_ts, &mut last_ts, ts);
            }
            _ => {}
        }
    }

    let (first_ts, last_ts) = (first_ts?, last_ts?);
    // Codex has no session titles; use the first prompt's opening line as a
    // stand-in, collapsed to a single line so it fits a Markdown heading.
    let title = prompts.first().map(|p| {
        let first_line = p.lines().next().unwrap_or(p).trim();
        truncate(first_line, 60)
    });

    Some(SessionActivity {
        source: Source::Codex,
        title,
        cwd: cwd.unwrap_or_else(|| PathBuf::from(".")),
        git_branch: None,
        first_ts,
        last_ts,
        prompts,
        files_touched,
        // Codex tool calls are opaque JS via node_repl; edits are already
        // captured from patch output, so we record no separate shell actions.
        actions: Vec::new(),
    })
}

fn touch_range(
    first: &mut Option<DateTime<Utc>>,
    last: &mut Option<DateTime<Utc>>,
    ts: DateTime<Utc>,
) {
    *first = Some(first.map_or(ts, |f| f.min(ts)));
    *last = Some(last.map_or(ts, |l| l.max(ts)));
}

/// Extract absolute paths from `apply_patch` success output, whose lines look
/// like `M /abs/path`, `A /abs/path`, or `D /abs/path`.
fn parse_patched_files(stdout: &str) -> Vec<String> {
    stdout
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            let rest = line
                .strip_prefix("M ")
                .or_else(|| line.strip_prefix("A "))
                .or_else(|| line.strip_prefix("D "))?;
            let path = rest.trim();
            path.starts_with('/').then(|| path.to_string())
        })
        .collect()
}
