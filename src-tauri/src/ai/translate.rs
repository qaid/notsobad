//! Translate-on-open (issue #5).
//!
//! Detection and translation are deliberately TWO steps, not one combined
//! Ollama call. The original design asked the model itself to "translate, or
//! reply UNCHANGED if this is already English" in a single prompt — live
//! testing against the configured primary model
//! (`alibayram/erurollm-9b-instruct`) showed that prompt is unreliable: for
//! real French/Dutch email bodies it frequently answered UNCHANGED (i.e.
//! claimed already-English) instead of translating, and was non-deterministic
//! across identical repeated calls (UNCHANGED, UNCHANGED, then garbled mixed
//! output for the exact same input). A small quantized model given an easy
//! "just say UNCHANGED" escape hatch takes it rather than doing the harder
//! job. A plain "translate this to English" prompt (no UNCHANGED branch)
//! translated correctly and consistently in the same tests. So: detect
//! language with `whatlang` (deterministic, offline, no LLM call — see
//! Cargo.toml's comment on the dependency) and only call Ollama with a plain
//! translate prompt when the body is detected non-English. This also avoids
//! paying LLM latency on every English email open, the common case.
//!
//! Detection itself went through two more fixes after the first pass — see
//! `is_non_english`'s doc comment for the full reasoning and measured
//! numbers. Short version: whatlang's unconstrained ~90-language detector
//! let Afrikaans (a Dutch relative) steal enough trigram margin from short
//! genuine Dutch text to mark it "unreliable," silently skipping a needed
//! translation; scoping detection to `DETECTABLE_LANGS` (Eng/Nld/Fra, what
//! this owner actually receives per CLAUDE.md) fixed that. But the allowlist
//! alone isn't sufficient either — a plain "detected lang != Eng" call
//! without a reliability gate lets short/casual English land on Nld or Fra at
//! near-zero confidence and get mistranslated. Both the allowlist AND
//! `is_reliable()` are needed together.

use super::ollama::{self, OllamaError};
use super::Task;

/// Result of a translate call: which model actually answered (primary or
/// fallback), and the translated text. `None` means detection decided the
/// body was already English, so Ollama was never called.
pub struct TranslateOutcome {
    pub model_used: String,
    pub translated: Option<String>,
}

/// Strip markup down to plain text before language detection / the LLM
/// prompt. `mail-parser`'s own HTML decoder (already a dependency for #3's
/// sync path, so this adds no new crate) rather than a hand-rolled
/// tag-stripper — real HTML mail (nested tags, entities, style blocks) is
/// exactly what it already knows how to walk correctly. Two reasons this
/// matters, not just tidiness: whatlang's trigram detector is confused by
/// tag noise, and erurollm's 4096-token context window (see /api/tags) is
/// wasted on markup instead of the actual sentence.
fn plain_text(body: &str, is_html: bool) -> String {
    if is_html {
        mail_parser::decoders::html::html_to_text(body)
    } else {
        body.to_string()
    }
}

/// The three languages this app's owner actually receives (CLAUDE.md: "Dutch
/// and French most common"). Detection is scoped to exactly this allowlist
/// rather than whatlang's full ~90-language set — NOT for speed, but for
/// accuracy: live testing found whatlang's unconstrained detector confuses
/// short/informal English with Afrikaans (a close relative) at low
/// confidence, and that same Afrikaans candidate steals enough
/// trigram-frequency margin from genuine short Dutch text to flip its
/// `is_reliable()` bit to false. Removing Afrikaans (and every other
/// language not in scope) from consideration fixed both: an
/// Eng/Nld/Fra-only contest reliably calls short Dutch as Dutch and short
/// English as English. See translate.rs's module doc for the full story.
const DETECTABLE_LANGS: [whatlang::Lang; 3] = [whatlang::Lang::Eng, whatlang::Lang::Nld, whatlang::Lang::Fra];

/// True if `body` should be sent to the LLM for translation: detected (within
/// the allowlisted languages above) as non-English AND reliable.
///
/// Both the allowlist and `is_reliable()` are load-bearing together, not
/// redundant — each fixes a failure mode the other leaves open:
///   - Allowlist alone, no reliability gate: a short/casual English body can
///     land on Nld or Fra at near-zero confidence (measured as low as
///     0.0037) — pure noise, not signal — and get mistranslated as a false
///     positive.
///   - Reliability gate alone, no allowlist (the original design): whatlang's
///     full ~90-language contest lets Afrikaans (a close relative of Dutch)
///     steal enough trigram margin from genuine short Dutch text to flip
///     `is_reliable()` false, silently skipping a translation the user
///     needed — the worse direction to fail in.
///
/// Restricting the candidate set to the three languages this owner actually
/// receives (CLAUDE.md) removes Afrikaans from the contest, which makes
/// `is_reliable()` true for every genuine-foreign sample measured (Dutch,
/// short and long; French; HTML-stripped French) while every English sample
/// measured — including the near-zero-confidence Nld misdetect above — stays
/// unreliable. `is_reliable()` is whatlang's own margin-over-runner-up
/// computation, not a threshold picked here; a hand-picked confidence float
/// would be a magic number tuned to today's handful of samples and wrong on
/// the next borderline email.
///
/// Residual risk, accepted rather than engineered around: a genuine foreign
/// email that happens to land `is_reliable() == false` still gets skipped.
/// Every foreign sample tested so far (including short Dutch) came back
/// reliable, so this is a real but so-far-unobserved edge, not a known gap.
fn is_non_english(body: &str) -> bool {
    let detector = whatlang::Detector::with_allowlist(DETECTABLE_LANGS.to_vec());
    match detector.detect(body) {
        Some(info) => info.is_reliable() && info.lang() != whatlang::Lang::Eng,
        None => false,
    }
}

fn prompt_for(body: &str) -> String {
    format!("Translate the following text to English. Reply with ONLY the translation, no commentary or preamble:\n\n{body}")
}

/// Try the task's primary model, falling back to its configured fallback (if
/// any) ONLY when the primary isn't pulled. Any other error propagates
/// immediately — a fallback on a timeout/parse error would silently double
/// latency on transient failures instead of surfacing them.
///
/// `is_html` must reflect the stored `body_is_html` flag for this message
/// (mail-parser's own parse signal, not a guess) — HTML markup is stripped to
/// plain text before detection and before the prompt; see `plain_text`.
pub fn translate(task: Task, body: &str, is_html: bool) -> Result<TranslateOutcome, OllamaError> {
    let text = plain_text(body, is_html);
    if !is_non_english(&text) {
        return Ok(TranslateOutcome { model_used: String::new(), translated: None });
    }

    let prompt = prompt_for(&text);
    let primary = task.primary_model();
    match ollama::generate(primary, &prompt) {
        Ok(text) => Ok(finish(primary, text)),
        Err(OllamaError::ModelNotPulled { .. }) => {
            let Some(fallback) = task.fallback_model() else {
                return Err(OllamaError::ModelNotPulled { model: primary.to_string() });
            };
            let text = ollama::generate(fallback, &prompt)?;
            Ok(finish(fallback, text))
        }
        Err(e) => Err(e),
    }
}

fn finish(model: &str, text: String) -> TranslateOutcome {
    TranslateOutcome { model_used: model.to_string(), translated: Some(text.trim().to_string()) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translated_text_is_kept_and_trimmed() {
        let outcome = finish("m", "  Bonjour becomes Hello  ".to_string());
        assert_eq!(outcome.translated.as_deref(), Some("Bonjour becomes Hello"));
    }

    // Detection is deterministic and needs no Ollama call to test. These
    // guard the exact bug live testing found: French/Dutch email bodies must
    // be detected non-English (never silently skipped as "already English").
    #[test]
    fn detects_french_email_body_as_non_english() {
        assert!(is_non_english(
            "Bonjour Marie,\n\nJe vous ecris pour confirmer notre reunion de demain a 14h. \
             Merci de me faire savoir si cela vous convient toujours.\n\nCordialement,\nJean"
        ));
    }

    #[test]
    fn detects_dutch_email_body_as_non_english() {
        assert!(is_non_english(
            "Hallo Jan,\n\nIk schrijf u om onze vergadering van morgen om 14 uur te bevestigen.\n\n\
             Met vriendelijke groet,\nPiet"
        ));
    }

    /// Regression for the exact bug found in live testing: a SHORT Dutch
    /// snippet (no surrounding paragraphs) is the case where whatlang's
    /// unconstrained detector loses confidence to the Afrikaans candidate and
    /// flips `is_reliable()` false, which under the old
    /// `is_reliable() && lang != Eng` rule silently skipped translation. The
    /// allowlisted detector (`DETECTABLE_LANGS`) must still call this Dutch.
    #[test]
    fn detects_short_dutch_snippet_as_non_english() {
        assert!(is_non_english("Hallo Jan,\n\nIk schrijf u om onze vergadering van morgen om 14 uur te bevestigen."));
    }

    /// Regression for the mirror-image bug: whatlang's unconstrained detector
    /// misdetects short/informal English as Afrikaans. The allowlisted
    /// detector must still call this English (and thus skip translation).
    #[test]
    fn does_not_flag_short_informal_english_as_non_english() {
        assert!(!is_non_english("Hi Marie, just confirming our meeting tomorrow."));
    }

    /// Regression for the case that broke the allowlist-only rule: this exact
    /// English text lands on Nld at confidence 0.0037 under the Eng/Nld/Fra
    /// allowlist — pure noise, not signal. Only the `is_reliable()` gate (on
    /// top of the allowlist) catches this; an allowlist-only "lang != Eng"
    /// rule would mistranslate it.
    #[test]
    fn does_not_flag_english_email_with_near_zero_confidence_dutch_misdetect() {
        assert!(!is_non_english(
            "Hi Marie,\n\nJust confirming our meeting tomorrow at 2pm. Let me know if that still works.\n\nBest,\nJohn"
        ));
    }

    #[test]
    fn translate_skips_ollama_entirely_for_english_body() {
        // No Ollama server needs to be running for this: if is_non_english
        // returned true here, this test would hang/fail on the network call
        // instead of returning None immediately.
        let outcome =
            translate(Task::Translate, "Hi Marie, just confirming our meeting tomorrow.", false).unwrap();
        assert!(outcome.translated.is_none());
        assert_eq!(outcome.model_used, "");
    }

    #[test]
    fn plain_text_strips_html_markup_before_detection() {
        let html = "<html><body><p>Bonjour <b>Marie</b>,</p><p>Confirmation de reunion.</p></body></html>";
        let text = plain_text(html, true);
        assert!(!text.contains('<'), "tags must be stripped, got: {text}");
        assert!(text.contains("Bonjour"));
    }

    #[test]
    fn plain_text_leaves_plain_input_untouched() {
        assert_eq!(plain_text("Bonjour Marie", false), "Bonjour Marie");
    }

    #[test]
    fn translate_detects_non_english_html_body_after_stripping_tags() {
        // Regression for the HTML-body gap: without stripping, whatlang's
        // trigram detector sees tag noise ("html", "body", "p") mixed with
        // the French text and can misdetect the language, or the raw markup
        // (not the prose) is what would get sent to the LLM. This drives the
        // exact HTML-mail path through is_non_english via translate()'s
        // is_html branch, no live Ollama call needed since detection alone
        // decides whether to proceed.
        let html = "<html><body><p>Bonjour Marie,</p><p>Je vous ecris pour confirmer notre reunion \
                    de demain a 14h. Merci de me faire savoir si cela vous convient.</p></body></html>";
        assert!(is_non_english(&plain_text(html, true)), "HTML French body must still be detected non-English");
    }

    // Manual verification only, not part of the standard suite: hits the
    // real local Ollama server with the actually-configured primary model to
    // confirm the wiring end-to-end (detection + live translate + fallback).
    // Run with:
    //   cargo test --manifest-path src-tauri/Cargo.toml -- --ignored live_translate
    #[test]
    #[ignore]
    fn live_translate_against_real_ollama_french() {
        let outcome = translate(
            Task::Translate,
            "Bonjour Marie,\n\nJe vous ecris pour confirmer notre reunion de demain a 14h.",
            false,
        )
        .unwrap();
        println!("model_used={}, translated={:?}", outcome.model_used, outcome.translated);
        assert!(outcome.translated.is_some(), "French input must be translated, not skipped");
        assert!(
            outcome.translated.as_deref().unwrap().to_lowercase().contains("meeting")
                || outcome.translated.as_deref().unwrap().to_lowercase().contains("confirm"),
            "expected an actual English translation, got: {:?}",
            outcome.translated
        );
    }

    #[test]
    #[ignore]
    fn live_translate_against_real_ollama_dutch() {
        let outcome = translate(
            Task::Translate,
            "Hallo Jan,\n\nIk schrijf u om onze vergadering van morgen om 14 uur te bevestigen.",
            false,
        )
        .unwrap();
        println!("model_used={}, translated={:?}", outcome.model_used, outcome.translated);
        assert!(outcome.translated.is_some(), "Dutch input must be translated, not skipped");
    }

    #[test]
    #[ignore]
    fn live_translate_against_real_ollama_html_french() {
        let html = "<html><body><p>Bonjour Marie,</p><p>Je vous ecris pour confirmer notre reunion \
                    de demain a 14h.</p></body></html>";
        let outcome = translate(Task::Translate, html, true).unwrap();
        println!("model_used={}, translated={:?}", outcome.model_used, outcome.translated);
        assert!(outcome.translated.is_some(), "HTML French body must be translated, not skipped");
        let text = outcome.translated.unwrap();
        assert!(!text.contains('<'), "translated output must be plain text, got: {text}");
    }

    #[test]
    #[ignore]
    fn live_translate_against_real_ollama_english_passthrough() {
        let outcome =
            translate(Task::Translate, "Hi Marie, just confirming our meeting tomorrow at 2pm.", false)
                .unwrap();
        println!("model_used={}, translated={:?}", outcome.model_used, outcome.translated);
        assert!(outcome.translated.is_none(), "already-English input should skip the LLM call entirely");
    }
}
