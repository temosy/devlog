use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// Directory containing Claude Code project transcripts.
    pub claude_projects_dir: PathBuf,
    /// Extra git repositories to scan, in addition to those
    /// auto-discovered from session working directories.
    pub repos: Vec<PathBuf>,
    /// Ollama base URL.
    pub ollama_url: String,
    /// Ollama model used for summarization.
    pub model: String,
    /// Report language: "ja" or "en".
    pub lang: String,
}

impl Default for Config {
    fn default() -> Self {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        Self {
            claude_projects_dir: home.join(".claude/projects"),
            repos: Vec::new(),
            ollama_url: "http://localhost:11434".to_string(),
            model: "qwen2.5:14b".to_string(),
            lang: "ja".to_string(),
        }
    }
}

impl Config {
    /// Load from ~/.config/devlog/config.toml, falling back to defaults
    /// when the file does not exist.
    pub fn load() -> Result<Self> {
        let path = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("devlog/config.toml");
        if !path.exists() {
            return Ok(Self::default());
        }
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let mut config: Config =
            toml::from_str(&text).with_context(|| format!("failed to parse {}", path.display()))?;
        config.repos = config.repos.into_iter().map(expand_tilde).collect();
        config.claude_projects_dir = expand_tilde(config.claude_projects_dir);
        Ok(config)
    }
}

fn expand_tilde(path: PathBuf) -> PathBuf {
    if let Ok(stripped) = path.strip_prefix("~") {
        if let Some(home) = dirs::home_dir() {
            return home.join(stripped);
        }
    }
    path
}
