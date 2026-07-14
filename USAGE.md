# devlog Usage Guide

A CLI tool that generates daily worklogs from your Claude Code and Codex session transcripts and git logs.

Everything runs locally — your session content and code never leave your machine (the only network call is to your own Ollama endpoint).

日本語版: [USAGE.ja.md](USAGE.ja.md)

## How it works

1. Extracts the day's activity from your coding-agent transcripts — Claude Code (`~/.claude/projects/**/*.jsonl`) and Codex (`~/.codex/sessions/**/*.jsonl`): session titles, your actual requests, and files edited. Harness noise (tool results, injected skill bodies, subagent sidechains, AGENTS.md/environment injection) is filtered out automatically
2. Auto-discovers the git repositories you worked in — from session working directories *and* from the paths of files edited during sessions — and collects that day's commits
3. Summarizes everything into a per-project Markdown report using a local LLM via Ollama (default: `qwen2.5:14b`)

Each session in the report is tagged with its source (`[Claude Code]` / `[Codex]`), and work on the same repository from both agents is grouped under one project heading.

## Install

```sh
git clone https://github.com/temosy/devlog
cd devlog
cargo install --path .
```

Prerequisites:

- `git` on PATH
- A running [Ollama](https://ollama.com) with the model pulled (`ollama pull qwen2.5:14b`) — not needed if you only use `--no-llm`

## Basic commands

```sh
devlog today                        # today's worklog to stdout
devlog today --date 2026-07-10      # a specific day
devlog today --out log/2026-07-13.md    # write to a file (directories are created)
devlog today --dry-run              # inspect collected raw data only (no LLM call, fast)
devlog today --no-llm               # structured raw output without summarization
devlog today --lang en              # report language (default: ja)
```

Summarization can take a few minutes for a full day on a local 14B model. Use `--dry-run` when you just want a quick look at what was collected.

## Templates

```sh
devlog today --template daily      # daily worklog (default): bullets grouped by project
devlog today --template standup    # standup: done / in progress / blockers
devlog today --template weekly     # weekly: the 7 days ending on --date, highlights first
```

Weekly example (the week ending today): `devlog today --template weekly --out weekly/2026-W28.md`

## Daily routine (example)

Accumulate date-stamped files in a notes folder such as an Obsidian vault:

```sh
devlog today --out ~/vault/daily/$(date +%F).md
```

- Run once at the end of your workday
- Skim the output and fix any line that doesn't match reality

## Configuration (optional)

Works without any configuration. To override defaults, create `~/.config/devlog/config.toml`:

```toml
# All fields optional; defaults shown.
claude_projects_dir = "~/.claude/projects"
codex_sessions_dir = "~/.codex/sessions"
repos = []                            # repos to always scan, besides auto-discovered ones
ollama_url = "http://localhost:11434"
model = "qwen2.5:14b"
lang = "ja"
```

Repo auto-discovery only finds repositories you touched through an agent that day. List repos you also work on by hand in `repos` so their commits aren't missed. If either agent's directory doesn't exist, that source is simply skipped.

## Troubleshooting

- **`devlog: ollama not reachable ...` and raw output appears** → Ollama isn't running. Start `ollama serve`, or just use the raw output as-is
- **`no activity found`** → there really is no session/commit activity for that day, or the date is wrong. Check with `--dry-run`
- **Generation is slow** → switch `model` in `config.toml` to a smaller one (e.g. `qwen2.5:7b`, after `ollama pull`); summary quality will drop accordingly

## Known limitations

- Heading language ("プロジェクト:" vs "Project:") can vary between runs (LLM output jitter)
- When a session runs from a directory outside the repo it edits, work may be attributed to the session directory's name rather than the repo
- Codex sessions carry no title (the first prompt is used as a stand-in) and no per-session git branch; Codex shell actions are omitted since they run as opaque JS, though edited files are still captured
