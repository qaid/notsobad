//! Blocking HTTP client for the local Ollama API (issue #5).
//!
//! SAFETY (CLAUDE.md): this ONLY ever talks to `OLLAMA_BASE_URL`
//! (localhost). No mail content leaves the machine — do not point this at
//! anything else, ever.
//!
//! Blocking (`ureq`), matching this codebase's IMAP/SMTP style: callers run
//! this inside `tauri::async_runtime::spawn_blocking`, not directly in an
//! async fn (see commands::translate_message).

use serde::{Deserialize, Serialize};
use std::time::Duration;

const OLLAMA_BASE_URL: &str = "http://localhost:11434";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(120); // local LLM inference can be slow

/// Distinct from a generic transport/parse failure so callers can offer a
/// "pull this model" action instead of a dead-end error message (issue #5's
/// explicit constraint: don't swallow a 404 model-not-found into a generic
/// error).
#[derive(Debug)]
pub enum OllamaError {
    /// Ollama returned 404 with a "model not found"-shaped body for this model id.
    ModelNotPulled { model: String },
    /// Anything else: connection refused, timeout, non-404 HTTP error, bad JSON.
    Other(String),
}

impl std::fmt::Display for OllamaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OllamaError::ModelNotPulled { model } => {
                write!(f, "model '{model}' is not pulled; run: ollama pull {model}")
            }
            OllamaError::Other(msg) => write!(f, "{msg}"),
        }
    }
}

#[derive(Debug, Serialize)]
struct GenerateRequest<'a> {
    model: &'a str,
    prompt: &'a str,
    stream: bool,
}

#[derive(Debug, Deserialize)]
struct GenerateResponse {
    response: String,
}

#[derive(Debug, Deserialize)]
struct ErrorBody {
    error: String,
}

/// POST /api/generate with a single-turn prompt against `model`. Returns the
/// model's full text response (non-streaming: `stream: false`, so ureq gets
/// back one JSON object, not an NDJSON stream).
pub fn generate(model: &str, prompt: &str) -> Result<String, OllamaError> {
    let url = format!("{OLLAMA_BASE_URL}/api/generate");
    let body = GenerateRequest { model, prompt, stream: false };

    let agent = ureq::AgentBuilder::new().timeout(REQUEST_TIMEOUT).build();
    match agent.post(&url).send_json(&body) {
        Ok(resp) => resp
            .into_json::<GenerateResponse>()
            .map(|r| r.response)
            .map_err(|e| OllamaError::Other(format!("failed to parse Ollama response: {e}"))),
        Err(ureq::Error::Status(404, resp)) => {
            let text = resp.into_string().unwrap_or_default();
            let msg = serde_json::from_str::<ErrorBody>(&text).map(|b| b.error).unwrap_or(text);
            // Ollama's 404 body for an unpulled model is `{"error": "model
            // '<name>' not found..."}`. Any other 404 (unexpected, but don't
            // assume) falls back to a generic error instead of mislabeling it.
            if msg.contains("not found") {
                Err(OllamaError::ModelNotPulled { model: model.to_string() })
            } else {
                Err(OllamaError::Other(msg))
            }
        }
        Err(ureq::Error::Status(code, resp)) => {
            let text = resp.into_string().unwrap_or_default();
            Err(OllamaError::Other(format!("Ollama returned HTTP {code}: {text}")))
        }
        Err(e) => Err(OllamaError::Other(format!("request to Ollama failed (is it running at {OLLAMA_BASE_URL}?): {e}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_not_pulled_display_includes_pull_command() {
        let e = OllamaError::ModelNotPulled { model: "some/model".to_string() };
        assert_eq!(e.to_string(), "model 'some/model' is not pulled; run: ollama pull some/model");
    }
}
