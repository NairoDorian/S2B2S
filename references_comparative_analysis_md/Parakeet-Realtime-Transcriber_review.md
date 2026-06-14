# Parakeet-Realtime-Transcriber — Research/Reference

> Repo: `TheSethRose/Parakeet-Realtime-Transcriber` · HEAD: N/A (not a git working tree locally) · License: No LICENSE file (model is CC-BY-4.0) · Author: Seth Rose · Platforms: macOS (Linux supported; Windows "planned")
> Nature: independent · Category: D (Research/Reference)
> Role for S2B2S: Reference implementation of a VAD → segment → batch-ASR → post-process pipeline with a three-trigger endpointing policy directly adaptable to S2B2S conversation turn-detection. The threading model, duplicate filter, and documented-but-broken overlap mechanism are all directly instructive.

---

## 1. What Parakeet-Realtime-Transcriber Is

A terminal-based, single-machine, real-time transcription tool written in Python (~1,931 lines across 7 modules). It captures audio from a microphone or system audio (via the external macOS "Background Music" loopback app), applies Voice Activity Detection (VAD) to gate processing, cuts audio into 5-20 second segments at natural pause boundaries, transcribes each segment using NVIDIA Parakeet TDT 0.6B v2 (through the NeMo toolkit), post-processes the text by grouping fragments into sentences and filtering duplicates, and optionally persists every sentence to a Neon cloud PostgreSQL database under named recording sessions. Two companion CLI scripts (`combine.py`, `export.py`) merge a session segments into a single record and export it as a Markdown file.

There is no GUI, no server, no streaming partials -- it is a clean, minimal reference implementation of a *segment-based* live transcription loop. The README claims "v2.0 production ready" status, and the code quality is consistent with a polished side project.

**Problem it solves:** Real-time transcription of live audio (meetings, lectures, system audio like YouTube) with clean sentence-level output, automatic segmentation, and persistent storage.

---

## 2. Tech Stack

### 2.1 Frontend (if applicable)
None. Terminal-only via `print()` and `input()`.

### 2.2 Backend / Core

| Layer | Choice | Purpose |
|---|---|---|
| Language | Python 3.8+ | All application logic |
| ASR model | `nvidia/parakeet-tdt-0.6b-v2` (NeMo) | FastConformer encoder + TDT decoder, 600M params, English, punctuation/capitalization built-in, ~600MB download |
| ASR framework | `nemo_toolkit[asr]` | Model loading and `.transcribe()` API; implicitly uses GPU if CUDA present, otherwise CPU |
| Audio capture | `sounddevice` (PortAudio) | 16 kHz mono InputStream, blocksize 1024, callback thread |
| VAD | `webrtcvad` | Aggressiveness 0-3, default 2, 30 ms frame duration |
| DSP | `numpy`, `scipy` | Float32 buffer management; `scipy.io.wavfile.write` for temp WAV files |
| Storage | `psycopg2-binary` → Neon PostgreSQL | `DATABASE_URL` from `.env` via `python-dotenv`; schema: `recordings` + `combined_recordings` |
| ML backend | `torch`, `torchaudio` | Required by NeMo for model inference |
| Tooling | `setup.sh` (uv-based venv) | Auto-installs `uv` if missing, creates `.venv`, installs dependencies |

### 2.3 Key Dependencies (non-obvious ones)
- **webrtcvad**: Not `silero-vad` or `vad-rs` -- a simple, lightweight Python binding. Uses 30ms frame duration; float32 audio is converted to int16 per frame.
- **Neon serverless PostgreSQL**: A "local" tool that hard-requires a cloud database. The `DatabaseManager.__init__()` raises `ValueError` if `DATABASE_URL` is missing.
- **Background Music (external macOS app)**: The repo has no loopback code. System audio capture is entirely delegated to this external tool. The app simply treats the Background Music virtual device as any other audio input device.

---

## 3. Architecture & Source Map

```
Parakeet-Realtime-Transcriber/
├── main.py                    (229 lines)  Orchestrator: RealTimeTranscriber class, producer/consumer threading,
│                                           audio_callback, transcribe_worker, start_transcription, CLI entry point
├── audio_capture.py           (193 lines)  AudioCapture (VAD state machine) + AudioSegmentManager (three-trigger
│                                           endpointing policy, overlap declaration [unused], buffer management)
├── transcription.py           (126 lines)  TranscriptionEngine: NeMo model load, temp WAV round-trip,
│                                           stdout/stderr suppression, transcribe_audio_chunk()
├── sentence_processor.py      (178 lines)  SentenceProcessor: duplicate filter (word-overlap + substring),
│                                           sentence buffer → re-split on [.!?], display, DB save via DatabaseManager
├── database.py                (534 lines)  DatabaseManager: Neon PostgreSQL CRUD, smart insert (same-second combine),
│                                           combine_recording_segments, convenience functions
├── combine.py                 (319 lines)  CLI: interactive/direct recording segment combination
├── export.py                  (352 lines)  CLI: export combined recordings to Markdown (front matter + content)
├── setup.sh                    (34 lines)  uv-based venv creation and dependency install
├── requirements.txt             (9 lines)  Python dependency manifest
├── .env.example                 (3 lines)  DATABASE_URL template
├── .gitignore                  (103 lines) Standard Python + audio + secrets ignore patterns
├── README.md                  (218 lines)  Project overview, features, quick start, structure, performance claims
├── docs/
│   ├── setup.md               (155 lines)  macOS-focused setup guide with Background Music + Neon DB config
│   ├── usage.md               (267 lines)  Usage scenarios: meetings, system audio, lectures, quality tips
│   ├── api.md                 (401 lines)  API reference for DatabaseManager, RealTimeTranscriber, SentenceProcessor
│   ├── DATABASE_SETUP.md      (100 lines)  Docker/local PostgreSQL alternative (contradicts Neon cloud setup)
│   └── troubleshooting.md     (500 lines)  Comprehensive troubleshooting: install, audio, DB, model, runtime
└── .github/                              Copilot prompt/instruction scaffolding from author repo template --
                                          NOT part of the application; 22 prompt templates + 5 instruction templates
```

### Data Flow Diagram
```
sounddevice.InputStream (16kHz mono, blocksize=1024, PortAudio callback thread)
        │
        ├─► AudioSegmentManager.add_audio_data()      (np.append to current_segment buffer)
        ├─► AudioCapture.process_vad_frames()          (webrtcvad, 30ms frames, track speech/silence state)
        └─► AudioSegmentManager.should_transcribe_segment()
                 │  Three cut triggers:
                 │   1. "natural pause"   : >=0.8s since last speech AND segment >=5s
                 │   2. "max duration"    : segment reaches 20s
                 │   3. "silence timeout" : >=1.5s silence (speech inactive) AND segment >=1s
                 ▼
        queue.Queue ──► transcribe_worker (daemon thread)
                              │  writes segment to temp .wav (scipy.io.wavfile.write)
                              │  asr_model.transcribe([tempfile])
                              ▼
                      SentenceProcessor
                              │  is_duplicate(): >80% word-set overlap with last output
                              │     OR substring containment → drop
                              │  sentence_buffer.append(text)
                              │  extract_complete_sentences(): join buffer,
                              │     re.split on [.!?], emit sentences >=15 chars
                              ▼
                  console print + DatabaseManager.insert_recording_segment_smart()
                                      (Neon PostgreSQL, "recordings" table)
```

---

## 4. Feature Inventory

### 4.1 STT Pipeline
- **Batch ASR via temp file:** Each audio segment is written to a `tempfile.NamedTemporaryFile(suffix='.wav')`, converted from float32 to int16 via `(audio_chunk * 32767).astype(np.int16)`, saved with `scipy.io.wavfile.write()`, and passed to `asr_model.transcribe([path])`. The temp file is `os.unlink()`d in a `finally` block.
  - File: `transcription.py` lines 70-126
- **NeMo output suppression:** Both model loading and per-segment transcription redirect `sys.stdout` and `sys.stderr` to `StringIO()` to suppress NeMo verbose logging and tqdm progress bars. This is a pragmatic hack; failure in the suppression block restores original streams via try/finally.
  - File: `transcription.py` lines 37-65 (model load), 94-104 (per-transcription)
- **Minimal text filter:** Only text longer than 3 characters is returned from `transcribe_audio_chunk()`. The SentenceProcessor further requires 15 characters for display.
  - File: `transcription.py` line 109, `sentence_processor.py` line 131
- **No streaming partials:** Output is only final text per segment. There is no intermediate/partial result mechanism.

### 4.2 Voice Activity Detection
- **webrtcvad only:** No silero-vad, no multi-stage VAD cascade. Aggressiveness 2 (configurable 0-3). Frame duration fixed at 30ms (constant: `vad_frame_duration = 30`, computed: `vad_frame_size = int(sample_rate * 30 / 1000) = 480 samples`).
  - File: `audio_capture.py` lines 38-40
- **VAD state machine in AudioCapture:** Tracks `is_speech_active`, `last_speech_time`, `speech_start_time`, `silence_frames`, `speech_frames`. State is separated from segmentation policy -- `AudioSegmentManager` reads AudioCapture state but does not own it.
  - File: `audio_capture.py` lines 68-105 (process_vad_frames), lines 42-48 (state variables)
- **Float32 → int16 conversion per frame:** Audio is converted frame-by-frame inside `process_vad_frames()`: `audio_int16 = (audio_data * 32767).astype(np.int16)`.
  - File: `audio_capture.py` line 80
- **Graceful VAD failure:** Individual frame `vad.is_speech()` failures are caught silently (`except Exception: pass`), so a single bad frame does not crash the pipeline.
  - File: `audio_capture.py` lines 101-103


### 4.3 Three-Trigger Endpointing Policy (Segmentation)
This is the most architecturally valuable feature. All logic lives in `AudioSegmentManager.should_transcribe_segment()`.

| Trigger | Condition | When Active | Minimum Segment |
|---------|-----------|-------------|-----------------|
| "natural pause" | `time_since_speech > 0.8s` | Speech IS active | >= 5s (`min_segment_duration`) |
| "max duration" | `segment_duration >= 20s` | Speech IS active | N/A (hard cap) |
| "silence timeout" | `time_since_speech > 1.5s` | Speech NOT active | >= 1s |

Key design: The three triggers are **independent** and checked sequentially with if/elif/else. The "max duration" trigger is checked second (after pause check), meaning a 20+ second segment is cut even if the pause threshold has not been reached. The "silence timeout" only fires when `is_speech_active` is False, ensuring trailing utterances (short speech bursts followed by silence) are flushed even if they do not meet the 5-second minimum for natural-pause cuts.

File: `audio_capture.py` lines 144-180.

State reset policies differ by trigger type:
- "natural pause" and "silence timeout": `reset_speech_state()` sets `is_speech_active = False` and `speech_frames = 0`
- "max duration": `is_speech_active` is NOT reset -- speech may continue in the next segment
  - File: `main.py` lines 100-101

### 4.4 Text Post-Processing (SentenceProcessor)
- **Duplicate filter (dual-check):**
  1. **Word-overlap check (>80%):** Convert both new and last text to lowercase word-sets. Compute Jaccard-like overlap: `len(intersection) / len(new_words)`. If >0.8, it is a duplicate.
  2. **Substring check:** If `new_text.lower().strip() in last_transcription.lower()`, it is a duplicate.
  - File: `sentence_processor.py` lines 42-69
  - Note: The substring check is asymmetric -- it catches cases where the new text is a subset of the old, but NOT the reverse. This is correct because ASR re-transcription of overlap would produce shorter fragments.
- **Sentence buffer + re-splitter:** Incoming transcriptions are appended to `sentence_buffer`. `extract_complete_sentences()` joins all buffer text, splits on `[.!?]` (preserving punctuation via `re.split(r'([.!?])', full_text)`), and emits only sentences >= `min_sentence_length` (default 10) characters. Incomplete trailing text is kept in the buffer for the next segment.
  - File: `sentence_processor.py` lines 71-110
- **Emission gate:** Only sentences >= 15 characters are displayed/DB-saved (line 131), separate from the buffer internal 10-char minimum (line 96). Net effect: 10-14 char sentences accumulate in the buffer until more text arrives; >=15 char sentences are emitted immediately.

### 4.5 Database Storage (PostgreSQL)
- **Smart insert (same-second combination):** `insert_recording_segment_smart()` checks if a segment already exists at the same integer-second timestamp. If so, it appends the new text to the existing row with `combined_text = f"{existing_text} {segment_text}".strip()`. This prevents one-second from having many rows when ASR is fast.
  - File: `database.py` lines 120-200
- **Timestamp as PostgreSQL INTERVAL:** Timestamps (float seconds from session start) are stored as `timedelta(seconds=...)` → PostgreSQL `INTERVAL` type. Queries extract epoch seconds via `EXTRACT(EPOCH FROM segment_timestamp)::INTEGER`.
  - File: `database.py` lines 97, 151, 161-162
- **Recording combine → Markdown export:** `combine.py` collapses all segments of a named session into a `combined_recordings` row. `export.py` reads that row and generates a Markdown file with YAML front matter (`title`, `date`, `segments`, `duration`) in an `export/` directory.
  - File: `combine.py` lines 148-237, `export.py` lines 65-124
- **Hard dependency on cloud DB:** The constructor raises `ValueError("DATABASE_URL environment variable is required for Neon connection")` if no env var is set. There is no local/SQLite fallback.
  - File: `database.py` lines 27-28

### 4.6 Configuration & Settings
- All tuning parameters are hard-coded as constructor defaults in `RealTimeTranscriber.__init__()`:
  - `sample_rate=16000`, `max_segment_duration=20`, `min_segment_duration=5`, `vad_aggressiveness=2`
  - Configurable only by modifying the `main()` call or programmatic instantiation
- Thresholds in `AudioSegmentManager` are class constants: `pause_threshold = 0.8`, `silence_threshold = 1.5` (lines 127-128)
- No config file, no CLI arguments for runtime tuning

### 4.7 CLI/Export Tools
- `combine.py`: Interactive listing of all sessions → select → title → combine → optionally delete originals. Also accepts a recording name as CLI arg for direct mode.
- `export.py`: Interactive listing of combined recordings → select → generate sanitized Markdown file in `export/` dir. Filename sanitization: lowercase, replace whitespace with dashes, strip special chars. Direct mode via CLI arg.

---

## 5. Key Code Patterns & Techniques

### 5.1 Producer/Consumer Threading Model (main.py)
```
PortAudio callback thread (producer)
  │  audio_callback(): VAD + segmentation check (lightweight, must be fast)
  │  Puts segment numpy array into queue.Queue (unbounded)
  ▼
queue.Queue
  │  get(timeout=1.0): blocks with timeout to allow clean shutdown
  ▼
transcribe_worker daemon thread (consumer)
  │  transcribe_audio_chunk(): temp file + NeMo inference (heavy, slow)
  │  sentence_processor.process_transcription(): duplicate filter + sentence split
  ▼
Main thread: time.sleep(0.1) loop, Ctrl-C handler
```

Key details:
- The worker thread is a **daemon** thread (line 142), meaning it will die if the main thread exits. This is safe because all state is in-memory numpy arrays and the queue.
- `queue.Queue` is **unbounded** -- there is no backpressure mechanism. For a Python reference implementation this is acceptable; for S2B2S Rust backend, a bounded channel (e.g., `tokio::sync::mpsc::channel` with capacity) would be appropriate.
- The worker `queue.get(timeout=1.0)` is the primary polling loop; on timeout it just loops back to check `self.is_running`, enabling clean shutdown within ~1s of Ctrl-C.
- Lines: `main.py` 63-120 (callback + worker)

### 5.2 VAD Detection vs. Segmentation Policy Separation (audio_capture.py)
- `AudioCapture` owns: VAD model, speech/silence counters, `is_speech_active`, `last_speech_time`
- `AudioSegmentManager` owns: audio buffer, pause/silence/overlap thresholds, `should_transcribe_segment()` which *reads* AudioCapture state
- This is a clean separation: **detection** vs. **policy**. S2B2S `audio_toolkit/vad/` and `transcription_coordinator.rs` should maintain the same boundary.
- Lines: `audio_capture.py` 23-105 (AudioCapture), 108-193 (AudioSegmentManager)

### 5.3 Duplicate Filter Design (sentence_processor.py)
The dual-check approach (word-overlap + substring) is pragmatic but imprecise:
- The >80% word-overlap check uses a Jaccard-like ratio that is **asymmetric** (`len(intersection) / len(new_words)`), meaning a 3-word new text that is a subset of a 20-word last text would have 100% overlap and be flagged duplicate -- correct behavior.
- The substring check catches exact containment (case-insensitive).
- **Not handled:** near-duplicates with slightly different wording (e.g., "I think that right" vs "I think that is right"). This is acceptable for a reference implementation but would need semantic dedup for production.
- S2B2S equivalent: The `is_duplicate()` pattern maps to a post-STT dedup function before text reaches the Brain or clipboard.
- Lines: `sentence_processor.py` 42-69

### 5.4 Performance Patterns
- **np.append buffer growth:** `current_segment = np.append(self.current_segment, audio_data)` at `audio_capture.py` line 142. This is O(n^2)-ish for long sessions (each append copies the entire growing array). For a 1-hour session at 16kHz, the buffer would hold ~57.6 million float32 samples (~230 MB) before reset. In practice, segments reset every 5-20 seconds, keeping the per-segment buffer to at most 320k samples (~1.3 MB). For S2B2S Rust backend, a pre-allocated ring buffer or `Vec::with_capacity()` would eliminate this concern entirely.
- **Temp file per segment:** Each ~5-20 second segment is written to disk, read back by NeMo, then deleted. This is acceptable for a reference implementation but adds latency (~10-50ms for file I/O per segment). S2B2S transcribe-rs engine operates in-memory, avoiding this overhead.

### 5.5 Error Handling Patterns
- **VAD frame failure swallowed:** `except Exception: pass` (audio_capture.py line 103) -- prevents a single bad audio frame from killing the pipeline.
- **Transcription stdout/stderr restoration:** Always uses try/finally to restore original stdout/stderr after capturing output (transcription.py lines 57-65, 94-104).
- **Temp file cleanup:** `os.unlink(temp_filename)` in a finally block with its own try/except (transcription.py lines 118-124).
- **Database rollback on error:** Every database mutation has a `connection.rollback()` in the except block (database.py lines 114-118, 196-200, 376-380).
- **No retry logic anywhere:** If a segment transcription fails (exception in `transcribe_audio_chunk`), that segment audio is lost permanently.


### 5.6 The Overlap Bug (Declared But Not Wired)
This is the single most important caveat for S2B2S:

**What the README claims (line 12):**
> "Rolling Buffer: 3-second overlap prevents information loss between segments."

**What the code declares (audio_capture.py lines 134-136):**
```python
self.overlap_duration = 3.0  # seconds of overlap between segments
self.overlap_samples = int(sample_rate * self.overlap_duration)
self.previous_segment = np.array([], dtype=np.float32)
```

**What the code actually does (audio_capture.py lines 186-188):**
```python
def reset_segment(self):
    """Reset the current segment buffer."""
    self.current_segment = np.array([], dtype=np.float32)
```

The `reset_segment()` method simply clears `current_segment`. It never:
- Saves the tail of the current segment to `previous_segment`
- Prepends `previous_segment` (or its tail) to the next segment
- References `overlap_duration`, `overlap_samples`, or `previous_segment` anywhere in the codebase

**Consequence:** Each segment is fully independent. A word that spans the cut boundary between two segments will be split -- the first half is in one ASR call, the second half in another. Neither the ASR model nor the post-processor can stitch them. The duplicate filter (which the README positions as a feature) partially compensates for this by removing repeated text, but without overlap there is rarely any repetition to filter.

**For S2B2S:** If overlap is desired, it MUST be wired in `reset_segment()`: copy the last N samples of `current_segment` to `previous_segment` before clearing, then prepend that to the next segment buffer. The S2B2S audio pipeline (Rust, cpal-based) should implement overlap in the segment-creation phase, not rely on post-hoc text dedup.

---

## 6. Relation to S2B2S

### Comparison Table

| Aspect | Parakeet-RT | S2B2S | Verdict |
|--------|-------------|-------|---------|
| **Language** | Python (~1,931 LOC) | Rust (~50k+ LOC) + TypeScript (~15k+ LOC) | S2B2S is vastly more complex |
| **ASR model** | Parakeet TDT 0.6B v2 (NeMo, single model) | Parakeet V3 + Whisper + Moonshine (transcribe-rs, multi-model) | S2B2S has model choice |
| **VAD** | webrtcvad only (single-stage) | TripleVAD: RMS → RNNoise → Silero ONNX (3-stage cascade) | S2B2S has more robust VAD |
| **Endpointing** | 3-trigger policy: pause(0.8s) / max(20s) / silence(1.5s) | VAD-based in transcription_coordinator.rs | Parakeet-RT 3-trigger is more explicit and tunable |
| **Streaming partials** | None (final per segment) | Supported via transcribe-rs streaming | S2B2S is richer |
| **Overlap** | Configured (3s) but NOT wired -- dead code | Not implemented | Parakeet-RT shows what NOT to do |
| **Duplicate filter** | Word-overlap (>80%) + substring containment | Not implemented | Parakeet-RT has useful dedup logic |
| **Threading** | Producer/consumer with unbounded Queue | Async (tokio) with bounded channels | Both follow same pattern; S2B2S has backpressure |
| **Text post-processing** | Sentence buffer + regex re-split on [.!?] | 5-stage pipeline: ITN → CustomWords → Markdown strip → TN → Cleanup | S2B2S is far more sophisticated |
| **Database** | Neon cloud PostgreSQL (hard dependency) | SQLite (rusqlite, local) via history manager | S2B2S is local-first |
| **UI** | Terminal only | Tauri 2.x + React + Tailwind CSS + i18n (20 languages) | S2B2S is a full desktop app |
| **Platform** | macOS (Linux partial) | Windows 11 + macOS + Linux | S2B2S is fully cross-platform |
| **Conversation mode** | None | Streaming LLM Brain → Sentence splitter → TTS with barge-in | S2B2S has the full loop |

### What Parakeet-RT Does Better
1. **Endpointing policy is explicit and documented.** The three triggers with their conditions and minimums are 40 lines of clear code. S2B2S `transcription_coordinator.rs` (which does similar work) is much larger and harder to extract the policy from.
2. **Duplicate filter is a concrete, cheap solution.** S2B2S has no equivalent. The word-overlap + substring approach could be ported to Rust in ~30 lines.
3. **Separation of VAD state from segmentation policy.** AudioCapture owns state; AudioSegmentManager makes decisions by reading it. This is clean architecture that S2B2S `triple_vad.rs` → `transcription_coordinator.rs` boundary could emulate more explicitly.

### What S2B2S Does Better
1. **TripleVAD cascade** is more noise-robust than webrtcvad alone.
2. **Streaming partials** provide lower perceived latency.
3. **Multi-model STT** gives users choice (accuracy vs. speed vs. platform).
4. **Full text normalization pipeline** (ITN/TN/Markdown/Cleanup) produces vastly better output.
5. **No cloud dependency** -- SQLite is local; Neon would be a single point of failure.
6. **Cross-platform** with actual Windows and Linux support.
7. **Full conversation loop** (STT → Brain → TTS) with barge-in, which Parakeet-RT has no equivalent for.

---

## 7. Harvest List (Features Worth Copying)

| Feature to harvest | From file | Effort (XS/S/M/L/XL) | Why valuable for S2B2S |
|---|---|---|---|
| Three-trigger endpointing policy | `audio_capture.py` lines 144-180 | S | Replace or augment the current VAD-based cut logic in `transcription_coordinator.rs` with explicit pause/max-duration/silence-flush thresholds. This makes turn-detection for conversation mode predictable and tunable. |
| Duplicate filter (word-overlap) | `sentence_processor.py` lines 42-69 | S | Add post-STT dedup in Rust using a `HashSet<String>` word-comparison. Prevents the Brain from receiving repeated text due to ASR segment boundary artifacts. |
| VAD state → segmentation policy separation | `audio_capture.py` classes AudioCapture + AudioSegmentManager | S | Document and enforce the same boundary in S2B2S: `TripleVad` detects, `TranscriptionCoordinator` decides. Currently this boundary exists but is less explicit. |
| Sentence buffer + re-split pattern | `sentence_processor.py` lines 71-110 | M | S2B2S already does sentence splitting in `brain/client.rs` for TTS. The buffer-accumulate-then-flush pattern could improve the dictation mode output readability by waiting for complete sentences rather than emitting fragments. |
| Smart DB insert (same-second combine) | `database.py` lines 120-200 | XS | The S2B2S history manager (`managers/history.rs`) could use a similar same-second dedup to avoid flooding the SQLite database with near-identical rows during rapid dictation. |
| Overlap implementation (fix the bug) | `audio_capture.py` lines 131-136 (declaration) + logic to wire it | S | If S2B2S implements segment overlap, the pattern is: save tail of current segment → prepend to next segment. Parakeet-RT has the right data structures declared but never used. |

---

## 8. Known Issues, Caveats & Limitations

| Issue | Severity | Impact |
|-------|----------|--------|
| **Overlap declared but not wired** (README says 3s overlap; code never uses `overlap_duration`, `overlap_samples`, or `previous_segment`) | High | Words crossing segment boundaries are permanently lost. The duplicate filter exists to clean overlap artifacts that can never occur in the shipped code. |
| **`np.append` buffer growth is O(n^2)** (`audio_capture.py` line 142) | Low | Mitigated by segment resets every 5-20s. Only a concern for very long `max_segment_duration` settings. |
| **Hard cloud DB dependency** (`database.py` line 28 raises if no `DATABASE_URL`) | Medium | A "local" tool that cannot run without a Neon account. In CI/offline environments, this is a hard failure. No local SQLite fallback. |
| **No LICENSE file** | Legal | Code is "all rights reserved" by default. Study it, do NOT copy code directly into S2B2S (which is MIT-licensed). Patterns and ideas are fine; verbatim code is not. |
| **Unbounded `queue.Queue`** (`main.py` line 53) | Low | If the ASR worker falls behind, the queue grows without bound. No backpressure. In practice, segments are large (5-20s) and the pipeline is single-threaded, so this is unlikely to cause issues for a single session. |
| **No retry on transcription failure** (`transcription.py` line 115 returns None) | Medium | A single ASR error silently drops an entire segment audio. No retry, no fallback. |
| **English-only** | Medium | The Parakeet TDT 0.6B v2 model is English-only. Multilingual support would require a different model. |
| **macOS-centric setup** (`docs/setup.md` assumes Homebrew, Background Music app, coreaudiod) | Low | Linux is partially supported (sounddevice works), but Windows is explicitly "planned." The Background Music loopback strategy is macOS-only. |
| **Documentation inconsistency** (`docs/DATABASE_SETUP.md` describes Docker/local PostgreSQL; `docs/setup.md` describes Neon cloud) | Low | Two different database setup strategies documented. The actual code uses Neon (cloud) exclusively via `DATABASE_URL`. The Docker docs appear to be from an earlier iteration or template. |
| **No streaming partials** | Medium (by design) | The segment-based approach means users wait 5-20 seconds for any text to appear. For dictation use cases this is slow; for long-form transcription (lectures, meetings) it is acceptable. S2B2S streaming partials are superior for interactive use. |
| **Temp file I/O per segment** (`transcription.py` lines 87-91) | Low | Writing a WAV to disk for each segment adds ~10-50ms latency. The NeMo API requires file paths; direct numpy-array inference would be faster but is not exposed. |
| **Sentence buffer can grow unbounded** (`sentence_processor.py` line 124) | Low | If ASR produces text without sentence-ending punctuation, the buffer accumulates indefinitely. No max-buffer-size guard. |

---

## 9. Strengths & Weaknesses

### Strengths
1. **Cleanest possible teaching example** of a VAD → segment → batch-ASR → post-process pipeline. Every module has a single, clear responsibility and the data flow is linear and traceable.
2. **Three-trigger endpointing policy** is the most directly reusable architectural idea. The conditions are simple, independent, and well-documented in code comments. This is the skeleton of conversational turn detection.
3. **Excellent module separation:** AudioCapture (VAD detection) and AudioSegmentManager (segmentation policy) are distinct classes with clear ownership boundaries.
4. **Pragmatic error handling:** VAD frame failures are swallowed, stdout/stderr is always restored, temp files are always cleaned up, database operations always rollback on error.
5. **Self-documenting code:** Variable names (`pause_threshold`, `silence_threshold`, `should_transcribe_segment`, `transcribe_reason`) make the logic readable without comments.
6. **Companion tooling:** `combine.py` and `export.py` are complete, production-quality CLI tools that demonstrate proper database interaction patterns.
7. **Comprehensive documentation:** 5 markdown files covering setup, usage, API, database, and troubleshooting (1,423 total doc lines for a ~1,931 LOC codebase).

### Weaknesses
1. **The overlap bug:** The README primary differentiator ("Rolling Buffer: 3-second overlap prevents information loss between segments") is dead code. This significantly undermines trust in the project claims.
2. **Hard cloud dependency:** A transcription tool should not require a remote database. Local SQLite would be more appropriate for the use case.
3. **No streaming partials:** For a "real-time" tool, the 5-20 second delay before text appears is slow. Streaming ASR (sending audio incrementally and receiving partial hypotheses) would dramatically improve UX.
4. **Single model, single language:** No extensibility for different ASR engines or languages.
5. **No config file:** All parameters are hard-coded. Changing thresholds requires editing source code.
6. **macOS-centric:** Heavy reliance on macOS-specific tools (Background Music, coreaudiod). The README says "Windows support planned" but no code paths exist for it.
7. **np.append for buffer growth:** While mitigated by segment resets, this is an anti-pattern for audio processing. A pre-allocated ring buffer would be more appropriate.
8. **No concurrent segments:** Only one segment is processed at a time. For very fast ASR (GPU), the worker could be processing segment N while segment N+1 is being accumulated.

---

## 10. Bottom Line / Verdict

Parakeet-Realtime-Transcriber is a **valuable reference implementation** despite its flaws. Its primary contribution to S2B2S is the **three-trigger endpointing policy** -- pause (0.8s), max-duration (20s), silence-flush (1.5s) -- which can be directly adapted as S2B2S conversation turn-detection strategy in `transcription_coordinator.rs`. The clean separation of VAD detection from segmentation policy, the duplicate filter pattern (word-overlap + substring), and the producer/consumer threading model are all directly instructive. The documented-but-broken overlap mechanism serves as a cautionary tale: if S2B2S implements segment overlap, it must be wired into the buffer reset logic, not just declared as a variable. The single most valuable idea is that **three independent cut conditions are better than one heuristic** for endpointing -- this maps directly to S2B2S need to detect when a user has finished speaking in conversation mode.

Study it for patterns and architecture. Do NOT copy code directly (no LICENSE). Port the ideas to Rust with proper concurrency, backpressure, and cross-platform audio I/O.

---

## Appendix A: Streaming Parameters — S2B2S Reference Values

Parakeet-RT hard-coded streaming constants, extracted for direct reference in S2B2S design:

| Parameter | Value | Location | S2B2S Relevance |
|-----------|-------|----------|-----------------|
| `pause_threshold` | 0.8 seconds | `audio_capture.py:127` | Use as S2B2S turn-end detection threshold for conversation mode |
| `silence_threshold` | 1.5 seconds | `audio_capture.py:128` | Use as S2B2S flush timeout for trailing audio after speech ends |
| `max_segment_duration` | 20 seconds | `audio_capture.py:111` (default) | Upper bound for a single ASR segment / conversation turn |
| `min_segment_duration` | 5 seconds | `audio_capture.py:111` (default) | Minimum audio before the pause trigger can fire |
| `min_silence_segment` | 1.0 second | `audio_capture.py:176` | Minimum audio before the silence timeout can fire |
| `overlap_duration` | 3.0 seconds | `audio_capture.py:134` | Intended (but unused) overlap. If implemented, this is the target value |
| `sample_rate` | 16000 Hz | `audio_capture.py:34` | Standard for Parakeet/Whisper ASR models |
| `vad_frame_duration` | 30 ms | `audio_capture.py:39` | VAD processing granularity |
| `blocksize` | 1024 samples | `main.py:153` | PortAudio callback granularity (~64ms at 16kHz) |
| `vad_aggressiveness` | 2 (0-3) | `audio_capture.py:35` | webrtcvad-specific; maps roughly to medium sensitivity |

---

## Appendix B: How This Informs S2B2S Conversation Turn Detection

The three-trigger endpointing policy from Parakeet-RT maps directly to S2B2S conversation mode as follows:

1. **"Natural pause" → Turn end detection.** When the user is speaking and pauses for >=0.8s, S2B2S should treat this as the end of their conversational turn, send the transcribed text to the Brain, and begin TTS playback. The 0.8s threshold is empirically validated by Parakeet-RT design and documented as a Parakeet best practice.

2. **"Max duration" → Turn length cap.** In conversation mode, if a user speaks continuously for >=20s (or a configurable S2B2S equivalent), force-end their turn to prevent the LLM from waiting indefinitely. This ensures conversational pacing.

3. **"Silence timeout" → Flush trailing audio.** After the user turn ends (speech inactive), if there is >=1.5s of silence, flush any remaining audio to ASR and send the final text to the Brain. This catches short trailing utterances like "right?" or "you know?" that would not trigger the 5s minimum for the natural-pause trigger.

**Implementation strategy for S2B2S:**
- Port the three independent conditions to Rust in `transcription_coordinator.rs`
- Replace hard-coded constants with user-configurable settings (expose in the Settings UI)
- Add a state reset distinction: pause/silence triggers reset speech-detection state; max-duration triggers do NOT (speech may continue in next turn)
- Wire the Brain invocation to the turn-end event rather than to raw VAD state

---

## Appendix C: File Line Count Summary

| File | Lines | Role |
|------|-------|------|
| `database.py` | 534 | Database manager (largest module) |
| `export.py` | 352 | Markdown export CLI |
| `combine.py` | 319 | Segment combination CLI |
| `main.py` | 229 | Main orchestrator + threading |
| `audio_capture.py` | 193 | AudioCapture + AudioSegmentManager |
| `sentence_processor.py` | 178 | Duplicate filter + sentence processing |
| `transcription.py` | 126 | NeMo ASR engine |
| `README.md` | 218 | Project readme |
| `requirements.txt` | 9 | Dependencies |
| `.env.example` | 3 | Environment template |
| `.gitignore` | 103 | Ignore patterns |
| `setup.sh` | 34 | Installation script |
| `docs/troubleshooting.md` | 500 | Troubleshooting guide |
| `docs/api.md` | 401 | API reference |
| `docs/usage.md` | 267 | Usage guide |
| `docs/setup.md` | 155 | Setup guide |
| `docs/DATABASE_SETUP.md` | 100 | Database setup guide |
| **Total Python source** | **1,931** | Core application |
| **Total documentation** | **1,641** | README + docs/ |
| **Total** | **3,572** | All human-authored content |
