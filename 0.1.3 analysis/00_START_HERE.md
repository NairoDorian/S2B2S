# S2B2S — Review & Roadmap (START HERE)

> Reviewed at commit `1332d3c` (**v0.1.3**). Tauri 2 app, Rust backend (~105 files / ~31.8k LOC) + React/TS frontend (~108 components / ~17k LOC). Forked from Handy (MIT).

This folder is **5 short, linked files** instead of one 2,000-line monster. Read them in order, or jump to what you need.

| File                                                         | What it answers                                                              |
| ------------------------------------------------------------ | ---------------------------------------------------------------------------- |
| **00_START_HERE.md** (this)                                  | The 5 headline findings + the "if you only do 5 things" list                 |
| **[01_STATE_OF_THE_PROJECT.md](01_STATE_OF_THE_PROJECT.md)** | What's _actually_ done, what's half-done, what's claimed-done-but-isn't      |
| **[02_ROADMAP_IN_ORDER.md](02_ROADMAP_IN_ORDER.md)**         | Where to go, in phases — the ordered plan                                    |
| **[03_CLEANUP_KILL_LIST.md](03_CLEANUP_KILL_LIST.md)**       | Exact files/dirs to delete, merge, or keep (fixes the "getting lost")        |
| **[04_CODE_FINDINGS.md](04_CODE_FINDINGS.md)**               | Code-level issues with `file:line` evidence (god files, panics, i18n, stubs) |

---

## The honest one-paragraph summary

S2B2S is **further along than an "early alpha" usually is** — the core STT → Brain → TTS loop is real and reasonably architected, there are **206 Rust test annotations**, 9 CI workflows, and a clean backend manager pattern. But the project is **drowning in its own documentation and ambition**. There are **66 markdown files (~24,500 lines)**, multiple competing roadmaps, an entire folder reviewing _other people's_ projects, and a README that marks nearly everything "✅ Complete" when several of those items are stubs, partials, or ~72% done. **You're not lost because the code is bad — you're lost because nothing tells you the truth about what's finished, and there are six documents all claiming to be the plan.**

---

## The 5 headline findings

**1. Documentation sprawl is the real bug.**
66 MD files, ~24,500 lines. A 1,857-line review, a 1,785-line "improvements" doc, a 767-line changelog, and a ~1 MB `references_comparative_analysis_md/` folder reviewing 26 _other_ projects. There are **at least 6 overlapping "what to do next" documents**. No single source of truth. → see [03](03_CLEANUP_KILL_LIST.md).

**2. The README overclaims "✅ Complete."**
The roadmap table marks ~40 items complete. Verified against code, several are not:

- **Native wgpu overlay** = a pure stub that only logs a line and returns `Ok` (`overlay_fx/native/mod.rs`). README itself calls this "🚧 Placeholder" two rows below calling the overlay system "Complete."
- **20-language i18n** = English has **663** translation keys; **all 19 other languages have exactly 477** — frozen, ~28% untranslated.
- **Streaming STT** = honestly marked "Partial," which is correct, but it sits in a table of green checkmarks so it reads as done.
  → see [01](01_STATE_OF_THE_PROJECT.md).

**3. "Local-first" quietly depends on Python.**
5 of the local TTS backends (Piper, Kokoro, Kitten, Pocket) shell out to a **Python venv** with pip-installed packages. The README sells "download and run, works offline," but first-run actually needs Python 3.8+, a venv setup script, and network installs. This is your single biggest fragility and support-burden risk. → see [04](04_CODE_FINDINGS.md#python-venv).

**4. A handful of god-files concentrate the risk.**
`managers/model.rs` (2,233 lines), `settings.rs` (2,185), `shortcut/mod.rs` (1,513), `actions.rs` (1,391), `clipboard.rs` (1,167). These are where bugs hide and where you'll feel "lost" inside the code. → see [04](04_CODE_FINDINGS.md#god-files).

**5. 292 `.unwrap()`/`.expect()` calls outside tests.**
Each is a potential panic/crash in a desktop app users keep open all day. Not all are dangerous, but the hot paths (audio, clipboard, server spawn) need an audit. → see [04](04_CODE_FINDINGS.md#panics).

---

## If you only do 5 things this month

1. **Declare one source of truth.** Adopt this `REVIEW/` set (or fold it into a single `STATUS.md`) and **delete or archive the 6 competing roadmaps**. Stop maintaining the README "Complete" table by hand. → [03](03_CLEANUP_KILL_LIST.md)
2. **Make the README honest.** Re-label every overclaimed row as 🚧/📋 with one line of truth. Trust returns when the docs stop lying. → [01](01_STATE_OF_THE_PROJECT.md#claimed-but-not-done)
3. **Decide the Python question.** Either (a) commit to bundling a runtime so it's truly one-click, or (b) make Piper-via-Rust (or SAPI on Windows) the _real_ zero-dependency default and clearly label the rest as "requires setup." Don't market local-first while shipping a venv. → [04](04_CODE_FINDINGS.md#python-venv)
4. **Move scratch out of the repo.** Delete/relocate `temp_export_onnx/`, `gemma_4_qat_mtp_e2b/`, `analysys/`, and the committed repomix snapshots. → [03](03_CLEANUP_KILL_LIST.md#scratch)
5. **Fix or formally shelve the partials** (native overlay, streaming STT, i18n sync) instead of leaving them as green checkmarks. A shelved feature behind a flag is fine; a "done" feature that's a stub is not. → [02](02_ROADMAP_IN_ORDER.md#phase-2)

---

## How to read the status labels in these docs

| Label               | Meaning                                                             |
| ------------------- | ------------------------------------------------------------------- |
| ✅ **Done**         | Verified in code, wired end-to-end, looks production-shaped         |
| 🟡 **Partial**      | Real code exists but has gaps, missing paths, or isn't fully wired  |
| 🔴 **Claimed/Stub** | Marked done somewhere in the repo, but the code is a stub or absent |
| 📋 **Planned**      | Not started; lives only in plans/docs                               |

Everything below is judged against the **code at `1332d3c`**, not against the docs.
