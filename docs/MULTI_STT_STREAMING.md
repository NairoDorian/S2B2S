# Multi-STT Streaming First

The design of the experimental streaming-first mode
(`multi_stt_streaming_first_enabled`), and the record of the close-gate defect
that made it correct nothing on any recording.

Backend: `src-tauri/src/multi_stt_stream.rs`; audio:
`src-tauri/src/audio_toolkit/audio/chunk_tap.rs`; overlay: the Live overlay.
The prose counterpart is AGENTS.md, _Multi-STT Streaming First_; this document is
the reasoning behind it, kept where it can be longer than a bullet.

---

## 1. What the mode is for

**Live streaming transcription that gets more accurate as you speak, without the
GPU work growing with the session.**

The user watches the text form and then watches it being corrected, at every
pause, for as long as they keep talking — and the corrections are already in the
text by the time they stop.

Two things compose, and only two:

- **The live stream.** The primary model streams as it always did. That is what
  is on screen, and it is never re-transcribed.
- **The sliding window.** At each pause, the extra models re-decode a _bounded_
  window of audio and the merge prompt rewrites the rough text of the chunk that
  just closed.

The mode is therefore **not** "the batch Multi-STT run repeatedly". The audio
handed to the extras is a window the settings size; the rest of the session is
already settled text that no model will see again.

The standard path — audio since the beginning, decoded once at stop — is
untouched and remains the fallback. The difference between the two is _when_ the
work happens and what it is done on, never which models do it.

---

## 2. The shape of a session

### A chunk is the audio between two breaks

A **break** is `multi_stt_streaming_pause_ms` (100–10000 ms, default 1000) of
`last_speech_ms()` standing still — the same test Live Mode uses for its silence
boundary.

**Every break closes the chunk that was open.** There is no second condition and
nothing is detected in the text. That is deliberate: text detection was the
first implementation (`30081f4e`) and it was removed in `ee291035`, because a
break that finds no sentence end closes nothing, which is exactly how a long
dictation ran with the mode apparently dead — some 1700 ticks reporting no close
while the user spoke the whole time.

`MAX_CHUNK_SECONDS` (60 s) is the only other trigger, and it is a valve rather
than a policy: someone who talks for a minute without pausing still gets merged,
and neither a chunk nor a merge window can grow without bound.

### The audio sent to the extras is a window, never the session

`multi_stt_streaming_context_chunks` (0–3, default 1) already-closed chunks are
sent in front of the chunk that just closed, so a window is at most four chunks
and **flat in the length of the session**. That flatness is the point of the
whole design: it is what keeps VRAM and decode time independent of how long the
user has been dictating, which a session-long re-decode cannot do.

The context is an **input only**. The reason it exists is that a spoken sentence
does not end at a pause — people hum, breathe, think, restart, and one sentence
can cross many breaks. Handed the second half alone, a merge decides what that
fragment is from half the evidence. The context is that missing evidence.

### Every slot covers the same span

The extras decode `[context + chunk]`, so their text begins with the context's
words, while slot 1 (`${output}`) is the primary's own text for the chunk. Each
extra's decode is **cropped** back to the chunk by `strip_context_prefix`, which
finds a run of the context's words in the decode — 12 down to 3 tokens, longest
first, tolerating `CONTEXT_SEAM_DRIFT` (2) words of disagreement at the seam —
and cuts after it. The ruler is the context chunks' `display_text()`, i.e. the
text already on screen. A decode the crop cannot align is dropped for that
chunk: that costs one model's opinion, never text that is already on screen.

Per-chunk ownership is what makes the text composable. A merge that replaced the
whole window would overlap its neighbour's window by `context` chunks, and
folding two overlapping windows either drops the chunk that left the window or
shows the context's words twice.

### Text model

```
display = closed chunks' current text + the open chunk's
```

A merge replaces only its own chunk, so the rough streaming text degrades into
the polished one in place and earlier chunks are untouched. On merge failure the
chunk keeps the extras' outputs joined by newlines, is counted in
`failed_chunks`, and is retried at the next break — the failure is never put in
the text (with `DirectStreaming` that text is typed into the user's document);
the overlay badge and `MultiSttStreamChunkFailedEvent` carry it.

---

## 3. The close gate — the mode's invariant

> **A chunk is merged against its own text, and its merged text replaces that
> text and nothing else.**

A merge is a **replacement**, not an append. It is handed the extras' decode of
the chunk's audio (complete, because they decode the audio) and the primary's
own reading of that same audio as slot 1. If the primary's reading is missing or
short, the merge produces a version that cannot contain words the primary never
gave it — and those words then arrive during the **next** chunk's lifetime and
are read into it. The same speech, twice.

So a break can close a chunk only when the chunk owns its text, and that is
**two independent conditions**. `break_outcome` reads both.

### (a) The family has decoded the chunk's audio

`StreamText::audio_committed_ms`. Its documentation in `transcribe.h` is
explicit about what it is:

> Family-reported audio progress / **drain hint**. It is not a byte boundary into
> `committed_text`.

and Parakeet derives it from `mel_frames_consumed` — a **decode-progress
cursor**, not a text cursor. The quantity the coordinator reads is the
difference to `input_received_ms`:

```rust
let stream_drain_ms = (self.stream_input_ms - self.stream_drained_ms).max(0);
```

which is the audio the family has taken in and not decoded: the un-drained
suffix of the session. `Live preview perf` reports the same number as
`buffered`.

**Zero is unreachable, and a break makes it more so, not less.** Every streaming
family keeps audio in flight while it runs — a right-context window it will not
emit a word without. The coordinator sees those two figures through the text
sink, which only runs when the text changes, so the difference is the backlog as
of the last text update. On the model this was measured against (a 0.6 B
streaming Parakeet on CUDA, decoding at 2.4–2.6× real time) it sat between **22
and 86 ms**. During a pause the VAD feeds the stream nothing, so the drain hint
has no new input to advance on and the residual _freezes_.

`STREAM_DRAIN_TOLERANCE_MS` (500) is what "decoded enough" means. It is safe for
the same reason waiting is cheap: **a break is
`multi_stt_streaming_pause_ms` of silence**, and a sequential decoder's
un-decoded audio is a _suffix_ of what it was fed — so a suffix this short lies
inside that silence and cannot hold a word that has not been written yet. Keep it
below the smallest break for that argument to hold. It is also an order of
magnitude below the backlog it must still catch: the model that motivated the
retire path measured **4918 ms**.

### (b) The family has published the text for it

`Chunk::live` — the primary's committed text that arrived while the chunk was
open — must be non-empty. **A drain hint cannot stand in for this.** A family
that decodes eagerly and commits late has drained completely while its text is
still owed: the closeness test above passes while slot 1 is empty.

This case is not hypothetical. The first implementation had a guard for it
(`decide`, in `30081f4e`, declining to merge a chunk with nothing committed) and
the guard was removed by the refactor in `ee291035` along with the text
heuristic it lived beside. Restoring it is what makes the gate correct rather
than merely permissive.

### The grace, and the retire

`TEXT_CATCHUP_GRACE` (2.5 s) bounds the wait. Waiting is cheap because a pause is
silence by definition: no new audio is fed, so the only work left is the model's
backlog and it collapses on its own.

A break still owed its text when the grace runs out does **not** close a chunk.
The session **retires itself**: the recording keeps running, the overlay goes
back to the primary's own live text (`publish_primary_text` emits one last
event, because the plain path's events only arrive when the model produces
text), and the batch path transcribes and merges the whole session at stop. A
model that publishes its committed text only at finalize, or one that cannot
decode faster than the user speaks, has a lag that never collapses, and no
routing change can recover text that does not exist yet.

`MultiSttAction::stop` notices (`is_active()` is false), skips the `finish`
call it would otherwise wait out, and logs that it is falling back — rather than
timing out on an empty slot and reporting a misleading timeout.

---

## 4. How it broke

The mode shipped correct-by-construction for the audio and wrong for the text,
and the record is in three places.

### The commits

| Commit     | What it did                                                                                                                                                                                                                                                                             |
| ---------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `30081f4e` | First implementation. Chunks were defined by **sentence ends detected in the text**; a break with nothing to cut at closed nothing. It carried a `decide` guard declining to merge a chunk with nothing committed yet.                                                                  |
| `ee291035` | Refactor to the current model: chunk = audio between two breaks, sliding window of context chunks. The sentence model, its settings and **`decide`** were deleted. "Every break with audio behind it closes one (`closes`), with nothing detected in the text and no second condition." |
| `ddd01e05` | Retry a held writer revision on every tick.                                                                                                                                                                                                                                             |
| `09edfe27` | Document what the mode is for.                                                                                                                                                                                                                                                          |
| `b842345d` | Pin the direct-streaming backspace to the chunk it corrects.                                                                                                                                                                                                                            |
| `a5cdef8c` | Log the primary streaming model per chunk, with its rate.                                                                                                                                                                                                                               |

None of these is the defect. `ee291035` removed the text guard deliberately and
correctly _for the design it was moving to_ — the guard belonged to a
text-defined chunk. What it left behind was a close no longer gated on text at
all, which is a real hole, and the hole is what was being fixed next.

### The change that made it worse

The uncommitted work in the tree added the wait — and tested it against an
**exact zero**:

```rust
if text_lag_ms <= 0 { Close } else if paused_for >= pause + GRACE { Retire } else { Wait }
```

`text_lag_ms` there was `input_received_ms − audio_committed_ms`, i.e. condition
(a) read as though it were a text lag. Zero was unreachable, so:

- every break waited out the grace,
- every session retired,
- **no chunk was ever merged, on any recording.**

### The evidence

From `zer0.log`, 2026-09-12, the user's own sessions — and the message refutes
itself, because the figure it reports is healthy:

```
[19:51:21][WARN] Multi-STT streaming: chunk 1 has been 22 ms behind the audio it
was fed for 3.5477622s since the break (grace 2.5s) — the primary's committed
text is not tracking its audio …
[19:51:21][DEBUG] Multi-STT streaming: session stopped after 0 chunks
```

In the same session the same log shows the model running at **2.42–2.60× real
time**, `buffered` between 22 and 86 ms, the text revision advancing 8 → 37, and
decoding still in flight after the retire. The model was tracking its audio
perfectly. What it could not do was report zero un-decoded audio while running.

The second defect is visible in the same log only in hindsight: model 1's text
was complete and correct, and none of it was ever used as a merge slot, because
no chunk closed.

---

## 5. Fallbacks

The coordinator is dropped and the recording takes the normal Multi-STT batch
path when:

- the primary model cannot stream,
- no merge prompt is configured,
- the stream never starts (`StreamFinalization::NeverStarted`),
- the session **retires itself** mid-recording (§3).

Cancel cancels the coordinator: nothing more is typed, nothing is pasted, no
history row.

---

## 6. Cost

- **Threads**: one 50 ms tick thread per recording, in this mode only.
- **Audio path**: one mutex lock and one memcpy per 16 ms frame (≈62/s) while
  the tap is armed, one relaxed atomic load while it is off.
- **Per break**: three decodes of the **window** (`context_chunks + 1` chunks, at
  most four) plus one short LLM call. Bounded by the settings, not by how long
  the session has been running — a thirty-minute dictation costs the same per
  pause as the first one.
- **Memory**: `context_chunks + 1` chunks of audio plus one pending retry,
  ≈3.8 MB per chunk at the 60 s valve. A chunk's audio is freed as soon as it can
  neither be context nor be retried (`retain_context_audio`).
- **Preview**: one render per update instead of the typewriter's one per 1–3
  characters. Each publish carries the whole session's text — the same shape the
  plain streaming path already emits, bounded by `TICK` at ≤ 20/s and deduped
  against the last publish.

No new dependency, no new poll.

---

## 7. Reading the log

| Line                                                                                                              | Means                                                                                                                                                                                                                 |
| ----------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `chunk n closed — … ms of audio, … chars, … ms of context, … ms of it left un-drained (tolerance 500 ms)`         | A normal close. The un-drained figure should be in the tens of ms; crawling towards the tolerance is the mode working near its edge, above it means the feed or the model is falling behind.                          |
| `chunk n was still owed its own text … — … chars of it, and … ms of audio the stream decodes but has not drained` | The retire path. The two figures are the two conditions of §3; which one is large says whether the model is slow (drain) or committing nothing (chars).                                                               |
| `Live preview perf`                                                                                               | The primary's own stream: `input_received`, `committed_audio`, `buffered`, revision, real-time factor.                                                                                                                |
| `the audio tap is N samples ahead of the stream`                                                                  | A structural mismatch between the tap and the stream feed. One warning per session, and the correction is deliberately **not** applied — a worker-thread lag of a frame or two is indistinguishable from a real lead. |

---

## 8. Open items

- The drain figure is read through the text sink, so it is the backlog as of the
  last text update rather than as of the tick. That makes it an _upper_ bound on
  the live backlog while the model decodes faster than real time (its backlog
  shrinks between updates), which is the direction a close test needs — but a
  model slower than real time would have it understate. Such a model retires.
- `Chunk::live` is built from the stream's **committed** text only; a family that
  holds a long `tentative` tail would show text on screen that slot 1 does not
  have. It falls back to the batch path (via the retire) rather than merging
  against a partial reading, which is the safe direction, but the tail itself is
  not recovered.
