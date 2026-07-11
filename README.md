# devlog

Generate daily worklogs from your Claude Code session transcripts and git logs — fully local, powered by Ollama.

If you spend your day driving coding agents, the record of what you did already exists: the agent's session transcripts and your git history. `devlog` reads both and writes your daily report so you don't have to.

## How it works

1. Parses Claude Code transcripts (`~/.claude/projects/**/*.jsonl`) for the target day: session titles, your actual requests, files edited, shell actions. Harness noise (tool results, injected skill bodies, subagent sidechains) is filtered out.
2. Auto-discovers the git repositories you worked in — from session working directories *and* from the files edited during sessions — and collects that day's commits.
3. Sends the collected activity to a local LLM via Ollama (default: `qwen2.5:14b`) to produce a concise, grouped-by-project Markdown report. Nothing leaves your machine.

## Usage

```sh
devlog today                       # today's worklog to stdout
devlog today --date 2026-07-10     # a specific day
devlog today --template standup    # done / in progress / blockers
devlog today --template weekly     # the 7 days ending on --date
devlog today --out ~/vault/daily/2026-07-12.md   # write into e.g. an Obsidian vault
devlog today --dry-run             # inspect collected raw data, no LLM call
devlog today --no-llm              # raw structured output without summarization
devlog today --lang en             # report language (default: ja)
```

If Ollama is unreachable, `devlog` falls back to raw structured output instead of failing.

## Configuration

Optional, at `~/.config/devlog/config.toml`:

```toml
# All fields optional; defaults shown.
claude_projects_dir = "~/.claude/projects"
repos = []                                # extra repos beyond auto-discovered ones
ollama_url = "http://localhost:11434"
model = "qwen2.5:14b"
lang = "ja"
```

## Install

```sh
cargo install --path .
```

Requires `git` on PATH and a running [Ollama](https://ollama.com) with the configured model pulled (`ollama pull qwen2.5:14b`), unless you use `--no-llm`.

## Privacy

Session transcripts contain your code and prompts. `devlog` processes everything locally and its only network call is to your own Ollama endpoint.

## License

MIT
