# Improving S2B2S with ideas worth stealing from AivoRelay

> **Audience:** the next coding agent tasked with implementing improvements in **S2B2S**.
> **You will NOT have AivoRelay's source code.** This document is your only window into it.
> Everything you need to re-implement each idea — architecture, key structs, algorithms, and
> representative reference code — is captured below. Where exact fidelity matters, it is called out.
>
> **Author of this note:** an analysis agent that diffed both repos on **2026-06-19**.
> **Method:** found the common ancestor, read AivoRelay's distinctive modules in full, and mapped
> each idea onto S2B2S's *current* architecture.
> **Scope rule the author followed:** only recommend things that are (a) genuinely *additive* to
> S2B2S, (b) high value, and (c) *well-engineered* in AivoRelay (so worth emulating, not just
> reinventing). Things S2B2S already does as well or better are deliberately omitted.

---

## 0. TL;DR — priority-ordered shortlist

Both apps are MIT-licensed forks of [cjpais/Handy](https://github.com/cjpais/Handy). They split from
a **common ancestor dated 2026-01-04** (`d3a0281` — "fix: prevent crash on macos 26.x beta during
startup"). Since then S2B2S added ~339 commits, AivoRelay ~851. They still both periodically sync
upstream, so low-level scaffolding (the `ShortcutAction` trait, `settings.bindings`, `actions.rs`
dispatch, `transcription` manager, `tauri-specta` bindings) still *rhymes* between them. **That makes
porting unusually cheap: the integration seams have the same shape.**

The two forks chose opposite centers of gravity:

- **S2B2S = "SpeechToBrainToSpeech": a local-first voice OS.** Its crown jewels are TTS (8 backends +
  ITN/TN/markdown sanitization), a streaming local "Brain" (llama.cpp w/ GPU offload + 10 providers),
  conversation mode, overlay FX, wake word. It is *ahead* of AivoRelay on local AI.
- **AivoRelay = "AI Voice Relay": a cloud-relay productivity tool for Windows.** Its crown jewels are
  cloud realtime STT (Soniox / Deepgram / OpenAI-realtime with interim results), transcription
  **profiles**, a secure browser **connector**, AI-replace-selection, file transcription + diarization,
  system-audio capture, and a notably **security-hardened** posture.

So this is **not** a "catch up" list. S2B2S is not behind. It is a cherry-pick list: a dozen places
where AivoRelay solved a problem cleanly and S2B2S would benefit from the same solution.

| #  | Idea | Why it matters for S2B2S | Tier | Effort | Risk |
|----|------|--------------------------|------|--------|------|
| S1 | **Canonical base URLs for known providers** | S2B2S currently sends provider API keys to whatever `base_url` is stored — an exfiltration risk. | S · Security | S | Low |
| S2 | **Authenticate / origin-check `control_server`** | S2B2S's `:43117` control API is unauthenticated; any local page/process can drive speak/brain/command. | S · Security | M | Low |
| S3 | **Webview hardening** (disable browser accelerator keys) | Stops the app webview behaving like a browser (refresh, devtools, find) in release. | S · Security | XS | None |
| S4 | **RAII recording session + binding-matched state machine** | Cures "stuck/poisoned recording" states (overlay stuck on, mic stuck muted, leaked cancel shortcut). | S · Robustness | M | Low |
| S5 | **Generation-counter + token-identity async cancellation** | Cleanly cancel stale Brain/LLM/timer work without races. | S · Robustness | S | Low |
| S6 | **Path-traversal validation + TTL temp-artifact cleanup** | Hardens every place S2B2S touches the filesystem with user-influenced paths. | S · Security | S | Low |
| A1 | **Transcription Profiles** | Instant switch between language/prompt/LLM presets, each with its own hotkey. Huge UX win. | A · Feature | L | Med |
| A2 | **Keyboard-layout → language detection** | "Match my keyboard" STT language; powers profile auto-switch. | A · Feature | S | Low |
| A3 | **Smart Decapitalize After Edit** | After you backspace-correct mid-sentence, the next inserted chunk starts lowercase. | A · Polish | S | Low |
| A4 | **Streaming output quality kit** | Stable-prefix safety buffer + whitespace-preserving correction + incremental paste/delete. | A · Quality | M | Med |
| A5 | **Quick-tap (tap vs hold) dual-action keys** | One key does two things depending on press duration. | A · Feature | S | Low |
| A6 | **Dictation stats + final-output hook** | "You've dictated N words"; plus a clean extension point. | A · Polish | XS | None |
| A7 | **Per-app transcript context + dynamic prompt vars** | LLM post-processing gets `${current_app}`, `${time_local}`, recent-transcript context. | A · Feature | S | Low |
| A8 | **Microphone auto-switch** | Auto-select your headset when it appears (wildcard name match + manual fallback). | A · Feature | S | Low |
| A9 | **Clipboard backup/restore-all-formats** | Capture the user's text selection without nuking their clipboard. | A · Polish | S | Low |
| B1 | **Cloud realtime STT** (Soniox/Deepgram/OpenAI-realtime) | Interim-results streaming over WebSocket; endpointing, keepalive, preconnect. | B · Feature | XL | Med |
| B2 | **AI Replace Selection** | Select text → speak instruction → LLM transforms it in place, in any app. | B · Feature | L | Med |
| B3 | **File transcription + diarization + SRT/VTT** | Drag a file in, get a (multi-speaker) transcript / subtitles. | B · Feature | L | Med |
| B4 | **System-audio (loopback) capture** | Transcribe what your speakers play (meetings, videos), optionally mixed with mic. | B · Feature | L | Med |
| B5 | **Region / screenshot capture** | Grab a screen region to attach to a prompt. | B · Feature | M | Low |
| B6 | **Secure browser connector** | Push voice (+selection/+screenshot) to a ChatGPT/Claude tab. | B · Feature | XL | Med |
| B7 | **Voice Command Center** (PowerShell) | Speak → run a script, with confirmation. **Security-sensitive.** | B · Feature | M | High |
| B8 | **Live Preview window** | A floating, editable real-time transcript window with edit hotkeys. | B · Feature | L | Med |

Effort key: XS < S < M < L < XL.

**Suggested order:** ship all of Tier S first (small, safe, and they harden everything you build
after). Then A1 (Profiles) + A2 + A3 as a UX bundle. Then pick from Tier B by appetite.

---

## 1. Context you need before touching anything

### 1.1 The shared skeleton (why porting is cheap)

Because both forks descend from Handy and still sync upstream, these seams are *structurally the same*
in both repos — you can port into them by analogy:

- **Bindings**: `settings.bindings: HashMap<String, ShortcutBinding>`, with
  `shortcut::register_shortcut` / `unregister_shortcut`, and a `ShortcutAction` trait whose
  implementors expose `start(app, binding_id)` / `stop(app, binding_id, reason)`. S2B2S's
  `TranscribeAction` (in `src-tauri/src/actions.rs`) implements it. **AivoRelay adds new features
  largely by adding new `ShortcutAction` implementors + new binding ids** — that's the pattern you'll
  reuse for Profiles, AI-Replace, etc.
- **Settings**: one big `AppSettings` struct in `src-tauri/src/settings.rs`, serde + `specta::Type`
  for typed frontend bindings, regenerated via `cargo test export_bindings`. Adding a feature = adding
  fields here (and any sub-structs), then regenerating bindings.
- **Post-processing entry**: in S2B2S, `actions.rs::post_process_transcription(app, settings, text)` is
  where the transcript meets the LLM. Several ideas below hook in *right here* (profile overrides,
  `${output}`/context variables, decapitalize, text replacement order).
- **Transcription flow**: S2B2S routes through `transcription_coordinator.rs`
  (`send_input` / `notify_cancel` / `notify_processing_finished`) and the `transcription` manager.
  AivoRelay routes through a slightly different `RecordingSession` RAII state machine (see S4) — when
  you port robustness ideas, you'll be reconciling these two approaches.
- **Secrets**: **S2B2S already has `SecretMap`** (a `HashMap<String,String>` newtype with a redacting
  `Debug` impl) and stores keys in the OS keychain. So the *secret-redaction* and *keychain* battles
  are already won in S2B2S. Don't re-port those; only the small deltas in S1/S6 matter.

### 1.2 What S2B2S already has (do NOT re-port these)

S2B2S already ships, often more elaborately than AivoRelay: local STT (Parakeet V3, Whisper,
Moonshine, Nemotron, SenseVoice, GigaAM, Canary, Cohere), `multi_stt`, `parakeet_streaming`, TTS (8
backends) with a 5-stage ITN/TN/markdown normalization pipeline, a streaming Brain with 10 providers +
llama.cpp GPU offload, conversation memory, overlay FX, wake word, **Custom Words fuzzy correction**
(`word_correction_threshold`, n-gram), keychain secrets + `SecretMap` redaction, `auto_submit`,
post-process **actions** + `external_script_path`, crash logging, 20-language i18n. When a feature
below overlaps one of these, the note is explicit about the *delta* only.

### 1.3 Licensing / attribution (read this before pasting code)

All reference code below is **distilled from AivoRelay (`MaxITService/AIVORelay`), MIT-licensed**, by an
agent that read the source. Both projects are MIT; S2B2S is also MIT. Re-using these ideas and adapting
this code into S2B2S is squarely within the license. **Do the right thing:** keep AivoRelay's
`THIRD_PARTY_LICENSES.md`/attribution conventions in mind, and add a short credit (e.g.
`// Adapted from AivoRelay (MIT) — <feature>`) at the top of files you port. The code in this note is
*representative reconstruction* meant to convey intent and the tricky bits; treat it as a spec to
implement against S2B2S's real types, not as drop-in source.

---

## 2. Tier S — Security & Robustness (ship these first)

These are small, safe, and they harden everything you build afterward. Do them before the feature tiers.

### S1 — Canonical base URLs for known providers (anti-exfiltration)

**The problem in S2B2S today.** `llm_client.rs` builds requests as
`format!("{}/chat/completions", provider.base_url.trim_end_matches('/'))` and sends the provider's API
key to that URL. `shortcut/mod.rs::change_post_process_base_url_setting` gates edits behind
`provider.allow_base_url_edit`, but it then writes `provider.base_url = base_url` verbatim, and the
client trusts it. There is **no HTTPS enforcement, no scheme check, and — crucially — no "for a known
provider, ignore the stored URL"** rule. If the stored settings are ever tampered with (corrupted
file, malicious import, a future "share settings" feature, a bug), the key for e.g. `openai` goes
wherever `base_url` points. AivoRelay closes this hole.

**AivoRelay's approach (`url_security.rs`).** For **known** providers the canonical base URL is
hard-coded and the configured value is *ignored*; only `custom` providers may supply a URL, and even
then HTTPS is required unless an explicit per-provider `allow_insecure_http` opt-in is set. Scheme is
validated (no `ftp://`, etc.), and URLs are normalized (trim whitespace + trailing slash). This is
backed by a thorough unit-test suite — port the tests too; they *are* the spec.

```rust
// url_security.rs — distilled. Apply the same shape to S2B2S's LLM + cloud-TTS providers.

pub const LLM_OPENAI_BASE_URL: &str    = "https://api.openai.com/v1";
pub const LLM_ANTHROPIC_BASE_URL: &str = "https://api.anthropic.com/v1";
pub const LLM_GROQ_BASE_URL: &str      = "https://api.groq.com/openai/v1";
pub const LLM_CEREBRAS_BASE_URL: &str  = "https://api.cerebras.ai/v1";
pub const LLM_OPENROUTER_BASE_URL: &str= "https://openrouter.ai/api/v1";
pub const LLM_ZAI_BASE_URL: &str       = "https://api.z.ai/api/paas/v4";

fn parse_network_url(input: &str, context: &str) -> Result<reqwest::Url, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() { return Err(format!("{} is empty.", context)); }
    reqwest::Url::parse(trimmed).map_err(|e| format!("{} is invalid: {}", context, e))
}
fn normalize_url(url: &reqwest::Url) -> String { url.as_str().trim_end_matches('/').to_string() }

fn validate_network_base_url(input: &str, allow_insecure_http: bool, context: &str)
    -> Result<String, String>
{
    let url = parse_network_url(input, context)?;
    match url.scheme() {
        "https" => Ok(normalize_url(&url)),
        "http" if allow_insecure_http => Ok(normalize_url(&url)),
        "http"  => Err(format!("{} must use HTTPS. Plain HTTP is allowed only for a Custom \
                                endpoint after enabling the advanced insecure HTTP override.", context)),
        scheme  => Err(format!("{} must use http:// or https://, but got '{}://'.", context, scheme)),
    }
}

/// THE KEY FUNCTION: known providers ignore the stored base_url entirely.
pub fn canonical_llm_provider_base_url(p: &PostProcessProvider) -> Result<String, String> {
    match p.id.as_str() {
        "openai"     => Ok(LLM_OPENAI_BASE_URL.into()),
        "anthropic"  => Ok(LLM_ANTHROPIC_BASE_URL.into()),
        "groq"       => Ok(LLM_GROQ_BASE_URL.into()),
        "cerebras"   => Ok(LLM_CEREBRAS_BASE_URL.into()),
        "openrouter" => Ok(LLM_OPENROUTER_BASE_URL.into()),
        "zai"        => Ok(LLM_ZAI_BASE_URL.into()),
        "custom"     => validate_network_base_url(&p.base_url, p.allow_insecure_http, "Custom LLM base URL"),
        // unknown provider id => still force https, never trust an arbitrary scheme
        _            => validate_network_base_url(&p.base_url, false, "LLM provider base URL"),
    }
}
```

**S2B2S integration.**
1. Add `url_security.rs` (or fold into `llm_client.rs`). Enumerate S2B2S's known LLM providers
   (OpenAI, Anthropic, Gemini, Groq, Cerebras, OpenRouter, Z.ai, Bedrock, …) **and the cloud TTS
   providers** (`tts/backends/openai.rs`, `elevenlabs.rs`, `cartesia.rs`) — those also carry keys and
   also deserve canonical endpoints. Apple-Intelligence / on-device pseudo-URLs are pass-through.
2. In `llm_client.rs`, replace `provider.base_url` with `canonical_llm_provider_base_url(provider)?`
   at the two call sites (`/chat/completions`, `/models`). Do the same in the cloud TTS backends.
3. Add `allow_insecure_http: bool` to `PostProcessProvider` (and cloud-TTS provider configs),
   defaulting `false`. Surface it in settings UI only for `custom`.
4. **Port the tests verbatim** — they cover trailing-slash normalization, http-rejection,
   override-acceptance, scheme rejection, and "known preset ignores override."

This is the single highest-value/lowest-risk item in the whole document. Do it first.

---

### S2 — Authenticate (or at least origin-lock) the local `control_server`

**The problem in S2B2S today.** `control_server.rs` opens a plain TCP HTTP server on
`127.0.0.1:43117` exposing `/health`, piper-status, **`speak`**, **`brain`**, and **`command`**
endpoints with no authentication. Binding to loopback is *necessary but not sufficient*: any local
process, and — via DNS-rebinding or permissive CORS — potentially a malicious *web page* the user
visits, can reach a loopback server and make S2B2S speak text, run the Brain, or execute commands.
"It's only localhost" is not a security boundary by itself.

**What AivoRelay does (the `connector` module).** AivoRelay's outbound bridge is a masterclass in
hardening a local server. Even though its purpose differs (it *pushes* voice to a browser extension
rather than *accepting* control), its defensive design is exactly what S2B2S's inbound control server
should adopt. From its own module docs and code, the layers are:

- **Bind 127.0.0.1 only**, and keep the **route set minimal** (least surface).
- **Exact-origin CORS allowlist.** It prefers `CorsPolicy::Exact(origin)` (a single validated origin
  string turned into a `HeaderValue`); wildcard `Any` is available only behind an explicit
  `connector_allow_any_cors` flag, and starting without a valid allowlist logs a loud warning and
  serves nothing useful. Browsers cannot read responses from a loopback server they aren't allow-listed
  for — this defeats the casual "evil web page hits localhost" attack.
- **Password-bootstrapped sessions, not per-request passwords.** Protocol "v3": the
  `connector_password` is a *bootstrap secret only*. A client proves knowledge of it once (an HKDF-style
  derivation using a fixed context string, e.g. `b"AivoRelay Connector Protocol v3 password auth key"`)
  and receives a **per-session** symmetric key pair (`[u8;32]` enc + `[u8;32]` mac). The long-lived
  password is never replayed on every call.
- **Optional authenticated encryption.** When `connector_encryption_enabled`, payloads are encrypted
  with a fresh random nonce per message (AEAD), flagged via an `x-...-payload-encrypted` header.
- **Auth backoff** on repeated failures, and **password rotation** support (auto-generate + a
  "pending password" handshake so the extension can be re-paired).
- **Capability split**: bulk data (e.g. screenshots) is fetched from a separate `/blob/<id>` route by
  id, not inlined — smaller, auditable messages.

**S2B2S integration — pragmatic path (you don't need the full v3 protocol):**

1. **Add a shared secret.** Generate a random token on first run, store it in the keychain (S2B2S
   already has keychain plumbing). Require it on `speak`/`brain`/`command` via an
   `Authorization: Bearer <token>` header (or `X-S2B2S-Token`). `/health` may stay open.
2. **Constant-time compare** the token (`subtle::ConstantTimeEq` or a manual XOR-accumulate) to avoid
   timing leaks.
3. **Reject requests with a browser-y `Origin`/`Sec-Fetch-Site: cross-site`** unless explicitly
   allow-listed — this blocks DNS-rebinding/CSRF-style abuse from web pages while leaving legit local
   scripts (no `Origin` header) working.
4. **Gate the dangerous endpoints behind settings.** `command` (and arguably `brain`/`speak`) should
   be **off by default** and require the user to opt in, mirroring how AivoRelay gates connector
   features behind explicit enable flags.
5. Keep `MAX_BODY_BYTES` (already present — good) and the read timeout.

If you later build the outbound connector (B6), reuse this same hardened server core.

---

### S3 — Webview hardening: disable browser accelerator keys (Windows, release)

**Why.** A Tauri WebView2 window still honors browser shortcuts (F5 reload, F12/devtools, Ctrl+F
find, etc.). In a shipped desktop app those are at best confusing and at worst a way to poke at the
webview. AivoRelay disables them in release builds on Windows. Tiny, zero-risk, ships polish.

```rust
// webview_hardening.rs — port as-is (Windows + release only; no-op elsewhere).
#[cfg(all(target_os = "windows", not(debug_assertions)))]
pub fn disable_browser_accelerator_keys(window: &tauri::WebviewWindow) {
    let label = window.label().to_string();
    let _ = window.with_webview(move |webview| unsafe {
        use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2Settings3;
        use windows::core::Interface;
        let result = webview.controller()
            .CoreWebView2()
            .and_then(|core| core.Settings())
            .and_then(|s| s.cast::<ICoreWebView2Settings3>())
            .and_then(|s| s.SetAreBrowserAcceleratorKeysEnabled(false));
        if let Err(e) = result {
            log::warn!("Failed to disable WebView2 accelerator keys for '{}': {}", label, e);
        }
    });
}
#[cfg(any(not(target_os = "windows"), debug_assertions))]
pub fn disable_browser_accelerator_keys(_window: &tauri::WebviewWindow) {}
```

**Integration.** Call it once per webview window right after creation in S2B2S's setup (where the main
+ overlay + conversation windows are built). Keep it a no-op in debug so devtools still work while
developing. (S2B2S already depends on `webview2-com`/`windows` crates via Tauri, so this adds no new
deps on Windows.)

---

### S4 — RAII recording session + binding-matched state machine

**Why.** This is AivoRelay's fix for the class of bug they describe in commit history as "recover from
**poisoned recording state**": the recording ends in some unexpected way (cancel, error, panic,
double-stop, a *second* hotkey firing mid-flight) and the app is left with the **overlay stuck on, the
mic stuck muted, and the cancel-shortcut leaked**. S2B2S coordinates recording via
`transcription_coordinator.rs`; adopting AivoRelay's RAII + explicit state machine makes these stuck
states structurally impossible.

**Two cooperating ideas:**

**(a) `SessionState` as the single source of truth** — `Idle` / `Recording{session, binding_id}` /
`Processing{binding_id}`. New recordings are blocked while `Processing`, but **cancellation is always
allowed**. State transitions go through helpers that *match on binding id* so one action's stop can
never steal another action's session.

**(b) `RecordingSession` is an RAII guard.** It registers the cancel shortcut on creation and
releases every resource it actually acquired **exactly once** — on explicit `finish()` (happy path:
Recording→Processing) *or* on `Drop` (cancel/error/panic). Each resource is tracked with an
`AtomicBool` so cleanup is idempotent and only releases what was taken.

```rust
// recording_session.rs — distilled. The important bits are: (1) the enum, (2) idempotent
// per-resource cleanup via AtomicBool, (3) finish()-vs-Drop, (4) binding-id-matched takes.

pub enum SessionState {
    Idle,
    Recording { session: Arc<RecordingSession>, binding_id: String },
    Processing { binding_id: String }, // blocks new recordings; cancel still allowed
}
pub type ManagedSessionState = Mutex<SessionState>;

pub struct RecordingSession {
    app: AppHandle,
    cancel_shortcut_registered: AtomicBool,
    mute_applied: AtomicBool,
    cleaned_up: AtomicBool, // ensures finish() then Drop doesn't double-clean
}

impl RecordingSession {
    pub fn new_with_resources(app: &AppHandle, _will_register_cancel: bool, will_apply_mute: bool) -> Self {
        Self { app: app.clone(),
               cancel_shortcut_registered: AtomicBool::new(false),
               mute_applied: AtomicBool::new(will_apply_mute),
               cleaned_up: AtomicBool::new(false) }
    }
    pub fn register_cancel_shortcut(&self) {
        if !self.cancel_shortcut_registered.swap(true, SeqCst) {
            shortcut::register_cancel_shortcut(&self.app);
        }
    }
    /// Happy path: Recording -> Processing. After this, Drop is a no-op.
    pub fn finish(&self) {
        if self.cleaned_up.swap(true, SeqCst) { return; }
        self.do_cleanup();
    }
    fn do_cleanup(&self) {
        if self.cancel_shortcut_registered.swap(false, SeqCst) {
            shortcut::unregister_cancel_shortcut(&self.app);
        }
        if self.mute_applied.swap(false, SeqCst) {
            self.app.state::<Arc<AudioRecordingManager>>().remove_mute();
        }
    }
}
impl Drop for RecordingSession {
    fn drop(&mut self) {
        if self.cleaned_up.load(SeqCst) { return; }       // already finished cleanly
        self.do_cleanup();                                 // unexpected exit
        hide_recording_overlay(&self.app);                 // also clear UI on cancel/panic
        change_tray_icon(&self.app, TrayIconState::Idle);
    }
}

/// Take ONLY if the binding id matches — prevents cross-action session theft.
pub fn take_session_if_matches(app: &AppHandle, expected: &str) -> Option<Arc<RecordingSession>> {
    let st = app.state::<ManagedSessionState>();
    let mut g = st.lock().expect("session state poisoned");
    if let SessionState::Recording { binding_id, .. } = &*g {
        if binding_id == expected {
            if let SessionState::Recording { session, .. } =
                std::mem::replace(&mut *g, SessionState::Idle) { return Some(session); }
        }
    }
    None
}
pub fn exit_processing(app: &AppHandle) {
    let st = app.state::<ManagedSessionState>();
    let mut g = st.lock().expect("session state poisoned");
    if matches!(&*g, SessionState::Processing { .. }) { *g = SessionState::Idle; }
}
```

**S2B2S integration.** This is the most invasive Tier-S item because it touches the hot path. Two
options:
- **Full adoption:** introduce `ManagedSessionState` + `RecordingSession`, and have
  `TranscribeAction::start` create the session, `::stop` call `take_session_if_matches` then `finish()`
  before kicking off async transcription, and the async tail call `exit_processing`. Cancellation paths
  just drop the session.
- **Minimal adoption (recommended first step):** keep `transcription_coordinator.rs`, but wrap the
  resources it acquires (cancel shortcut, mute, overlay/tray) in a small RAII guard with the same
  `AtomicBool`-tracked idempotent cleanup, and add the **binding-id match check** so a second hotkey
  can't clobber the first session. That alone kills most "stuck overlay / stuck mute" reports with far
  less churn. Note S2B2S's continuous-voice / conversation modes have their *own* lifecycles — make
  sure the guard is per-flow, not a global singleton.

---

### S5 — Generation-counter + token-identity async cancellation

Two tiny patterns for cancelling in-flight async work correctly. S2B2S has a **streaming Brain** and
LLM post-processing that can outlive the user's intent (user starts a new dictation while the previous
LLM call is still streaming). These give you race-free cancellation.

**(a) Generation counter** — "cancel everything started before now." Perfect for LLM/Brain requests.

```rust
// llm_operation.rs — port verbatim; trivially generalizable.
pub struct LlmOperationTracker {
    current_operation_id: AtomicU64,
    cancelled_before_id: AtomicU64,
}
impl LlmOperationTracker {
    pub fn start_operation(&self) -> u64 { self.current_operation_id.fetch_add(1, SeqCst) + 1 }
    pub fn cancel(&self) {
        let cur = self.current_operation_id.load(SeqCst);
        self.cancelled_before_id.store(cur + 1, SeqCst);
    }
    pub fn is_cancelled(&self, id: u64) -> bool { id < self.cancelled_before_id.load(SeqCst) }
}
```
Usage: each Brain/post-process request grabs `let op = tracker.start_operation();`, and inside the
SSE/token loop checks `if tracker.is_cancelled(op) { return; }` before emitting each chunk. A new
dictation calls `tracker.cancel()`.

**(b) Token-identity check before firing** — for *timers* that may be superseded. AivoRelay's
auto-stop timer (below) verifies via `Arc::ptr_eq` that it is *still the active token* before doing
anything, so a timer that wakes up right as a newer recording replaced it does nothing.

```rust
// recording_auto_stop.rs — the crucial race-guard. Also a useful "max recording length" feature.
pub struct AutoStopToken { pub notify: tokio::sync::Notify }
pub type ManagedAutoStopToken = Mutex<Option<Arc<AutoStopToken>>>;

pub fn start_auto_stop_timer(app: &AppHandle, binding_id: &str, timeout_secs: u64) {
    let token = Arc::new(AutoStopToken { notify: tokio::sync::Notify::new() });
    *app.state::<ManagedAutoStopToken>().lock().unwrap() = Some(Arc::clone(&token));
    let (app, token2) = (app.clone(), Arc::clone(&token));
    tauri::async_runtime::spawn(async move {
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_secs(timeout_secs)) => {}
            _ = token2.notify.notified() => { return; } // cancelled
        }
        // Only fire if we are STILL the active token (else a newer recording replaced us).
        let still_active = {
            let mut g = app.state::<ManagedAutoStopToken>().lock().unwrap();
            match g.take() {
                Some(cur) if Arc::ptr_eq(&cur, &token) => true,
                Some(cur) => { *g = Some(cur); false } // put the newer one back
                None => false,
            }
        };
        if still_active { /* stop+paste, or cancel, per settings */ }
    });
}
pub fn cancel_auto_stop_timer(app: &AppHandle) {
    if let Some(t) = app.state::<ManagedAutoStopToken>().lock().unwrap().take() { t.notify.notify_one(); }
}
```

**S2B2S integration.** (a) Add an `LlmOperationTracker` to the Brain manager and post-process path —
S2B2S already streams tokens, so you just add the id-check in the loop and a `.cancel()` on new input
/ barge-in. (b) Wire `start_auto_stop_timer` to a new `recording_auto_stop_*` settings group; it
doubles as a safety "max recording length" so a stuck PTT never records forever.

---

### S6 — Path-traversal validation + TTL temp-artifact cleanup

Wherever S2B2S writes/reads files with any user-influenced component (model downloads, exported
transcripts, future file-transcription artifacts, screenshots), borrow AivoRelay's two small habits:

**(a) Canonicalize and verify containment** before reading a path that came from the frontend:

```rust
fn validate_artifact_path(requested: &str, base_dir: &Path) -> Result<PathBuf, String> {
    let canonical_dir  = fs::canonicalize(base_dir).map_err(|e| e.to_string())?;
    let canonical_path = fs::canonicalize(requested)
        .map_err(|_| "File no longer available".to_string())?;
    if !canonical_path.starts_with(&canonical_dir) { return Err("Invalid path".into()); }
    if canonical_path.extension().and_then(|x| x.to_str()) != Some("json") {
        return Err("Invalid file type".into());
    }
    Ok(canonical_path)
}
```

**(b) TTL cleanup of temp artifacts** — when you write transient files under `temp_dir()`, give them a
TTL (AivoRelay uses 24h) and sweep stale ones on each new write, so crashes don't leak files forever:

```rust
const ARTIFACT_TTL: Duration = Duration::from_secs(60 * 60 * 24);
fn cleanup_old_artifacts(dir: &Path) {
    let cutoff = SystemTime::now().checked_sub(ARTIFACT_TTL).unwrap_or(UNIX_EPOCH);
    for entry in fs::read_dir(dir).into_iter().flatten().flatten() {
        if entry.path().is_file() {
            if let Ok(m) = entry.metadata() { if let Ok(t) = m.modified() {
                if t < cutoff { let _ = fs::remove_file(entry.path()); }
            }}
        }
    }
}
```

---

## 3. Tier A — High-value UX (self-contained)

### A1 — Transcription Profiles (the headline feature)

**The idea.** A *profile* is a named bundle of settings (language, STT prompt, LLM post-processing
on/off + prompt + model, push-to-talk, output routing, …) that you can switch between instantly. Each
profile gets **its own hotkey**, and there's a **cycle** hotkey to rotate through the active ones.
Example profiles a user might keep: "Code dictation (no LLM cleanup)", "Email (formal LLM rewrite)",
"French → English translation", "Quick notes". For S2B2S specifically this is *especially* powerful
because S2B2S has three pipelines (dictate / read-aloud / conversation) and many models — profiles let
a user pin a whole configuration to one key.

**Data model (the important part).** The design that makes this clean is **override-with-fallback**:
each overridable setting is paired with an explicit boolean (or an `Option`) that says "this profile
overrides the global value." If the boolean is false / `None`, you fall back to the global setting.
This keeps profiles small and avoids them silently going stale when globals change.

```rust
// settings.rs — TranscriptionProfile (distilled). Each profile also owns a binding id
// "transcribe_<id>", registered just like any other shortcut.
#[derive(Serialize, Deserialize, Debug, Clone, Type)]
pub struct TranscriptionProfile {
    pub id: String,                         // e.g. "profile_1704067200000"
    pub name: String,                       // "French to English"
    pub language: String,                   // "fr" | "es" | "auto" | "os_input" (see A2)
    pub translate_to_english: bool,
    #[serde(default)] pub description: String,

    // ---- STT prompt override ----
    #[serde(default)] pub system_prompt: String,
    #[serde(default)] pub stt_prompt_override_enabled: bool, // if false -> global per-model prompt

    // ---- behavior ----
    #[serde(default = "default_true")] pub include_in_cycle: bool,
    #[serde(default = "default_true")] pub push_to_talk: bool,
    #[serde(default)] pub preview_output_only_enabled: bool,  // route to a preview window, don't auto-insert

    // ---- LLM post-processing overrides (the money feature) ----
    #[serde(default)] pub llm_post_process_enabled: bool,
    #[serde(default)] pub llm_prompt_override: Option<String>, // None -> global selected prompt
    #[serde(default)] pub llm_model_override: Option<String>,  // None -> global model for provider
}

impl TranscriptionProfile {
    pub fn resolve_prompt(&self) -> Option<String> {
        if self.stt_prompt_override_enabled {
            let t = self.system_prompt.trim();
            if t.is_empty() { None } else { Some(self.system_prompt.clone()) }
        } else { None }
    }
}

/// Resolve the STT prompt: profile override (even if empty) wins; else global per-model prompt.
pub fn resolve_stt_prompt(profile: Option<&TranscriptionProfile>,
                          per_model_prompts: &HashMap<String, String>,
                          model_id: &str) -> Option<String> {
    if let Some(p) = profile { if p.stt_prompt_override_enabled { return p.resolve_prompt(); } }
    per_model_prompts.get(model_id).filter(|s| !s.trim().is_empty()).cloned()
}
```

Top-level `AppSettings` gains: `transcription_profiles: Vec<TranscriptionProfile>`,
`active_profile_id: String`, `profile_switch_overlay_enabled: bool`. There is always a non-deletable
**"Default Profile"** that maps to the global settings.

**The `${output}` prompt variable.** When a profile (or the global config) supplies an LLM prompt,
`${output}` is the placeholder for the transcribed text. Flow: speak → STT → substitute `${output}`
into the prompt → LLM → result. e.g. prompt `"Translate this to Finnish: ${output}"`. Implement as a
plain string replace right before the LLM call (combine with A7's dynamic variables).

**How switching + per-profile hotkeys work.**
- For each profile, register a binding id `transcribe_<profile.id>` (S2B2S already namespaces bindings;
  see `settings::action_binding_id` / `is_transcribe_binding`). Its `ShortcutAction` is just
  `TranscribeAction` parameterized to set `active_profile_id = <id>` for that capture.
- A **cycle** binding rotates `active_profile_id` through profiles where `include_in_cycle == true`.
- On switch, if `profile_switch_overlay_enabled`, show a tiny overlay toast with the new profile name
  (reuse S2B2S's overlay system).

**S2B2S integration.**
1. Add the structs + `AppSettings` fields; regenerate bindings (`cargo test export_bindings`).
2. In `actions.rs`, thread `Option<&TranscriptionProfile>` (the active one) into the capture path.
   At STT, use `resolve_stt_prompt(...)` and the profile's `language`/`translate_to_english`. At
   `post_process_transcription`, honor `llm_post_process_enabled` / `llm_prompt_override` /
   `llm_model_override` *before* falling back to global post-process settings.
3. Add profile CRUD commands + a Settings UI panel (Speech → Profiles). When a profile is created,
   register its binding; when deleted, unregister and clean its key from `bindings`.
4. Because S2B2S also has **conversation** and **read-aloud**, consider letting a profile optionally
   pin a TTS voice / Brain model too — a natural S2B2S-only extension of the same override pattern.

This is the largest Tier-A item but the highest user-visible payoff. Build it on top of A2.

---

### A2 — Keyboard-layout → language detection ("match my keyboard")

**Why.** Pairs perfectly with profiles and with multilingual dictation: let `language = "os_input"`
resolve the STT language from the user's **current keyboard layout**. Deterministic and intuitive —
switch your OS keyboard to Russian, dictation goes Russian. AivoRelay ships a complete cross-platform
implementation; port it whole.

```rust
// input_source.rs — cross-platform current-keyboard-layout -> ISO-639-1. Port the full map.
static INPUT_SOURCE_MAP: LazyLock<HashMap<&'static str, &'static str>> = LazyLock::new(|| {
    let mut m = HashMap::new();
    // macOS TIS ids (sample — the real table has ~40 entries):
    m.insert("com.apple.keylayout.US", "en"); m.insert("com.apple.keylayout.Russian", "ru");
    m.insert("com.apple.keylayout.German", "de"); m.insert("com.apple.keylayout.French", "fr");
    // Windows KLID hex (low word of HKL):
    m.insert("00000409", "en"); m.insert("00000419", "ru");
    m.insert("00000407", "de"); m.insert("0000040c", "fr");
    // Linux XKB:
    m.insert("us", "en"); m.insert("ru", "ru"); m.insert("de", "de"); m.insert("fr", "fr");
    m // ... (full table covers en/ru/de/es/fr/it/pt/ja/zh/ko/ar/he/tr/pl/nl/uk/el/sv/no/da/fi/cs/hu/ro/th/vi/hi/id/ms)
});

#[cfg(target_os = "windows")]
pub fn get_current_input_source() -> Option<String> {
    use windows::Win32::UI::Input::KeyboardAndMouse::GetKeyboardLayout;
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};
    unsafe {
        let hwnd = GetForegroundWindow();
        let tid  = GetWindowThreadProcessId(hwnd, None);
        let hkl  = GetKeyboardLayout(tid);
        Some(format!("{:08x}", (hkl.0 as usize) & 0xFFFF)) // low word = lang id
    }
}
#[cfg(target_os = "macos")]
pub fn get_current_input_source() -> Option<String> {
    // `defaults read com.apple.HIToolbox AppleSelectedInputSources`, find "KeyboardLayout Name"
    // -> "com.apple.keylayout.<Name>"; fallback to an "Input Source ID" starting with "com.apple".
    /* ...parse stdout... */ None
}
#[cfg(target_os = "linux")]
pub fn get_current_input_source() -> Option<String> {
    // `setxkbmap -query` -> first of "layout: us,ru"; fallback to $LANG primary tag.
    /* ...parse stdout... */ None
}

pub fn input_source_to_language(src: &str) -> Option<&'static str> {
    INPUT_SOURCE_MAP.get(src).copied()
        .or_else(|| INPUT_SOURCE_MAP.get(src.split('-').next().unwrap_or(src)).copied())
}
pub fn get_language_from_input_source() -> Option<String> {
    input_source_to_language(&get_current_input_source()?).map(|s| s.to_string())
}
```

**Bonus: language canonicalization helper.** AivoRelay also normalizes/validates language codes
against a provider's supported set (its `language_resolver.rs` does this for Soniox). The reusable
core is `canonicalize_language_code`: lowercase, `_`→`-`, map `zh-Hans`/`zh-Hant`→`zh`, take the
primary subtag (`de-CH`→`de`). Keep that helper handy for B1 and for validating profile languages.

```rust
fn canonicalize_language_code(raw: &str) -> Option<String> {
    let mut n = raw.trim().to_lowercase();
    if n.is_empty() { return None; }
    n = n.replace('_', "-");
    if n == "zh-hans" || n == "zh-hant" { return Some("zh".into()); }
    let primary = n.split('-').next().unwrap_or("").trim();
    if primary.is_empty() { None } else { Some(primary.to_string()) }
}
```

**S2B2S integration.** Add `input_source.rs`. In the capture path, if the resolved language is
`"os_input"`, call `get_language_from_input_source()` and fall back to `auto` if `None`. Surface
`os_input` as a language option in the global Speech settings and in profiles (A1). S2B2S already
pulls in the `windows` crate on Windows, so no new dependency there.

---

### A3 — Smart Decapitalize After Edit

**The behavior.** You dictate a sentence, press **Backspace** to fix a word mid-sentence, then resume
speaking. Normally STT capitalizes the first word of the new chunk (it thinks it's a new sentence),
giving you `"...the quick Brown fox"`. With this feature, after you press the edit key, the **next**
inserted chunk gets its first letter lowercased once. Non-blocking (a passive key listener), one-shot,
configurable timeout, and it works in both standard and realtime modes. Small, delightful, very
self-contained — and the author has the **full source**, so you can reproduce it almost exactly.

**The subtlety** (why it's a state machine, not an `if`): standard STT emits text *later* (after
recording stops), while realtime STT emits *chunks*. So the trigger must survive across both timings.
The module tracks three things: a realtime deadline, a post-recording "monitor window", and an "armed
for delayed standard output" flag.

```rust
// text_replacement_decapitalize.rs — distilled; the global-state shape + one-shot semantics matter.
#[derive(Default)]
struct DecapState {
    realtime_trigger_until: Option<Instant>,  // realtime/chunk deadline
    standard_monitor_until: Option<Instant>,  // post-recording window where the edit key can arm output
    standard_output_armed: bool,              // a delayed final transcription should be decapitalized
}
static STATE: Lazy<Mutex<DecapState>> = Lazy::new(|| Mutex::new(DecapState::default()));

#[derive(Clone, Copy)] enum ApplyMode { RealtimeChunk, StandardOutput }

/// Call when the monitored edit key (default Backspace) is pressed.
pub fn mark_edit_key_pressed(timeout_ms: u32, arm_standard_output: bool) {
    let now = Instant::now();
    let mut s = STATE.lock().unwrap();
    s.realtime_trigger_until = Some(now + Duration::from_millis(timeout_ms.max(1) as u64));
    let monitor_active = s.standard_monitor_until.map_or(false, |d| now <= d);
    if arm_standard_output || monitor_active { s.standard_output_armed = true; }
}

/// Realtime: apply + consume on the finalized chunk. (A "preview" variant applies WITHOUT consuming,
/// so interim UI shows the lowercase form but the one-shot only fires on the real chunk.)
pub fn maybe_decapitalize_next_chunk_realtime(text: &str) -> String { transform(text, ApplyMode::RealtimeChunk, true) }
pub fn maybe_decapitalize_next_chunk_standard(text: &str) -> String { transform(text, ApplyMode::StandardOutput, true) }

fn transform(text: &str, mode: ApplyMode, consume: bool) -> String {
    if text.is_empty() || !trigger_pending(mode) { return text.to_string(); }
    let Some((idx, ch)) = text.char_indices().find(|(_, c)| c.is_alphabetic()) else { return text.into() };
    if !ch.is_uppercase() { return text.into(); }            // already lower -> nothing to do
    let lowered = ch.to_lowercase().to_string();
    if lowered == ch.to_string() { return text.into(); }     // char has no lowercase form
    if consume { consume_trigger(mode); }
    let end = idx + ch.len_utf8();
    format!("{}{}{}", &text[..idx], lowered, &text[end..])    // lowercase only the first letter
}
```
Helpers `trigger_pending`/`consume_trigger` just lock `STATE`, expire stale deadlines, and clear flags.
There's also `begin_standard_post_recording_monitor(window_ms)` (start a window after recording stops
so an edit key *then* still arms the delayed output) and
`promote_pending_realtime_trigger_to_standard_output()` (if the user pressed the edit key *just before*
starting a standard recording, latch it so the eventual final output is decapitalized). An
`indicator_state(enabled)` returns `{eligible, armed}` for an optional overlay glyph.

**S2B2S integration.**
1. Add `text_replacement_decapitalize.rs` (it's nearly standalone — only depends on `once_cell`/`std`).
2. **Passive key listener:** S2B2S already runs a global key listener (`rdev` / `shortcut/key_listener.rs`).
   Hook a non-consuming observer for the configured edit key (default Backspace) that calls
   `mark_edit_key_pressed(timeout_ms, arm_standard_output=<true for standard mode>)`. Do **not** swallow
   the key — the user's Backspace must still work.
3. **Apply at the output boundary:** in S2B2S's paste/insert path, run the final text through
   `maybe_decapitalize_next_chunk_standard(...)`; in the streaming path run each chunk through the
   realtime variant. Order: apply decapitalize **after** custom-words/text-replacement, right before
   paste (see A4's fixed pipeline order).
4. Settings: `decapitalize_after_edit_enabled`, `..._timeout_ms`, `..._edit_key`. Optional overlay
   indicator via `indicator_state`.
5. **Port the unit tests** — this module's correctness lives in its tests (preview-doesn't-consume,
   monitor-arms-standard-output, expired-trigger-doesn't-latch, first-alphabetic-skips-punctuation).

---

### A4 — Streaming output quality kit (for `parakeet_streaming` and any cloud realtime)

S2B2S has `parakeet_streaming_enabled` and sherpa-onnx streaming. AivoRelay's `soniox_stream_processor.rs`
encodes hard-won lessons for turning a *revisable token stream* into *clean inserted text*. Three
transferable pieces:

**(a) Stable-prefix "safety buffer."** Streaming STT revises its most recent words. If you apply
fuzzy custom-word correction (S2B2S has this) to words that then change, you've already pasted the
wrong thing. Solution: accumulate raw chunks, but only emit the **stable prefix**, holding back the
**last N words** (default 3) until they settle. Flush the remainder at end-of-utterance.

```rust
pub struct StreamProcessor { pending_raw: String, stable_tail_words: usize /* + correction cfg */ }

impl StreamProcessor {
    pub fn push_chunk(&mut self, raw: &str) -> String {
        if raw.is_empty() { return String::new(); }
        self.pending_raw.push_str(raw);
        let end = stable_prefix_end(&self.pending_raw, self.stable_tail_words);
        if end == 0 { return String::new(); }                 // nothing settled yet
        let stable = self.pending_raw[..end].to_string();
        self.pending_raw.drain(..end);
        self.process_pipeline(&stable)                         // fuzzy -> replacements -> decap
    }
    pub fn flush(&mut self) -> String {                        // call at end of utterance
        let rest = std::mem::take(&mut self.pending_raw);
        if rest.is_empty() { String::new() } else { self.process_pipeline(&rest) }
    }
}

/// Byte index where the stable prefix ends: everything except the last `tail_words` tokens.
fn stable_prefix_end(text: &str, tail_words: usize) -> usize {
    if text.is_empty() { return 0; }
    if tail_words == 0 { return text.len(); }
    let mut starts = Vec::new(); let mut in_tok = false;
    for (i, c) in text.char_indices() {
        if c.is_whitespace() { in_tok = false; }
        else if !in_tok { starts.push(i); in_tok = true; }
    }
    if starts.len() <= tail_words { 0 } else { starts[starts.len() - tail_words] }
}
```
Only enable the buffer when fuzzy correction is on *and* a "keep safety buffer" setting is true;
otherwise `tail_words = 0` (emit everything immediately) for lowest latency.

**(b) Whitespace-preserving correction.** S2B2S's `apply_custom_words` (like AivoRelay's) tokenizes on
whitespace and rejoins with single spaces — which would destroy a streaming chunk's tabs/newlines/
double-spaces. Fix: only correct the *core* between leading/trailing whitespace, and **skip correction
entirely if the chunk has complex internal whitespace** (`"  "`, `\n`, `\r`, `\t`):

```rust
fn apply_custom_words_preserving_ws(text: &str, words: &[String], thr: f64, ngram: bool) -> String {
    let lead = text.len() - text.trim_start().len();
    let trail = text.len() - text.trim_end().len();
    let core = &text[lead .. text.len() - trail];
    if core.is_empty() { return text.into(); }
    if core.contains("  ") || core.chars().any(|c| matches!(c, '\n'|'\r'|'\t')) {
        return text.into();                                   // don't normalize formatting
    }
    let fixed = apply_custom_words(core, words, thr, ngram);   // S2B2S's existing fn
    format!("{}{}{}", &text[..lead], fixed, &text[text.len()-trail..])
}
```

**(c) Incremental paste session (paste + delete-last-chars).** For realtime insertion into the
focused field, AivoRelay maintains a streaming paste session: `begin_streaming_paste_session`,
`paste_stream_chunk(text)`, `delete_last_stream_characters(count)` (to revise interim text), and
`end_streaming_paste_session`. The `delete_last_stream_characters` is what lets interim results be
corrected in place without retyping everything. Also: a **one-shot leading-whitespace mode** (Preserve
/ RemoveIfPresent / AddIfMissing) decides whether the very first emitted chunk gets a leading space —
tracked with a `leading_applied` flag so it only happens once.

**Fixed pipeline order** (document it in code): `fuzzy custom words → text replacements → decapitalize
→ paste-delta`.

**S2B2S integration.** Wrap S2B2S's streaming STT output in a `StreamProcessor` that owns the
safety-buffer + ws-preserving correction; route inserts through an incremental paste session in the
clipboard/typing layer (S2B2S has `typing_tool`/`paste_method`/Linux `wtype`/`xdotool` — implement
delete-last-chars per backend, e.g. emit N backspaces). Gate the buffer behind
`*_keep_safety_buffer_enabled`.

---

### A5 — Quick-tap (tap vs hold) dual-action keys

**The idea.** One hotkey behaves differently for a quick tap vs a hold. AivoRelay uses it for
AI-Replace ("tap" with no spoken instruction → a default transform; "hold + speak" → instructed
transform), screenshots, and send-to-extension. The mechanic: compare the captured audio length (in
samples) against a ms threshold converted to samples.

```rust
fn quick_tap_threshold_samples(threshold_ms: u32) -> usize {
    // samples = ms * sample_rate / 1000   (use your capture sample rate, e.g. 16000)
    (threshold_ms as usize * SAMPLE_RATE) / 1000
}
fn is_quick_tap(samples_len: usize, threshold_ms: u32) -> bool {
    samples_len < quick_tap_threshold_samples(threshold_ms)
}
```
On `stop`, if `is_quick_tap(...)`, **skip transcription** and run the "tap" branch; otherwise transcribe
and run the "hold" branch.

**S2B2S integration.** S2B2S is push-to-talk-centric, so it already knows press/release timing and
buffers samples — adding a `*_quick_tap_threshold_ms` setting and the sample-length check in
`TranscribeAction::stop` is small. Good first use: a "tap to toggle read-aloud / hold to dictate" key,
or "tap to open conversation / hold to dictate."

---

### A6 — Dictation stats + a clean final-output hook

**Two small things.** (1) A "you've dictated N words / M characters" counter (with a "since" timestamp
so you can show a rate). (2) A tidy extension point — a single function every final dictation passes
through — so future per-output behaviors have one obvious home.

```rust
// text_output_hooks.rs — the extension point (port verbatim, then grow it).
pub enum FinalTextOutputSource { Dictation /* , Conversation, ReadAloud ... add as needed */ }
pub struct FinalTextOutput<'a> { pub source: FinalTextOutputSource, pub text: &'a str }

pub fn before_final_text_output(app: &AppHandle, out: FinalTextOutput<'_>) {
    match out.source {
        FinalTextOutputSource::Dictation => record_dictation_stats_for_text(app, out.text),
    }
}
```
`AppSettings` gains: `dictation_stats_enabled`, `dictation_word_count`, `dictation_character_count`,
and optional `*_since_ms`. `record_dictation_stats_for_text` increments counts (split_whitespace for
words, `chars().count()` for characters) and persists.

**S2B2S integration.** Call `before_final_text_output(app, FinalTextOutput { source: Dictation, text })`
at the single point where dictation text is finalized (just before paste). Surface the totals in a
Settings/stats panel. The hook also becomes the natural place to call A3's decapitalize and A4's
pipeline, keeping the output path tidy.

---

### A7 — Per-app transcript context + dynamic prompt variables

**The idea.** Make LLM post-processing context-aware. Two parts: (1) a rolling buffer of the last few
things you dictated **into the current app** (so the LLM can keep continuity), and (2) dynamic template
variables like `${current_app}`, `${time_local}`, and `${prev_transcript}` that get substituted into
prompts. S2B2S already tracks the active app (`active_app.rs`) and has LLM post-processing, so this is
a natural, cheap upgrade.

```rust
// transcript_context.rs — per-app rolling buffer with expiry. Port nearly verbatim.
struct Entry { text: String, last_updated: Instant }
static CTX: Lazy<Mutex<HashMap<String, Entry>>> = Lazy::new(|| Mutex::new(HashMap::new()));

pub fn get_short_prev_transcript(app_name: &str, max_words: usize, expiry: Duration) -> String {
    if app_name.is_empty() || max_words == 0 { return String::new(); }
    let mut m = CTX.lock().unwrap();
    m.retain(|_, e| e.last_updated.elapsed() < expiry);
    m.get(app_name).map(|e| last_n_words(&e.text, max_words)).unwrap_or_default()
}
pub fn update_transcript_context(app: &str, transcript: &str, max_words: usize, expiry: Duration) {
    if app.is_empty() || transcript.trim().is_empty() || max_words == 0 { return; }
    let mut m = CTX.lock().unwrap();
    m.retain(|_, e| e.last_updated.elapsed() < expiry);
    let incoming = last_n_words(transcript, max_words);
    let e = m.entry(app.to_string()).or_insert_with(|| Entry { text: String::new(), last_updated: Instant::now() });
    e.text = if e.text.is_empty() { incoming } else { last_n_words(&format!("{} {}", e.text, incoming), max_words) };
    e.last_updated = Instant::now();
}
fn last_n_words(t: &str, n: usize) -> String {
    let w: Vec<&str> = t.split_whitespace().collect();
    if w.len() <= n { w.join(" ") } else { w[w.len()-n..].join(" ") }
}
```

**Variable substitution** (do this right before the LLM call, alongside `${output}` from A1):
```rust
let prompt = prompt
    .replace("${output}", transcript)
    .replace("${current_app}", &current_app_name)
    .replace("${time_local}", &local_time_string)
    .replace("${prev_transcript}", &get_short_prev_transcript(&current_app_name, max_words, expiry));
```

**Audio clean-up.** AivoRelay also filters filler words/stutters from transcripts. **S2B2S already has
`custom_filler_words`** — so this is *not* a port; just make sure the filler-word pass runs before the
LLM and that `${prev_transcript}` is captured *after* it.

**S2B2S integration.** Add `transcript_context.rs` + settings
`llm_context_prev_transcript_enabled/_max_words/_expiry_seconds`. Call `update_transcript_context`
after each successful dictation (keyed by `active_app.rs`'s app name); call `get_short_prev_transcript`
during prompt assembly. Implement the variable substitution in `post_process_transcription`.

---

### A8 — Microphone auto-switch (wildcard name match + manual fallback)

**The idea.** Auto-select your preferred mic when it appears. Configure a name pattern (substring or
`*`/`?` wildcard, case-insensitive). Before each recording, reconcile: if a connected device matches
the pattern, select it; else fall back to the user's last *manual* selection; show a tiny overlay toast
on change. S2B2S has `selected_microphone` but no auto-switch.

```rust
// managers/microphone_auto_switch.rs — distilled. The wildcard matcher is the reusable nugget.
fn wildcard_match(pattern: &str, s: &str) -> bool {           // supports * and ?
    let (p, c): (Vec<char>, Vec<char>) = (pattern.chars().collect(), s.chars().collect());
    let (mut pi, mut ci, mut star, mut mi) = (0usize, 0usize, None::<usize>, 0usize);
    while ci < c.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == c[ci]) { pi += 1; ci += 1; }
        else if pi < p.len() && p[pi] == '*' { star = Some(pi); mi = ci; pi += 1; }
        else if let Some(s0) = star { pi = s0 + 1; mi += 1; ci = mi; }
        else { return false; }
    }
    while pi < p.len() && p[pi] == '*' { pi += 1; }
    pi == p.len()
}
fn matches_mask(device: &str, pattern: &str) -> bool {
    let (p, d) = (pattern.trim().to_lowercase(), device.to_lowercase());
    if p.is_empty() { return false; }
    if p.contains('*') || p.contains('?') { wildcard_match(&p, &d) } else { d.contains(&p) }
}

// reconcile_selected_microphone_before_recording(app):
//   names = list_input_devices()
//   target = if auto_enabled && !mask.empty {
//       first name matching mask  OR  last_manual_microphone (if still present)
//   } else { current selected }
//   if target != current { write settings; refresh active stream; show overlay; emit event }
```

**S2B2S integration.** Add settings `selected_microphone_auto_switch_enabled`,
`selected_microphone_name_pattern`, `last_manual_microphone`. Remember the user's manual pick whenever
they choose a device in the UI. Call `reconcile_selected_microphone_before_recording` at the top of
`TranscribeAction::start`. S2B2S already lists input devices (`audio_toolkit::audio::device`) and can
re-point the active stream — wire those in.

---

### A9 — Clipboard backup/restore-all-formats (selection capture without clobbering)

Several features (A1 preview insert, B2 AI-replace, B6 connector-with-selection) need to **read the
user's current text selection**. The portable way is "send Ctrl+C, read clipboard" — but that destroys
whatever was on the clipboard. AivoRelay backs up **all** clipboard formats first and restores them
after.

```rust
// clipboard.rs — the round-trip (distilled API).
pub fn backup_all_formats() -> Result<ClipboardBackup, String>;          // text, html, image, ...
pub fn restore_all_formats(backup: ClipboardBackup) -> Result<RestoreStats, String>;

pub fn capture_selection_text(app: &AppHandle) -> Result<String, String> {
    let backup = backup_all_formats()?;
    send_copy_keystroke();                 // Ctrl+C (platform input layer)
    wait_for_clipboard_update_briefly();   // small bounded wait/poll
    let text = read_clipboard_text().unwrap_or_default();
    let _ = restore_all_formats(backup);   // best-effort restore even on error
    Ok(text)
}
```

**S2B2S integration.** S2B2S already has `clipboard.rs` + `ClipboardHandling`. Add (or verify)
all-format backup/restore and a `capture_selection_text` helper. This is a prerequisite for B2/B6 and a
nice standalone robustness fix for the existing double-copy read-aloud trigger (don't leave the user's
clipboard mangled).

---

## 4. Tier B — Bigger net-new features (real engineering, real payoff)

These are larger than Tier A. Each is genuinely useful and each is independently shippable. Order
inside the tier is roughly value-per-effort. None of them are "you're behind" — S2B2S's local AI stack
is ahead of AivoRelay. These are *adjacent capabilities* AivoRelay built out that you currently don't
have.

### B1 — Cloud realtime STT providers (Soniox / Deepgram / OpenAI-realtime)

**Effort: XL.** **What it is:** streaming speech-to-text over a WebSocket to a cloud provider, with
**interim** (revisable) results arriving every few hundred ms and **final** results locking text in.
This is the cloud-relay counterpart to your local `parakeet_streaming`. S2B2S is local-first, so frame
this as an *optional* engine the user can select per-profile (see A1), never the default.

**Why bother when you have local streaming?** Three reasons AivoRelay users care about: (1) cloud
realtime models currently lead on accented speech and noisy far-field audio; (2) they need zero VRAM,
which matters when the GPU is busy with your local Brain; (3) some languages are far better served by
Soniox/Deepgram than by local models. It's a quality/latency/VRAM tradeoff knob, not a replacement.

**Architecture (provider-agnostic).** AivoRelay has one trait-ish shape implemented three times. Distill
it to a single internal interface so your coordinator doesn't care which cloud is behind it:

```rust
// The shape every realtime provider conforms to. Names distilled from
// soniox_realtime.rs / deepgram_realtime.rs / openai_realtime_whisper.rs.
pub trait RealtimeSttSession: Send {
    /// Open the socket, send the auth/config frame, spawn the read loop.
    fn start(cfg: RealtimeConfig, sink: RealtimeSink) -> Result<Self, String> where Self: Sized;
    /// Feed one chunk of PCM (f32 mono @ provider's expected sample rate).
    fn push_audio(&mut self, samples: &[f32]) -> Result<(), String>;
    /// Ask the provider to flush and emit a final for everything so far.
    fn finalize(&mut self) -> Result<(), String>;
    /// Close the socket. Idempotent (see S4 cleanup discipline).
    fn stop(&mut self) -> Result<(), String>;
}

/// Callbacks the session fires as text arrives. The coordinator wires these
/// into the same paste pipeline used by local streaming.
pub struct RealtimeSink {
    pub on_interim: Box<dyn Fn(InterimText) + Send>,   // revisable; may shrink/grow
    pub on_final:   Box<dyn Fn(FinalText)   + Send>,   // locked; append to committed text
    pub on_error:   Box<dyn Fn(String)      + Send>,
    pub on_closed:  Box<dyn Fn()            + Send>,
}

pub struct RealtimeConfig {
    pub provider: RealtimeProvider,            // Soniox | Deepgram | OpenAiRealtime
    pub api_key: SecretString,                 // pulled from keychain, never logged
    pub language_hint: Option<String>,         // canonicalized (see A2)
    pub sample_rate_hz: u32,
    pub endpoint_silence_ms: u32,              // provider endpointing (see knobs below)
    pub keepalive_secs: u32,                   // ping cadence to keep socket warm
    pub finalize_timeout_ms: u32,              // how long to wait for trailing finals
}
```

**The knobs that actually matter** (AivoRelay learned these the hard way — copy the *defaults* and the
*reasoning*, not just the fields):

- **Endpoint silence (`endpoint_silence_ms`).** How much trailing silence the provider waits for before
  it decides an utterance ended. Too low → it chops you mid-sentence; too high → laggy finals. AivoRelay
  exposes this per provider because Soniox and Deepgram disagree on a good default.
- **Keepalive ping (`keepalive_secs`).** Idle sockets get dropped by load balancers. A periodic ping
  (Deepgram has a documented keepalive message; for others, a tiny silent-audio frame) keeps the
  connection warm so the *next* utterance doesn't pay reconnect latency.
- **Finalize timeout (`finalize_timeout_ms`).** When the user releases PTT you send `finalize()`, but the
  final text arrives asynchronously a beat later. You must wait *bounded* time for it, then give up and
  paste what you have. Don't block forever; don't cut it off at zero.
- **Instant stop vs. graceful finalize.** Two stop modes: "I'm done, flush everything" (graceful, wait
  for finalize) and "abort now, discard" (instant, for cancel). Map these to your existing
  `notify_cancel` vs `notify_processing_finished` in `transcription_coordinator.rs`.
- **Preconnect / warm socket.** Optionally open the socket when the user *arms* PTT (key down) rather
  than when audio starts, shaving the TLS+WS handshake off the first word's latency. AivoRelay does this
  behind a flag because it costs a connection per key-press-without-speech.

**Interim → paste wiring.** The hard part is that interim results *revise* — the provider may retract the
last few words and re-emit them. This is exactly what A4's stable-prefix buffer + incremental-paste
session solve. So **B1 depends on A4.** The flow:

```
on_interim(text):
    committed_prefix = stable_prefix_end(text, SAFETY_WORDS)   // A4: hold back last N words
    new_tail = committed_prefix[len(already_pasted):]
    if interim shrank below already_pasted:
        delete_last_stream_characters(already_pasted - committed_prefix)   // A4 revision
    paste_stream_chunk(new_tail)                                            // A4 incremental paste
on_final(text):
    // lock it: run fuzzy correction (A4) + replacements + decapitalize (A3), then commit
```

**S2B2S integration.**
- New module `src-tauri/src/multi_stt/realtime/` (you already have `multi_stt/` — put cloud realtime
  beside the local engines so the selector is uniform).
- Register providers in the same place `multi_stt` enumerates engines; expose them as STT engine options
  selectable per Transcription Profile (A1).
- Keys go through `SecretMap` + keychain (you already have this — see §1.2). **Apply S1**: these
  providers have fixed endpoints, so hardcode them as canonical URLs; do not let a stored `base_url`
  redirect a Soniox key to an attacker.
- Reuse `transcription_coordinator.rs` for routing; the realtime sink calls the same `send_input` path.

**Provider notes (from the three implementations):**
- **Soniox** (`soniox_realtime.rs`, `soniox_stt.rs`): token-level streaming with per-token "is_final"
  flags; the stable-prefix logic in `soniox_stream_processor.rs` (A4) was written for exactly this.
- **Deepgram** (`deepgram_realtime.rs`, `deepgram_stt.rs`): utterance-level with `is_final` +
  `speech_final`; has a real keepalive control message; interim_results flag must be on.
- **OpenAI realtime** (`openai_realtime_whisper.rs`): realtime transcription session; heavier per-token
  cost, fewer endpointing knobs — treat as the "highest quality, least tunable" option.

> Scope control: if XL is too much for one pass, ship **Deepgram only** first (cleanest protocol), prove
> the A4 paste pipeline against a real revising stream, then add Soniox and OpenAI. The trait above means
> adding provider #2 and #3 is mostly protocol glue.

---

### B2 — AI Replace Selection (transform highlighted text in place)

**Effort: M.** **What it is:** user highlights text anywhere, holds a hotkey, speaks an instruction
("make this formal", "translate to German", "fix grammar"), and the selection is **replaced** by the
LLM's rewrite. AivoRelay's `ai_replace_with_llm` (in `actions.rs`). This is one of the highest-delight
features in AivoRelay and you already have every building block.

**Flow:**
```
1. capture_selection_text(app)            // A9: backup clipboard, Ctrl+C, read, restore
2. if selection empty AND quick-tap:      // A5: tap = "use spoken text as-is" mode
       treat spoken transcript as the content, no transform
   else:
       instruction = transcribe(mic)      // your existing STT
       rewrite = brain.run(prompt(instruction, selection))   // your streaming Brain!
3. paste(rewrite)                          // overwrites the still-selected text
4. on any error: restore original (selection untouched), surface a toast
```

**Why this is cheap for S2B2S specifically:** AivoRelay calls a *cloud* LLM here. **You have a local
streaming Brain** (`src-tauri/src/brain/`, llama.cpp + 10 providers). So your version is strictly
better: it can run fully offline and stream the replacement in. Wire `ai_replace` to the same Brain
entry point your conversation mode uses.

**The prompt contract** (keep it boring and strict so it doesn't editorialize):

```rust
fn ai_replace_prompt(instruction: &str, selection: &str) -> String {
    format!(
"You are a text transformation engine. Apply the user's instruction to the INPUT.
Output ONLY the transformed text. No preamble, no explanation, no quotes around it.
If the instruction is a question about the text, still output only the answer text.

INSTRUCTION: {instruction}
INPUT:
{selection}"
    )
}
```

**Critical safety/UX details AivoRelay got right:**
- **Always restore on error.** If the LLM call fails or returns empty, the *original selection must
  survive*. A9's backup makes this trivial — but also keep the selection itself intact (don't paste
  empty). Losing a paragraph to a failed rewrite is unforgivable UX.
- **Quick-tap empty-instruction mode (A5).** Tap (not hold) with no selection = "just type what I say"
  — a fast path that doubles as a fallback when selection capture fails.
- **Stream into place if you can.** Because your Brain streams, you can delete the selection and paste
  tokens as they arrive (A4 incremental paste) for a live rewrite effect. Gate behind a flag; some apps
  don't tolerate rapid synthetic edits.

**S2B2S integration.**
- Add `ai_replace` as a new `ShortcutAction` in `actions.rs` (mirror `TranscribeAction`'s start/stop
  shape). New binding id `"ai_replace"`; it becomes a per-profile-capable action like the rest.
- Selection capture: A9 in `clipboard.rs`.
- LLM: reuse `brain/` streaming entry; respect the user's selected Brain provider/model.
- This is also a natural **profile action** (A1): a "Formalize" profile vs a "Translate→DE" profile, each
  with a baked-in instruction prefix via `${output}`-style variables.

---

### B3 — File transcription + speaker diarization + SRT/VTT export

**Effort: L.** **What it is:** drop in an audio/video file, get back a transcript — optionally with
**speaker labels** ("who said what") and exportable as **SRT/VTT subtitles** or plain text. AivoRelay's
`file_transcription_diarization.rs` + `subtitle.rs`. Completely separate from the live dictation path
(it's batch, not realtime), so it won't destabilize your hot path.

**The diarization data model** (this is the clever part — copy it):

```rust
/// What the provider returns: speakers keyed by the provider's own opaque IDs
/// (e.g. "spk_a1b2", or an int per provider). Order/keys are NOT stable across files.
struct RawSpeakerBlock {
    provider_speaker_key: String,   // provider's opaque speaker handle
    text: String,
    start_ms: u64,
    end_ms: u64,
}

/// What we render: provider keys mapped to friendly *sequential* IDs (1,2,3…)
/// in order of first appearance, so the UI shows "Speaker 1 / Speaker 2".
struct DiarizedTranscriptBlock {
    speaker_id: u32,                // 1-based, assigned by first-appearance order
    speaker_label: String,         // editable display name ("Speaker 1" or renamed)
    text: String,
    start_ms: u64,
    end_ms: u64,
}
```

Two transforms make it usable:
1. **Provider-key → sequential-ID mapping by first appearance.** Walk the raw blocks; the first new
   `provider_speaker_key` you see becomes Speaker 1, the next new one Speaker 2, etc. Deterministic,
   human-friendly, stable within a file.
2. **Consecutive-block merge.** Adjacent blocks from the *same* speaker get concatenated into one block
   (providers over-segment). Merge when `speaker_id` matches and the time gap is small.

**Rename without re-transcribing** (the feature that makes diarization actually pleasant): persist the
diarized result to a **TTL'd temp artifact** (see S6 — 24h cleanup) keyed by a transcript id. When the
user renames "Speaker 1" → "Alice", you reload that artifact and re-render with the new label map — no
re-hitting the STT provider. AivoRelay's `reapply_diarized_transcript` does exactly this.

```rust
// Distilled: re-render a stored diarized transcript with a new label map.
fn reapply_diarized_transcript(
    transcript_id: &str,
    label_overrides: &HashMap<u32, String>,   // speaker_id -> new display name
) -> Result<String, String> {
    let path = temp_artifact_path(transcript_id);          // S6: validated, sandboxed
    let blocks: Vec<DiarizedTranscriptBlock> = load_json(&path)?;
    Ok(render_blocks(&blocks, label_overrides))            // "[Alice] hello\n[Bob] hi"
}
```

**Subtitle export** (`subtitle.rs` — this one you can lift almost verbatim, it's pure & dependency-free):

```rust
pub enum OutputFormat { Text, Srt, Vtt }

pub fn get_format_extension(format: OutputFormat) -> &'static str {
    match format { OutputFormat::Text => "txt", OutputFormat::Srt => "srt", OutputFormat::Vtt => "vtt" }
}

// segments_to_srt / segments_to_vtt walk timed segments and emit standard cue blocks:
//   SRT:  "1\n00:00:01,000 --> 00:00:04,000\nHello there\n\n"
//   VTT:  "WEBVTT\n\n00:00:01.000 --> 00:00:04.000\nHello there\n\n"
// The only real work is the timestamp formatter (ms -> HH:MM:SS,mmm for SRT / .mmm for VTT).
```

**Path-traversal validation is mandatory here** (S6): the user picks an arbitrary input path and an
arbitrary output path. Canonicalize and bound the output to an allowed directory; validate the input is
a readable file of an expected type. This is the single biggest footgun in the whole feature.

**S2B2S integration.**
- New module `src-tauri/src/file_transcription/` (batch; do not touch the live coordinator).
- Reuse whichever STT backend supports diarization. Local models generally don't diarize; cloud ones
  (Deepgram, etc.) do — so this pairs naturally with B1's providers. If you ship B3 before B1, gate
  diarization behind "requires a diarization-capable engine" and offer plain transcription otherwise.
- Frontend: a drop zone + a results view with editable speaker names + export buttons (Text/SRT/VTT).
- Temp artifacts under your app data dir with the S6 TTL sweeper.

---

### B4 — System-audio loopback capture (transcribe what's playing)

**Effort: L (Windows-first).** **What it is:** capture the *output* audio (what's coming out of your
speakers — a meeting, a video, a call) and transcribe it, optionally **mixed with the mic** so you get
both sides of a conversation. AivoRelay's `managers/live_sound_audio.rs`.

**The one insight that will save you days** (verbatim from the source's own comments): in **"Both"
mode** (mic + loopback together), **the mic drives the clock and the loopback fills a ring buffer.**
Why? On Windows WASAPI, *loopback capture produces no callbacks during silence* — if nothing is playing,
you get zero frames, so you can't use loopback as your timing source. The mic always produces frames at a
steady cadence, so you let the **mic callback** be the heartbeat: each mic frame, you grab whatever
loopback samples are currently buffered and mix them in.

```rust
// Distilled from live_sound_audio.rs. The architecture, not the literal bytes.
//
// Mic drives the clock: its frame callback mixes in loopback samples and pushes
// the result to the realtime manager. The loopback recorder's callback is
// replaced to simply fill a ring buffer consumed by the mic callback. This is
// required because loopback produces no callbacks during silence on WASAPI.
fn wire_both_mode(
    mic_recorder: &mut AudioRecorder,
    loopback_recorder: &mut AudioRecorder,
    manager: Arc<RealtimeManager>,
) {
    let loopback_buf = Arc::new(Mutex::new(Vec::<f32>::new()));

    // Loopback callback: just append to the shared ring buffer. No downstream push.
    let lb = loopback_buf.clone();
    loopback_recorder.set_on_frame(move |frame: &[f32]| {
        let mut buf = lb.lock().unwrap();
        buf.extend_from_slice(frame);
        cap_ring_buffer(&mut buf);            // keep it bounded
    });

    // Mic callback: the heartbeat. Mix mic + buffered loopback, push the mix.
    let lb2 = loopback_buf.clone();
    mic_recorder.set_on_frame(move |mic_frame: &[f32]| {
        let mixed = mix_with_secondary_buf(mic_frame, &lb2);   // drains buffered loopback
        manager.push_audio(&mixed);
    });
}
```

Three capture sources total:
- **Mic only** — your existing path.
- **System loopback only** — transcribe a video/meeting with no mic. Loopback *is* the clock here (fine,
  because if nothing's playing there's nothing to transcribe anyway).
- **Both** — the mic-clock-drives trick above.

**Device enumeration.** Loopback devices come from the *output* device list, not input. AivoRelay uses
`cpal`'s output enumeration: `list_output_devices()` feeds the loopback recorder
(`AudioCaptureSource::SystemOutputLoopback => list_output_devices()`).

**Independent pipeline.** AivoRelay deliberately gives loopback capture its **own** `AudioRecorder` +
its **own** realtime managers, bypassing the singleton mic recorder. Mirror that: don't try to multiplex
your existing single recorder. A second independent recorder instance is cleaner and avoids state
collisions with live dictation (and plays nicely with S4's per-session model).

**S2B2S integration.**
- New `src-tauri/src/managers/loopback_audio.rs` (or under your `audio/` module).
- Pairs with B1 (cloud realtime) or `parakeet_streaming` (local) as the transcription sink — loopback is
  just another audio source feeding a realtime STT session.
- **Platform reality:** clean loopback is Windows-first (WASAPI). macOS needs an aggregate device or a
  virtual driver (BlackHole) and is much fussier; Linux is PipeWire/PulseAudio monitor sources. Ship
  Windows, feature-detect the rest, and don't block the feature on cross-platform parity.
- Expose capture-source (Mic / System / Both) as a per-profile or global setting.

---

### B5 — Region / screenshot capture

**Effort: M (Windows-first).** **What it is:** drag a rectangle on screen, capture that region as a PNG.
On its own it's a screenshot tool; its real purpose is to **feed an image to the connector (B6) or to a
vision-capable Brain** — "look at this and explain it", "what's this error". AivoRelay's
`region_capture.rs` + `commands/region_capture.rs`.

**The shape:**
```rust
pub struct VirtualScreenInfo { /* bounds across all monitors, DPI-aware */ }
pub struct SelectedRegion    { pub x: i32, pub y: i32, pub width: u32, pub height: u32 }
pub enum   RegionCaptureResult { Captured(SelectedRegion), Cancelled }
pub struct RegionCaptureState { /* tracks an in-flight selection across the overlay */ }

pub fn get_virtual_screen_info() -> Result<VirtualScreenInfo, String>;  // multi-monitor virtual desktop
pub fn on_region_selected(app: &AppHandle, region: SelectedRegion);     // grab pixels -> PNG -> emit
pub fn on_region_cancelled(app: &AppHandle);
pub fn base64_encode(data: &[u8]) -> String;                            // for inline transport
```

**Two things AivoRelay handles that are easy to get wrong:**
- **Virtual screen / multi-monitor + DPI.** Coordinates must be in the *virtual desktop* space spanning
  all monitors, and DPI-scaled correctly, or your rectangle captures the wrong pixels on a secondary
  hi-DPI display. `get_virtual_screen_info` centralizes that math; the function is `#[cfg]`-split per OS
  (note the two definitions at different lines — Windows vs. non-Windows).
- **A transparent always-on-top overlay** for the drag interaction, with a clean cancel (Esc →
  `on_region_cancelled`). This reuses your `overlay_fx/` infrastructure — you already render overlays, so
  the selection rectangle is a new overlay mode rather than net-new windowing.

**S2B2S integration.**
- New `src-tauri/src/region_capture.rs` + a Tauri command module; render the selection overlay through
  your existing `overlay_fx/`.
- Output a PNG path (temp artifact, S6 TTL) **and/or** base64. The path form is better for large images
  (the connector's `/blob` route, B6, exists precisely so you don't shove megabytes of base64 through a
  message channel).
- Natural consumers: B6 connector ("send screenshot + voice to ChatGPT") and a vision Brain path if you
  add one. Standalone value is modest; it shines as an input to B6.
- **Platform reality:** Windows-first, same as B4. macOS needs Screen Recording permission and a
  different capture API; gate per-OS.

---

### B6 — Secure browser connector

**Effort: XL.** **What it is:** a browser extension talks to S2B2S over a hardened localhost server;
S2B2S can push **voice transcripts + selected text + screenshots** straight into a ChatGPT / Claude /
Gemini web tab, and the page can pull queued messages. AivoRelay's `managers/connector.rs` (2467 lines)
+ a browser extension. This is the single most security-sensitive feature in AivoRelay, and it is the
direct reason S2's hardened-server work exists.

**Do NOT build this until S2 is done.** B6 *is* the consumer of S2's threat model. If you build S6→B6 on
an unauthenticated server you ship a remote-code-ish surface (any web page on the machine could poke it).
The whole point of the connector's design is that it's safe to expose to a browser.

**The security model (reuse the exact same core as S2):**
```
1. Bind 127.0.0.1 only. Never 0.0.0.0.
2. Handshake: extension presents a password/token (bootstrapped once, shown in S2B2S UI).
   Server derives a per-session AEAD key from it.
3. Every /messages and /blob request must carry the session credential AND a valid
   nonce + MAC; the server verifies origin is the exact expected extension origin.
   (See connector.rs header comment: "Every subsequent /messages and /blob request must include…")
4. Auth failures back off (rate-limit brute force).
5. Large payloads (screenshots) are NOT inlined — they're stored and fetched via
   GET /blob/{att_id} with the same auth. (route "/blob/{att_id}" -> handle_get_blob)
```

**The message queue** (the functional heart — distilled):
```rust
pub struct QueuedMessage { /* id, text, optional attachment ref, timestamps, state */ }

impl Connector {
    // Plain text -> queue. Returns a message id the extension will fetch.
    pub fn queue_message(&self, text: &str) -> Result<String, String>;

    // Text + image: store the image as a blob, queue a message that *references* it
    // by a localhost /blob URL the page fetches separately. This is why B5/B6 pair up.
    pub fn queue_bundle_message(&self, text: &str, image_path: &PathBuf) -> Result<String, String>;
    pub fn queue_bundle_message_bytes(&self, text: &str, bytes: &[u8], mime: &str) -> Result<String, String>;
}
// Events to the UI: MessageQueuedEvent / MessageDeliveredEvent / MessageCancelledEvent
// let the desktop app show delivery state per message.
```

The extension polls `GET /messages` (authenticated), renders/sends them into the AI web app's input,
fetches any `/blob/{id}` attachments, and reports delivery back so the desktop UI can show
queued→delivered→done.

**Why this is compelling for S2B2S:** it bridges your *local* voice/vision capture to the *web* AI tools
people already pay for, without an API key — the user's existing ChatGPT/Claude session does the work.
Combined with B5 ("screenshot this") and B2 ("transform this"), it's a coherent "voice+vision to my
browser AI" story that complements your local Brain rather than competing with it.

**S2B2S integration.**
- New `src-tauri/src/managers/connector.rs` built **on the S2 server core** (extract S2's authenticated
  axum server into something both `control_server` and `connector` share).
- Ship/port the browser extension (separate artifact; the protocol contract above is what matters).
- Reuse B5 for screenshots, A9 for selection capture, your STT for voice.
- **Gate off by default.** First-run must require explicit enable + show the pairing token. Treat parity
  with S2's "dangerous endpoints opt-in" stance.

> This is the biggest single item in the doc. If you only have appetite for the *idea*, the takeaway is:
> S2's hardened server + a message queue + a `/blob` route + a paired extension = a safe local↔browser
> bridge. Everything else is protocol detail.

---

### B7 — Voice Command Center (⚠️ security-sensitive)

**Effort: M, but read the warning.** **What it is:** speak a command, and the app runs a shell action —
either a **pre-registered** command matched by fuzzy trigger, or, failing that, an **LLM-generated**
command. AivoRelay's `commands/voice_command.rs` executes PowerShell with a configurable
`ExecutionPolicy` (`Bypass | Unrestricted | RemoteSigned | Default`), in silent or windowed modes.

**S2B2S already has a safer primitive for the *common* case:** `post_process_actions` +
`external_script_path`. For "run my script after a transcript", you don't need B7 — you have it. So scope
B7 to the *net-new* part: **fuzzy-triggered, voice-invoked, pre-registered commands.**

**The hard line on the LLM-generated part — port AivoRelay's discipline exactly:**
- **NEVER auto-run an AI-generated command.** AivoRelay's flow is fuzzy-trigger match → (only if no
  match) LLM fallback → **mandatory user confirmation** before anything executes. The confirmation step
  is not optional and not skippable. Reproduce that precisely.
- **Show the exact command** that will run, in a dialog, every time, for the LLM path.
- **Default to the most restrictive execution policy.** Don't default to `Bypass`. Map the policy enum
  but make `Default`/`RemoteSigned` the out-of-box value; `Bypass`/`Unrestricted` require deliberate
  opt-in with a visible warning.
- **Pre-registered commands** (user typed them in settings themselves) are the safe path — those can run
  on fuzzy-trigger match without an LLM in the loop, because the user authored them.

```rust
// The decision flow to reproduce (commands/voice_command.rs, distilled):
fn handle_voice_command(spoken: &str, registry: &[RegisteredCommand]) -> Decision {
    if let Some(cmd) = fuzzy_match(spoken, registry, THRESHOLD) {
        Decision::RunRegistered(cmd)          // user-authored -> may run on match
    } else {
        let candidate = brain.suggest_command(spoken);   // LLM fallback
        Decision::ConfirmRequired(candidate)  // MUST show dialog; NEVER auto-run
    }
}

pub enum ExecutionPolicy { Default, RemoteSigned, Unrestricted, Bypass }  // default = Default/RemoteSigned
```

**S2B2S integration.**
- If you build it: `src-tauri/src/commands/voice_command.rs`, registry persisted in settings, fuzzy match
  reusing the same matcher family as your `custom_words` correction.
- LLM suggestion via your local Brain (better: the suggestion never leaves the machine).
- **Cross-platform:** AivoRelay is PowerShell-specific. Generalize to `sh`/`pwsh`/`cmd` per-OS, or scope
  to Windows initially.
- **Honestly: this is the lowest-priority Tier B item for a local-first voice OS**, because the
  auto-run-shell surface is risky and you already cover the safe 80% with `post_process_actions`. Port
  the *confirmation discipline* as a documented principle even if you skip the feature.

---

### B8 — Live Preview window (editable real-time transcript)

**Effort: L.** **What it is:** instead of pasting straight into the focused app, dictation streams into a
dedicated **preview window** the user can read and edit *before* committing it, then flush it into the
target app. AivoRelay's `managers/preview_output_mode.rs` + a preview UI with a rich hotkey set. Great
for long-form dictation, messy environments, or when you want to LLM-polish before it lands.

**The hotkey vocabulary** (each is a small, independent action against the preview buffer):
- close, clear, **flush** (commit visible text to the target app), **process** (run LLM on the buffer),
  insert (push to app without closing), delete-until-dot (delete back to previous sentence boundary),
  delete-last-word. `preview_output_mode.rs` tracks `processing_llm` / `flush_visible` state so the UI
  can show "thinking" vs "ready to flush".

**The genuinely interesting idea — sliding-window LLM** (worth stealing even if you skip the rest):
run the LLM continuously over a **sliding window** of the live transcript, so polish/translation streams
in *as you speak* rather than as a final pass. The preview window is the natural surface for this because
it's already showing revisable text. This is a neat fusion of your streaming Brain + streaming STT that
neither app fully exploits yet.

```rust
// Preview buffer state (distilled from preview_output_mode.rs):
struct PreviewState {
    active: bool,
    processing_llm: bool,     // an LLM pass is in flight over the buffer
    flush_visible: bool,      // there is committed-enough text ready to flush
    // + the live text buffer and a committed/uncommitted split
}
// set_processing_llm(app, true/false) toggles the spinner; flush() emits buffer -> target app.
```

**S2B2S integration.**
- New preview window (Tauri webview) + `src-tauri/src/managers/preview_output_mode.rs`.
- Render through your existing overlay/window infra; the buffer feeds from the same realtime STT sink as
  normal dictation, just routed to the window instead of the active app.
- The "process" hotkey calls your Brain; "flush" calls your normal paste path.
- Pairs beautifully with A3 (decapitalize), A4 (incremental paste into the *app* on flush), and the
  sliding-window Brain idea above. This is the most "research-y / highest-ceiling" Tier B item.

---

## 5. Tier C — Polish & developer-experience (small, satisfying)

Low-effort items that punch above their weight. Most are XS–S.

### C1 — Shared hotkey-guide manifest (data-driven shortcut help)

**Effort: S.** AivoRelay drives its entire in-app shortcut help from one JSON manifest
(`src/lib/hotkeyGuideManifest.json` + `hotkeyGuide.ts`) instead of hand-maintaining a help screen.
Categories list `bindingIds`; **feature gates** hide rows when a feature is off; **dynamic prefixes**
expand per-profile bindings (one row → one row per Transcription Profile). Shape:

```jsonc
{
  "version": 1,
  "featureGates": {                       // bindingId visibility tied to a settings bool
    "voice_command": "voice_command_enabled",
    "send_to_extension": "send_to_extension_enabled"
  },
  "categories": [
    {
      "id": "recording",
      "title": "Recording",
      "titleKey": "hotkeySidebar.categories.recording",   // i18n key
      "bindingIds": ["transcribe", "transcribe_default", "cancel", "repaste_last", "cycle_profile"],
      "dynamicPrefixes": []               // e.g. expand "transcribe_<profile>" per profile
    }
  ]
}
```

**Why port it:** the moment you add A1 profiles, A5 quick-tap actions, B2 ai-replace, etc., a static help
screen rots instantly. A manifest keeps help in sync with bindings for free and gives you i18n keys and
feature-gating in one place. Low risk, pure additive. Pairs with A1 (dynamic per-profile rows).

### C2 — LLM post-process benchmark tool

**Effort: S–M.** AivoRelay has `run_llm_post_process_benchmark` /
`build_llm_post_process_benchmark_result` (`actions.rs`): run the *same* input through different
LLM prompts/models and compare outputs + latency side by side. For S2B2S — which has **10 LLM providers
and a local llama.cpp Brain** — this is more valuable than it is for AivoRelay: it's how a user decides
"is local-Qwen good enough for my post-processing, or do I want a cloud model?" without guesswork.

```rust
// Distilled: compare candidates on one input.
pub async fn run_llm_post_process_benchmark(
    input: &str,
    candidates: &[BenchmarkCandidate],   // {provider, model, prompt}
) -> Result<LlmPostProcessBenchmarkResult, String>;
// Result holds per-candidate {output, latency_ms, token_counts} for a comparison table.
```

Build a small settings-page panel: paste a sample transcript, pick candidates, see a table. Reuses your
existing Brain/provider plumbing entirely.

### C3 — Output whitespace modes

**Effort: XS.** AivoRelay's `OutputWhitespaceMode { Preserve, RemoveIfPresent, AddIfMissing }`
(`settings.rs`) controls leading whitespace on inserted text. Sounds trivial; matters a lot when you
dictate *into the middle* of existing text — should "hello" become " hello" or "hello"? `AddIfMissing`
adds a leading space when the preceding char isn't whitespace; `RemoveIfPresent` strips it; `Preserve`
leaves the model's output alone. The A4 streaming kit already references this enum (`leading_mode`), so
porting A4 nudges you toward porting this too.

### C4 — LF→CRLF conversion option

**Effort: XS.** `change_convert_lf_to_crlf_setting` (`shortcut.rs`) — some Windows targets (older
editors, certain text fields) want CRLF line endings. A simple toggle that converts `\n`→`\r\n` on output
prevents "all my line breaks vanished" bug reports. Windows-relevant only; trivial to add to the final
output stage (pairs with A6's `before_final_text_output` hook — do the conversion there).

### C5 — Secret-string ergonomics (you mostly have this)

**Effort: XS, mostly a checklist.** AivoRelay leans on `secure_keys.rs` secret wrappers. **S2B2S already
has `SecretMap` with a redacting `Debug` impl + OS keychain** (§1.2) — you're ahead here. The only delta
worth checking: ensure *every* new secret field added by B1 (cloud STT keys), B6 (connector token) routes
through `SecretMap`/keychain and **never** lands in a plain `Debug`/log line. When you add S1's canonical
URLs, the same discipline applies to anything embedded in a URL. No new type needed — just don't regress.

---

## 6. Suggested implementation sequence (a sane roadmap)

Dependencies are real here; this order minimizes rework. Each "wave" is independently shippable.

**Wave 0 — Security foundation (do first, no exceptions).**
`S1` canonical URLs → `S3` webview hardening → `S6` path-traversal + TTL cleanup → `S2` authenticate
`control_server`. These are mostly self-contained, low-risk, and **S2 is a hard prerequisite for B6**.
S1 is the single highest value/lowest risk item in the whole document — do it literally first.

**Wave 1 — Recording-core robustness.**
`S4` RAII session + binding-matched state machine → `S5` generation-counter + token-identity
cancellation. These harden the hot path and make every later feature (profiles, realtime, preview) safer
to build on. Do them before piling new actions onto `actions.rs`.

**Wave 2 — The headline UX.**
`A1` Transcription Profiles first (it reshapes how bindings/settings resolve — everything downstream
becomes "profile-aware"). Then `A2` keyboard-layout language (feeds A1's `language="os_input"`), `A9`
clipboard backup/restore (prerequisite for B2/B6), `A5` quick-tap, `A6` stats+final-output hook.

**Wave 3 — Text-quality kit.**
`A3` decapitalize → `A4` streaming output kit (stable-prefix + incremental paste + fuzzy) → `C3`
whitespace modes → `C4` CRLF. Port A3/A4's **unit tests** — they encode painful edge cases. This wave
makes both your local streaming and any future cloud streaming feel polished.

**Wave 4 — Context & convenience.**
`A7` per-app transcript context + dynamic prompt variables → `A8` mic auto-switch → `C1` hotkey manifest
(now that profiles + actions exist to populate it) → `C2` LLM benchmark panel.

**Wave 5 — Big adjacent capabilities (pick by user demand).**
`B2` AI-replace-selection (cheap, high delight, uses your Brain) → `B3` file transcription + diarization
+ subtitles (isolated batch path) → `B1` cloud realtime STT (depends on A4) → `B4` loopback (depends on a
realtime sink) → `B5` region capture → `B6` connector (**depends on S2**; biggest item) → `B8` live
preview + sliding-window Brain → `B7` voice command center (lowest priority; port the *confirmation
discipline* regardless).

**Rule of thumb:** never start a Tier B item before Wave 0–1 are done. Most B features quietly assume the
robustness and security work already exists.

---

## 7. Per-feature → S2B2S integration map (where each change lands)

Quick reference so the implementer knows which existing files to touch. "+new" = create; "edit" =
modify existing. Paths relative to repo root.

| Item | Create / edit in S2B2S |
| --- | --- |
| S1 canonical URLs | +new `src-tauri/src/url_security.rs`; **edit** `llm_client.rs` (route `base_url` through canonicalizer), TTS backend URL builders in `src-tauri/src/tts/`, `control_server.rs` |
| S2 auth control_server | **edit** `src-tauri/src/control_server.rs` (bearer token from keychain, constant-time compare, reject browser `Origin`, gate endpoints off-by-default) |
| S3 webview hardening | +new `src-tauri/src/webview_hardening.rs`; **edit** window setup in `lib.rs`/main to install on release builds |
| S4 RAII session | +new `src-tauri/src/recording_session.rs`; **edit** `actions.rs` (`TranscribeAction` uses session), `transcription_coordinator.rs` |
| S5 cancellation | +new `src-tauri/src/llm_operation.rs` + `recording_auto_stop.rs`; **edit** `brain/` call sites + auto-stop timer |
| S6 path/TTL | +new `src-tauri/src/temp_artifacts.rs` (validate + sweep); **edit** any file-writing path (B3, B5 outputs) |
| A1 profiles | **edit** `settings.rs` (add `TranscriptionProfile`, resolution helpers), `actions.rs` (`transcribe_<id>` bindings, `resolve_stt_prompt`), bindings registration |
| A2 kbd language | +new `src-tauri/src/input_source.rs`; **edit** profile language resolution (A1), STT language plumbing |
| A3 decapitalize | +new `src-tauri/src/text_replacement_decapitalize.rs`; **edit** output pipeline + a passive key listener; **port tests** |
| A4 streaming kit | +new `src-tauri/src/streaming/` (stable-prefix, incremental paste, fuzzy-ws); **edit** `parakeet_streaming`, future B1 sink; **port tests** |
| A5 quick-tap | **edit** key handler in `shortcut.rs`/`actions.rs` (sample-length vs threshold) |
| A6 stats + hook | +new `src-tauri/src/text_output_hooks.rs` + a stats store; **edit** final-output path |
| A7 transcript context | +new `src-tauri/src/transcript_context.rs`; **edit** prompt builder for `${current_app}`/`${time_local}`/`${prev_transcript}` |
| A8 mic auto-switch | +new `src-tauri/src/managers/microphone_auto_switch.rs`; **edit** pre-recording device selection in audio init |
| A9 clipboard | **edit** `src-tauri/src/clipboard.rs` (`backup_all_formats`/`restore_all_formats`/`capture_selection_text`) |
| B1 cloud realtime | +new `src-tauri/src/multi_stt/realtime/` (per-provider); **edit** engine enumeration, `transcription_coordinator.rs`; **needs A4 + S1** |
| B2 ai-replace | +new `ai_replace` action in `actions.rs`; **edit** binding registration; **uses A9 + brain/** |
| B3 file/diarization | +new `src-tauri/src/file_transcription/` + lift `subtitle.rs`; **uses S6**; pairs with B1 providers |
| B4 loopback | +new `src-tauri/src/managers/loopback_audio.rs`; second independent `AudioRecorder`; **uses a realtime sink** |
| B5 region capture | +new `src-tauri/src/region_capture.rs` + command module; **edit** `overlay_fx/` for the selection overlay |
| B6 connector | +new `src-tauri/src/managers/connector.rs` on the **S2 server core** + browser extension; **needs S2 + B5 + A9** |
| B7 voice command | +new `src-tauri/src/commands/voice_command.rs`; **uses brain/ + fuzzy matcher**; (note: `post_process_actions` already covers the safe case) |
| B8 live preview | +new preview window + `src-tauri/src/managers/preview_output_mode.rs`; **uses overlay infra + brain/ + A4** |
| C1 hotkey manifest | +new `src/lib/hotkeyGuideManifest.json` + `hotkeyGuide.ts`; **edit** help UI to render from it |
| C2 LLM benchmark | +new benchmark fn in `actions.rs` + a settings panel; **uses brain/ + providers** |
| C3 whitespace modes | **edit** `settings.rs` (`OutputWhitespaceMode`) + output stage |
| C4 CRLF | **edit** settings + final output stage (do it in A6's hook) |
| C5 secret hygiene | **edit** new secret fields (B1/B6) to route through existing `SecretMap`/keychain |

---

## 8. Attribution & licensing

Both projects are MIT forks of [cjpais/Handy](https://github.com/cjpais/Handy). You own S2B2S; AivoRelay
(MaxITService/AIVORelay) is MIT. You may freely adapt its code into S2B2S. To stay clean and courteous:

- **Keep the MIT license intact.** S2B2S already ships MIT; nothing changes there. MIT only requires that
  the original copyright notice + license text be preserved in copies of *that* code.
- **Credit adapted code inline.** Where you port a non-trivial chunk, add a short header comment so future
  maintainers know the provenance:
  ```rust
  // Adapted from AivoRelay (MaxITService/AIVORelay), MIT License.
  // Source: src-tauri/src/<original_file>.rs — <feature> (<date>).
  ```
- **Carry AivoRelay's copyright where you copy substantial verbatim blocks.** For large near-verbatim
  files (e.g. `subtitle.rs`), include AivoRelay's MIT copyright line alongside yours, or add an entry to a
  `THIRD_PARTY_NOTICES`/`NOTICE` file listing AivoRelay as an MIT source. Either satisfies MIT.
- **The reference code in this document is a guide, not a drop-in.** It's distilled/renamed to match
  S2B2S conventions and to be readable without AivoRelay's source. Treat it as a spec to (re)implement and
  test against, not as files to paste blind. Where this doc says "lift almost verbatim" (e.g. subtitle
  formatting), pull the real file from AivoRelay and add the attribution header above.
- **Shared upstream heritage helps you.** Because both forks still track Handy, the trait/settings/binding
  scaffolding lines up (§1.1) — most ports are "add a new impl/field," not "rearchitect." That's why these
  are cheap.

> Net: you're not "catching up" to AivoRelay — S2B2S leads on local AI (TTS pipeline, streaming local
> Brain, conversation mode, wake word). This document is a **cherry-pick list**: take AivoRelay's security
> hardening (Tier S) and its best UX/feature ideas (Tiers A–C), fold them into your stronger local-first
> core, and keep the attribution tidy.

---

## Appendix A — AivoRelay settings inventory (completeness checklist)

AivoRelay's `settings.rs` carries **~393 settings fields** (vs S2B2S's leaner set). Field *density* per
area is a good signal of where AivoRelay invested — and a checklist of knobs the implementer may want
when building each feature. Counts below are approximate field counts per prefix cluster, mapped to the
Tier item that introduces them. **You do not need most of these** — port the *feature*, then add only the
knobs your users actually ask for. This is a "did I miss an option" reference, not a spec to replicate.

| Cluster (≈field count) | Maps to | What the knobs cover |
| --- | --- | --- |
| `soniox_*` (~59) | B1 | Soniox realtime: api key, model, language hints, endpointing, keepalive, finalize timeout, context biasing (`soniox_context_*`), live-preview integration (`soniox_live_preview_*`), sliding-LM-window (`..sliding_lm_window`), instant-stop, preconnect. The biggest cluster — Soniox was AivoRelay's flagship engine. |
| `recording_*` (~39) | S4, A-overlay | Recording lifecycle + the recording overlay: bar styling (`recording_overlay_bar_*`), decapitalize indicator (`recording_overlay_decapitalize_indicator_*`), auto-stop (`recording_auto_stop_*`, → S5), push-to-talk behavior. Much of this is overlay *customization* you may not want (you have `overlay_fx/`); the auto-stop + PTT bits map to S4/S5. |
| `voice_command_*` (~23) | B7 | Voice Command Center: enable, registry, execution policy, silent/windowed mode, reasoning/LLM-fallback (`voice_command_reasoning_*`, `voice_command_use_*`), confirmation. ⚠️ security-sensitive — see B7. |
| `ai_replace_*` / `ai_*` (~17) | B2 | AI-replace-selection: enable, instruction prompt, model/provider, translate-to (`translate_to_*`), quick-tap empty mode. Cheap to port (you have the Brain). |
| `post_process_*` (~14) | C2, A6 | LLM post-processing + benchmark (`post_process_benchmark_*`, `post_process_reasoning_*`). **S2B2S already has `post_process_actions`** — only the *benchmark* panel (C2) is net-new. |
| `connector_*` (~14) | B6 | Secure browser connector: enable, port, pairing/auth, allowed origin, blob settings. **Depends on S2.** |
| `send_*` (~11, e.g. `send_to_extension_*`, `send_screenshot_*`) | B5, B6 | "Send to extension" action gates (with-selection, with-screenshot). Feature gates that also drive the C1 hotkey manifest. |
| `deepgram_*` (~11) | B1 | Deepgram realtime: key, model, endpointing, keepalive, interim results, diarization toggle (feeds B3). |
| `text_*` (~10) | A3, A4, C3 | Text replacement + output shaping: replacements list, decapitalize (→A3), whitespace mode (→C3). |
| `screenshot_*` / `region` (~9) | B5 | Region/screenshot capture: enable, output format, hotkey. |
| `live_sound_*` (~9) | B4 | System-audio loopback: capture source (Mic/System/Both), per-engine endpointing (`live_sound_deepgram_endpointing_*`), device selection. |
| `llm_*` (~6) | brain/ | Generic LLM provider/model/prompt config. S2B2S's `brain/` + 10 providers already exceeds this. |
| `microphone_*` (~3) | A8 | Mic auto-switch: enable, preferred-name patterns, manual override. |
| `dictation_*` (~5) | A6 | Dictation stats: counts, durations, history toggles. |
| `saved_window_*` / `remember_window_*` (~6) | B8 | Persisted preview/aux window geometry (`saved_window_*`, `remember_window_*`). |
| `whisper_*` (~2), `local_*` (~3), `custom_*` (~4), `transcription_*` (~3) | A1, multi_stt | STT engine selection, custom words/replacements, **transcription profiles** (`transcription_profile*` → A1). |
| `selected_*` (~6), `provider_*` (~4), `model_*` (~5), `id`/`name` (~12) | A1, B1 | Per-profile/per-provider selection state + the profile/provider struct `id`/`name` fields (these inflate the raw count because profiles and providers are `Vec<Struct>` with their own fields). |
| `output_*` (~3), `paste_*` (~2), `push_*` (~2) | A4, A5 | Output/paste mode + push-to-talk tuning. |
| `allow_*` (~3), `base_*` (~3), `error_*` (~3), `debug_*` (~3) | S1, misc | `allow_insecure_http` (→S1!), `base_url` fields (→S1 canonicalization), error-handling + debug toggles. |

**How to read this table:** clusters at the top (soniox, recording, voice_command) are where AivoRelay
spent the most config surface — they're the most "finished" features but also the most over-knobbed.
When you implement the corresponding Tier item, start with a *minimal* settings footprint and grow it
only on real demand. The clusters most worth their fields for S2B2S: `*_realtime`/`soniox`/`deepgram`
(B1), `ai_replace` (B2), `live_sound` (B4), `connector` (B6), and the `transcription_profile`/`base_url`/
`allow_insecure_http` fields (A1 + S1).

---

## Appendix B — One-screen summary for the implementer

If you read nothing else:

1. **Do Wave 0 security first.** `S1` canonical URLs is the highest value/lowest risk change in this
   document — known providers ignore stored `base_url`; only `"custom"` allows arbitrary HTTPS URLs with
   an explicit `allow_insecure_http` opt-in. Then `S3`, `S6`, `S2`. **`S2` is required before `B6`.**
2. **Harden the recording core** (`S4` RAII binding-matched session, `S5` generation-counter +
   token-identity cancellation) before adding new actions to `actions.rs`.
3. **`A1` Transcription Profiles is the headline feature** and it makes everything downstream
   profile-aware — build it before the other UX items. It rides on the shared Handy
   `settings.bindings` + `ShortcutAction` skeleton you both still have, so it's cheap.
4. **Port the text-quality kit with its tests** (`A3` decapitalize, `A4` stable-prefix + incremental
   paste + whitespace-safe fuzzy). The tests encode the painful edge cases.
5. **You already lead on local AI.** S2B2S's TTS pipeline, streaming local Brain, conversation mode, and
   wake word have no AivoRelay equivalent. This document is a cherry-pick of AivoRelay's security
   hardening and its best cloud/UX/file features — fold them into your stronger local-first core.
6. **The reference code here is a spec, not paste-in.** Re-implement to S2B2S conventions, add
   `// Adapted from AivoRelay (MIT)` headers on non-trivial ports, and lift `subtitle.rs` near-verbatim
   (with attribution) since it's pure and dependency-free.

*End of note.*
