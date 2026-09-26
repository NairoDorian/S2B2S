# Plan: Parakeet Ultra / Redux in ZER0, R2T2 Q4_K_M fix, and the 2026-09-26 transcribe.cpp update

Status: **parts 1–4 landed** (catalog, R2T2 Q4_K_M hosting and build flags in
`d4e93372`; transcribe.cpp now pinned at `ba949120`). Part 5 and the §6 runtime
checks (downloads, per-backend runs, benchmarks) are still open. Written
2026-09-26 for the next agent.
All facts below were checked against this tree and against the fork on that date.
Line numbers are approximate; search for the quoted symbols.

The work comes in five parts:

1. Pull the new transcribe.cpp. Nothing else works until this is done.
2. Add two catalog entries (parakeet-ultra, parakeet-redux).
3. Fix the R2T2 Q4_K_M entry, which cannot download today.
4. Build flags.
5. Optional follow-ups: the parakeet→R2T2 draft prior, and scores.

Read `AGENTS.md` first (repo conventions), then
`docs/CONFUCIUS4_R2T2_INTEGRATION.md`, which shows how a hand-authored catalog
entry works and why it matters.

---

## 0. Where things live (orientation)

| Concern                                              | File                                                                                                                                                                                                           |
| ---------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Catalog baked into the binary                        | `src-tauri/src/catalog/catalog.json` (`include_str!`), loader `src-tauri/src/catalog/mod.rs`                                                                                                                   |
| Catalog generator                                    | `scripts/gen_catalog.py`. `ORG = "handy-computer"` repos come from HF cards; third-party repos come from `AUTHORED_MODELS` (R2T2 is the only one today)                                                        |
| Model discovery (downloads, `models/` dir, HF cache) | `src-tauri/src/managers/model.rs`: `discover_custom_transcribe_models`, `discover_hf_cache_models_in`, `native_streaming_latency_kind`                                                                         |
| App models folder                                    | `%APPDATA%\com.nairodorian.zer0\models\` (`portable::app_data_dir(..).join("models")`). It currently holds `r2t2-q4_k_m.gguf`                                                                                  |
| GGUF header probe (for uncatalogued files)           | `src-tauri/src/managers/gguf_meta.rs`. It reads the KV block only and never tensor infos, so the new ggml type id 96 used by redux is harmless here                                                            |
| transcribe.cpp dependency                            | `src-tauri/Cargo.toml`: `transcribe-cpp`, git `NairoDorian/transcribe.cpp`, `branch = "main"`; features `dynamic-backends`, `cuda`, `vulkan` on Windows x64 and Linux                                          |
| Pin refresh                                          | `scripts/check-transcribe-deps.ts`, run before every `bun run tauri dev*` / `build:*`. It `git ls-remote`s the fork and `cargo update -p transcribe-cpp -p transcribe-cpp-sys` when the fork's `main` is ahead |
| Build lanes                                          | `scripts/tauri-runner.ts` + `scripts/build-options.ts` (`FAST_BUILD_OPTIONS`, `FULL_BUILD_OPTIONS`); native flags go to `transcribe-cpp-sys/build.rs` via `TRANSCRIBE_CMAKE_ARGS`                              |
| Streaming chunk control (R2T2)                       | `NativeStreamingLatencyKind::R2T2ChunkMs`, chosen by id hint `"confucius4-r2t2"` in `native_streaming_latency_kind()`                                                                                          |

How a file becomes a model in the app:

1. **Catalog default quant:** listed as a normal downloadable model.
2. **Any other catalog quant found in `models/`:**
   `discover_custom_transcribe_models` → `catalog::file_in_catalog(filename)` →
   surfaces with the full catalog card (name, languages, scores). It is matched
   **by filename only**, so the file just has to sit in `models/` under its
   published filename.
3. **An uncatalogued `.gguf` in `models/`:** header probe → "Not officially
   supported" custom entry with no scores.
4. **HF cache:** catalog quants (same repo) surface with the catalog card; other
   files are probed.

So a catalog entry is both the download link and the model card. It also makes
the manual "drop the file next to `r2t2-q4_k_m.gguf`" workflow show the proper
card.

---

## 1. Pull the new transcribe.cpp (hard prerequisite)

`src-tauri/Cargo.lock` pins `transcribe-cpp` at
`8a12cf66146f12c992195aa23fdd4f010e29e556`. The fork's `main` is **16 commits
ahead**, and none of this session's work is in the pin:

| Fork commit            | What ZER0 gains                                                                                                                                                                                                                                                                                                                                      |
| ---------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `81022f38`             | ggml type **TQ1_G128** (id 96, patch `patches/ggml/0003`). **Redux cannot load without it.**                                                                                                                                                                                                                                                         |
| `0611d23f`             | parakeet-ultra / parakeet-redux support in the parakeet arch                                                                                                                                                                                                                                                                                         |
| `c20c4bfa`             | parakeet GPU fixes: graphs stay on GPU, flash-attn probe, GLU fix, Vulkan batch fix; CPU repacked GEMM; ternary runtime layouts (CUDA → Q2_0, CPU/Vulkan → Q4_0, lossless)                                                                                                                                                                           |
| `31deedf0`             | Python tooling UTF-8 fix (no app impact)                                                                                                                                                                                                                                                                                                             |
| `77895e09`             | **`GGML_CUDA_GRAPHS` ON by default** (CMake + the `-sys` crate passes it explicitly), `TRANSCRIBE_SPEC_PRIOR_TEXT` prototype, `transcribe::env::utf8()`                                                                                                                                                                                              |
| `8e50b61f` + follow-up | R2T2 CUDA: KV-window trim in flash-attn, MMVQ row blocking, prior draft K=5; packed Q/K/V and encoder flash made **opt-in** after they flipped 14/100 transcripts. Drift-free default: **decode 1.25×, total 1.22×** vs the previous build, **100/100 FLEURS clips byte-identical** (fr/de/en/es/it). Q4_K_M on CUDA: 66–70× realtime on a 29 s clip |

**Action**

1. Stop any running ZER0 dev app. A running app locks `transcribe.dll`, and cargo
   then fails with os error 32.
2. Run `bun run tauri dev:fast`. The pin refreshes automatically. Confirm that
   `Cargo.lock` now shows the fork's current `main` SHA for both
   `transcribe-cpp` and `transcribe-cpp-sys`.
3. Expect a full native rebuild (CUDA kernels included), which takes several
   minutes.
4. Commit the `Cargo.lock` change.

**Verify**

- The startup log shows the CUDA backend.
- R2T2 still streams: offline, primary stream, and Multi-STT column.
- Language hints still work. (An empty second Multi-STT column is a
  language-hint trap, not a streaming bug.)

---

## 2. Parakeet Ultra and Parakeet Redux

### 2.1 What the models are

Both are 0.6B Parakeet-TDT (FastConformer encoder, TDT decoder). They load as
`general.architecture = "parakeet"`, which the app already knows. They cover the
same 25 European languages as `parakeet-tdt-0.6b-v3`, with
`stt.capability.lang_detect = true` and `streaming = false`.

- **Ultra:** Moondream's fine-tune (`moondream/parakeet-ultra`).
- **Redux:** Moondream's native-ternary version (`moondream/parakeet-redux`). Its
  encoder linears are stored as **TQ1_G128** (1.75 bits per weight). Every file
  is smaller than the upstream checkpoint.

Public repos (by the user, `Nairod785`) with README + QUANTIZATION.md:

- https://huggingface.co/Nairod785/parakeet-ultra-gguf
- https://huggingface.co/Nairod785/parakeet-redux-gguf

Measured quality and speed (transcribe.cpp; RTX 4070 Laptop + i9-13900H, 29 s
German clip, warm):

|                | FLEURS-fr WER (676 utts) | CUDA     | CPU    | Vulkan   |
| -------------- | ------------------------ | -------- | ------ | -------- |
| ultra F16      | 4.65 % [4.22–5.13]       |          |        |          |
| ultra Q8_0     | 4.62 % [4.21–5.11]       | ~255× RT |        | ~190× RT |
| ultra Q4_K_M   | 4.98 % [4.54–5.46]       |          | 26× RT |          |
| redux TQ1_F16  | 8.32 % [7.77–8.90]       |          |        |          |
| redux TQ1_Q4_K | 8.18 % [7.63–8.77]       |          | 21× RT |          |

These WER numbers are the corrected ones. Earlier published numbers were
~1.6–1.8 pp too high because of a reference-encoding bug, fixed 2026-09-26.

### 2.2 Exact files (pinned revisions, verified 2026-09-26 via the HF API)

`Nairod785/parakeet-ultra-gguf` @ `b03613ba54a195238f0e915359f5a5c78269ddc6`

| filename                        | quant  | size_bytes | sha256                                                           |
| ------------------------------- | ------ | ---------- | ---------------------------------------------------------------- |
| parakeet-ultra-0.6b-Q4_K_M.gguf | Q4_K_M | 485425632  | 1865a03092b566251a9a0a7cc1036872821225047759e1f73fa476694bb453f5 |
| parakeet-ultra-0.6b-Q5_K_M.gguf | Q5_K_M | 548946400  | 2a943b4574664abc96b2dbb2b46cd149bc162fda41080febbf5f949db73ad055 |
| parakeet-ultra-0.6b-Q6_K.gguf   | Q6_K   | 610342368  | e1c6c0860397473dc4b7831e2da70fa53f5d472d1791ae174982897d3d60ab6a |
| parakeet-ultra-0.6b-Q8_0.gguf   | Q8_0   | 739508704  | 283562ac9b513f39244fe23c6632738c167d32731a5f4693319a10ca498550a8 |
| parakeet-ultra-0.6b-F16.gguf    | F16    | 1255869984 | 06d3d511e03b2f36aac831f11ce05d088fd071e0fa5685dc70cda1cc7a4a04e4 |

`Nairod785/parakeet-redux-gguf` @ `87cbc354ce32bc9fe144b5b7bcdd9c68538907a9`

| filename                          | quant    | size_bytes | sha256                                                           |
| --------------------------------- | -------- | ---------- | ---------------------------------------------------------------- |
| parakeet-redux-0.6b-TQ1_Q4_K.gguf | TQ1_Q4_K | 156696672  | 24a8b9af6ab1fd05eb33d5e8fc5b00c7459af0108ac397a9635267ea4e814374 |
| parakeet-redux-0.6b-TQ1_Q8_0.gguf | TQ1_Q8_0 | 159121504  | 74f43ba852479e86e29df92cdbc89aa8215c7e8070f711be424ff466415b6184 |
| parakeet-redux-0.6b-TQ1_F16.gguf  | TQ1_F16  | 179312288  | 98f34a4dee8c5cf82a251281717851feda84a3291052e819809706c76bbb758f |

Re-check before committing:

```
curl -s "https://huggingface.co/api/models/Nairod785/parakeet-ultra-gguf?blobs=true"
```

The `sha` field is the revision, and each sibling carries `size` and `lfs.sha256`.
A README-only commit changes the revision but not the files, so either revision
works. Pin the newest one.

Default quants:

- **ultra:** `Q8_0`. Same as `parakeet-tdt-0.6b-v3`. WER is at the F16 level
  (4.62 vs 4.65 %), and it is the file measured at ~255× on CUDA.
- **redux:** `TQ1_Q4_K`. It is the smallest (157 MB) and measured the lowest WER
  of the three (8.18 %). The three redux files differ only in how the small dense
  remainder is stored.

### 2.3 Add them to `scripts/gen_catalog.py` → `AUTHORED_MODELS`, then regenerate

Do **not** hand-edit only `catalog.json`: the next generator run drops anything
not in `AUTHORED_MODELS`. That already happened to R2T2's Q4_K_M row (see §3).

Template, filling the fields exactly as the R2T2 entry does:

```python
{
    "id": "Nairod785/parakeet-ultra-gguf",
    "revision": "b03613ba54a195238f0e915359f5a5c78269ddc6",
    "slug": "parakeet-ultra-0.6b",
    "name": "Parakeet Ultra 0.6B",
    "architecture": "parakeet",
    "family": "parakeet",
    "parameters": "0.6B",
    "description": "Moondream's Parakeet TDT fine-tune: v3's 25 European languages, lower WER",
    "base_model": "moondream/parakeet-ultra",
    "license": "cc-by-4.0",
    "language_count": 25,
    "languages": ["bg", "hr", "cs", "da", "nl", "en", "et", "fi", "fr", "de", "el", "hu", "it",
                  "lv", "lt", "mt", "pl", "pt", "ro", "ru", "sk", "sl", "es", "sv", "uk"],
    "capabilities": {"streaming": False, "translate": False, "lang_detect": True, "timestamps": "token"},
    "speed_score": 79,      # PROVISIONAL: same graph and size as parakeet-tdt-0.6b-v3 (79)
    "accuracy_score": 88,   # PROVISIONAL: v3's value until the reference-machine WER run
    "files": [ ...5 rows from §2.2... ],
    "default_quant": "Q8_0",
    "recommended": False,
    "recommended_rank": None,
},
{
    "id": "Nairod785/parakeet-redux-gguf",
    "revision": "87cbc354ce32bc9fe144b5b7bcdd9c68538907a9",
    "slug": "parakeet-redux-0.6b",
    "name": "Parakeet Redux 0.6B",
    ...same architecture / family / languages / capabilities...,
    "description": "Native ternary Parakeet TDT (1.75 bpw encoder): 157 MB, 25 European languages",
    "base_model": "moondream/parakeet-redux",
    "speed_score": 75,      # PROVISIONAL: CPU 21x vs ultra's 26x on the same host
    "accuracy_score": 80,   # PROVISIONAL: WER ~3.5 pp above ultra on FLEURS-fr
    "files": [ ...3 rows from §2.2... ],
    "default_quant": "TQ1_Q4_K",
},
```

Checks while doing it:

- **Token timestamps:** check `"timestamps": "token"` against what the parakeet
  arch reports for these files at load (`transcribe_capabilities`). Ultra/redux
  use the unchanged TDT head, so it should match v3.
- **Ternary quant labels:** `"TQ1_Q4_K"`, `"TQ1_Q8_0"` and `"TQ1_F16"` are free
  strings. Grep the frontend (`src/components/model-selector/*`, `src/lib/*`) for
  any quant whitelist, sort order or regex that assumes `Q\d` / `F16` / `F32`,
  and make the ternary labels display and sort sensibly (smallest first is fine).
- **Mirrors:** `mirror_fallbacks()` would try `blob.handy.computer`, which does
  not host these repos. It 404s and falls back to HF; the sha256 still governs.
  This is the same situation as R2T2, so no change is needed.
- **Catalog tests:** `cargo test -p <app crate> catalog` runs:
  - `ids_are_unique`
  - `scores_are_normalised_0_to_1`
  - `every_catalog_model_has_mirror_fallbacks_with_hashes`
  - `catalog_architectures_are_known_to_capability_probe`

  All must pass. `parakeet` is already a known architecture.

- **Regenerate:** `HF_TOKEN=$(hf auth token) uv run scripts/gen_catalog.py`
  (see the header of that script for the output path). Diff `catalog.json`: only
  the new entries and the §3 fix should change. If the generator rewrites
  unrelated handy-computer entries (their HF cards moved on), keep those changes
  out of this commit unless intended.

### 2.4 "Files in the same folder as the R2T2 Q4" workflow

Once §2.3 is in, copying any of the eight files above, under its **published
filename**, into `%APPDATA%\com.nairodorian.zer0\models\` makes it appear with
the full card.

- **Default quants (ultra Q8_0, redux TQ1_Q4_K):** these are in
  `predefined_filenames`. Confirm in the UI that they then show as _downloaded_,
  not as a second entry.
- **Local copies from the fork:** they are under
  `C:\Users\Z\Downloads\PROJECTS\transcribe-fork\models\parakeet-{ultra,redux}-0.6b\`
  with the same filenames as on HF, byte-identical to the uploads (sha256 above).
  Copy rather than re-download.

### 2.5 Runtime behaviour to verify (both models)

**Backends:**

- Offline transcription on CUDA, CPU and Vulkan.
- Redux on CUDA should log a Q2_0 runtime layout; on CPU and Vulkan, Q4_0. The
  switch is `TRANSCRIBE_TERNARY_RUNTIME=q4_0|q2_0|native` and is for debugging
  only.

**Language:**

- Auto-detect.
- A forced language (`fr`, `de`).
- A language outside the 25. It should be refused or disabled in the picker,
  like v3.

**Multi-STT:**

- Add ultra as a column next to R2T2. Both are offline in that column; check the
  VRAM budget (see §4.3).
- Parakeet has no native streaming, so it behaves like v3 there.

**CPU thread hazard:**

- The parakeet decoder runs on the host CPU. More threads than hardware threads
  was measured **~60× slower**.
- Make sure ZER0 never passes `n_threads` greater than the logical core count,
  from either settings or Multi-STT.
- Grep `n_threads` in `src-tauri/src/managers/transcription.rs` and
  `multi_stt_stream.rs`.

**Benchmark:**

- Use the `scripts/bench-stt.ts` protocol: exactly 3 runs per configuration,
  discard run 1, average runs 2 and 3.
- Do this on CPU and CUDA for ultra Q8_0 and redux TQ1_Q4_K.
- Replace the provisional scores with the measured ones. The generator formula
  is `speed_from_rtf` / `acc_from_wer`, but only if the same reference-machine
  protocol as the other entries is used. Otherwise keep the provisional values
  and say so in the comment.

**Android/other:** out of scope for ZER0 desktop. The fork has an
`android-arm64` preset if needed later (`docs/tools/android-build.md` in the
fork).

---

## 3. R2T2: the Q4_K_M entry is broken; fix it

`catalog.json` lists `r2t2-q4_k_m.gguf` (1186939968 bytes, sha256 `d740d663…`)
**under `davidxifeng/Confucius4-R2T2-gguf` @ `a8e6b385…`**. Downloads fetch
`resolve/<revision>/<filename>` from the entry's own repo, and **that file does
not exist in davidxifeng's repo**. It exists only in the user's
**`Nairod785/Confucius4-R2T2-Q4_K_M-GGUF`**. So:

- **Clicking download for the Q4_K_M quant fails with a 404, then HF also 404s.**
- It only works today because the user copied the file into `models/` by hand
  (the filename match then surfaces it).
- **The row is not in `gen_catalog.py`'s `AUTHORED_MODELS`.** It was hand-added
  to the JSON, so the next regeneration silently deletes it.

**Fix:** remove the Q4_K_M row from the davidxifeng entry. Add a second
`AUTHORED_MODELS` entry:

```python
{
    "id": "Nairod785/Confucius4-R2T2-Q4_K_M-GGUF",
    "revision": "99cdc8b760b84d1147c2844d4a1f816f30e63fcf",
    "slug": "Confucius4-R2T2-Q4_K_M",
    "name": "Confucius4-R2T2 (Q4_K_M, 1.19 GB)",
    # copy architecture/family/parameters/languages/capabilities/license/base_model
    # from the davidxifeng entry verbatim
    "description": "R2T2 at 1.19 GB: per-tensor Q4_K / Q6_K recipe, multilingual-validated, fastest on CUDA",
    "speed_score": 30, "accuracy_score": 90,   # same provisional values as the Q8 entry until benchmarked
    "files": [{"filename": "r2t2-q4_k_m.gguf", "quant": "Q4_K_M", "size_bytes": 1186939968,
               "sha256": "d740d6636f2ea2f3736800c3c88a6e22ecb6c0f26c567fe22b0572ae9c2c4ec8"}],
    "default_quant": "Q4_K_M",
    "recommended": False, "recommended_rank": None,
},
```

Things that must keep working with the second entry:

- **Streaming chunk control.** `native_streaming_latency_kind()` matches the
  lowercase hint `"confucius4-r2t2"`, which the new id also contains, so the
  80–2000 ms chunk control applies.
  - Confirm with the Q4 selected: the chunk slider shows, and the per-model
    chunk value in `AppSettings::native_streaming_chunk_ms` persists **per model
    id**. The Q4 entry then has its own stored value, defaulting to 320 ms.
  - Decide whether Q4 and Q8 should share one value. `src/lib/modelId.ts` has a
    cross-quant/base-repo lookup, but it keys on the _same repo_, so two repos do
    not share.
- **Tests keyed on the R2T2 id.** `src/lib/modelId.test.ts` and
  `streamReveal.test.ts` mention r2t2. Grep for `davidxifeng` across `src/` and
  `src-tauri/src/` and extend any logic that hard-codes that repo id.
- **Settings migration.** Users who already selected the Q4 through the broken
  row have a stored model id under the davidxifeng repo. Map
  `davidxifeng/Confucius4-R2T2-gguf/r2t2-q4_k_m.gguf` (check the exact stored id
  format in `settings.rs`) to the new id on load. That keeps the selected model
  and its chunk setting.
- **Default quant.** Consider making the Q4 the recommended R2T2 on CUDA. With
  the new fork build it is the fastest configuration: arm M decode is ~1.6×
  faster than Q8_0 on CUDA, and multilingual screens pass (German is the
  discriminating screen). Keep Q8_0 as the davidxifeng entry's default: it is the
  upstream reference file.

### 3.1 R2T2 behaviour changes arriving with the new pin

| Change                                             | What ZER0 needs to do                                                                                                                                                          |
| -------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| CUDA graphs now ON (was OFF in ZER0's CUDA builds) | Nothing. About +15 % decode.                                                                                                                                                   |
| KV trim, MMVQ (default on)                         | Nothing. Output is byte-identical to the old build (100/100 FLEURS clips, 5 languages).                                                                                        |
| Packed QKV (`TRANSCRIBE_QKV_PACK=1`, opt-in)       | Leave off. It is ~3–5 % faster decode, but it uses ~130 MB more VRAM and is not byte-identical.                                                                                |
| `TRANSCRIBE_ENCODER_FLASH=1` (opt-in)              | Leave off: no net speed gain on short clips, and not byte-identical.                                                                                                           |
| `spec_k_drafts` (run param) now accepts up to 16   | ZER0 doesn't set it; the default stays off unless a prior is supplied. Nothing to do.                                                                                          |
| Streaming path                                     | Unchanged: the streaming seed cap stays 8, and the chunk semantics are unchanged. Re-run the R2T2 acceptance list in `CONFUCIUS4_R2T2_INTEGRATION.md` once after the pin bump. |

---

## 4. Build flags ("all the bells and whistles")

### 4.1 What the lanes pass today (`scripts/build-options.ts`)

`FAST_BUILD_OPTIONS` (this machine; `dev:fast` / `build:fast`):

```
cudaArchitectures: "auto"   → sm_89 only (RTX 4070)
-DTRANSCRIBE_VULKAN=ON
-DGGML_CUDA_FA_QUANTS=all
-DTRANSCRIBE_X86_CONSERVATIVE=OFF
-DGGML_NATIVE=ON -DGGML_AVX2=ON -DGGML_FMA=ON -DGGML_AVX_VNNI=ON
modelSet: FULL_MODEL_SET
```

`FULL_BUILD_OPTIONS` (distribution): `default` CUDA matrix, Vulkan, FA quants all,
and a commented-out `// "-DGGML_CUDA_GRAPHS=ON"`.

The `-sys` crate adds, on Windows x64 with `dynamic-backends`:

- `TRANSCRIBE_GGML_BACKEND_DL=ON`
- `GGML_CPU_ALL_VARIANTS=ON`
- `TRANSCRIBE_X86_CONSERVATIVE=ON`, which the fast lane's
  `-DTRANSCRIBE_X86_CONSERVATIVE=OFF` overrides because `TRANSCRIBE_CMAKE_ARGS`
  are applied last
- Release profile with forced `/O2`
- **and now `GGML_CUDA_GRAPHS=ON` for every CUDA build** (fork `77895e09`)

### 4.2 Assessment and recommended edits

| Flag                                                                        | Verdict                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                  |
| --------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `GGML_CUDA_GRAPHS`                                                          | **Now on by default** from the fork. Delete the commented-out line in `FULL_BUILD_OPTIONS` and replace it with a note saying so. Do not add `-DGGML_CUDA_GRAPHS=OFF` anywhere.                                                                                                                                                                                                                                                                                                                                           |
| `GGML_CUDA_FA_QUANTS=all`                                                   | Valid (`ggml/cmake/common.cmake` accepts `all`). It only adds kernels for **quantized KV caches**; transcribe.cpp uses F16 KV by default. It costs compile time and binary size for no runtime gain today. Keep only if a quantized-KV setting is planned; otherwise drop it from both lanes (the default set already covers f16-f16).                                                                                                                                                                                   |
| `GGML_NATIVE=ON`, `GGML_AVX2`, `GGML_FMA`, `GGML_AVX_VNNI` in the fast lane | **Mostly inert.** With `dynamic-backends`, ggml builds `GGML_CPU_ALL_VARIANTS` (x64 … alderlake, each with its own flags) and picks the best variant for the host at runtime. On the i9-13900H that is the AVX-VNNI-capable variant, so VNNI is already used without these flags. MSVC has no real `-march=native`. Harmless; leave them or remove them for clarity. To prove what runs: the startup log names the chosen CPU backend variant, and `llvm-objdump -d ggml-cpu-*.dll \| grep -c vpdpbusd` shows VNNI code. |
| `TRANSCRIBE_X86_CONSERVATIVE=OFF` in fast                                   | Fine for this machine's personal build. Never in `FULL` (SIGILL on older CPUs).                                                                                                                                                                                                                                                                                                                                                                                                                                          |
| `cudaArchitectures: "auto"` (fast) / `"default"` (full)                     | Correct.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| `TRANSCRIBE_VULKAN=ON`                                                      | Keep. Redux and ultra run on Vulkan (~190× RT for ultra); it is the fallback for non-NVIDIA GPUs.                                                                                                                                                                                                                                                                                                                                                                                                                        |
| Model set `FULL_MODEL_SET`                                                  | Keep. Parakeet and qwen3_asr are both in it; the `arch-dl` split is off.                                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| LTO / static backends                                                       | Not exposed by the `-sys` crate lanes. The fork's personal max lane (`scripts/bench/build-maxx.ps1 -Lto`) can build them for an A/B. The work is GPU-bound, so expect little; measure before plumbing it into ZER0.                                                                                                                                                                                                                                                                                                      |

Net: after the pin bump, ZER0's CUDA builds get every runtime improvement from
this session with **no flag change required**. The edits above are clean-up
(remove the stale graphs comment; decide on FA_QUANTS).

### 4.3 VRAM budget note (Multi-STT, 8 GB)

Approximate resident sizes:

- **ultra Q8_0:** ~0.75 GB weights.
- **redux TQ1_Q4_K on CUDA:** the Q2_0 runtime layout is lossless but slightly
  larger than the 157 MB file.
- **R2T2 Q4_K_M:** 1.19 GB + KV cache (+ ~130 MB only if `TRANSCRIBE_QKV_PACK=1`). The KV cache is sized
  by `n_ctx`; the log line `~7168 MiB KV max` is the _maximum_ for the full
  context, not what is allocated per run.

Measure actual usage with `nvidia-smi` during a 4-column Multi-STT session
before recommending combinations.

---

## 5. Optional follow-ups (not required for "usable in ZER0")

1. **Parakeet → R2T2 draft prior.** In Multi-STT, parakeet-ultra finishes long
   before R2T2. The fork can use its transcript as a speculative draft:
   byte-identical output with K=5, **~1.84× R2T2 decode** (2.38× at K=15, not
   byte-identical).
   - **Why it isn't usable from the app yet:** the prototype reads the
     process-global env var `TRANSCRIBE_SPEC_PRIOR_TEXT`, which is wrong for a
     multi-session app.
   - **Fork side:** first add a per-run field (e.g. `const char * draft_prior_text`
     in `transcribe_run_params`, with the usual `struct_size` versioning) in the
     fork's own working copy (`PROJECTS/transcribe-fork`, never patched from
     Handy_V2), push it, and let the pin refresh.
   - **ZER0 side:** in `multi_stt_stream.rs` (offline finalize path), pass the
     parakeet column's final text into the R2T2 run.
   - Streaming R2T2 does not benefit; it already uses its own seed.
2. **Scores:** replace the provisional speed/accuracy scores (§2.3, §3) with
   reference-machine measurements.
3. **Docs:** add ultra, redux and the R2T2 Q4 entry to `docs/STT_BENCHMARKS.md`
   and `docs/MULTI_STT_STREAMING.md` once measured.

---

## 6. Acceptance checklist

- [ ] `Cargo.lock` pins the fork's current `main` (≥ `8e50b61f`) for both crates;
      app builds with `dev:fast` and `build:fast`.
- [ ] Catalog: ultra (5 files) and redux (3 files) entries in `AUTHORED_MODELS`
      and in the regenerated `catalog.json`; catalog tests pass.
- [ ] Download ultra Q8_0 and redux TQ1_Q4_K from the app. The sha256 check
      passes, and each transcribes the German and a French sample on CUDA, CPU
      and Vulkan.
- [ ] Copy a non-default quant (e.g. ultra Q4_K_M) into `models/`. It appears
      with the catalog card, not as "Not officially supported".
- [ ] R2T2 Q4_K_M: its own entry under `Nairod785/…`, downloadable (404 gone),
      streaming chunk control present, old selection migrated, and streaming
      acceptance re-run.
- [ ] `build-options.ts` clean-up done (graphs comment; FA_QUANTS decision).
- [ ] Benchmarks: 3 runs, drop the first, average runs 2 and 3; CPU and CUDA.
- [ ] Formatting: this repo's `format:check` hook fails tree-wide for unrelated
      files (41 files, CRLF locale JSONs). If it blocks the commit, commit with
      `--no-verify` and run the Rust steps (`cargo fmt`, `cargo clippy`,
      `cargo test`) and prettier on the touched files, `.md` included, by hand.
