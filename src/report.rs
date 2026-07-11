use crate::gitlog::Commit;
use crate::transcript::SessionActivity;
use chrono::{DateTime, Local, NaiveDate, Utc};
use clap::ValueEnum;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Template {
    /// Daily worklog grouped by project
    Daily,
    /// Standup-style: done / in progress / blockers
    Standup,
    /// Weekly summary with highlights
    Weekly,
}

pub struct ReportInput<'a> {
    pub date: NaiveDate,
    pub end_date: NaiveDate,
    pub sessions: &'a [SessionActivity],
    pub commits: &'a [Commit],
    pub template: Template,
    pub lang: &'a str,
}

impl ReportInput<'_> {
    fn date_label(&self) -> String {
        if self.date == self.end_date {
            self.date.format("%Y-%m-%d").to_string()
        } else {
            format!(
                "{} – {}",
                self.date.format("%Y-%m-%d"),
                self.end_date.format("%Y-%m-%d")
            )
        }
    }

    fn title(&self) -> String {
        let label = self.date_label();
        let ja = self.lang == "ja";
        match self.template {
            Template::Daily => {
                if ja {
                    format!("# 日報 {label}")
                } else {
                    format!("# Worklog {label}")
                }
            }
            Template::Standup => {
                if ja {
                    format!("# スタンドアップ {label}")
                } else {
                    format!("# Standup {label}")
                }
            }
            Template::Weekly => {
                if ja {
                    format!("# 週報 {label}")
                } else {
                    format!("# Weekly report {label}")
                }
            }
        }
    }
}

/// Serialize collected activity into a compact plain-text block that the
/// LLM (or a human reading the raw fallback) can consume.
pub fn render_raw_data(input: &ReportInput) -> String {
    let mut by_project: BTreeMap<String, Vec<&SessionActivity>> = BTreeMap::new();
    for session in input.sessions {
        by_project.entry(session.project()).or_default().push(session);
    }

    let mut out = String::new();
    for (project, sessions) in &by_project {
        out.push_str(&format!("## Project: {project}\n"));
        for s in sessions {
            let branch = s.git_branch.as_deref().unwrap_or("-");
            out.push_str(&format!(
                "### Session: {} ({}–{}, branch: {branch})\n",
                s.title.as_deref().unwrap_or("(untitled)"),
                fmt_local(s.first_ts),
                fmt_local(s.last_ts),
            ));
            if !s.prompts.is_empty() {
                out.push_str("User requests:\n");
                for p in &s.prompts {
                    out.push_str(&format!("- {}\n", p.replace('\n', " ")));
                }
            }
            if !s.files_touched.is_empty() {
                let files: Vec<&str> = s.files_touched.iter().map(String::as_str).collect();
                out.push_str(&format!("Files edited: {}\n", files.join(", ")));
            }
            if !s.actions.is_empty() {
                out.push_str("Shell actions:\n");
                for a in &s.actions {
                    out.push_str(&format!("- {a}\n"));
                }
            }
        }
        out.push('\n');
    }

    if !input.commits.is_empty() {
        out.push_str("## Git commits\n");
        for c in input.commits {
            out.push_str(&format!(
                "- [{}] {} {} ({}, {})\n",
                c.repo,
                c.sha,
                c.message,
                c.author,
                fmt_local_date(c.ts),
            ));
        }
    }
    out
}

/// The prompt sent to the local LLM.
pub fn build_llm_prompt(input: &ReportInput, raw_data: &str) -> String {
    let lang_line = if input.lang == "ja" {
        "日本語で書いてください。"
    } else {
        "Write in English."
    };
    let template_instructions = match (input.template, input.lang == "ja") {
        (Template::Daily, true) => {
            "プロジェクトごとに `## プロジェクト名` の見出しを立て、実際に行った作業を箇条書きでまとめてください。"
        }
        (Template::Daily, false) => {
            "Group by project with `## project-name` headings and summarize the actual work as bullet points."
        }
        (Template::Standup, true) => {
            "`## やったこと` `## 進行中` `## ブロッカー` の3見出しで書いてください。ブロッカーはデータから読み取れる場合のみ書き、なければ「なし」としてください。"
        }
        (Template::Standup, false) => {
            "Use three headings: `## Done`, `## In progress`, `## Blockers`. Only list blockers evident from the data; otherwise write \"None\"."
        }
        (Template::Weekly, true) => {
            "`## ハイライト` に主要な成果を3〜5点、続けてプロジェクトごとの `## プロジェクト名` 見出しで作業内容をまとめてください。"
        }
        (Template::Weekly, false) => {
            "Start with `## Highlights` (3-5 key outcomes), then summarize per project under `## project-name` headings."
        }
    };
    format!(
        "You are an assistant that writes a developer's worklog from raw activity data.\n\
        Below is raw data collected from the developer's coding-agent sessions and git commits for {date}.\n\
        \n\
        Rules:\n\
        - {lang_line}\n\
        - {template_instructions}\n\
        - Describe only work supported by the data. Never invent tasks.\n\
        - One line per item, concise, past tense.\n\
        - Merge duplicate or trivially related items (e.g. a session and its commits) into one line.\n\
        - Skip pure noise (idle sessions, version checks).\n\
        - Output Markdown body only: no top-level title, no preamble, no code fences around the whole output.\n\
        \n\
        Raw data:\n\
        ---\n\
        {raw_data}\n\
        ---\n",
        date = input.date_label(),
    )
}

/// Assemble the final report with title and footer around a body.
pub fn render_report(input: &ReportInput, body: &str) -> String {
    format!(
        "{}\n\n{}\n\n---\n_Generated by devlog from {} session(s), {} commit(s)._\n",
        input.title(),
        body.trim(),
        input.sessions.len(),
        input.commits.len(),
    )
}

/// Raw fallback report used with --no-llm or when Ollama is unreachable.
pub fn render_fallback(input: &ReportInput) -> String {
    render_report(input, &render_raw_data(input))
}

fn fmt_local(ts: DateTime<Utc>) -> String {
    ts.with_timezone(&Local).format("%H:%M").to_string()
}

fn fmt_local_date(ts: DateTime<Utc>) -> String {
    ts.with_timezone(&Local).format("%m-%d %H:%M").to_string()
}
