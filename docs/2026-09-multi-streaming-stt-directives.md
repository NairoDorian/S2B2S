# Multi Streaming STT — the directives, and what was built

Written 2026-09-22. This is the record of what was asked for the nested
**Experimental Multi Streaming STT** mode, in the words it was asked in, and the
code that answers each directive. It is the companion to
`2026-09-build-lanes-per-model-backends-and-multi-streaming.md` (§4 is the
design; this file is the requirements and their disposition) and to
`MULTI_STT_STREAMING.md` (the coordinator the mode runs inside).

Convention, same as the earlier directive records: a directive is quoted
verbatim, then followed by **what it means**, **where it lives** and **how it
was tested**. Nothing here is aspirational — every entry describes code in the
tree at the date above.

---

## D1 — Two live texts, and a merged third block under them

> the new multi streaming stt mode should show the 2 live streaming
> transcriptions into 2 text blocks in the overlay, an then in a 3rd text blok
> under the 2 other text block, this 3rd texto block serve as the final merged &
> cleaned multi streaming stt

**Meaning.** The overlay gets one text block per running streaming model, and
_below_ them a block holding the merge-and-clean output — the debug view's
shape, and the "3 blocks" of the later directive D6 with two models.

**Where it lives.**

- The blocks are the overlay's existing stream-text path, keyed by stream slot:
  `StreamTextEvent.slot: Option<u8>` (`managers/transcription.rs`). Model 1 is
  slot `0` (`PRIMARY_STREAM_SLOT`), model _n_ of the Multi-STT list is slot
  _n_ (`multi_streaming::stream_slot_of`), and `STREAM_SLOTS = 4` sizes every
  per-slot array in the manager so a fourth model has somewhere to run.
- The merged block rides `MERGE_BLOCK_SLOT = STREAM_SLOTS as u8` (= 4,
  `multi_stt_stream.rs`): a slot _no model can occupy_, so the merged text can
  never be drawn as one more model column. It is published by
  `publish_merged_block` and carries `whole_session: true`.
- The overlay (`src/overlay/RecordingOverlay.tsx`) reads the two apart from the
  event's own shape, not from a numeric range: `slot` present **and**
  `whole_session` → the merged block (`setMergeText`); `slot` present without it
  → one model column (`applyExtraText(slot, …)`); `slot` absent → the production
  path, untouched. `extraColumns()` renders one `.stext-col` per model, in slot
  order, each with its `1`/`2`/… mark; `.smerged` renders below them, marked
  `M`, in the accent colour.

**Tested.** The discriminator is deliberately not a setting
(`debugStream()` answers from what the backend actually sent), so a build that
emits the block draws it and a build that does not, does not.

---

## D2 — A pause in the speech triggers the merge and clean

> every breaks pause in the speech during that mode trigger the merge and clean
> over the 2 streaming live transcriptions done by the stremaing models

**Meaning.** The merge is the parent mode's pause-triggered merge, over live
texts instead of re-decoded ones.

**Where it lives.** The pause test, the chunk lifecycle, the retry, the failure
badge and the merge call are the parent coordinator's, unchanged:
`multi_stt_stream::Coordinator`, armed with `TextSource::Live`. What the nested
mode changes is one thing — where a chunk's per-model texts come from:

| `TextSource` | A chunk's extra texts                              | Cost per break            |
| ------------ | -------------------------------------------------- | ------------------------- |
| `ReDecode`   | each extra batch-decodes the chunk window, cropped | N decodes + 1 merge       |
| `Live`       | each extra's own live text for the chunk's span    | 1 merge, **no inference** |

A live text needs no cropping: the streams are cut on the same breaks, so
`strip_context_prefix` is not applied and
`multi_stt_streaming_context_chunks` is not read — there is no decode to feed a
context window to.

**Tested.** `multi_stt_stream::start(app, tm, rm, primary_supports_streaming,
TextSource::Live, &extra_slots)` vs `TextSource::ReDecode, &[]` is the only
difference between the two modes' arming calls in `actions.rs`; the arming log
names the source, so a session's log says which one ran.

---

## D3 — It is the original experimental mode, with streaming models

> it's basically a modification of the original experimental mode but only to
> work with stremaing models and to trigger the merge and clean promt at the
> breaks / pause and fill that 3rd text block with the output of the merge and
> clean steps ... this is like the debug mode of this mode

**Meaning.** Not a second mode with its own rules — the same mode, restricted to
models that stream.

**Where it lives.**

- One coordinator, two `TextSource`s (above). There is no second coordinator and
  none was written: `multi_stt_stream.rs` holds the only one.
- `src-tauri/src/multi_streaming.rs` is the part the coordinator cannot do for
  itself, and all it does: decide which slots stream, lease each extra's engine
  for the session, open each stream when its model is up, and give the engines
  back at the end.
- Only **streaming-capable** slots take part (`streaming_slots` →
  `catalog … supports_streaming`). A non-streaming slot is **skipped rather than
  loaded** — the point is a live text to merge, and the model would otherwise sit
  in memory for nothing. The preload is narrowed to the same list (the `keep`
  closure in `actions.rs`), so the preload and the session cannot disagree.
- Refusals degrade honestly: no streaming slot, a non-streaming primary, or no
  merge prompt → `multi_streaming::start` returns false and the caller arms the
  parent mode instead, which may then arm with its batch extras.

---

## D4 — Production shape: one block, corrected in place, merged text pasted

> but then the final results should actually look more like the original
> experimental mode where only 1 text block exists in the overlay ... it's really
> supposed to be an extension of that first experimental mode, but only with
> streaming models so that it does not trigger any other transcription during the
> break it uses the live stremaing text of the streamings models, 2 or more by
> the way, so it corrects the 1st txt block in the same way the 1st experimental
> mode did ... the final transcription pasted will finally be the final results
> merged and cleaned also

**Meaning.** Debug off → exactly one block, which is model 1's live text
corrected in place at every pause, and the pasted result is the merged text.

**Where it lives.**

- `multi_stt_streaming_multi_debug_view` (off by default) is the switch. It
  reaches the coordinator at arm time as `debug_view` and decides **two** things,
  both of which have to agree or the two views disagree about what is on screen:
  1. **What the coordinator publishes.** Debug on → `publish_primary_text` and
     `publish` send the per-slot raw events to the sink and the merged text to
     `MERGE_BLOCK_SLOT`; debug off → the corrected session text goes out on
     `slot: None`, the production event every other consumer already knows.
  2. **Whether the sink is exclusive.** A slot event is dropped when the sink is
     exclusive, so the debug view — which needs the raw per-slot events — runs
     with `exclusive = false`, and the production view keeps the parent's
     exclusivity. `let exclusive = !(source == TextSource::Live &&
settings.multi_stt_streaming_multi_debug_view);`
- With debug off, the single block _is_ the merged text: each closed chunk's
  merge replaces the chunk's span, and the open chunk's live text follows it. The
  text model is the parent's — one document, corrected in place, not appended to.
- The pasted result is the same merged text: with `TextSource::Live` the session
  outcome's final text is the merged text, and `MultiSttAction` forwards it as
  the transcription (the live streams' own results ride alongside for statistics
  and history only).
- **No batch STT anywhere in the session.** No extra decode runs during the
  recording, and `task2`/`extra_model_2..4` remain `None`: `merge_requested` must
  not fall through to the batch-merge branch, because in this mode the merge
  already happened per chunk.

**Cost, which is the reason the mode exists.** A break costs one LLM round trip
and no inference at all, where the parent mode's break costs one decode per extra
model plus the merge.

---

## D5 — "2 or more", not two

> 2 or more by the way

**Meaning.** The mode is not a two-model mode with the third and fourth slots
ignored.

**Where it lives.** `multi_streaming::streaming_slots` collects **every**
streaming-capable slot among models 2, 3 and 4, in the list's order, and the
session opens one stream per slot, one waiter thread per slot
(`EXTRA_MODELS = 3`; with the primary that is up to four live models in one
session). A session with one streaming slot works as two texts; with none it
refuses. The overlay needs no change for this: it draws one column per numbered
event it receives, so three models draw three columns with no code that knows
the number three.

---

## D6 — The debug toggle: N model blocks on top, the merged block below

> really think it through and for developpement and debug purposes add this multi
> streaming stt debug mode toggle to show the 3 blocks of texts to really see in
> details what is happening and once the debug mode is off, it should be 1 text
> block only in the overlay that show the 1st model stremaing and every chinks
> gets merged with the other stremaing transcriptions and cleaned , no batch stt,
> only 2 or more streaming models in 1 text block like the previous experimental
> mode did, but add this debug mode to add 2 or mode text block on top of the
> final one where the merge & clean happen, there are as many blocks of text on
> top of the final main one as there as of streaming models running in this mode,
> this is meant to be a debug mode to show all the streaming models live stt on
> top of the merge and cleaned one that gets aggregated every breaks/pause
> chunks..., so make it all work as I asked please

**Meaning.** One toggle, two views, and the number of blocks on top is the number
of models actually running — not a fixed three.

| Debug view | Overlay                                                                                              |
| ---------- | ---------------------------------------------------------------------------------------------------- |
| **on**     | one block per streaming model (marked `1`, `2`, …) + the merged block (`Merge & Cleaned`) under them |
| **off**    | one block: model 1's live text, corrected in place at every pause                                    |

_A model's block appears as soon as its stream is live — before that model has
said anything — so a running model's block can never be mistaken for a model that
was not started. See D8._

**Where it lives.**

- Setting `multi_stt_streaming_multi_debug_view` (`#[serde(default)] bool`,
  default `false`). It appears in the Multi-STT panel **inside** the nested
  toggle's section (`MultiSttSettings.tsx`), as a `ToggleSwitch` with its own
  `multiStt.streamingFirst.debugView.*` copy.
- The panel's other sliders: the **pause** slider is kept in this mode (the pause
  is what ends a chunk in both forms), the **context** slider is hidden (nothing
  decodes, so a context window means nothing).
- Backend: `debug_view` decides the publish shape and the sink's exclusivity, as
  in D4. One flag, one place, so the two views cannot half-apply.

**Default, and why.** Off: the production shape is what the mode is for, and a
debug view that is on by default is a debug view that ships. The nested toggle
itself is off by default, under a parent mode that is off by default.

---

## D7 — Write the directives down

> write it all don inside md files my directives here like the previous
> directives, and work on this until finished, and test the final build compile
> with bun run dev:fast and commit and push of course

>

> write down all my directives inside the md files, keep up with all my
> instructions and the last one

**Meaning.** This file, plus the status and layout updates in
`2026-09-build-lanes-per-model-backends-and-multi-streaming.md`, and the
`dev:fast` build/commit/push that close the step.

---

## D8 — What the first real run through the debug view found

> with the debug mode I see the merge and cleaned block of text labeled 'M' it
> should be labeled "Merge & Cleaned" and the text block 1 but I don't see the
> text block 2 for the 2nd streaming model loaded , please debug this

**Meaning, in two parts.** (1) Name the merged block rather than lettering it.
(2) A streaming model that is _running_ must have its block, whether or not it
has produced text — the second block was missing, and "missing" and "silent" have
to be different things in this view.

### D8.1 — The label

The merged block's mark is the settings' own name for what the mode does at a
pause, `overlay.mergeAndCleaned` = "Merge & Cleaned", in all 26 locales (English
text, as ever, until translated). Spelled out rather than lettered, because a
letter read against the numerals above it ("M" beside "1" and "2") reads as one
more model whose name starts with M.

### D8.2 — The second block, and why it was not there

Two independent causes, both fixed:

1. **The view hid empty columns.** A model's column was rendered only once it had
   text. A model that is streaming and silent therefore had no column at all —
   indistinguishable from a model that was never started, which is the one thing
   this view exists to tell apart. Now a column appears with the stream: the
   backend announces each extra slot the moment its stream is live
   (`announce_stream_slot`, an empty-text event on that slot, gated exactly like
   every other numbered-slot event so the production view never sees it), and the
   overlay renders a column per known slot with a muted placeholder, "no live text
   yet" (`overlay.awaitingText`), while it is empty.
2. **The model was genuinely transcribing nothing** — see D8.3. The view was
   telling the truth; there was simply no way to see it.

With both fixes the two cases stop looking alike, and the log says which one it
is: `Live streaming transcription started (model '…', slot 1, …)` is logged when
the extra's stream goes live, and `finish_extras` already logs `'…' never
streamed on slot 1, so the merge has one text less` when it never did. A live
stream that stays empty is therefore the language-hint case, and a missing stream
is named as such.

### D8.3 — A silent extra is almost always a language hint, not a broken stream

Reproduced against the fork's own CLI with `jfk.wav` and the configured extra,
`nemotron-3.5-asr-streaming-0.6b`:

| Language hint | Streaming result                                     |
| ------------- | ---------------------------------------------------- |
| `en-US`, `en` | full transcript                                      |
| `fr`, `fr-FR` | empty                                                |
| `de-DE`       | empty                                                |
| `es-ES`       | the full English transcript (_not_ empty — the trap) |

Offline it is the same story, and `german.wav` with `de-DE` transcribes fully.
The model is prompt-conditioned — it is _told_ which language to transcribe, and
its own doc says a language **must** be provided. Speech in one language under
another language's hint yields nothing, and an unrelated hint can yield the
speech back unchanged. So the second block was empty because the Multi-STT panel
had model 2 set to `fr` while the speech was English.

Nothing in the fork was wrong, and nothing in the mode was wrong. What was wrong
was that the app said nothing about it: `finalize_stream_on` now warns when an
extra stream that ran the whole session produced no text, naming the slot, the
model, and the language hint it streamed under, and pointing at that model's
language in the Multi-STT settings. In the production view this warning is the
only signal there is — a silent extra has no column there, and its absence
surfaces only as a merge that quietly had one text less.

### D8.4 — Found while in there, changed on purpose

`transcription.rs` applied the panel's per-slot language/translate preferences
only to `EXTRA_STREAM_SLOT` (`1`) rather than to every extra slot, so a third or
fourth model would have streamed with the primary's settings — the exact failure
the code's own comment said it was preventing. Now gated on any non-primary slot,
and the single-use constant is gone. The mapping slot→model and model→settings is
pinned by a test, because it is read in both directions now (settings on the way
into a decode, model on the way back out of a finished stream) and a disagreement
would be silent.

### D8.5 — Found while in there, deliberately _not_ changed

`native_streaming_latency_presets` is keyed by full file name, and the user's map
holds `…-Q6_K.gguf` → `fastest` while the configured extra is `…-Q8_0.gguf`. The
key never matches, so the extra runs at the `Accurate` default, which for this
checkpoint is its own `att_context_right = 13` — 1.12 s of lookahead, the largest
of the model's four `att_context_size_choices`. That is consistent with the
coordinator's own complaint when a chunk is still owed its text after a break
(`chunk N was still owed its own text … (grace 2.5s)`), though the grace is also
about compute and the link is not proven. Lowering the right context would cut
the lookahead and cost accuracy, which the standing constraint below forbids, so
this is reported rather than changed: the fix is to key these presets by model
**variant** instead of by quantised file name, which is a settings-shape decision
and not this step's.

---

## D9 — Fix the end of `build:fast`

> can you help me fix the issue at the end please ?

**Meaning.** The pasted `bun run build:fast` built the app and the MSI and then
aborted in the NSIS step, so the installer the release exists for was never
produced. Fix that end of the command.

**Where it lives.** Two lines in `src-tauri/nsis/installer.nsi`, the file
`bundle.windows.nsis.template` points the bundler at. `build:fast` failed with
`macro named "RestartManager_StartSession" not found` at
`CheckIfAppIsRunning`'s first expansion, because the repo's template is a fork of
the tauri-v2.9.1 one while the bundler's own `utils.nsh` is rendered from the
installed CLI (3.0.0-alpha.2) and now inserts three macros from
`RestartManager.nsh`: the include that defines them was missing, and the include's
macro needs an absolute path at both call sites. §5 of
`2026-09-build-lanes-per-model-backends-and-multi-streaming.md` is the full
account — the CLI template's 18-hunk diff (the embedded one, extracted from the
binary), which hunks are needed and which are deliberately left, and the
two-stage verification.

**Tested.** `makensis` on a copy of the rendered script (the cheap loop: about a
minute against the bundler's eight) compiles and writes a 30.9 MB
`nsis-output.exe`, and the full `bun run build:fast` re-run then reports
`Finished 2 bundles at:` with the `.msi` and the `-setup.exe`, which installs the
app. The command still exits 1 one step past the bundles, on the Tauri updater
signing — `~/.tauri/zer0.key` is not on this machine, so the pubkey pinned in
`plugins.updater.pubkey` has no private half to sign with. That ending is
pre-existing and already documented in BUILD.md; it is a release secret only the
maintainer can supply, so it is reported and untouched. The template's own header
comment carries the merge instruction and now names what was merged and why, so
the next Tauri bump re-diffs rather than re-discovers.

---

## Standing constraints this work was held to

- **"never commit something that would make current models slower in anyway"** —
  nothing in this change touches an existing path's speed. The nested mode is
  off by default; with it off every path is the one it was, including the
  parent mode's (`TextSource::ReDecode`, exclusive sink, no extra slots). With it
  on, a session does _less_ work per break than the parent mode does: the extras
  are already streaming, so the re-decode is gone.
- The **sched-free leak fix** stays untouched, as ever.
- The **multi-STT serializer** was correctly reverted and is not re-added here.
- Read-only reference trees stay read-only; this work is in `Handy_V2`.

## Not in this step

- Per-model backend selection for the streaming extras (§2 of the build-lanes
  note) — the extras still load on the global accelerator setting.
- Release profiles for the new toggle beyond `#[serde(default)]` — the field
  needs no schema bump, and `bindings.ts` regenerates at debug-build startup.
