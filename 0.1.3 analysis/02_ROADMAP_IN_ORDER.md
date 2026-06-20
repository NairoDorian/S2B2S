# 02 — Roadmap, In Order

[← State of the Project](01_STATE_OF_THE_PROJECT.md) · next → [Cleanup Kill-List](03_CLEANUP_KILL_LIST.md)

The principle: **stop the bleeding → make the core bulletproof → finish what's half-done → only then build new ambition.** Don't start a phase until the one before it is genuinely closed. This ordering is chosen specifically to get you _un-lost_ fastest.

---

## Phase 0 — Stop the bleeding (days, not weeks) {#phase-0}

You cannot steer while you're lost. This phase is almost entirely _deletion and truth-telling_, not coding. It's the highest-leverage work in the whole list.

1. **Adopt one source of truth.** Pick this `REVIEW/` set (or collapse it into a single `STATUS.md`). Everything about "what's done / what's next" lives there and nowhere else.
2. **Make the README honest.** Re-label the roadmap table per [01](01_STATE_OF_THE_PROJECT.md). Replace 🔴 rows with 🚧/📋 + a one-line truth. ~30 minutes.
3. **Execute the kill-list.** Delete/archive the 6 competing roadmaps, the competitor-review folder, scratch dirs, and repomix snapshots — see [03](03_CLEANUP_KILL_LIST.md). Target: **66 MD files → ~12**.
4. **Consolidate AI-assistant docs.** Merge `CLAUDE.md` + `CRUSH.md` into `AGENTS.md`. → [03](03_CLEANUP_KILL_LIST.md#ai-docs).
5. **Stop hand-maintaining status tables.** If you want a feature matrix, generate it; otherwise keep prose. A hand-edited table is what drifted into lying.

**Exit criteria:** a newcomer (or you, in two weeks) can open the repo and find exactly _one_ document that says what's done and what's next, and it's true.

---

## Phase 1 — Make the core bulletproof {#phase-1}

The STT→Brain→TTS loop is your whole product. Make it boring and reliable before adding anything.

1. **Resolve the Python question** (decide before you build anything else — it touches install, CI, and support):
   - **Option A — Truly local-first:** replace the Python TTS default with a Rust/ONNX path (Piper has Rust bindings; sherpa-onnx is already in your reference set) or default to SAPI on Windows. Python engines become opt-in "advanced."
   - **Option B — Own the runtime:** bundle an embedded Python (e.g. a pinned standalone build) + prebuilt wheels so first-run is genuinely one-click, offline.
   - Pick one and update the README promise to match. → [04](04_CODE_FINDINGS.md#python-venv)
2. **Panic audit on hot paths.** Triage the 292 `.unwrap()/.expect()`. Convert the audio, clipboard, server-spawn, and IPC-boundary ones to handled errors. → [04](04_CODE_FINDINGS.md#panics)
3. **First-run experience.** This is the make-or-break for an alpha. Walk a _fresh machine_ through onboarding (`src/components/onboarding/`): model download, mic permission, venv/runtime setup, first dictation. Every failure here is a lost user. Add clear failure states, not silent dead-ends.
4. **Frontend/e2e tests where it matters.** You have **1** test total on the frontend. Add Playwright coverage for the three pipelines and onboarding. You don't need 80% — you need the money paths to not silently break.
5. **CI consolidation.** 9 workflows, several redundant (4 trigger on push-to-main). Collapse to ~3: `ci` (lint+test+typecheck on PR), `build` (reusable, multi-OS), `release`. → [03](03_CLEANUP_KILL_LIST.md#ci)

**Exit criteria:** fresh-machine install → working dictation + read-aloud + one Brain reply, with no manual Python fiddling and no unhandled panic on the common paths.

---

## Phase 2 — Finish (or formally shelve) the partials {#phase-2}

Close the 🟡/🔴 items so the roadmap stops lying. **A feature behind a feature-flag with an honest label is "done enough." A stub called "Complete" is not.**

1. **Native wgpu overlay (Track B):** either implement it (vendor from your `Cross_Platform_Rust_WebGPU_CursorFX` reference, wire the wgpu-29 surface, add the missing render loop) **or** delete `overlay_fx/native/` and the OS-native toggle and keep only the Tauri overlay. Don't leave the stub. → [01](01_STATE_OF_THE_PROJECT.md#claimed-but-not-done)
2. **i18n sync:** run `check-translations.ts` in CI as a gate. Backfill the 186 missing keys per language (machine-translate + flag for review), or honestly downgrade to "English + community translations (partial)." → [04](04_CODE_FINDINGS.md#i18n)
3. **Streaming STT:** decide if it's a supported feature or an experiment. If supported, make finalization robust and document the Python-server requirement; if experimental, gate it and label it.
4. **multi_stt:** confirm it's tested + documented, or shelve it. Two STT code paths is a maintenance tax you should pay deliberately, not by accident.
5. **Continuous voice echo behavior:** document the known limits (no AEC yet) so users don't file bugs for expected behavior.

**Exit criteria:** zero 🔴 rows anywhere in the repo. Every feature is ✅, 🟡-with-flag, or 📋.

---

## Phase 3 — Reduce the maintenance surface {#phase-3}

Now that it's honest and reliable, make it _maintainable_ so future-you doesn't get lost again.

1. **Split the god-files.** Break up `model.rs` (2,233), `settings.rs` (2,185), `shortcut/mod.rs` (1,513), `actions.rs` (1,391), `clipboard.rs` (1,167) into focused modules. → [04](04_CODE_FINDINGS.md#god-files)
2. **Settings schema discipline.** `settings.rs` is the second-biggest file and every feature bolts a field onto it. Group settings into sub-structs (audio, stt, tts, brain, overlay) and version the schema with explicit migrations.
3. **Document the architecture once, well.** A single `ARCHITECTURE.md` (the diagrams from the current README are good) — then stop re-explaining it in five places.
4. **Dependency hygiene.** Audit the 27 npm deps / Rust crate tree for unused entries now that experiments are removed.

**Exit criteria:** no source file over ~800 lines without a deliberate reason; settings changes are localized; one architecture doc.

---

## Phase 4 — The ambitious stuff (only after 0–3) {#phase-4}

This is everything in `futuristic_analysis/` and the README "Later" rows. It's exciting, and it's exactly the kind of thing that made the repo sprawl. **Gate it behind the discipline above.** Rough priority if/when you get here:

1. **Profiles (per-app settings)** — high user value, contained scope. Best first "new" feature.
2. **MCP tool use for the Brain** — turns the assistant from talker into doer; big differentiator.
3. **Full-duplex + acoustic echo cancellation** — the natural-conversation holy grail; hard, do it when the core is rock-solid.
4. **Speaker diarization** — niche; defer.
5. **Transparent GPU overlay / screen understanding / avatar v2** — the moonshot. Genuinely cool, genuinely a rabbit hole. Only with a co-maintainer or after 1.0.
6. **Android companion** — a whole second platform. Has its own plan already; treat as a separate project, not a feature.

---

## The one-line version of this whole file

> **Delete and tell the truth (Phase 0) → make install + the 3 pipelines bulletproof (Phase 1) → close every half-feature (Phase 2) → break up the god-files (Phase 3) → _then_ build the future (Phase 4).**

Next: **[03 — Cleanup Kill-List →](03_CLEANUP_KILL_LIST.md)**
