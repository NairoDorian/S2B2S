/**
 * Assert against the REAL recording overlay window, over CDP.
 *
 *     WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222 bun run dev:fast
 *     bun tests/overlay-window.ts
 *
 * The companion to `tauri-window.ts`, and the overlay's acceptance check from
 * the React → Solid 2 migration (see CHANGELOG, 2026-09-13). The overlay is a native window whose geometry
 * the backend reads back *from this webview* (`overlay_stream_text_height`), so
 * a browser-only check cannot prove it works — it has to run in the real
 * webview, against the real Rust backend.
 *
 * It drives every branch of `RecordingOverlay` by emitting the backend's own
 * events from the page (`plugin:event|emit`, which Tauri broadcasts back to
 * every listener in the window), so the overlay's handlers run exactly as they
 * do during a recording — and the commands they call (`get_app_settings`,
 * `overlay_stream_text_height`, `overlay_scope_frame`) hit real Rust.
 *
 * What it does NOT cover, because only a human at a microphone can: the
 * typewriter's timing against a live stream, the backend's own window sizing,
 * and the height report's round trip. That last one is not observable from the
 * page — `overlay_stream_text_height` returns the cached cap while Rust's own
 * streaming flag is off (which it is, since these events are emitted rather
 * than coming from a real recording), and Tauri freezes
 * `__TAURI_INTERNALS__.invoke`, so there is no seam to watch it through. The
 * effect that reports is the same shape as the elapsed timer asserted below.
 * Run it, then dictate once.
 *
 * No dependencies: Bun has a global `WebSocket`. Zero-arg, exits 1 with the
 * reason on any mismatch.
 */

import { appEnvVar } from "../scripts/lib/env-flag";

const PORT = Number(appEnvVar("CDP_PORT") ?? 9222);

type Target = {
  type: string;
  url: string;
  title: string;
  webSocketDebuggerUrl?: string;
};

function fail(msg: string): never {
  console.error(`FAIL  ${msg}`);
  process.exit(1);
}

function assert(ok: boolean, label: string, detail = "") {
  if (!ok) fail(`${label}${detail ? ` — ${detail}` : ""}`);
  console.log(`ok    ${label}`);
}

const targets = (await (
  await fetch(`http://127.0.0.1:${PORT}/json`)
).json()) as Target[];

// The overlay is its own webview with its own entry, and it is listed first
// because it boots first — `tauri-window.ts` excludes this URL for the same
// reason this file requires it.
const page = targets.find(
  (t) => t.type === "page" && t.url.includes("/src/overlay/"),
);
if (!page?.webSocketDebuggerUrl) {
  console.error("no overlay target. Targets seen:");
  targets.forEach((t) => console.error(`  ${t.type}  ${t.url}`));
  fail(
    "is the dev build running, and was it started with --remote-debugging-port?",
  );
}

const ws = new WebSocket(page.webSocketDebuggerUrl);
await new Promise((res, rej) => {
  ws.onopen = res;
  ws.onerror = rej;
});

// Errors are the point of the reload below: this is the only place that can
// catch a boot-time crash in the Solid entry (a bad import, a throw in a
// component body) in the real webview.
const consoleErrors: string[] = [];
let id = 0;
const pending = new Map<number, (v: any) => void>();
ws.onmessage = (e) => {
  const msg = JSON.parse(String(e.data));
  if (msg.id && pending.has(msg.id)) {
    pending.get(msg.id)!(msg);
    pending.delete(msg.id);
    return;
  }
  if (msg.method === "Runtime.exceptionThrown") {
    const d = msg.params?.exceptionDetails;
    consoleErrors.push(d?.exception?.description ?? d?.text ?? "exception");
  } else if (
    msg.method === "Runtime.consoleAPICalled" &&
    (msg.params?.type === "error" || msg.params?.type === "assert")
  ) {
    consoleErrors.push(
      (msg.params.args ?? [])
        .map((a: any) => a.value ?? a.description ?? a.type)
        .join(" "),
    );
  }
};

function send(method: string, params: unknown = {}) {
  const n = ++id;
  return new Promise<any>((res) => {
    pending.set(n, res);
    ws.send(JSON.stringify({ id: n, method, params }));
  });
}

/** Throws rather than exiting, so `until` can retry across a navigation. */
async function evaluate<T>(expression: string): Promise<T> {
  const r = await send("Runtime.evaluate", {
    expression,
    returnByValue: true,
    awaitPromise: true,
  });
  const ex = r.result?.exceptionDetails;
  if (ex) throw new Error(ex.exception?.description ?? ex.text);
  return r.result?.result?.value as T;
}

async function until<T>(
  label: string,
  expression: string,
  ok: (v: T) => boolean,
  timeoutMs = 20_000,
): Promise<T> {
  const deadline = Date.now() + timeoutMs;
  let last: unknown = "<never evaluated>";
  do {
    try {
      const v = await evaluate<T>(expression);
      last = v;
      if (ok(v)) return v;
    } catch (e) {
      last = `threw: ${(e as Error).message}`;
    }
    await new Promise((r) => setTimeout(r, 200));
  } while (Date.now() < deadline);
  fail(`${label} — last value was ${JSON.stringify(last)}`);
}

/** Emit a backend event; Tauri broadcasts it back to this window's listeners. */
async function emit(event: string, payload: unknown) {
  await evaluate(
    `window.__TAURI_INTERNALS__.invoke("plugin:event|emit", ${JSON.stringify({
      event,
      payload,
    })})`,
  );
}

const classOf = (sel: string) =>
  `(document.querySelector(${JSON.stringify(sel)})?.getAttribute("class") ?? "<absent>")`;
const countOf = (sel: string) =>
  `document.querySelectorAll(${JSON.stringify(sel)}).length`;
const textOf = (sel: string) =>
  `(document.querySelector(${JSON.stringify(sel)})?.textContent ?? "<absent>")`;

/**
 * The work label, asserted without hardcoding copy: the app's language is a
 * setting, so what matters is that a translated string is there and that a raw
 * key is not (which is what a broken i18n wiring looks like), not which words
 * this locale chose. The value is printed for the eye.
 */
async function assertWorkLabel(where: string) {
  const label = await evaluate<string>(textOf(".swork-label"));
  assert(
    label.length > 0 && !label.includes("overlay."),
    `${where} shows a translated work label`,
    JSON.stringify(label),
  );
}

// ---- 1. a real Tauri webview, serving the overlay's own entry -------------

await send("Runtime.enable");
await send("Page.enable");

const platform = await evaluate<string>(
  `window.__TAURI_INTERNALS__ ? "present" : "absent"`,
);
if (platform !== "present")
  fail("__TAURI_INTERNALS__ is absent — this is not a Tauri webview");
assert(page.url.includes("/src/overlay/"), `overlay target: ${page.url}`);

// Reload so the boot below is this session's, with its errors captured.
await send("Page.reload", { ignoreCache: false });
await until<string>(
  "the page finished loading",
  `document.readyState`,
  (s) => s === "complete",
);

// `#root` is empty by design at rest — the entry renders
// `<Show when={isVisible()}>` and nothing is visible yet — so the boot signal
// is the entry module having evaluated far enough to inject its stylesheet.
// (The behavioural proof is check 2: if the app had not mounted, no emitted
// event could produce a card, and it would never appear.)
await until<number>(
  "the Solid entry evaluated (its CSS is in the document)",
  `document.querySelectorAll("style").length`,
  (n) => n > 0,
);

// Show the real window, so layout is real: the height the overlay would report
// is measured from a laid-out DOM, which a hidden WebView2 window may not have.
await evaluate(
  `window.__TAURI_INTERNALS__.invoke("plugin:window|show", { label: "recording_overlay" })`,
).catch(() => {
  // Not fatal: the DOM checks below do not need a painted window.
});

// ---- 2. the streaming (Live) overlay --------------------------------------

await emit("show-overlay", "streaming");

const stage = await until<string>(
  "the Live card rendered for `streaming`",
  classOf(".ov-stage"),
  (c) => c !== "<absent>",
);
assert(
  /^ov-stage (top|bottom)$/.test(stage),
  `placement class came from settings: ${stage}`,
);

const scardResting = await until<string>(
  "the card rendered",
  classOf(".ov-stage .scard"),
  (c) => c !== "<absent>",
);
assert(
  !scardResting.includes("open"),
  "no text yet, so the card is not open",
  scardResting,
);

const dotArming = classOf(".sdot");
assert(
  (await evaluate<string>(dotArming)) === "sdot arming",
  "the dot is arming before microphone samples flow",
);

await emit("recording-ready", null);
const dotReady = await until<string>(
  "the dot left the arming state",
  classOf(".sdot"),
  (c) => c !== "sdot arming",
);
console.log(`ok    recording-ready cleared arming: ${dotReady}`);

// Text arrives -> the card opens and the text lands in the right spans.
await emit("stream-text-event", {
  committed: "hello world",
  tentative: "and more",
});
const opened = await until<string>(
  "the card opened on the first text",
  classOf(".ov-stage .scard"),
  (c) => c.includes("open"),
);
console.log(`ok    card opens on text: ${opened}`);
// `overlay_direct_mode` reveals text one to three characters per tick, so the
// first frame is a prefix of what was emitted. Waiting for the full string
// therefore covers the typewriter as well as the binding: the text has to
// *arrive*, not merely be assigned. With direct mode off the same wait is a
// no-op and the block below says which path it took.
const firstFrame = await evaluate<string>(textOf(".stext-cap .committed"));
const committed = await until<string>(
  "the committed text finished arriving",
  textOf(".stext-cap .committed"),
  (t) => t === "hello world ",
);
console.log(
  `ok    committed text: ${JSON.stringify(firstFrame)} -> ${JSON.stringify(committed)} ` +
    (firstFrame !== committed
      ? "(revealed progressively by the typewriter)"
      : "(applied as one block)"),
);
const tentative = await until<string>(
  "the tentative text finished arriving",
  textOf(".stext-cap .tentative"),
  (t) => t === "and more",
);
assert(
  tentative === "and more",
  "tentative text rendered",
  JSON.stringify(tentative),
);
assert(
  (await evaluate<number>(countOf(".scaret"))) === 1,
  "the caret is blinking while listening",
);

// Speech stats: the clock is speech-only, and the rate needs enough words.
await emit("speech-activity-event", { speaking: false, speech_ms: 6000 });
const stats = await until<string>(
  "the statistics cluster rendered",
  classOf(".sstats"),
  (c) => c !== "<absent>",
);
assert(stats.includes("quiet"), "silence mutes the cluster", stats);
assert(
  (await evaluate<string>(textOf(".sstat-speech"))) === "0:06",
  "the speech clock counts speech, not recording time",
);
assert(
  (await evaluate<string>(textOf(".sstat-wpm"))).startsWith("40"),
  "4 words over 6s of speech reads 40 wpm",
  await evaluate<string>(textOf(".sstat-wpm")),
);

// The elapsed timer: an effect whose compute reads a dependency list and whose
// apply owns an interval that writes a signal. It is asserted here, while the
// listening row is on screen — the working row below has no timer (the React
// original did not render one either). It is asserted because it is the same
// shape as the height report (both are `createEffect(compute, apply)` with a
// teardown returned), and unlike the height report it is visible.
await until<string>(
  "the elapsed timer rendered once the card opened",
  textOf(".stimer"),
  (t) => t !== "<absent>",
);
const tick0 = await evaluate<string>(textOf(".stimer"));
await new Promise((r) => setTimeout(r, 1600));
const tick1 = await evaluate<string>(textOf(".stimer"));
assert(tick1 !== tick0, `the elapsed timer ticks: ${tick0} -> ${tick1}`);

// Finalizing: the listening row becomes a spinner row, caret dropped.
await emit("stream-phase-event", { phase: "working", kind: "transcribing" });
await until<number>(
  "the working row replaced the listening row",
  countOf(".sspinner"),
  (n) => n === 1,
);
assert(
  (await evaluate<number>(countOf(".scaret"))) === 0,
  "the caret is dropped once finalizing",
);
await assertWorkLabel("the working row");
const scardWorking = await evaluate<string>(classOf(".ov-stage .scard"));
assert(
  !scardWorking.includes("working"),
  "text is on screen, so the card stays open while working",
  scardWorking,
);

// The whole-session mode marks failed merges in the badge rather than in the
// text, because with DirectStreaming that text is typed into the user's
// document. (What the same event also does — report a height to the backend —
// is not observable here; see the header.) The badge lives in the listening
// row, so the stream first returns to listening after finalizing, as it does
// between chunks in a real Multi-STT streaming session.
await emit("stream-phase-event", { phase: "listening", kind: "transcribing" });
await until<number>(
  "the listening row returned after finalizing",
  countOf(".scaret"),
  (n) => n === 1,
);
await emit("stream-text-event", {
  committed: "a longer session text that should still report its height",
  tentative: "",
  whole_session: true,
  failed_chunks: 2,
});
const badge = await until<string>(
  "the failed-chunk badge rendered",
  textOf(".sfail"),
  (t) => t !== "<absent>",
);
assert(badge.includes("2"), "the merge-failure badge carries the count", badge);

// ---- 3. the keyed remount -------------------------------------------------
// A new streaming session must rebuild the card (it replays the pop-in and
// never animates out of the previous panel's size). Brand the current node and
// prove the next session does not reuse it.
await evaluate(
  `(document.querySelector(".ov-stage .scard").__probe = "old") || true`,
);
await emit("hide-overlay", null);
await until<number>("the overlay hid", countOf(".ov-stage"), (n) => n === 0);
await emit("show-overlay", "streaming");
await until<boolean>(
  "a fresh card for the new session",
  `document.querySelector(".ov-stage .scard").__probe === "old"`,
  (v) => v === false,
);
console.log("ok    a new session remounts the card");

// ---- 4. the minimal overlay ----------------------------------------------

await emit("hide-overlay", null);
await until<number>(
  "the overlay hid again",
  countOf(".ov-stage"),
  (n) => n === 0,
);
await emit("show-overlay", "transcribing");
const compact = await until<string>(
  "the minimal card rendered for a non-streaming state",
  classOf(".ov-stage .scard"),
  (c) => c !== "<absent>",
);
assert(
  compact.includes("compact"),
  "the minimal card is the compact one",
  compact,
);
assert(compact.includes("cworking"), "it shows the working state", compact);
await assertWorkLabel("the minimal card");

await emit("hide-overlay", null);
await until<number>(
  "the overlay hid at the end",
  countOf(".ov-stage"),
  (n) => n === 0,
);

// ---- 5. it booted and ran clean ------------------------------------------

if (consoleErrors.length) {
  console.error("console errors seen:");
  consoleErrors.forEach((e) => console.error(`  ${e}`));
  fail(`${consoleErrors.length} console error(s) in the overlay webview`);
}
assert(true, "no console errors or uncaught exceptions");

await evaluate(
  `window.__TAURI_INTERNALS__.invoke("plugin:window|hide", { label: "recording_overlay" }).catch(() => {})`,
).catch(() => {});

console.log("\nok    the overlay is Solid and behaves");
ws.close();
process.exit(0);
