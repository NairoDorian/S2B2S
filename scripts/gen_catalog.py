# /// script
# requires-python = ">=3.10"
# dependencies = ["huggingface_hub", "fsspec"]
# ///
"""
Model catalog generator for this project.

Merges three sources into one catalog.json:
  1. HF card `transcribe_cpp` block  -> capabilities + benchmarks (canonical)
  2. a tiny GGUF header range-read    -> display labels only
  3. local CURATION (this file)       -> recommended set, editorial descriptions

Emits catalog.json to be committed and `include_str!`'d into the Rust binary.
Run:  HF_TOKEN=$(hf auth token) uv run gen_catalog.py [out_path]
"""
import json, os, re, sys, math, struct, datetime
from concurrent.futures import ThreadPoolExecutor, as_completed
from huggingface_hub import HfApi, HfFileSystem

ORG = "handy-computer"
CATALOG_VERSION = 2

# Download sources tried in order after Hugging Face itself. Each entry is a
# base URL; the full file URL is `{mirror}/{repo_id}/{revision}/{filename}`
# (the same three values that form the HF resolve URL, so a mirror is a plain
# static file host). Mirrors are untrusted: every download is verified against
# the per-file `sha256` below, so listing one only affects availability.
MIRRORS = ["https://blob.handy.computer"]

# ───────────────────────── scoring (one constant each) ──────────────────────
SPEED_SCALE = 8.0    # speed = 100·(1 − e^(−rtf/8))     grows toward 100
ACC_SCALE   = 15.0   # accuracy = 100·e^(−wer/15)        decays from 100
SPEED_FLOOR = 10     # no reference-machine rtf (too heavy to benchmark) → floor, it's slow
def speed_from_rtf(rtf):
    return SPEED_FLOOR if not rtf or rtf <= 0 else round(100 * (1 - math.exp(-rtf / SPEED_SCALE)))
def acc_from_wer(wer):
    return None if wer is None else round(100 * math.exp(-wer / ACC_SCALE))

# ───────────────────────── curation (CJ owns; editorial, not derivable) ──────
# slug -> {rank?, rec?, desc?, use_case?, default_quant?, hidden?}.  All optional.
# desc is hand-written UI copy; models without one use factual generated summaries.
# rank = editorial sort position (broad ordering). rec = the small "Recommended"
# badge / onboarding subset — independent of rank, so a model can rank high
# without carrying the recommended tag.
CURATION = {
    "parakeet-unified-en-0.6b":        {"rank": 1, "rec": True, "desc": "Fast, accurate live English transcription"},
    "nemotron-3.5-asr-streaming-0.6b": {"rank": 2, "rec": True, "desc": "Live multilingual transcription across 28 languages"},
    "canary-180m-flash":               {"rank": 3, "rec": True, "desc": "Tiny and instant, runs well on any hardware"},
    "cohere-transcribe-03-2026":       {"rank": 4, "rec": True, "desc": "Highest accuracy, 14 languages, slower"},
    "whisper-medium":                  {"rank": 5, "rec": True, "desc": "Broadest language, but may run a bit slow"},
    # ranked (sorted high) but NOT tagged recommended
    "Voxtral-Mini-4B-Realtime-2602":   {"rank": 6, "desc": "Live multilingual, excellent on powerful machines"},
    "parakeet-tdt-0.6b-v3":            {"rank": 7, "desc": "Fast and accurate. Supports 25 European languages"},
    "parakeet-tdt-0.6b-v2":            {"rank": 8, "desc": "English only. The best model for English speakers"},
    "Qwen3-ASR-0.6B":                  {"rank": 9, "desc": "Excellent multilingual model"},
    "Fun-ASR-MLT-Nano-2512":           {"rank": 10, "desc": "A tiny multilingual model"},
    # description-only (unranked, not recommended) — carried over from the legacy .bin entry
    "Breeze-ASR-25":                   {"desc": "Optimized for Taiwanese Mandarin. Code-switching support."},
    # Sortformer emits speaker segments only; this catalog is for models
    # that produce transcription text.
    "diar_streaming_sortformer_4spk-v2.1": {"hidden": True},
}
# temporary capability corrections pending a card re-push (remove once cards fixed)
OVERRIDES = {
    "granite-4.0-1b-speech": {"timestamps": "none"},
    "granite-speech-4.1-2b": {"timestamps": "none"},
}

# ──────────────── authored entries (published outside ORG) ───────────────────
# Repos that live outside ORG and therefore are never returned by the
# `author=ORG` listing, merged verbatim after generation. They are authored by
# hand rather than derived because every input the pipeline would normally read
# is either absent or actively wrong for them:
#
#   * the HF card carries none of the `transcribe_cpp` capability metadata that
#     supplies `languages` / `capabilities`, so a generated entry would advertise
#     zero languages and no streaming;
#   * `general.architecture` is the audio.cpp package marker `audiocpp`, not a
#     transcribe-cpp arch name — taking it verbatim would fail the Rust
#     `catalog_architectures_are_known_to_capability_probe` gate, so `architecture`
#     is the transcribe-cpp family the model actually loads as;
#   * `general.languages` and `stt.capability.*` are absent from the GGUF, so the
#     language list is transcribed from the publisher's own model spec.
#
# `revision` MUST stay pinned: it is what makes `size_bytes`/`sha256` meaningful,
# since acquisition fetches `resolve/<revision>/<file>`. Sizes and hashes here are
# the verified bytes at that revision. Note the mirror list at the top of this
# file does not host these repos, so a mirror miss 404s and falls back to HF —
# the hash check still governs, as for every other entry.
PARAKEET_V3_LANGUAGES = ["bg", "hr", "cs", "da", "nl", "en", "et", "fi", "fr", "de", "el", "hu", "it",
                         "lv", "lt", "mt", "pl", "pt", "ro", "ru", "sk", "sl", "es", "sv", "uk"]

AUTHORED_MODELS = [
    {
        # NetEase Youdao Confucius4-R2T2 — a Qwen3-ASR-1.7B fine-tune with
        # Longest-Stable-Prefix streaming. Published as an audio.cpp GGUF package
        # (config/tokenizer are embedded in the GGUF's own metadata, which is why
        # one file is a complete model here). This is the upstream reference
        # download point that audio.cpp's own model spec names.
        "id": "davidxifeng/Confucius4-R2T2-gguf",
        "revision": "a8e6b385d7df7eae9519363e07034a209004797a",
        "slug": "Confucius4-R2T2",
        "name": "Confucius4-R2T2",
        # The transcribe-cpp family it loads as, not the GGUF's `audiocpp` marker.
        "architecture": "qwen3_asr",
        "family": "qwen3",
        # Same parameter count as its base (F16 is within 0.02% of Qwen3-ASR-1.7B's
        # F16), so it takes the base's label rather than a re-derived one.
        "parameters": "2.0B",
        "description": "Real-time streaming transcription in 80 ms to 2 s chunks, across 30 languages",
        "base_model": "Qwen/Qwen3-ASR-1.7B",
        "license": "other",
        "language_count": 30,
        "languages": ["zh", "en", "yue", "ar", "de", "fr", "es", "pt", "id", "it",
                      "ko", "ru", "th", "vi", "ja", "tr", "hi", "ms", "nl", "sv",
                      "da", "fi", "pl", "cs", "fil", "fa", "el", "hu", "mk", "ro"],
        "capabilities": {
            "streaming": True,
            "translate": False,
            "lang_detect": True,
            # Partial results only; the streaming decoder emits no timestamps.
            "timestamps": "none",
        },
        # Provisional editorial scores, NOT benchmarked on the reference machine:
        # accuracy inherits the base model's (the streaming fine-tune shares its
        # weights and was trained for exactly this), and speed is placed below the
        # base's 38 because every chunk re-encodes all accumulated audio. Replace
        # both once the reference-machine benchmark covers this model.
        "speed_score": 30,
        "accuracy_score": 90,
        "files": [
            {"filename": "r2t2-q8_0.gguf", "quant": "Q8_0", "size_bytes": 2477512064,
             "sha256": "19f5ccd624484bcb5d44301437de41560b0ecc40c430e8850dfeefefbe82ccf5"},
            # Per-tensor Q4_K / Q6_K recipe, fastest on CUDA. davidxifeng's repo does
            # not host it, so the row names the repo that does: `repo`/`revision`
            # redirect this one file's download while it stays a quant of R2T2.
            # Keep `sha256` last: the one-line-per-file formatter below needs it there.
            {"filename": "r2t2-q4_k_m.gguf", "quant": "Q4_K_M", "size_bytes": 1186939968,
             "repo": "Nairod785/Confucius4-R2T2-Q4_K_M-GGUF",
             "revision": "b1ea19256fb77a8d8ab7b091dc75378e15952605",
             "sha256": "d740d6636f2ea2f3736800c3c88a6e22ecb6c0f26c567fe22b0572ae9c2c4ec8"},
            {"filename": "r2t2-f16.gguf", "quant": "F16", "size_bytes": 4092155264,
             "sha256": "d1b531ceaf5640d98352d3a9180238d99d36d393e160afd4692031077e7bae2c"},
        ],
        # The only variant verified to load and stream natively end to end.
        "default_quant": "Q8_0",
        "recommended": False,
        "recommended_rank": None,
    },
    {
        # Moondream's Parakeet TDT fine-tune, same graph and languages as v3.
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
        "languages": PARAKEET_V3_LANGUAGES,
        "capabilities": {"streaming": False, "translate": False, "lang_detect": True, "timestamps": "token"},
        # PROVISIONAL, not reference-machine measured: v3's values (same graph
        # and size). FLEURS-fr WER 4.62 % at Q8_0.
        "speed_score": 79,
        "accuracy_score": 88,
        "files": [
            {"filename": "parakeet-ultra-0.6b-Q4_K_M.gguf", "quant": "Q4_K_M", "size_bytes": 485425632,
             "sha256": "1865a03092b566251a9a0a7cc1036872821225047759e1f73fa476694bb453f5"},
            {"filename": "parakeet-ultra-0.6b-Q5_K_M.gguf", "quant": "Q5_K_M", "size_bytes": 548946400,
             "sha256": "2a943b4574664abc96b2dbb2b46cd149bc162fda41080febbf5f949db73ad055"},
            {"filename": "parakeet-ultra-0.6b-Q6_K.gguf", "quant": "Q6_K", "size_bytes": 610342368,
             "sha256": "e1c6c0860397473dc4b7831e2da70fa53f5d472d1791ae174982897d3d60ab6a"},
            {"filename": "parakeet-ultra-0.6b-Q8_0.gguf", "quant": "Q8_0", "size_bytes": 739508704,
             "sha256": "283562ac9b513f39244fe23c6632738c167d32731a5f4693319a10ca498550a8"},
            {"filename": "parakeet-ultra-0.6b-F16.gguf", "quant": "F16", "size_bytes": 1255869984,
             "sha256": "06d3d511e03b2f36aac831f11ce05d088fd071e0fa5685dc70cda1cc7a4a04e4"},
        ],
        "default_quant": "Q8_0",
        "recommended": False,
        "recommended_rank": None,
    },
    {
        # Moondream's native-ternary Parakeet: encoder linears in TQ1_G128
        # (1.75 bpw, ggml type 96 — needs the fork's patches/ggml/0003).
        "id": "Nairod785/parakeet-redux-gguf",
        "revision": "87cbc354ce32bc9fe144b5b7bcdd9c68538907a9",
        "slug": "parakeet-redux-0.6b",
        "name": "Parakeet Redux 0.6B",
        "architecture": "parakeet",
        "family": "parakeet",
        "parameters": "0.6B",
        "description": "Native ternary Parakeet TDT (1.75 bpw encoder): 157 MB, 25 European languages",
        "base_model": "moondream/parakeet-redux",
        "license": "cc-by-4.0",
        "language_count": 25,
        "languages": PARAKEET_V3_LANGUAGES,
        "capabilities": {"streaming": False, "translate": False, "lang_detect": True, "timestamps": "token"},
        # PROVISIONAL: CPU 21x vs ultra's 26x on the same host; FLEURS-fr WER
        # 8.18 % at TQ1_Q4_K, ~3.5 pp above ultra.
        "speed_score": 75,
        "accuracy_score": 80,
        "files": [
            {"filename": "parakeet-redux-0.6b-TQ1_Q4_K.gguf", "quant": "TQ1_Q4_K", "size_bytes": 156696672,
             "sha256": "24a8b9af6ab1fd05eb33d5e8fc5b00c7459af0108ac397a9635267ea4e814374"},
            {"filename": "parakeet-redux-0.6b-TQ1_Q8_0.gguf", "quant": "TQ1_Q8_0", "size_bytes": 159121504,
             "sha256": "74f43ba852479e86e29df92cdbc89aa8215c7e8070f711be424ff466415b6184"},
            {"filename": "parakeet-redux-0.6b-TQ1_F16.gguf", "quant": "TQ1_F16", "size_bytes": 179312288,
             "sha256": "98f34a4dee8c5cf82a251281717851feda84a3291052e819809706c76bbb758f"},
        ],
        "default_quant": "TQ1_Q4_K",
        "recommended": False,
        "recommended_rank": None,
    },
]

# ───────────────────────── helpers ──────────────────────────────────────────
ARCH = ["whisper","moonshine-streaming","moonshine","parakeet","canary-qwen","canary","voxtral",
        "granite-speech","granite","qwen3","gigaam","sensevoice","cohere","fun-asr","nemotron","medasr",
        "moss","sortformer"]
ACR = {"asr":"ASR","ctc":"CTC","rnnt":"RNNT","tdt":"TDT","nar":"NAR","mlt":"MLT"}
SCALAR = {0:("<B",1),1:("<b",1),2:("<H",2),3:("<h",2),4:("<I",4),5:("<i",4),
          6:("<f",4),7:("<?",1),10:("<Q",8),11:("<q",8),12:("<d",8)}

def slug(repo): return repo.split("/")[1].replace("-gguf", "")
def family(s, tags):
    for f in ARCH:
        if f in s.lower(): return "moonshine" if f.startswith("moonshine") else f.split("-")[0]
    for t in tags or []:
        if t in ("whisper","moonshine","parakeet","canary","voxtral","granite","qwen3","gigaam","sensevoice","cohere"):
            return t
    return "other"
def pretty(s):
    return " ".join(ACR.get(p.lower(), p if (p.isupper() or any(c.isdigit() for c in p)) else p.capitalize())
                    for p in s.split("-"))
def quant_of(fn):
    m = re.search(r"-(F32|F16|BF16|Q\d[\w_]*?)\.gguf$", fn);  return m.group(1) if m else "?"
def parse_params(size_label):
    """`general.size_label` ("0.6B" / "1.7B" / "62M") -> count in billions, or None."""
    m = re.match(r"([\d.]+)\s*([BM])", str(size_label or "").strip(), re.I)
    if not m: return None
    v = float(m.group(1));  return v if m.group(2).upper() == "B" else v / 1000
def pick_wer(b):
    if "wer_librispeech_test_clean" in b: return "librispeech", b["wer_librispeech_test_clean"]
    if "wer_fleurs_en" in b: return "fleurs_en", b["wer_fleurs_en"]
    fl = [k for k in b if k.startswith("wer_fleurs")]
    return (fl[0].replace("wer_", ""), b[fl[0]]) if fl else (None, {})
def headline_wer(d):
    for q in ("q8_0","f16","q5_k_m","q6_k","q4_k_m","f32","bf16"):
        if isinstance(d, dict) and q in d: return d[q], q
    return None, None
LANG_NAMES = {"en":"English","ar":"Arabic","ja":"Japanese","ko":"Korean","ru":"Russian",
              "uk":"Ukrainian","vi":"Vietnamese","zh":"Chinese"}
def auto_desc(langs, caps):
    feats = []
    if caps["translate"]:   feats.append("translation")
    if caps["lang_detect"]: feats.append("auto language detection")
    if caps["streaming"]:   feats.append("streaming")
    if caps["timestamps"] != "none": feats.append(f"{caps['timestamps']}-level timestamps")
    if len(langs) > 1:
        base = f"{len(langs)}-language speech-to-text"
    else:
        # unknown code falls back to the raw code so it's caught in diff review
        lang = LANG_NAMES.get(langs[0], langs[0]) if langs else "English"
        base = f"{lang} speech-to-text"
    return base + (" with " + ", ".join(feats) + "." if feats else ".")

api = HfApi(token=os.environ.get("HF_TOKEN"))
fs  = HfFileSystem(token=os.environ.get("HF_TOKEN"))

GGUF_WANT = {"general.architecture", "general.name", "general.basename", "general.size_label"}
def probe_header(repo, filename, nbytes=65536):
    """Range-read only friendly general.* labels (no tensors, no capabilities).

    Capabilities come from HF card data here and from Rust's GGUF probe at
    runtime. Keep this as a narrow display-label reader so the catalog generator
    does not become a second source of truth for model behavior.
    """
    out = {}
    try:
        with fs.open(f"{repo}/{filename}", "rb", block_size=nbytes) as f:
            buf = f.read(nbytes)
    except Exception:
        return out
    if buf[:4] != b"GGUF": return out
    u32 = lambda o: struct.unpack_from("<I", buf, o)[0]
    u64 = lambda o: struct.unpack_from("<Q", buf, o)[0]
    def rstr(o): n = u64(o); return buf[o+8:o+8+n].decode("utf-8","replace"), o+8+n
    def skip(o, vt):
        if vt in SCALAR: return o + SCALAR[vt][1]
        if vt == 8: return o + 8 + u64(o)
        if vt == 9:
            et = u32(o); cnt = u64(o+4); o += 12
            if et in SCALAR: return o + SCALAR[et][1]*cnt
            if et == 8:
                for _ in range(cnt): o += 8 + u64(o)
                return o
        raise ValueError
    off = 24                      # magic+ver+n_tensors+n_kv
    n_kv = u64(16)
    try:
        for _ in range(n_kv):
            key, off = rstr(off); vt = u32(off); off += 4
            if key in GGUF_WANT and vt == 8:
                out[key], off = rstr(off)
            elif key in GGUF_WANT and vt in SCALAR:
                fmt, sz = SCALAR[vt]; out[key] = struct.unpack_from(fmt, buf, off)[0]; off += sz
            else:
                off = skip(off, vt)
            if GGUF_WANT.issubset(out): break
    except Exception:
        pass                      # ran past the buffer (hit tokenizer) — keep what we got
    return out

def lfs_sha256(x):
    """Content sha256 from the sibling's LFS/Xet info (dataclass or dict by hub version)."""
    lfs = getattr(x, "lfs", None)
    if lfs is None: return None
    return getattr(lfs, "sha256", None) or (lfs.get("sha256") if isinstance(lfs, dict) else None)

def gguf_files(repo, siblings):
    """Return GGUF files with mandatory size + sha256 metadata.

    `QuantFile.size_bytes` is a non-null `u64` in Rust. Failing generation here
    keeps a transient/malformed HF listing from producing a catalog that panics
    at app startup when deserialized by `include_str!("catalog.json")`.

    `sha256` is the trust anchor for downloads (HF or mirror alike), so it is
    equally mandatory. Every GGUF is LFS/Xet-tracked; a missing hash means the
    listing is broken, not that the file is small.
    """
    files = []
    invalid = []
    for x in siblings:
        if not x.rfilename.endswith(".gguf"):
            continue
        sha = lfs_sha256(x)
        if type(x.size) is not int or x.size <= 0 or not sha:
            invalid.append(x.rfilename)
            continue
        files.append({
            "filename": x.rfilename,
            "quant": quant_of(x.rfilename),
            "size_bytes": x.size,
            "sha256": sha,
        })
    if invalid:
        raise ValueError(f"{repo}: missing/invalid size or sha256 metadata for {', '.join(invalid)}")
    return sorted(files, key=lambda f: f["size_bytes"])

def build(repo):
    info = api.model_info(repo, files_metadata=True)
    if not info.sha:
        raise ValueError(f"{repo}: listing has no commit sha to pin")
    cd = info.card_data.to_dict() if info.card_data else {}
    s = slug(repo)
    cur = CURATION.get(s, {})
    b = dict(cd.get("transcribe_cpp") or {});  b.update(OVERRIDES.get(s, {}))
    langs = cd.get("language") or []

    files = gguf_files(repo, info.siblings)
    q8 = next((f for f in files if "Q8" in f["quant"]), files[-1] if files else None)
    gg = probe_header(repo, q8["filename"]) if q8 else {}

    caps = {"streaming": bool(b.get("streaming")), "translate": bool(b.get("translate")),
            "lang_detect": bool(b.get("lang_detect")), "timestamps": b.get("timestamps", "none")}
    ryz = b.get("rtf_ryzen_4750u") or {};  rtf = ryz.get("vulkan", ryz.get("cpu"))
    eval_set, werd = pick_wer(b);  hw, hwq = headline_wer(werd)

    # Default-quant policy by model size (params from general.size_label):
    #   >=1B  -> Q5_K_M   (large models tolerate it; ~30% less download/RAM)
    #   <1B   -> Q8_0     (already small; keep reference quality)
    # A per-model CURATION `default_quant` overrides the policy. If the policy
    # quant is absent for a model, fall back: Q8_0 → any Q8 → Q5_K_M → smallest.
    params = parse_params(gg.get("general.size_label"))
    policy = "Q5_K_M" if (params is not None and params >= 1.0) else "Q8_0"
    have = lambda q: next((f["quant"] for f in files if f["quant"] == q), None)
    default_quant = (cur.get("default_quant")
                     or have(policy)
                     or have("Q8_0")
                     or next((f["quant"] for f in files if "Q8" in f["quant"]), None)
                     or have("Q5_K_M")
                     or (files[0]["quant"] if files else None))

    return {
        "id": repo,
        # Pinned commit: downloads fetch `resolve/{revision}/{file}` so the bytes
        # provably match the hashes below even if the repo moves on. Identity
        # stays repo+filename; the pin only scopes acquisition.
        "revision": info.sha,
        "slug": s,
        "name": gg.get("general.name") or pretty(s),         # friendly name (from GGUF)
        "architecture": gg.get("general.architecture"),
        "family": family(s, info.tags),
        "parameters": gg.get("general.size_label"),          # "0.6B" / "1.7B" / "62M"
        "description": cur.get("desc") or auto_desc(langs, caps),
        "base_model": cd.get("base_model"),
        "license": cd.get("license"),
        "language_count": len(langs),
        "languages": langs,
        "capabilities": caps,
        "speed_score": speed_from_rtf(rtf),
        "accuracy_score": acc_from_wer(hw),
        "files": files,
        "default_quant": default_quant,
        "recommended": bool(cur.get("rec")),         # small badge/onboarding subset
        "recommended_rank": cur.get("rank"),          # editorial sort position (independent)
    }

def main():
    repos = [m.id for m in api.list_models(author=ORG, limit=500)]
    models = []
    failures = []
    with ThreadPoolExecutor(max_workers=10) as ex:
        futs = {ex.submit(build, r): r for r in repos}
        for f in as_completed(futs):
            try:
                m = f.result()
                if not CURATION.get(m["slug"], {}).get("hidden"): models.append(m)
            except Exception as e:
                failures.append((futs[f], e))
                print(f"!! {futs[f]}: {e}", file=sys.stderr)
    if failures:
        print(f"catalog generation failed for {len(failures)} repo(s)", file=sys.stderr)
        raise SystemExit(1)
    # Hand-authored entries ride in after the generated ones: they are keyed by
    # repo id, not by a repo the listing returned, so they cannot collide, and the
    # sort below places them by their own rank/speed like any other model.
    models.extend(dict(m) for m in AUTHORED_MODELS)
    models.sort(key=lambda m: (not m["recommended"], m["recommended_rank"] or 1e9,
                               m["family"], -(m["speed_score"] or 0), m["slug"]))
    catalog = {
        "catalog_version": CATALOG_VERSION,
        "generated_at": datetime.datetime.now(datetime.timezone.utc).isoformat(timespec="seconds"),
        "mirrors": MIRRORS,
        "models": models,
    }
    text = json.dumps(catalog, indent=2, ensure_ascii=False)
    # print each `languages` array on a single line for readability
    text = re.sub(r'"languages": \[(.*?)\]',
                  lambda m: '"languages": [' + ", ".join(re.findall(r'"[^"]*"', m.group(1))) + ']',
                  text, flags=re.S)
    # one line per `files[]` entry: a file is one diff line, so schema additions
    # and hash changes don't cascade into per-key comma churn
    text = re.sub(r'\{\s+("filename":.*?"sha256": "[0-9a-f]{64}")\s+\}',
                  lambda m: "{" + re.sub(r",\s+", ", ", m.group(1)) + "}",
                  text, flags=re.S)
    out = sys.argv[1] if len(sys.argv) > 1 else os.path.join(os.path.dirname(__file__), "catalog.json")
    open(out, "w").write(text)
    print(f"wrote {out}: {len(models)} models, {os.path.getsize(out)/1024:.1f} KB", file=sys.stderr)

if __name__ == "__main__":
    main()
