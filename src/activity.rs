use chrono::{DateTime, Utc};
use std::collections::BTreeSet;
use std::path::PathBuf;

/// Which coding agent a session came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Claude,
    Codex,
}

impl Source {
    /// Short tag shown next to a session in the report.
    pub fn tag(self) -> &'static str {
        match self {
            Source::Claude => "Claude Code",
            Source::Codex => "Codex",
        }
    }
}

/// Activity extracted from one agent session, restricted to entries whose
/// timestamp falls inside the report range.
#[derive(Debug)]
pub struct SessionActivity {
    pub source: Source,
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

    /// A session with no prompts/edits/actions in range is noise (e.g. an
    /// idle window that was open but not used).
    pub fn is_empty(&self) -> bool {
        self.prompts.is_empty() && self.files_touched.is_empty() && self.actions.is_empty()
    }
}

pub const MAX_PROMPT_CHARS: usize = 300;
pub const MAX_PROMPTS_PER_SESSION: usize = 20;
pub const MAX_ACTIONS_PER_SESSION: usize = 30;

/// Prompts starting with these markers are harness noise, not user intent.
/// Covers both Claude Code and Codex injected content.
const PROMPT_NOISE_PREFIXES: &[&str] = &[
    // Claude Code
    "<local-command",
    "<command-name>",
    "<system-reminder>",
    "<task-notification",
    "Caveat:",
    "[Request interrupted",
    "Base directory for this skill",
    // Codex
    "# AGENTS.md instructions",
    "<user_instructions>",
    "<environment_context>",
    "<permissions instructions>",
    // Terminal output pasted into the prompt; the surrounding requests
    // and shell actions already describe that work.
    "➜",
];

/// Normalize a raw prompt string: trim, drop harness noise, cap length.
/// Returns None when the text is empty or pure noise.
pub fn clean_prompt(text: &str) -> Option<String> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    if PROMPT_NOISE_PREFIXES.iter().any(|p| text.starts_with(p)) {
        return None;
    }
    Some(truncate(text, MAX_PROMPT_CHARS))
}

pub fn truncate(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max_chars).collect();
        format!("{truncated}…")
    }
}
