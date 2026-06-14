# Universal Project Analysis Template

> This template was derived from the existing 6 analysis files in the `archive/` folder.
> Every agent analyzing a reference project MUST follow this structure.
> Agents may ADD sections when a project has unique features worth highlighting.
> Agents may SKIP sections that don't apply (e.g., "Diff Analysis" only applies to forks).

---

## STANDARD TABLE OF CONTENTS (MANDATORY SECTIONS)

```
# [Project Name] — [Type Tag]

> Repo: `owner/repo` · HEAD: `sha` · License: · Author: · Platforms:
> Nature: [fork-of-X | independent | structural-fork | framework | library]
> Role for S2B2S: [concrete value — what to learn/copy/avoid]

---

## 1. What [Project] Is
[1-3 paragraphs. What does it do? What problem does it solve? Who is it for?]

## 2. Tech Stack
### 2.1 Frontend (if applicable)
| Layer | Choice | Purpose |
### 2.2 Backend / Core
| Layer | Choice | Purpose |
### 2.3 Key Dependencies (non-obvious ones)

## 3. Architecture & Source Map
[ASCII tree or markdown tree showing every module/folder with 1-line descriptions]
[Group by subsystem: audio, STT, TTS, brain, UI, settings, etc.]

## 4. Feature Inventory
[Complete list. For each feature: what it does, how it's implemented, what file(s)]
[For complex projects, split into sub-sections like:
  4.1 STT Pipeline
  4.2 TTS Pipeline  
  4.3 Voice Activity Detection
  4.4 Text Processing
  4.5 UI/UX Features
  4.6 Platform Features
  4.7 Configuration & Settings]

## 5. Key Code Patterns & Techniques
[Specific patterns worth studying/copying. Include:
 - File paths and line counts of key modules
 - State machines, threading models, trait designs
 - Performance optimizations
 - Platform-specific tricks
 - Error handling patterns]

## 6. Relation to S2B2S
[For fork projects: what S2B2S inherited, what differs]
[For independent projects: comparison table showing what S2B2S does better, what this project does better]
| Aspect | This Project | S2B2S | Verdict |

## 7. Harvest List (Features Worth Copying)
| Feature to harvest | From file | Effort (XS/S/M/L/XL) | Why valuable for S2B2S |

## 8. Known Issues, Caveats & Limitations
| Issue | Severity | Impact |
[List bugs, stubs, TODOs, dead code, platform limitations, license issues]

## 9. Strengths & Weaknesses
### Strengths
### Weaknesses

## 10. Bottom Line / Verdict
[2-3 sentence executive summary. Is it worth studying? What's the single most valuable idea?]
```

---

## OPTIONAL ADDITIONAL SECTIONS

Agents may add any of these when the project warrants it:

- **Hardware/GPU Acceleration Details** — when a project handles GPU/ML acceleration
- **Diff Analysis vs Parent** — ONLY for forks (what was added/removed/changed)
- **Streaming/Runtime Performance** — when latency/throughput is a core concern
- **Security & Privacy Notes** — when there are auth, key storage, or privacy features
- **Testing & CI Notes** — when the project has notable test infrastructure
- **License Compliance Notes** — when licensing matters for code reuse
- **Database/Schema Details** — when the project has interesting persistence

---

## STEPS TO FOLLOW (what the agent must DO)

1. **List the project directory** — use `read` on the project root to see all files/folders
2. **Read every README, AGENTS.md, docs/** folder — understand what the project claims to be
3. **Map the source tree** — use `glob` to find all source files (*.rs, *.ts, *.tsx, *.py, *.js, etc.)
4. **Read key entry points** — main.rs, lib.rs, App.tsx, package.json, Cargo.toml, pyproject.toml
5. **Read config files** — all .json, .toml, .yaml, .env files
6. **Read every source file** — focus on understanding what each module does, its role, its size (lines)
7. **Read every documentation file** — all .md files, doc comments in code
8. **Identify ALL features** — make a complete inventory, nothing skipped
9. **Find dead code, stubs, TODOs, FIXMEs, known bugs** — grep for these patterns
10. **Write the analysis** — follow the template exactly, fill every section
11. **Save to the output path** — `C:\Users\Z\Downloads\PROJECTS\AZ\S2B2S\references_comparative_analysis_md\PROJECTNAME_review.md`

---

## QUALITY STANDARDS

- Minimum 100 lines for small projects, 200+ for medium, 400+ for large
- Every source file must be mentioned somewhere (file structure map or feature sections)
- Line counts for key files (helps judge complexity)
- Concrete file paths in all references (e.g., `src-tauri/src/managers/tts.rs`)
- Platform caveats must be explicit (Windows-only, macOS-only, Wayland limitations)
- When unsure about a feature's status (stub? working?), say so explicitly

---

## PROJECT CATEGORIES & AGENT ADAPTATIONS

### Category A: Forks of Handy (Handy, Parler, AIVORelay, Parrot)
- MUST include "Diff Analysis" section comparing to the parent
- MUST identify exactly what was inherited vs added
- MUST note the fork point (merge-base) if determinable

### Category B: Independent STT/TTS Apps (CopySpeak, TranscriptionSuite, voicebox, vox, whispering)
- MUST compare architecture choices to S2B2S's equivalent subsystem
- MUST identify what their engine abstraction looks like

### Category C: Libraries/Frameworks (transcribe-rs, sherpa-onnx, speechbrain, onnx-asr)
- MUST document the API surface, supported models, platform support
- MUST note how S2B2S uses them (if it does) or could use them

### Category D: Research/Reference Projects (Parakeet-RT, vibevoice-rs, pocket-tts-server, TTS-Audio-Suite, voirs, speech-recognition)
- Focus on unique algorithms, data flows, novel techniques
- What can be learned even if code can't be directly reused

### Category E: Utility/Visual Projects (Cross_Platform_Rust_WebGPU_CursorFX, TD_Web_Trail)
- Focus on the rendering pipeline, platform tricks, performance techniques
- Document the exact APIs and code patterns for S2B2S's overlay/avatar future

---

## CRITICAL REMINDERS

- READ EVERY FILE. No skipping. This is a deep analysis, not a skim.
- Be specific. "Uses rodio for playback" is weak. "rodio Sink + OutputStream with 200ms Windows preroll, pause/resume via Sink.pause(), gapless via pre-decode AudioContext" is strong.
- File paths. Always give exact paths with line counts.
- Honesty. If something is broken, a stub, or dead code — say so.
- Compare to S2B2S. Every analysis must answer: what can S2B2S learn from this?
