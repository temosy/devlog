use anyhow::{bail, Context, Result};
use serde_json::json;
use std::time::Duration;

pub struct Ollama {
    agent: ureq::Agent,
    base_url: String,
    model: String,
}

impl Ollama {
    pub fn new(base_url: &str, model: &str) -> Self {
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_secs(5))
            // Local 14B models can take minutes on a long day's data.
            .timeout(Duration::from_secs(600))
            .build();
        Self {
            agent,
            base_url: base_url.trim_end_matches('/').to_string(),
            model: model.to_string(),
        }
    }

    /// Quick reachability check so we can fall back to raw output with a
    /// clear message instead of a slow failure mid-generation.
    pub fn is_available(&self) -> bool {
        self.agent
            .get(&format!("{}/api/tags", self.base_url))
            .timeout(Duration::from_secs(3))
            .call()
            .is_ok()
    }

    pub fn generate(&self, prompt: &str) -> Result<String> {
        let response = self
            .agent
            .post(&format!("{}/api/chat", self.base_url))
            .send_json(json!({
                "model": self.model,
                "messages": [{"role": "user", "content": prompt}],
                "stream": false,
                "options": {"temperature": 0.3}
            }))
            .with_context(|| format!("ollama request to {} failed", self.base_url))?;
        let body: serde_json::Value = response.into_json().context("invalid ollama response")?;
        match body["message"]["content"].as_str() {
            Some(content) if !content.trim().is_empty() => Ok(strip_think_tags(content)),
            _ => bail!("ollama returned an empty response"),
        }
    }
}

/// Some models wrap chain-of-thought in <think>...</think>; drop it.
fn strip_think_tags(text: &str) -> String {
    let mut result = text.to_string();
    while let (Some(open), Some(close)) = (result.find("<think>"), result.find("</think>")) {
        if close > open {
            result.replace_range(open..close + "</think>".len(), "");
        } else {
            break;
        }
    }
    result.trim().to_string()
}
