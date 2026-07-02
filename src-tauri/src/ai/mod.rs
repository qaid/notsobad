//! Local AI layer (issue #5). Every call in this module goes to Ollama's HTTP
//! API on localhost — no cloud LLM, ever (CLAUDE.md). This module owns the
//! task -> model mapping so a model id is never hardcoded inline at a call
//! site; see `Task::primary_model`/`fallback_model`.

pub mod ollama;
pub mod translate;

/// An AI task this app performs. Each task maps to a primary model (and, for
/// translate, a fallback) — see CLAUDE.md's "AI task -> model mapping" table.
/// Centralizing the mapping here means a model id is a `match` arm in exactly
/// one place, not a string literal copy-pasted at every call site.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Task {
    Translate,
    // Triage and Summarize/Draft are #4/#6 scope; add their model mappings
    // here when those tasks land rather than hardcoding elsewhere.
}

impl Task {
    /// The model tried first for this task.
    pub fn primary_model(&self) -> &'static str {
        match self {
            Task::Translate => "alibayram/erurollm-9b-instruct",
        }
    }

    /// Fallback model if the primary isn't pulled. `None` = no fallback.
    pub fn fallback_model(&self) -> Option<&'static str> {
        match self {
            Task::Translate => Some("gemma4:e4b"),
        }
    }
}
