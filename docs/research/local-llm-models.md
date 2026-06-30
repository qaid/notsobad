# Research: Local LLMs for the email app (Ollama, M1 Max 32GB)

Date: 2026-06-30
Method: deep-research workflow — 5 search angles, 23 sources fetched, 98 claims extracted,
25 verified by 3-vote adversarial check, 22 confirmed, 3 killed.
(Synthesis agent returned a stub; this report is hand-assembled from the verified claims.)

## Headline finding

**A purpose-built translation model beats every general chat model you have for NL/FR→EN.**
The strongest verified option for your #1 feature (Dutch/French → English) is a
small European-language-specialised model, not Qwen or Gemma chat.

## Translation quality (verified, primary-source benchmarks)

- **EuroLLM-9B-IT** — best WMT24++ COMET-22 among open <10B models tested:
  EN→XX **84.19**, XX→EN **83.94**; beats Gemma-2-9B-IT by >3 COMET points in both
  directions. Built explicitly for all 24 EU languages incl. **Dutch + French**.
  Source: arXiv 2506.04079 (EuroLLM-9B Technical Report). **Top pick for translation.**
- **EuroLLM-22B** — bigger sibling, "competitive" results; heavier, only worth it if 9B
  quality is short. Source: arXiv 2602.05879.
- **TranslateGemma** (4B/12B/27B, built on Gemma 3) — strong dedicated MT model from Google;
  pullable on Ollama. Source: arXiv 2601.09012 + Google blog.
- **GemmaX2-28** (9B) — outperforms similar-size open models on WMT-24/FLORES-200.
  Source: arXiv 2502.02481 (NAACL 2025).
- **aya-expanse** (Cohere, 8B/32B) — multilingual incl. Dutch/English/French; 8B=5.1GB,
  32B=20GB on Ollama. Good all-rounder if you want one model for translate + chat.
- **Killed claim**: "Gemma3 family beats Qwen for translation" — refuted as overreach
  (the XCOMET numbers are real, but the general conclusion didn't hold up).

Key verified insight: **parameter count is a poor predictor of translation quality** in the
20-32B range (arXiv 2605.31452). Bigger ≠ better here; the specialised 9B EuroLLM wins.

## Availability on Ollama (verified against ollama.com/library)

- `alibayram/erurollm-9b-instruct` — community upload (~5.6GB); use this tag to pull EuroLLM-9B-IT
- `translategemma` — pullable, 4B/12B/27B
- `aya-expanse` — pullable, 8B (5.1GB) + 32B (20GB)
- `gemma4` — pullable, e2b/e4b/12b/26b(MoE)/31b(dense)  ← you have e4b
- `qwen3.6` — pullable, 27B (17GB) + 35B (24GB)  ← newer than your qwen3.5

## Memory fit (M1 Max 32GB) — verified

- macOS defaults the GPU to ~2/3 of unified memory under 64GB → **~21.3GB usable**.
  (One voter flagged the exact 64GB boundary % as under-sourced, but the ~21GB figure for
  a 32GB Mac is corroborated and is the number that matters.)
- 32GB is "the sweet spot for multi-model workflows" — you can keep a small triage model
  + a mid translate/summarize model resident at once (~16GB total) with room to spare.
- Anything up to ~20GB (e.g. aya-expanse:32b at 20GB, qwen3.6:27b at 17GB) fits;
  35B/24GB is the practical ceiling and tight.

## Recommended per-task model mapping (revised from research)

| Task | Model | Why |
|---|---|---|
| **Translate (NL/FR→EN)** | **EuroLLM-9B-IT** (pull) | Verified best <10B for EU langs incl. Dutch+French; +3 COMET over Gemma-2-9B. |
| Translate (fallback) | `gemma4:e4b` (installed) or TranslateGemma-12B | Already installed / dedicated MT. |
| Triage on arrival (fast) | `qwen2.5:3b` (installed) | Tiny + fast; classification doesn't need a big model. |
| Summarize threads | `qwen3.5:9b` (installed) or `gemma4:e4b` | Capable general models; both fit. |
| Draft replies | `qwen3.5:9b` (installed) | Good instruction-following; keep in user's voice via prompt. |

**Action**: pull **EuroLLM-9B-IT** for translation. Keep everything else from the installed
set. Optionally pull `qwen3.6:27b` later as a quality bump for summarize/draft (newer than
your 3.5), but not required for v1.

## Caveat

The synthesis step of the workflow failed (returned a stub), so this is assembled directly
from the 22 verified claims rather than a polished synthesis. All claims here passed a
3-vote adversarial check against primary sources (arXiv papers + official Ollama pages).
