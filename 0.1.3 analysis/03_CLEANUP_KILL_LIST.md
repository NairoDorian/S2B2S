# 03 — Cleanup Kill-List

[← Roadmap](02_ROADMAP_IN_ORDER.md) · next → [Code Findings](04_CODE_FINDINGS.md)

This is the concrete "getting lost" fix. **Today: 66 markdown files (~24,500 lines). Target: ~12.** Most of the bulk is research and superseded plans, not project docs. Nothing here touches `src/` or `src-tauri/src/` — it's safe and reversible (use a `git tag pre-cleanup` first).

> Legend: 🟢 **KEEP** · 🟡 **MERGE / TRIM** · 🔴 **DELETE / MOVE OUT**

---

## Root markdown files (17)

| File                                 | Lines | Action          | Why                                                                                                        |
| ------------------------------------ | ----- | --------------- | ---------------------------------------------------------------------------------------------------------- |
| `README.md`                          | 432   | 🟢 KEEP         | Fix the honesty (see [01](01_STATE_OF_THE_PROJECT.md)); otherwise good.                                    |
| `AGENTS.md`                          | 503   | 🟢 KEEP         | Becomes the **single** AI-assistant guide.                                                                 |
| `BUILD.md`                           | 343   | 🟢 KEEP         | Real build instructions.                                                                                   |
| `CONTRIBUTING.md`                    | 213   | 🟢 KEEP         | Standard, fine.                                                                                            |
| `CHANGELOG.md`                       | 767   | 🟡 TRIM         | Grown huge. Keep recent versions; archive pre-0.1.0 detail. Standardize on Keep-a-Changelog format.        |
| `CONTRIBUTING_TRANSLATIONS.md`       | 200   | 🟡 MERGE        | Fold into `CONTRIBUTING.md` as a section.                                                                  |
| `LLAMA_CPP.md`                       | 60    | 🟡 MERGE        | Fold into `BUILD.md` or a `docs/` page.                                                                    |
| `CLAUDE.md`                          | 83    | 🔴 MERGE→AGENTS | It's just a pointer file. → [AI docs](#ai-docs)                                                            |
| `CRUSH.md`                           | 174   | 🔴 MERGE→AGENTS | Overlaps AGENTS (dev commands, venv, model download). → [AI docs](#ai-docs)                                |
| `improvement-plan.md`                | 333   | 🔴 DELETE       | Superseded roadmap. Salvage any _live_ item into [02](02_ROADMAP_IN_ORDER.md), then delete.                |
| `AIVORELAY_INSPIRED_IMPROVEMENTS.md` | 1,785 | 🔴 ARCHIVE      | A giant idea-dump. Pull anything you're actually doing into the roadmap; archive the rest out of the repo. |
| `S2B2S_REVIEW.md`                    | 1,857 | 🔴 ARCHIVE      | Superseded by this `REVIEW/` set. Keep one, not both.                                                      |
| `reference_links.md`                 | 550   | 🔴 TRIM/MOVE    | Curate to a short list or move to `docs/`.                                                                 |
| `reference_github_links.md`          | 100   | 🔴 MERGE        | Merge into the one curated links file.                                                                     |
| `repomix-file-descriptions.md`       | 429   | 🔴 DELETE       | Generated artifact; stale the moment files move.                                                           |
| `S2B2S_ANDROID_COMPANION.md`         | 255   | 🔴 MOVE         | Android is a separate project. → `docs/android/` or its own repo.                                          |
| `android-port-plan.md`               | 251   | 🔴 MOVE         | Same — merge with the companion doc, move out.                                                             |

---

## Generated / scratch artifacts at root

| Item                          | Action    | Why                                                                                                |
| ----------------------------- | --------- | -------------------------------------------------------------------------------------------------- |
| `S2B2S_repomix.txt`           | 🔴 DELETE | Generated repomix snapshot — instantly stale, regenerate on demand.                                |
| `S2B2S_repomix_annotated.txt` | 🔴 DELETE | Same.                                                                                              |
| `repomix.config.json`         | 🟢 KEEP   | The config is fine; the _output_ shouldn't be committed. Add the `.txt` snapshots to `.gitignore`. |

---

## Directories {#scratch}

| Dir                                   | Files       | Action         | Why                                                                                                                                                                         |
| ------------------------------------- | ----------- | -------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `references_comparative_analysis_md/` | ~30 (~1 MB) | 🔴 MOVE OUT    | Reviews of **26 other projects** (Whispering, Handy, Parler, sherpa-onnx…). This is ~half your doc bulk and belongs in a research notes repo or wiki, not the product repo. |
| `analysys/`                           | 6           | 🔴 DELETE      | README says it's **superseded by `futuristic_analysis/`**, and it's already listed in `.gitignore`. Remove it (`git rm -r`).                                                |
| `futuristic_analysis/`                | 8           | 🟡 CONSOLIDATE | The aspirational vision. Collapse into a single `docs/VISION.md` (or keep the folder but make it clearly "future / not committed").                                         |
| `temp_export_onnx/`                   | ~13         | 🔴 DELETE/MOVE | Literally named "temp." ONNX export scripts + notes. Move to a tooling repo or a gitignored local dir.                                                                      |
| `gemma_4_qat_mtp_e2b/`                | 2           | 🔴 MOVE        | Model-experiment notes (`MULTIMODAL.md`, `REFERENCE.md`). Not product docs.                                                                                                 |
| `models/` (scripts)                   | —           | 🟢 KEEP        | Download scripts are real tooling.                                                                                                                                          |
| `scripts/`                            | —           | 🟢 KEEP        | Build/check tooling.                                                                                                                                                        |
| `nix/`, `.nix/`, flake files          | —           | 🟢 KEEP        | Nix packaging.                                                                                                                                                              |

> **Just moving `references_comparative_analysis_md/` and deleting the superseded plans removes well over half the markdown lines in the repo.**

---

## The 6 competing roadmaps — pick ONE

Right now "what to do next" is spread across all of these. Consolidate into **one** (`STATUS.md` or this `REVIEW/` set) and delete/archive the rest:

1. `README.md` roadmap table
2. `improvement-plan.md`
3. `AIVORELAY_INSPIRED_IMPROVEMENTS.md`
4. `S2B2S_REVIEW.md` §19 "Roadmap & Future Work"
5. `futuristic_analysis/07_IMPLEMENTATION_ROADMAP.md`
6. `analysys/05_IMPLEMENTATION_ROADMAP.md`

**This single act is probably the biggest cure for "I'm getting lost in there."**

---

## AI-assistant instruction files {#ai-docs}

Three files, heavy overlap:

| File        | Lines | Keep?                                                                    |
| ----------- | ----- | ------------------------------------------------------------------------ |
| `AGENTS.md` | 503   | 🟢 The keeper (emerging cross-tool standard).                            |
| `CLAUDE.md` | 83    | 🔴 Pointer-only → replace with a 2-line "see AGENTS.md".                 |
| `CRUSH.md`  | 174   | 🔴 Dev-commands/venv/model-download → already in AGENTS; merge & delete. |

---

## CI workflows {#ci}

9 workflows; several redundant. **Four trigger on push-to-main** (`main-build`, `code-quality`, `nix-check`, `test`) → duplicate runs and confusion about which is canonical.

| Current                                                   | Action                                                                          |
| --------------------------------------------------------- | ------------------------------------------------------------------------------- |
| `build.yml` (reusable)                                    | 🟢 Keep as the reusable build.                                                  |
| `release.yml`                                             | 🟢 Keep.                                                                        |
| `code-quality.yml` + `test.yml`                           | 🟡 Merge into one `ci.yml` (lint + typecheck + cargo test + translations gate). |
| `main-build.yml` + `build-test.yml` + `pr-test-build.yml` | 🟡 Collapse — one PR build, one main build, both calling `build.yml`.           |
| `playwright.yml`                                          | 🟢 Keep (fold into `ci.yml` once frontend tests grow).                          |
| `nix-check.yml`                                           | 🟢 Keep if you support Nix; else drop.                                          |

**Target: ~3 workflows** — `ci.yml`, `build.yml`, `release.yml`.

---

## Suggested final docs layout

```
README.md                 # honest, the front door
STATUS.md                 # the ONE source of truth (this REVIEW set, folded in)
ARCHITECTURE.md           # the diagrams, explained once
BUILD.md                  # + LLAMA_CPP merged in
CONTRIBUTING.md           # + translations section merged in
CHANGELOG.md              # trimmed
AGENTS.md                 # the ONE AI-assistant guide
LICENSE
docs/
  vision.md               # futuristic_analysis, consolidated
  android.md              # companion + port-plan, merged
  references.md           # short curated links
```

Everything else → deleted, archived to a `notes/` repo, or gitignored.

Next: **[04 — Code Findings →](04_CODE_FINDINGS.md)**
