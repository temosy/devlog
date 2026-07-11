use anyhow::Result;
use chrono::{DateTime, Utc};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// Activity extracted from one Claude Code session, restricted to entries
/// whose timestamp falls inside the report range.
#[derive(Debug)]
pub struct SessionActivity {
    pub title: Option<String>,
    pub cwd: PathBuf,
    pub git_branch: Option<String>,
    pub first_ts: DateTime<Utc>,
    pub last_ts: DateTime<Utc>,
    pub prompts: Vec<String>,
    pub files_touched: BTreeSet<String>,
    pub actions: Vec<String>,
}

impl SessionActivity {
    /// Human-readable project name derived from the working directory.
    pub fn project(&self) -> String {
        self.cwd
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| self.cwd.display().to_string())
    }

    /// A session with a title but no prompts/edits/actions in range is
    /// noise (e.g. an open idle window).
    pub fn is_empty(&self) -> bool {
        self.prompts.is_empty() && self.files_touched.is_empty() && self.actions.is_empty()
    }
}

const MAX_PROMPT_CHARS: usize = 300;
const MAX_PROMPTS_PER_SESSION: usize = 20;
const MAX_ACTIONS_PER_SESSION: usize = 30;

/// Prompts starting with these markers are harness noise, not user intent.
const PROMPT_NOISE_PREFIXES: &[&str] = &[
    "<local-command",
    "<command-name>",
    "<system-reminder>",
    "<task-notification",
    "Caveat:",
    "[Request interrupted",
    // Skill bodies injected by the harness when a skill is invoked.
    "Base directory for this skill",
    // Terminal output pasted into the prompt; the surrounding requests
    // and shell actions already describe that work.
    "➜",
];

/// Walk `<claude_projects_dir>/*/*.jsonl` and collect per-session activity
/// within [start, end).
pub fn collect(
    projects_dir: &Path,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> Result<Vec<SessionActivity>> {
    let mut sessions: BTreeMap<String, SessionActivity> = BTreeMap::new();

    let project_dirs = match std::fs::read_dir(projects_dir) {
        Ok(entries) => entries,
        Err(_) => return Ok(Vec::new()),
    };

    for project_dir in project_dirs.flatten() {
        if !project_dir.path().is_dir() {
            continue;
        }
        let Ok(files) = std::fs::read_dir(project_dir.path()) else {
            continue;
        };
        for file in files.flatten() {
            let path = file.path();
            if path.extension().is_none_or(|e| e != "jsonl") {
                continue;
            }
            // A file whose mtime predates the range start cannot contain
            // entries in range; skip without reading.
            if let Ok(meta) = file.metadata() {
                if let Ok(mtime) = meta.modified() {
                    if DateTime::<Utc>::from(mtime) < start {
                        continue;
                    }
                }
            }
            parse_file(&path, start, end, &mut sessions);
        }
    }

    let mut result: Vec<SessionActivity> = sessions
        .into_values()
        .filter(|s| !s.is_empty())
        .collect();
    result.sort_by_key(|s| s.first_ts);
    Ok(result)
}

fn parse_file(
    path: &Path,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    sessions: &mut BTreeMap<String, SessionActivity>,
) {
    let Ok(text) = std::fs::read_to_string(path) else {
        return;
    };
    for line in text.lines() {
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let Some(session_id) = value["sessionId"].as_str() else {
            continue;
        };

        // Session titles carry no timestamp filter: keep the latest one
        // even if it was assigned outside the range.
        if value["type"] == "ai-title" {
            if let (Some(session), Some(title)) =
                (sessions.get_mut(session_id), value["aiTitle"].as_str())
            {
                session.title = Some(title.to_string());
            }
            continue;
        }

        let entry_type = value["type"].as_str().unwrap_or("");
        if entry_type != "user" && entry_type != "assistant" {
            continue;
        }
        // Sidechain entries are subagent transcripts; the user-facing work
        // is already represented in the main chain.
        if value["isSidechain"] == true {
            continue;
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

        let session = sessions
            .entry(session_id.to_string())
            .or_insert_with(|| SessionActivity {
                title: None,
                cwd: PathBuf::from(value["cwd"].as_str().unwrap_or(".")),
                git_branch: value["gitBranch"].as_str().map(String::from),
                first_ts: ts,
                last_ts: ts,
                prompts: Vec::new(),
                files_touched: BTreeSet::new(),
                actions: Vec::new(),
            });
        session.first_ts = session.first_ts.min(ts);
        session.last_ts = session.last_ts.max(ts);

        match entry_type {
            "user" => {
                if let Some(prompt) = extract_prompt(&value) {
                    if session.prompts.len() < MAX_PROMPTS_PER_SESSION {
                        session.prompts.push(prompt);
                    }
                }
            }
            "assistant" => extract_tool_uses(&value, session),
            _ => {}
        }
    }
}

/// Extract a genuine user prompt; returns None for tool results and
/// harness-injected noise.
fn extract_prompt(value: &Value) -> Option<String> {
    let content = &value["message"]["content"];
    let text = if let Some(s) = content.as_str() {
        s.to_string()
    } else if let Some(items) = content.as_array() {
        items
            .iter()
            .filter(|i| i["type"] == "text")
            .filter_map(|i| i["text"].as_str())
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        return None;
    };
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    if PROMPT_NOISE_PREFIXES.iter().any(|p| text.starts_with(p)) {
        return None;
    }
    Some(truncate(text, MAX_PROMPT_CHARS))
}

/// Record file edits and shell actions from assistant tool_use blocks.
fn extract_tool_uses(value: &Value, session: &mut SessionActivity) {
    let Some(items) = value["message"]["content"].as_array() else {
        return;
    };
    for item in items {
        if item["type"] != "tool_use" {
            continue;
        }
        let name = item["name"].as_str().unwrap_or("");
        let input = &item["input"];
        match name {
            "Edit" | "Write" | "MultiEdit" | "NotebookEdit" => {
                if let Some(path) = input["file_path"].as_str() {
                    session.files_touched.insert(path.to_string());
                }
            }
            "Bash" => {
                let action = input["description"]
                    .as_str()
                    .map(String::from)
                    .or_else(|| input["command"].as_str().map(|c| truncate(c, 80)));
                if let Some(action) = action {
                    if session.actions.len() < MAX_ACTIONS_PER_SESSION {
                        session.actions.push(action);
                    }
                }
            }
            _ => {}
        }
    }
}

fn truncate(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max_chars).collect();
        format!("{truncated}…")
    }
}
