/**
 * Assert against the REAL Tauri window, over CDP.
 *
 *     WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222 bun run dev:fast
 *     bun tests/tauri-window.ts
 *
 * Why this exists next to the Playwright suite. `settings-window.spec.ts` runs
 * the app in a plain browser against a mocked IPC layer, which is what makes it
 * fast and hermetic — and also what it cannot see. This checks the same anchors
 * in the actual WebView2 window, where the IPC is the real Rust backend. The two
 * failure classes only this one catches:
 *
 *   - the mock answers a command the real backend answers differently (a
 *     plausible-looking fixture shape that is wrong in production), and
 *   - anything the native window owns: real window creation, the real webview
 *     booting the entry, `platform()` reading its injected global.
 *
 * It is deliberately NOT part of `bun run test:playwright`: it needs a running
 * dev build and a live backend, so it is a manual check for Phase 2 and Phase 3
 * (docs/PLAN_SOLIDJS_2.md §11.3 — "a browser-only check is not sufficient").
 *
 * No dependencies: Bun has a global `WebSocket`, so it speaks CDP directly.
 * Zero-arg, exits 1 with the reason on any mismatch.
 */

const PORT = Number(process.env.ZER0_CDP_PORT ?? 9222);

// Same anchors the Playwright net uses — roles and headings, never framework
// internals — so this survives the React → Solid swap unchanged.
const SECTIONS = [
  "General",
  "History",
  "Statistics",
  "Models",
  "Multi STT",
  "Transcribe Files",
  "Live Mode",
  "Live FFT",
  "Overlay",
  "Local LLM",
  "Advanced",
  "Debug",
  "Help",
  "About",
];

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

const targets = (await (
  await fetch(`http://127.0.0.1:${PORT}/json`)
).json()) as Target[];

// Both windows are served by the same dev server, so "contains :1420" matches
// the overlay too, and it is listed first. The main window is the one that is
// not the overlay's own entry.
const page = targets.find(
  (t) =>
    t.type === "page" &&
    t.url.includes("localhost:1420") &&
    !t.url.includes("/src/overlay/"),
);
if (!page?.webSocketDebuggerUrl) {
  console.error("no main-window target on :1420. Targets seen:");
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

let id = 0;
const pending = new Map<number, (v: any) => void>();
ws.onmessage = (e) => {
  const msg = JSON.parse(String(e.data));
  if (msg.id && pending.has(msg.id)) {
    pending.get(msg.id)!(msg);
    pending.delete(msg.id);
  }
};

async function evaluate<T>(expression: string): Promise<T> {
  const n = ++id;
  const r = await new Promise<any>((res) => {
    pending.set(n, res);
    ws.send(
      JSON.stringify({
        id: n,
        method: "Runtime.evaluate",
        params: { expression, returnByValue: true, awaitPromise: true },
      }),
    );
  });
  const ex = r.result?.exceptionDetails;
  if (ex)
    fail(
      `evaluating in the window threw: ${ex.exception?.description ?? ex.text}`,
    );
  return r.result?.result?.value as T;
}

/** Poll an expression until it satisfies `ok`, or give up. */
async function until<T>(
  label: string,
  expression: string,
  ok: (v: T) => boolean,
) {
  const deadline = Date.now() + 20_000;
  let last: T;
  do {
    last = await evaluate<T>(expression);
    if (ok(last)) return last;
    await new Promise((r) => setTimeout(r, 250));
  } while (Date.now() < deadline);
  fail(`${label} — last value was ${JSON.stringify(last)}`);
}

// ---- the checks ----------------------------------------------------------

// 1. A real Tauri webview, not a browser: this global is injected by Tauri's
//    build step and is what `@tauri-apps/plugin-os`'s `platform()` reads.
const platform = await evaluate<string>(
  `window.__TAURI_INTERNALS__ ? "present" : "absent"`,
);
if (platform !== "present")
  fail("__TAURI_INTERNALS__ is absent — this is not a Tauri webview");

// 2. The shell painted, and it painted from a REAL backend rather than mocks.
const nav = await until<number>(
  "the sidebar rendered its sections",
  `document.querySelectorAll('nav button').length`,
  (n) => n >= SECTIONS.length,
);

const missing = await evaluate<string[]>(
  `JSON.stringify(${JSON.stringify(SECTIONS)}.filter((label) =>
     ![...document.querySelectorAll("nav button")].some((b) => b.textContent.trim() === label)))`,
).then((v) => (typeof v === "string" ? JSON.parse(v) : v));
if (missing.length) fail(`sidebar is missing: ${missing.join(", ")}`);

// 3. The default page rendered its content, not just a frame around a crash.
const heading = await until<string>(
  "the General page rendered a heading",
  `(() => { const h = document.querySelector("h1, h2, h3");
     return h ? h.textContent.trim() : ""; })()`,
  (t) => t.length > 0,
);

const errors = await evaluate<string>(
  `document.querySelector("#root")?.textContent?.trim().length ? "ok" : "empty"`,
);
if (errors !== "ok") fail("#root is empty — the shell rendered nothing");

console.log(`ok    real window: ${page.url} (${page.title})`);
console.log(`ok    nav entries: ${nav}, first heading: ${heading}`);
ws.close();
