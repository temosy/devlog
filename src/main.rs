mod activity;
mod codex;
mod config;
mod gitlog;
mod report;
mod summarize;
mod transcript;

use anyhow::{Context, Result};
use chrono::{Duration, Local, NaiveDate, TimeZone, Utc};
use clap::{Parser, Subcommand};
use config::Config;
use report::{ReportInput, Template};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "devlog",
    version,
    about = "Generate worklogs from Claude Code sessions and git logs"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Generate a worklog for one day (default: today)
    Today(GenerateArgs),
}

#[derive(clap::Args)]
struct GenerateArgs {
    /// Target date (YYYY-MM-DD); defaults to today
    #[arg(long)]
    date: Option<NaiveDate>,
    /// Show what would be collected without generating a report
    #[arg(long)]
    dry_run: bool,
    /// Report template
    #[arg(long, value_enum, default_value_t = Template::Daily)]
    template: Template,
    /// Write the report to a file instead of stdout
    #[arg(long)]
    out: Option<PathBuf>,
    /// Skip LLM summarization and output the raw activity list
    #[arg(long)]
    no_llm: bool,
    /// Report language (ja/en); overrides config
    #[arg(long)]
    lang: Option<String>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Today(args) => generate(args),
    }
}

fn generate(args: GenerateArgs) -> Result<()> {
    let config = Config::load()?;
    let lang = args.lang.unwrap_or_else(|| config.lang.clone());

    let end_date = args.date.unwrap_or_else(|| Local::now().date_naive());
    // Weekly reports cover the 7 days ending on the target date.
    let start_date = match args.template {
        Template::Weekly => end_date - Duration::days(6),
        _ => end_date,
    };
    let start = local_midnight(start_date);
    let end = local_midnight(end_date + Duration::days(1));

    let mut sessions = transcript::collect(&config.claude_projects_dir, start, end)?;
    sessions.extend(codex::collect(&config.codex_sessions_dir, start, end)?);
    sessions.sort_by_key(|s| s.first_ts);
    // Candidate repo locations: session working directories plus the
    // directories of files edited during sessions (work often targets a
    // repo outside the session cwd).
    let mut candidates: std::collections::BTreeSet<PathBuf> =
        sessions.iter().map(|s| s.cwd.clone()).collect();
    for session in &sessions {
        for file in &session.files_touched {
            if let Some(dir) = std::path::Path::new(file).parent() {
                candidates.insert(dir.to_path_buf());
            }
        }
    }
    let candidates: Vec<PathBuf> = candidates.into_iter().collect();
    let repos = gitlog::discover_repos(&candidates, &config.repos);
    let commits = gitlog::collect(&repos, start, end);

    let input = ReportInput {
        date: start_date,
        end_date,
        sessions: &sessions,
        commits: &commits,
        template: args.template,
        lang: &lang,
    };

    if args.dry_run {
        print_dry_run(&input, &repos);
        return Ok(());
    }

    if sessions.is_empty() && commits.is_empty() {
        eprintln!("devlog: no activity found for {}", input.date);
        return Ok(());
    }

    let report = if args.no_llm {
        report::render_fallback(&input)
    } else {
        let ollama = summarize::Ollama::new(&config.ollama_url, &config.model);
        if !ollama.is_available() {
            eprintln!(
                "devlog: ollama not reachable at {}; falling back to raw output (use --no-llm to silence)",
                config.ollama_url
            );
            report::render_fallback(&input)
        } else {
            let raw_data = report::render_raw_data(&input);
            let prompt = report::build_llm_prompt(&input, &raw_data);
            let body = ollama
                .generate(&prompt)
                .context("LLM summarization failed")?;
            report::render_report(&input, &body)
        }
    };

    match args.out {
        Some(path) => {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&path, &report)
                .with_context(|| format!("failed to write {}", path.display()))?;
            eprintln!("devlog: wrote {}", path.display());
        }
        None => print!("{report}"),
    }
    Ok(())
}

fn print_dry_run(input: &ReportInput, repos: &[PathBuf]) {
    eprintln!(
        "devlog dry-run: {} — {} session(s), {} repo(s), {} commit(s)\n",
        input.date,
        input.sessions.len(),
        repos.len(),
        input.commits.len()
    );
    println!("{}", report::render_raw_data(input));
}

fn local_midnight(date: NaiveDate) -> chrono::DateTime<Utc> {
    let naive = date.and_hms_opt(0, 0, 0).unwrap();
    Local
        .from_local_datetime(&naive)
        .single()
        .unwrap_or_else(|| Utc.from_utc_datetime(&naive).into())
        .with_timezone(&Utc)
}
