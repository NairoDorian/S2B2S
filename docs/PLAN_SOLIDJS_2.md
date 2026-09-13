# HANDOVER: Migrate ZER0's frontend from React 19 to Solid 2.0.0-rc.8

**Audience:** the AI agent who picks this up. Read this file top to bottom before
touching anything; it is written to be self-contained — you should not need to
re-research Solid 2, the npm ecosystem, or the shape of this codebase.

**Status:** Phase 0 **complete** (toolchain spike + safety net, 2026-09-12).
Phase 1 **complete** (all six dependency removals, 2026-09-13) — `package.json`
lists no React-only library, the gate is green and behaviour is unchanged. Phase
2 (the overlay window) is next. Work sits uncommitted on `Handy_Multi_STT`;
nothing has been pushed. See the §6 verdicts for what was proven and what was
deliberately left open, and the Phase 1 progress table for what each completed
step measured.

**Author's note:** every version number, file path, count and import below was
read from the npm registry or the working tree on **2026-09-12**. Nothing is from
memory. Counts are exact and were produced by the commands quoted in §3.3, so you
can re-run them and compare.

**Documentation source: https://v2.solidjs.com/** — the v2 wiki, and the only
documentation that describes these APIs (the 1.x docs describe `solid-js/web`,
`solid-js/store`, `produce` and `onMount`, none of which exist in 2). The pages
this migration reads, so a later agent does not have to rediscover which one
answers which question:

| Page                                                                            | What it settles                                                                                                 |
| ------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------- |
| `/migration/from-react`                                                         | the React → Solid 2 mapping; the effect split, and what replaces `useMemo` / `useRef` / `useContext`            |
| `/concepts/reactivity`                                                          | signals, memos, ownership, where tracking actually happens                                                      |
| `/concepts/components-and-jsx`                                                  | props and destructuring, `class`, `For` / `Show`, callback refs, `Portal`                                       |
| `/concepts/stores`                                                              | `createStore` + **mutable draft setters** — the shape D5 converts the nine zustand stores to                    |
| `/concepts/mutations`                                                           | `createProjection` / `createMutation`, the store-write model                                                    |
| `/concepts/boundaries`                                                          | `Loading`, `Errored`, `Reveal` (what `Suspense` / `ErrorBoundary` / `SuspenseList` became)                      |
| `/guides/thinking-in-solid`                                                     | R4 in the framework's own words — the reasons a mechanical `useEffect` translation goes wrong                   |
| `/guides/avoid-unnecessary-effects`                                             | same, as a checklist; the 156 `useEffect` sites are the largest correctness risk in Phase 3                     |
| `/guides/lists`, `/guides/state-management`, `/guides/integrate-non-solid-code` | the Phase 3 codemods, the stores, and the two windows' non-React neighbours                                     |
| `/reference`                                                                    | the per-export API reference, grouped by import specifier (`solid-js`, `@solidjs/web`)                          |
| `/migration/from-solid-1`                                                       | the renames: `onMount`→`onSettled`, `ErrorBoundary`→`Errored`, `Index`→`For keyed={false}`, `unwrap`→`snapshot` |

---

## 0. Repo conventions you must follow (non-negotiable)

These come from `AGENTS.md` / `CLAUDE.md` and override your defaults.

- **Run `bun run precommit` before every commit.** `bun run precommit:full`
  before a PR or release. It runs, in order: identity mirrors → `meta:check` →
  `check:identity` → `check:translations` → lint → typecheck → unit checks →
  `format:check` → repomix; `:full` adds clippy and the Rust suite. The hook lives
  at `.githooks/pre-commit` (`bun run hooks:install` once per clone). The
  `unit checks` step is new as of Phase 1 step 6 — before it, the `*.test.ts`
  assert scripts under `src/` were run by nobody (see the step-6 block).
- **Commit prefixes:** `feat:`, `fix:`, `docs:`, `refactor:`, `chore:`. Focus the
  message on **why**. State the cost of any new thread, poll or dependency
  (`docs/PERFORMANCE.md` rule 10). End every message with
  `Co-Authored-By: Claude Code <noreply@anthropic.com>`.
- **`rtk` proxies every shell command** — `rtk git …`, `rtk cargo …` — **except
  `bun` commands, which are never proxied.** Write `bun run …`, never `rtk bun …`.
- **Never spell the product name by hand.** It comes from `scripts/app-meta.ts`
  and its generated mirrors (`src/lib/appIdentity.ts`,
  `src-tauri/src/app_identity.rs`). `check:identity` fails the build on a stale
  name. This migration should not touch any of that machinery.
- **`update-deps` always takes `--prerelease`** in this repo — it deliberately
  tracks the newest published version of everything. See §4 D1 for why that
  matters here.
- **Do not push.** Branch and commit only.
- `src/` carries **zero lint suppressions** today and should stay that way.

### Machine-specific traps that will waste your time

- **A running `handy.exe` blocks cargo.** It maps the staged transcribe DLLs, so
  every cargo command dies with `os error 32` until it is closed. Close it before
  `precommit:full`.
- **CRLF breaks the prettier gate invisibly.** This repo has `core.autocrlf = true`
  and **no `.gitattributes`**, while `.prettierrc` sets `endOfLine: "lf"`. Any
  `git stash pop` / `checkout` / `reset --hard` re-materializes files as CRLF and
  the next `precommit` dies in the `format` step — _after_ seven other steps
  passed, so it looks like a real formatting regression. Worse, CRLF↔LF hashes to
  the same blob under autocrlf, so `git diff --numstat` is **empty**. After any
  such operation, normalize the worktree to LF with a bun script
  (`text.replace(/\r\n/g, "\n")`), then confirm with `bun run format:check`.
- **Bash heredocs on this machine lose double backslashes** and choke on
  apostrophes. Write a temp `.ts` file with the Write tool and run it with `bun`
  instead of using a heredoc.
- **`playwright install` hangs on this machine, and it is an IPv6 problem, not a
  network one.** `cdn.playwright.dev` publishes an AAAA record
  (`2603:1061:14:174::1`) and this machine's IPv6 path is black-holed — SYNs get
  no answer and no RST. Playwright's downloader is uniquely exposed to it: it
  resolves via its own `dualStackLookup`, which queries `family: 6` **first** and
  returns that address, then sets `autoSelectFamily: true` with
  `autoSelectFamilyAttemptTimeout: 5000`. So every download dies ~5 s in with
  `socket hang up`, or at `socketTimeout` with
  `Request to … timed out after Nms`. A plain `https.request` to the same URL
  succeeds in ~110 ms, because Node's default resolver returns the A record.
  **`PLAYWRIGHT_DOWNLOAD_CONNECTION_TIMEOUT`, `--dns-result-order=ipv4first` and
  running the CLI under `node` instead of `bun` all fail to help** — the last one
  because the defect is in Playwright's own lookup, not the runtime. Force IPv4
  on the transfer instead: `curl -4 -L` the two zips from
  `https://cdn.playwright.dev/builds/cft/<browserVersion>/win64/` (read
  `browserVersion` and `revision` from `playwright-core/browsers.json`), extract
  them into `%LOCALAPPDATA%\ms-playwright\chromium-<rev>\` and
  `chromium_headless_shell-<rev>\` so each holds its own top-level folder
  (`chrome-win64\chrome.exe`), and write an empty `INSTALLATION_COMPLETE` beside
  it. At ~25 MB/s the whole repair is ~15 s. The same trap will hit any future
  tool that enables Happy Eyeballs on a host with both A and AAAA records.

---

## 1. Goal and non-goals

**Goal:** replace React 19 with **Solid 2.0.0-rc.8** across both frontend windows
— the settings window (`index.html` → `src/main.tsx`) and the recording overlay
(`src/overlay/index.html` → `src/overlay/main.tsx`).

**Non-goals — do not touch:**

- The Rust backend (`src-tauri/`), except where a phase explicitly says so.
  **`git diff --stat -- src-tauri/` should be empty across the whole migration.**
- `src/bindings.ts` — tauri-specta's generated output (2,025 LOC). It is
  framework-agnostic and must keep working verbatim.
- Tailwind 4 and every `.css` file.
- i18n _content_: the 25 locale files, key sets, `check:translations`.
- The identity / rename machinery (`scripts/app-meta.ts`, `check-identity.ts`).
- Tauri IPC, events, CSP, window config.

### Why this is worth doing in this app specifically

ZER0 is a real-time tool with a written latency budget (`docs/PERFORMANCE.md`).
Its heaviest frontend surfaces are all high-frequency streams, and the current
code keeps every one of them _out_ of React's render path **by hand**:

| Surface                                   | Today's workaround for React's re-render model                                                                                                                                                         |
| ----------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Live FFT spectrum (up to 60 Hz)           | `stores/liveFftStore.ts` keeps frames in a **module-level holder** — its own comment says "so a 60 Hz stream never re-renders the page"; canvases read it from their own `requestAnimationFrame` loops |
| Recording overlay scope + streaming text  | `overlay/OverlayScope.tsx`, `overlay/RecordingOverlay.tsx` — rAF loops and imperative refs                                                                                                             |
| `SpeechActivityEvent` (~150 ms heartbeat) | arrives into a React tree that must not re-render                                                                                                                                                      |
| System meters (CPU/RAM/GPU/VRAM, 1 Hz)    | `components/footer/SystemMeters.tsx`                                                                                                                                                                   |

Solid's fine-grained reactivity removes the _reason_ for those workarounds: a
signal write updates the one text node that read it, so a 60 Hz stream is cheap
without a holder/rAF/ref contraption. The same applies to the re-render-management
apparatus the codebase carries today — `React.memo` (42), `useCallback` (62),
`useMemo` (79), and the 293 `.current` reads behind 107 `useRef` calls. In Solid a
component body runs **once**, so those have no counterpart to translate to. They
are _deleted_.

**The honest counter-argument, which you should keep in view:** this is a
~29,300 LOC, 199-file rewrite whose entire safety net today is **two Playwright
smoke tests** that assert only that the dev server returns 200 and that
`<html>`/`<body>` exist (`tests/app.spec.ts`). There is no component test suite,
no snapshots, no visual regression. §6 Phase 0 builds the net **before** Phase 3
leans on it. Do not skip that, and do not reorder the phases.

---

## 2. Verified state of Solid 2 (re-check before you start; RC APIs move)

### 2.1 Versions, from the npm registry on 2026-09-12

| Package                | `dist-tags`                                                     | Note                                                    |
| ---------------------- | --------------------------------------------------------------- | ------------------------------------------------------- |
| `solid-js`             | `latest: 1.9.15`, `beta: 1.10.0-beta.0`, **`next: 2.0.0-rc.8`** | 2.x is reachable **only** via `next`                    |
| `@solidjs/web`         | `latest: 2.0.0-rc.0`, **`next: 2.0.0-rc.8`**                    | new package — the DOM renderer, split out of `solid-js` |
| `@solidjs/vite-plugin` | **`latest: 3.0.0-next.43`**, `next: 3.0.0-next.35`              | replaces `vite-plugin-solid`                            |
| `@kobalte/core`        | `0.13.14`                                                       | `peerDependencies: {"solid-js": "^1.9.8"}`              |
| `solid-sonner`         | `0.3.2`                                                         | `peerDependencies: {"solid-js": "^1.6.0"}`              |

`@solidjs/vite-plugin@3.0.0-next.43` declares
`peerDependencies: { "vite": "^8.0.0 || ^9.0.0", "solid-js": "^2.0.0-rc.7", "@solidjs/web": "^2.0.0-rc.7" }`
— this repo's Vite 8.3.0 satisfies it exactly. Its `@solidjs/start-devtools` and
`@testing-library/jest-dom` peers are optional (`peerDependenciesMeta`).

### 2.2 The finding that shapes the whole plan

**Solid 2 removed the `solid-js/web` and `solid-js/store` subpath exports.** The
migration guide is explicit: Solid 2 "does not provide the old `solid-js/web` or
`solid-js/store` package exports". The DOM renderer moved to `@solidjs/web`;
store APIs moved _into_ `solid-js`.

Every Solid 1 component library imports from one or both. So Kobalte and
solid-sonner are not merely "unverified against 2.0" — **they cannot resolve.**
There is no off-the-shelf Solid 2 component library to adopt, and **this plan
adopts none.** (Which costs little here — see §3.1.)

The v2 docs also state plainly: "Solid 2.0 is in its release candidate (RC) phase…
APIs may change before the stable release." Pin exactly (§4 D1) and budget a
re-sync pass when 2.0 goes stable.

### 2.3 Other Solid 2 changes you will hit

- Library packages changed name: `vite-plugin-solid` → `@solidjs/vite-plugin`.
- `solid-js/store` → `solid-js`; `solid-js/web` → `@solidjs/web`.
- **`"jsxImportSource": "@solidjs/web"`** with **`"jsx": "preserve"`** (not
  `react-jsx`). DOM `JSX`/`ComponentProps` types come from `@solidjs/web`;
  renderer-neutral `Component`/`Element` stay in `solid-js`.
- `render` and `hydrate` now come from `@solidjs/web`.
- **The store setter takes a mutable draft:** `setState("a","b",v)` becomes
  `setState((draft) => { draft.a.b = v })`. `produce` is gone; `storePath()` is
  the migration helper for path-setter form.
- **`createEffect` splits tracking from the effect:**
  `createEffect(() => value(), (v) => { sideEffect(v); return cleanup })`. Cleanup
  is **returned**, not registered with `onCleanup` inside.
- Renames you will need: `onMount` → `onSettled`; `ErrorBoundary` → `Errored`
  (fallback reads `error()`); `Suspense` → `Loading`; `SuspenseList` → `Reveal`;
  `Index` → `For keyed={false}` (item becomes an accessor, index stays a stable
  number); `createResource` → async memo + boundaries; `unwrap(store)` →
  `snapshot(store)`; `mergeProps` → `merge`; `splitProps(p, ["a"])` →
  `omit(p, "a")`; `createMutable` → `createStore` + draft setters.
- `batch` is gone (writes batch automatically); `on` is gone (dependencies live
  in the effect's compute phase); `startTransition`/`useTransition` are gone;
  `createDeferred` is gone.
- JSX forms: `classList` → object/array `class`; `use:` → a `ref` callback or a
  directive factory; `on:` / `oncapture:` → camel-case event props; `attr:` /
  `bool:` → standard attributes.
- DOM props: **`class`, not `className`**; style objects use CSS property names
  (`"background-color"`) and numeric values get **no automatic units**.

---

## 3. Verified state of the codebase

### 3.1 The third-party surface is small — this is the plan's best news

| Package          | Call sites  | Where                                                                                                                                                                                                    |
| ---------------- | ----------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `react`          | 145 files   | every `.tsx` + both entries                                                                                                                                                                              |
| `react-i18next`  | 106 files   | `const { t } = useTranslation()` ×108; `const { t, i18n } = …` ×6; `<Trans>` ×3; `initReactI18next` ×1. Re-counted for step 5: **106 files, 116 calls** (`{ t }` ×110, `{ t, i18n }` ×6, no third shape) |
| `lucide-react`   | 27 files    | **61 imported names → 58 distinct glyphs** — vendorable (corrected; see Phase 1)                                                                                                                         |
| `zustand`        | 9 stores    | 32 selector-hook call sites in 20 files; 8 `.getState()`; **0 `.setState()`; 0 `.subscribe()`**                                                                                                          |
| `react-dom`      | 4 files     | `createRoot` ×2 (the two entries); `createPortal` ×2 (`ui/Dialog.tsx`, `ui/Tooltip.tsx`)                                                                                                                 |
| `react-select`   | **1 file**  | ~~`components/ui/Select.tsx`~~ — **gone** in Phase 1 step 4                                                                                                                                              |
| `sonner`         | **2 files** | ~~`App.tsx` (mounts `<Toaster>`); `lib/sessionToast.ts` (wraps `toast`)~~ — **gone** in Phase 1 step 3                                                                                                   |
| `react-markdown` | **1 file**  | ~~`components/whats-new/MarkdownContent.tsx`~~ — **gone** in Phase 1 step 2                                                                                                                              |
| `immer`          | **1 file**  | ~~`stores/modelStore.ts`, 12 `produce(` calls~~ — **gone** in Phase 1 step 6                                                                                                                             |

**There is no component library** — no Kobalte, no Radix, no Headless UI. The
design system in `components/ui/` (19 files, 1,851 LOC — one file and 144 LOC
larger than the Phase 0 count, both from `Toaster.tsx`) is hand-rolled and already
framework-shaped: `Dropdown`, `Dialog`, `Slider`, `ToggleSwitch`,
`SettingContainer`, `SettingsGroup`, `TextDisplay`, `Textarea`, `Tooltip`,
`Button`, `Input`, `Alert`, `Badge`, `Select`, `AudioPlayer`, `PathDisplay`,
`ResetButton`, `SettingsGroup`. `components/icons/` already holds hand-written SVG
icon components — that is where the vendored lucide icons go.

### 3.2 React API surface, measured

|                       | count          |                              | count                            |
| --------------------- | -------------- | ---------------------------- | -------------------------------- |
| `useState`            | 214 (53 files) | `React.FC`                   | 154 (**136 files**)              |
| `useEffect`           | 156 (54 files) | `React.memo`                 | 42                               |
| `useRef` / `.current` | 107 / **293**  | `className`                  | **1,172**                        |
| `useMemo`             | 79             | `.map(` in JSX               | 74                               |
| `useCallback`         | 62             | `key={}`                     | 42                               |
| `useLayoutEffect`     | 5              | `createContext`/`useContext` | 2 (1 file: `ui/AudioPlayer.tsx`) |
| `useId`               | 3 (1 file)     | `createPortal`               | 4 (2 files)                      |
| `forwardRef`          | **0**          | `useSyncExternalStore`       | **0**                            |
| `useReducer`          | **0**          | `Suspense`                   | **0**                            |

Zero `forwardRef`, zero `Suspense`, zero `useReducer`, zero
`useSyncExternalStore`, and context in exactly one file. **The hard parts are
therefore collected in four places, not spread across 199 files: effects, refs,
the stores, and lists.**

### 3.3 Shape and size

```
199 files (145 .tsx + 54 .ts), ~29,295 LOC excluding locales

src/components/settings          15,319 LOC   102 files   ← the bulk of Phase 3
src/components/ui                 1,697 LOC    18 files   ← the design system
src/components/model-selector     1,448 LOC    10 files
src/overlay                       1,063 LOC     3 files   ← Phase 2
src/components/onboarding         1,040 LOC     4 files
src/stores                        1,924 LOC     8 files
src/lib                           1,730 LOC    16 files
src/components/whats-new            359 LOC     5 files
src/components/hotkey-sidebar       224 LOC     3 files
src/components/footer               211 LOC     4 files
src/hooks                            96 LOC     2 files
```

Largest files, in order: `bindings.ts` 2,025 (untouched); `HistorySettings.tsx`
1,244; `LiveFftSettings.tsx` 1,151; `MultiSttSettings.tsx` 1,015;
`LlamaSettings.tsx` 834; **`stores/settingsStore.ts` 815**;
`FileTranscriptionSettings.tsx` 682; `overlay/RecordingOverlay.tsx` 638;
`LiveModeSettings.tsx` 619; `StatisticsSettings.tsx` 616.

Reproduce the inventory yourself with:

```bash
# React hook / API usage
grep -ro '\buseState\b' --include=*.ts --include=*.tsx src | wc -l     # 214
grep -ro 'className' --include=*.tsx --include=*.ts src | wc -l         # 1172
grep -ro 'React\.FC' --include=*.tsx src | wc -l                        # 154

# third-party imports
grep -rhoE 'from "[^".@][^"]*"' --include=*.ts --include=*.tsx src \
  | sed 's/from "//;s/"//' | sed -E 's#^((@[^/]+/[^/]+)|([^/]+)).*#\1#' \
  | sort | uniq -c | sort -rn

# size
find src -name '*.ts' -o -name '*.tsx' | grep -v locales | xargs wc -l | tail -1
```

### 3.4 Entry points and config (current state)

`src/main.tsx` is nearly framework-agnostic — it has **zero React components**,
just a bootstrap that calls zustand's imperative API outside React. Only
`createRoot` + `StrictMode` are React-specific:

```tsx
import React from "react";
import ReactDOM from "react-dom/client";
// …installCompatShims(), applyTheme(), applyAccentColor(), applyUiScale(),
// …then two dynamic imports:
import { useModelStore } from "./stores/modelStore";
useModelStore.getState().initialize();

i18nReady.then(() => {
  ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
    <React.StrictMode>
      <App />
    </React.StrictMode>,
  );
});
```

`src/overlay/main.tsx` has the same shape, minus the ui-scale calls, ending in
`ReactDOM.createRoot(...).render(<React.StrictMode><RecordingOverlay /></React.StrictMode>)`.

`vite.config.ts` — two entries, React plugin:

```ts
export default defineConfig(async () => ({
  plugins: [react(), tailwindcss()],
  resolve: {
    alias: {
      "@": resolve(import.meta.dirname, "./src"),
      "@/bindings": resolve(import.meta.dirname, "./src/bindings.ts"),
    },
  },
  build: {
    rollupOptions: {
      input: {
        main: resolve(import.meta.dirname, "index.html"),
        overlay: resolve(import.meta.dirname, "src/overlay/index.html"),
      },
    },
    chunkSizeWarningLimit: 1000,
  },
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: "ws", host, port: 1421 } : undefined,
    watch: { ignored: ["**/src-tauri/**"] },
  },
}));
```

`tsconfig.json` — `"jsx": "react-jsx"` must become `"preserve"` + a
`jsxImportSource`. Paths: `@/*` → `./src/*`, `@/bindings` → `./src/bindings.ts`.

### 3.5 Key call sites you will need

- **`src/App.tsx`** (427 LOC) — the shell. 8 `useEffect`s, `useState`,
  `useLayoutEffect`, `useRef`, `ReactNode`. Contains the single page seam:
  `renderSettingsContent()` at line 44 picks the page from
  `SECTIONS_CONFIG[section].component`. Renders `<Toaster />` in a stable wrapper
  so crossing onboarding ↔ main app never remounts it (dropping in-flight toasts).
- **`src/components/ErrorBoundary.tsx`** — a React **class component**
  (`Component`, `ErrorInfo`). Becomes an `Errored` boundary in Solid 2.
- **`src/stores/settingsStore.ts`** (815 LOC) — the backbone. zustand +
  `subscribeWithSelector`. Holds `settings: Settings | null` as one big object,
  plus actions. **Every settings key needs an entry in `settingUpdaters`** — a
  key without one logs `No handler for setting` and is never persisted. Preserve
  that rule verbatim.
- **`src/hooks/useSettings.ts`** (96 LOC) — a pass-through that returns ~25
  fields from `useSettingsStore()`. Consumed widely via `useSettings()`.
- **`src/i18n/index.ts`** — `i18n.use(lazyLocaleBackend).use(initReactI18next).init({…})`.
  Locale files load on demand through a non-eager `import.meta.glob`, so each
  `translation.json` is its own chunk. **Do not break that chunking** — eager
  bundling once put all 24 locales (2.1 MB) into the startup chunk of both
  windows. `i18nReady` resolves once the fallback bundle is loaded and the
  language is synced from settings.
- ~~**`src/lib/compat.ts`** — installs an `Object.hasOwn` shim. Its comment says
  it exists **for react-markdown**. Once react-markdown is gone (Phase 1), this
  shim and its call in `main.tsx` are dead — remove them.~~ **DELETED** in Phase
  1 step 2: the file, the import and the call in `main.tsx`. `Object.hasOwn`
  appeared nowhere else in `src/`, and every webview this ships to has it
  (Chromium 93+ / Safari 15.4+), so the shim was never doing anything except
  standing in for a dependency that is now gone. `AGENTS.md`'s file list loses
  its entry in the same commit.
- **`src/stores/liveFftStore.ts`** — holds spectrum frames in a module-level
  holder read by the canvases' rAF loops. Solid makes this unnecessary; deleting
  it is a _separate_ commit after the store migration, with the §8 measurement.
- **`tests/app.spec.ts`** — 2 smoke tests. `playwright.config.ts` runs
  `bunx vite dev` on port 1420.

---

## 4. Decisions already made — do not re-litigate

### D1 — Pin the toolchain exactly, and teach `update-deps.ts` about it

Pin `solid-js@2.0.0-rc.8`, `@solidjs/web@2.0.0-rc.8`,
`@solidjs/vite-plugin@3.0.0-next.43` — **exact, not `^`**.

**The trap:** this repo's policy is `bun run update-deps --prerelease`. For
`@solidjs/web` the `latest` tag is `2.0.0-rc.0` while `next` is `2.0.0-rc.8`, and
for `solid-js` itself `latest` is `1.9.15`. A naive "newest published" resolution
can therefore move the renderer **backwards** (rc.8 → rc.0) or **down a major**
(2.x → 1.9.15). Add all three packages to `update-deps.ts`'s hold-back list — the
mechanism it already uses for `libc` — with the reason printed, until 2.0 goes
stable. Without this, the first `bun run update` after the migration silently
reverts it.

### D2 — Adopt no Solid 1 libraries; vendor or hand-roll

Forced by §2.2, and cheap because the app already owns its UI:

| React dep                  | Replacement                                                                                                          | Cost                                    |
| -------------------------- | -------------------------------------------------------------------------------------------------------------------- | --------------------------------------- |
| `lucide-react` (58 glyphs) | ~~vendor as SVG components into `components/icons/`~~ — **done**, Phase 1 step 1                                     | mechanical; write a one-shot script     |
| `react-select`             | ~~hand-rolled on the existing `ui/Dropdown.tsx` pattern~~ — **done**, Phase 1 step 4 (it is a combobox: see Phase 1) | 1 file, ~150 LOC                        |
| `react-markdown`           | ~~a framework-neutral parser (`marked`/`markdown-it`) + Solid JSX, **or** hand-roll~~ — **done**, Phase 1 step 2     | 1 file — look at the notes first        |
| `sonner`                   | ~~in-house `components/ui/Toaster.tsx` behind the existing `sessionToast` wrapper~~ — **done**, Phase 1 step 3       | 2 files; the abstraction already exists |
| `react-i18next`            | `i18next` core (already a direct dep) + a ~20-line `useTranslation` primitive                                        | 108 identical call sites                |
| `zustand` + `immer`        | `createStore` / `createSignal` from `solid-js`                                                                       | see D5                                  |

**Rejected:** adopting a Solid 1 library and pinning `solid-js@1.x` to satisfy its
peer range. That is a second, larger migration (Solid 1 → 2) with none of this
plan's benefit.

### D3 — Phase 3 is a big-bang cutover on a branch, not a hybrid tree

React and Solid _can_ coexist in one Vite build (two plugins with disjoint
`include`/`exclude`; TypeScript resolves JSX per file via a
`/** @jsxImportSource */` pragma), and the settings window has a genuine single
seam — `renderSettingsContent()` in `App.tsx:44`, driven by
`SECTIONS_CONFIG[section].component` — so one adapter could mount a Solid page
into a React shell. **The cost is that state does not cross that seam reactively**:
the stores would have to be readable from both runtimes, meaning duplicate
subscriptions or a bridge, for dozens of pages, for the whole migration.

**Do not build that bridge.** Phase 3 lands as an ordered commit series on a
branch and merges as one release; the trunk stays releasable until it does.
Phases 0–2 exist to shrink Phase 3 as far as it will go while shipping
continuously. Revisit islands only if Phase 3 outgrows a single release window.

### D4 — The overlay window goes Solid first

`src/overlay/` is 3 files / 1,063 LOC with its **own** `index.html`, entry point
and webview — a real production Solid 2 workload (Tauri events, IPC, raw-byte
`overlay_scope_frame`, `localStorage` theme sync, Tailwind, lazily loaded i18n,
canvas rAF loops) that does not touch the main window at all. It is a better
Phase 2 than a throwaway spike, and it de-risks every integration the main window
will need.

### D5 — Stores become `createStore`, one file at a time

`zustand` has only **32 selector-hook call sites in 20 files**, and **no
`.setState()` and no `.subscribe()` at all**; the imperative surface is 8
`.getState()` calls (5 inside `settingsStore.ts` itself). So this is small and
local. `settingsStore.ts` becomes a module-level `createStore` + exported
functions; `useSettings()` becomes a plain function returning accessors.
Two notes: `subscribeWithSelector` is imported by `settingsStore.ts` and
`modelStore.ts` but **never used** — drop it. And `immer`'s `produce` is
**removed in Solid 2** anyway; the store setter takes a mutable draft directly,
which is exactly what `produce` was emulating.

### D6 — `className` → `class` by codemod

1,172 occurrences. Mechanical, scriptable, and it must be **one commit** so it can
be reverted independently. Same pass: `React.FC<T>` → `(props: T)` across 136
files, delete `React.memo(...)` (42). `htmlFor` has 0 uses here, so nothing to do.

---

## 5. The React → Solid 2 translation table

Keep this open while you work. **The bold rows are where agents get it wrong.**

| React                          | Solid 2                                                                                                        |
| ------------------------------ | -------------------------------------------------------------------------------------------------------------- |
| `useState`                     | `createSignal` — read is **`v()`**, not `v`                                                                    |
| `useEffect`                    | `createEffect(compute, effect)` — two functions; compute tracks, effect runs untracked and **returns** cleanup |
| `useLayoutEffect`              | `createRenderEffect`                                                                                           |
| `useMemo`                      | plain function, or `createMemo` when it is expensive / multi-consumer / needs equality bounding                |
| `useCallback`                  | plain function — identity stability does not exist as a concept                                                |
| `useRef` (DOM handle)          | callback ref assigning to a local variable                                                                     |
| `useRef` (mutable box)         | a plain `let` variable                                                                                         |
| `useContext`                   | `createContext` + `useContext` from `solid-js`                                                                 |
| `React.FC<T>`                  | `(props: T)`                                                                                                   |
| `React.memo`                   | delete                                                                                                         |
| `createPortal`                 | `Portal` from `@solidjs/web`                                                                                   |
| `useId`                        | `createUniqueId`                                                                                               |
| `StrictMode`                   | **no counterpart** — delete it                                                                                 |
| `onMount` (Solid 1)            | `onSettled`                                                                                                    |
| `ErrorBoundary` (Solid 1)      | `Errored` — fallback reads `error()`                                                                           |
| `Suspense` (Solid 1)           | `Loading`                                                                                                      |
| `Index` (Solid 1)              | `For keyed={false}` — item becomes an accessor, index a stable number                                          |
| `createResource`               | async memo + boundaries                                                                                        |
| `produce` (immer)              | pass the draft callback straight to the store setter                                                           |
| `unwrap(store)`                | `snapshot(store)`                                                                                              |
| `mergeProps`                   | `merge`                                                                                                        |
| `splitProps(p, ["a"])`         | `omit(p, "a")`                                                                                                 |
| `className`                    | **`class`**                                                                                                    |
| `style={{ backgroundColor }}`  | `style={{ "background-color": … }}` — CSS names, **no automatic units**                                        |
| `.map()` with `key`            | `<For each={…}>{(item) => …}</For>` — item is a value, `index` a number                                        |
| positional lists               | `<For keyed={false}>` (item accessor, stable index) or `<Repeat>`                                              |
| `ref={el => …}`                | callback ref — called with the element; **ref arrays flatten in order**                                        |
| `onClick` / `onInput`          | same names, camelCase                                                                                          |
| `classList` (Solid 1)          | object/array `class`                                                                                           |
| `on:` / `oncapture:` (Solid 1) | camel-case event props                                                                                         |

### The four mistakes you will make if you are not deliberate

1. **Destructuring props.** `function Foo({ name }: { name: string })` reads
   `name` **eagerly at setup** and kills reactivity forever. Keep the props object
   intact: `function Foo(props) { … props.name … }`, or `() => props.name` /
   `createMemo` for a local reactive name. This is the single most common Solid
   error and it fails _silently_.
2. **Writing a dependency array.** Solid discovers dependencies from tracked
   reads. There is no array to write, and translating one mechanically produces
   either over-tracking or untracked reads — both silent.
3. **`onCleanup()` called inside `createEffect`.** In Solid 2 the cleanup is
   **returned from** the effect function.
4. **A component body is not a render function.** It runs **once**. `const x =
props.foo` at the top level is a one-time snapshot; `const y = someSignal()`
   is a one-time read. Anything that must update has to be a function or a memo.

### Context, specifically

The context object **is** the provider component in Solid 2 — there is no
`.Provider`: `<Theme value={value}>{children}</Theme>`. A provider-less read
throws. Module-level signals/stores are discouraged for shared state ("no owner").

---

## 6. Phases

Every phase: its own branch/PR with `bun run precommit:full` green, conventional
commits, the `docs/PERFORMANCE.md` rule-10 cost line where a phase adds or
removes cost, and the attribution footer.

### Phase 0 — Verdict spike + build the safety net

**(a) Toolchain proof.** Add a third Vite entry (`solid-spike.html`) rendering a
Solid 2 component **alongside the two existing React entries, in one build**. It
must demonstrate, or the plan changes:

1. `solid-js@2.0.0-rc.8` + `@solidjs/web@2.0.0-rc.8` install cleanly and
   `render(() => <App/>, el)` imports from `@solidjs/web`.
2. `@solidjs/vite-plugin` and `@vitejs/plugin-react` coexist with disjoint
   `include`/`exclude` sets, each transforming only its own files.
3. `tsconfig` accepts `"jsx": "preserve"` + `"jsxImportSource": "@solidjs/web"`
   with a per-file `/** @jsxImportSource react */` pragma for the React tree —
   **and `bun run typecheck` still passes on the existing 199 files unchanged.**
4. Tailwind 4 (`@tailwindcss/vite`, `App.css` with `source(".")`) styles a Solid
   component identically.
5. **`bun run lint` still runs and still enforces i18n.** The
   `i18next/no-literal-string` rule comes from `eslint-plugin-i18next` loaded
   through oxlint's alpha `jsPlugins` API; confirm it fires on **Solid** JSX text
   and not only React's. **This is the least certain item in the whole plan** —
   `src/` carries zero suppressions today and the i18n gate is a CI requirement.
6. The Tauri build produces a working window from the spike entry.

**(b) The safety net — do not skip this.** `tests/app.spec.ts` cannot distinguish
a working app from a blank page. Before Phase 3, add Playwright coverage that
would actually catch a migration regression: the settings window renders a known
page for a known section; switching sections via the sidebar works; a settings
toggle round-trips through the store; the overlay mounts; a toast renders. Tie
assertions to `data-*`/`role`, never to React internals, so they survive the swap.

**Done when:** a written verdict on all six items, and a suite that fails when the
app is deliberately broken. If item 5 fails, **stop and escalate** (§9 Q1).

> **Phase 0 complete, 2026-09-12.** Six for six proven, item 5 **GO**, and the net
> fails on three separate deliberate breaks. See the two verdicts below. §9 Q1 is
> closed; Q2–Q4 remain open and are to be asked **before Phase 3**, not before
> Phase 1.

#### Phase 0(a) — verdict, recorded 2026-09-12

Items 1–6 are **proven**. The spike
lives in `solid-spike.html`, `src/spike/main.tsx`, `src/spike/SpikeApp.tsx`,
with `vite.config.ts`'s third rollup input and the two plugin filters; the
`tsconfig.json` JSX comment block records the item-3 strategy in situ.

| #   | Item                                            | Verdict    | Evidence                                                                                                                                                                                                                                                                                         |
| --- | ----------------------------------------------- | ---------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| 1   | install + `render` from `@solidjs/web`          | **PROVEN** | All three pins install clean; `render` type-checks from `@solidjs/web` and its runtime is in the bundle. `@solidjs/compiler` (the plugin's default backend) resolves as a native `win32-x64-msvc` binary with a wasm fallback.                                                                   |
| 2   | disjoint plugin filters                         | **PROVEN** | Unminified build: the spike chunk holds `template(` ×3 (Solid's compiler signature), `createSignal` ×12, `createEffect` ×16, `onSettled` ×9 and **zero** `jsx(`/`react/jsx-runtime`; the main chunk holds **55** `jsx(` calls and **zero** `solid-js`. Neither plugin touches the other's files. |
| 3   | `jsx: "preserve"` + pragma, 199 files unchanged | **PROVEN** | `bun run typecheck` green with `jsx: "preserve"` / `jsxImportSource: "react"`, the existing tree untouched and both spike files checking under their own `@solidjs/web` pragma.                                                                                                                  |
| 4   | Tailwind 4 styles Solid identically             | **PROVEN** | The built `solid-spike.html` links the shared `App-*.css`, and `list-inside` + `text-cyan-400` — used in **no** non-spike file — are present in it, so `source(".")` reaches `src/spike/`.                                                                                                       |
| 5   | **oxlint still enforces i18n on Solid JSX**     | **GO**     | The rule validates JSX _syntax_, not React: `JSXText` has no React dependency. Prose in `src/spike/` is flagged; the all-caps control is exempt as designed.                                                                                                                                     |
| 6   | Tauri window from the spike entry               | **PROVEN** | Driven over CDP against the real WebView2 window (Edge 152) with live Rust IPC: `Count: 0` → `Count: 1` on a real click, `<For>` rendering 3 items, Tailwind computing `list-style-position: inside` and `font-weight: 600`. See below.                                                          |

**Item 5 is better than the plan feared, and the fallback is not needed.** Two
corrections to the plan's model of it:

- **Attributes are never validated at all.** `mode` defaults to `jsx-text-only`,
  where the only reporting path for JSX is `JSXText` — the `Literal` path is
  reachable only when `framework === "react"` _and_ the literal's parent is a
  `JSXElement`. So the ~1,172-site `className` → `class` rename in D6 cannot trip
  the rule, and neither can an attribute value. Even under `jsx-only` it could
  not: `isAllowedDOMAttr` blacklists only `placeholder`/`alt`/`aria-label`/
  `value`/`title`, and `class` is not among them. **R1's `scripts/` AST check is
  not needed** — kept as a contingency only if Phase 3 changes the `mode`.
- **`.oxlintrc.json` carried two options that are not options of this rule**
  (`markupOnly`, `ignoreAttribute`; the real key for the latter is
  `options['jsx-attributes']`). They were inert — `jsx-text-only` is the default,
  so behaviour never differed — and have been replaced with the rule's actual
  configuration.

Two side findings from the spike that Phase 3 must plan around:

- **`@solidjs/vite-plugin` returns `Plugin[]`, not a `Plugin`** — it must be
  spread (`...solid({...})`), and `include`/`exclude` are picomatch patterns
  resolved against the Vite root, not plain string prefixes.
- **`@solidjs/web`'s `main`/`module` point at `./dist/server.js`** — the
  _server_ renderer. Only the `browser` export condition yields `dist/web.js`, so
  anything resolving the package outside a bundler that honours that condition
  gets a renderer that will not mount.

**Item 3's strategy is deliberately inverted from the plan's wording.** The plan
says `jsxImportSource: "@solidjs/web"` globally plus a `/** @jsxImportSource
react */` pragma on the React tree — 145 files. The spike does the opposite:
`jsxImportSource: "react"` stays the default and Solid is opt-in per file, so the
spike cost **one** pragma instead of 145. Phase 3 flips the default and adds
React pragmas only if and where the hybrid window between commits needs them.

**§8's i18n chunking regression was checked and does not occur:** the build still
emits 25 separate `translation-*.js` chunks with the Solid entry present (§8).

**Items 1–4 also hold at runtime, not just in the build.** `tests/__spike.spec.ts`
drives the built spike in a real Chromium through Playwright and it passes:
`Count: 0` → click → `Count: 1` → click → `Count: 2` (a real `createSignal` write
propagating to one text node), `<For>` rendering all three items, and computed
styles from the Tailwind classes (`list-style-position: inside`, `font-weight:
600`) landing on Solid JSX. That is the end-to-end proof of item 2 — the two
plugins' scoping survives a serve-and-execute cycle, not merely a bundle grep —
and it retires any doubt about item 4 being a build artifact.

**Item 6's method in the plan was wrong, and the correction is worth keeping.**
The plan assumed "a transient `src-tauri/tauri.conf.json` window entry". There is
no such entry to add: this app declares `"windows": []` and builds **every**
window in Rust (`lib.rs:1252` for the main one, `overlay.rs:457` for the
overlay), so the config route does not exist and the Rust route would violate the
plan's own `git diff --stat -- src-tauri/`-must-be-empty constraint. It was
proven without touching either: `index.html`'s one `<script src>` was pointed at
`/src/spike/main.tsx`, the dev build was started with
`WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222`, and CDP was
used to read the live DOM out of the real window. `index.html` and `src-tauri/`
are both byte-identical to `HEAD` again.

That check is kept, hardened, as **`tests/tauri-window.ts`** (`bun
tests/tauri-window.ts` against a running dev build): it asserts the same
role/heading anchors as the Playwright net, but in the real webview against the
real backend — the one thing the mocked harness structurally cannot check, and
what §11.3 asks for in Phases 2 and 3. Its first run also confirmed the harness
is faithful: the app boots identically against real IPC.

**A lint finding the spike produced.** `SpikeApp.tsx` wrote `ref={box}` where the
comment above it claimed a callback ref. That passes the _value_ — `undefined` at
setup — so no ref was registered at all, and `tsc` accepted it because the prop
is optional. `oxlint`'s `no-unassigned-vars` caught it; the fix is
`ref={(el) => (box = el)}`. It is a small thing, but it is the reason
`bun run lint` stays a separate gate from `bun run typecheck` rather than a
duplicate of it.

#### Phase 0(b) — verdict: the net

`tests/settings-window.spec.ts`, 7 tests, plus `tests/harness/` (a hermetic Tauri
IPC mock and a fixture generator). It is built on two rules that are what make it
survive the swap — **assert on roles, headings and the app's own IPC, never on
framework internals**, and **pick anchors the app owns for its own reasons**.
`src/` gained no `data-testid` and no test-only attribute; the only `src/` change
the net required was an accessibility fix (`ToggleSwitch`'s `sr-only` checkbox had
no accessible name, so it was reachable only by position).

| Test                                           | What it pins                                                                                                                                                                                  |
| ---------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| boots into the General page                    | the shell mounts, plus the footer's version string — served by the app's own startup IPC, so a page painting inside a dead frame still fails                                                  |
| every sidebar section renders its page         | all 14 sections: each page's heading appears, `aria-current` follows it, no crash, and **no command the mock had no answer for**                                                              |
| a settings toggle reaches the backend          | a real round trip: the optimistic store write _and_ the value that left the webview                                                                                                           |
| a backend event surfaces as a toast            | the event plugin end to end, into the English locale                                                                                                                                          |
| the overlay mounts and renders only once shown | the second window's entry point, empty until `show-overlay` arrives                                                                                                                           |
| the model combobox filters, and creates        | Phase 1 step 4's control: the committed value is drawn, a non-matching query empties the list _and_ raises the create row, and a create commits through the backend alone (added with step 4) |
| the model combobox clears and closes correctly | `backspaceRemovesValue` writes an empty model; `escapeClearsValue` is false, so Escape closes and writes nothing (added with step 4)                                                          |

The last two needed one addition to the harness: an **option fixture**,
`settingsOverrides`, merged over the generated settings before the app boots. The
Post Process page is gated on `post_process_enabled`, so without it the sidebar
entry does not exist and there is nothing to test. It exists for _reachability_
only — the file's own rule stands, and nothing asserts on a value that came from
the fixture.

**The suite fails when the app is deliberately broken.** Six breaks, each
reverted immediately, each aiming at a different rule:

| Break                                                               | Result                                                               |
| ------------------------------------------------------------------- | -------------------------------------------------------------------- |
| a page renders nothing (`StartHidden` → `return null`)              | 2 failed — the sweep and the toggle test, `element(s) not found`     |
| the sidebar stops tracking (`aria-current` → `"true"`)              | 4 failed, every navigation test, `Expected "page" / Received "true"` |
| a setting stops persisting (`settingUpdaters.start_hidden` deleted) | 1 failed — only the toggle test, and only its persistence half       |
| `escapeClearsValue` flipped to `true` (Escape also clears)          | 1 failed — `Expected length: 1, Received length: 2`                  |
| the filter stops filtering (`matchesQuery` → `true`)                | 1 failed — `locator resolved to 2 elements`                          |
| the combobox loses its `aria-label`                                 | 2 failed — `element(s) not found` on `getByRole("combobox", {name})` |

**Two things the net caught about itself, which are worth more than the tests.**
The overlay test passed in isolation and failed under a full-suite run: the mock's
`plugin:event|emit` delivers to whoever is listening _at that instant_, with no
queue and no replay, so an emit issued as a navigation settles can precede the
app's own `await listen(...)`. The same latent race sat in the toast test. Both
now wait on the registration being observable in `__TAURI_CALLS__` rather than on
a delay — verified green 3× over under full parallelism.

The second is structural and is a **product** finding, not a harness one: a crash
in any settings page unmounts the entire window, sidebar included, because the
only `ErrorBoundary` in the tree wraps `WhatsNewGate` alone. That is why a
missing mock payload read as "the sidebar vanished" all session, and it is why the
sweep asserts `pageerror` empty on every page rather than only at the end. It is
recorded here as a finding; it is **not** fixed, because fixing it is a change to
the app's error-handling behaviour and belongs in its own commit with its own
reasoning. Phase 3 should decide whether the pages get their own boundary before
or after the cutover — before is cheaper.

**Left open, deliberately:**

- **The spike scaffolding is still in place** (`solid-spike.html`, `src/spike/`,
  the third `vite.config.ts` rollup input, the two plugin filters, and
  `tests/__spike.spec.ts`). It costs a ~37 kB chunk in every production build, so
  it comes out — but it comes out as the _first_ commit of Phase 1, not now: it is
  the only live proof of items 1–4 and Phase 2 re-proves them on the real overlay
  anyway.
- **`AccessibilityOnboarding` loops infinitely** when `onboarding_completed` is
  false: its `checkInitial` effect depends on a non-memoized callback, so it
  re-runs forever. Worked around in the fixture (which boots as a returning user,
  the state the main window is in) and **not fixed** — it is a real bug in the app,
  independent of this migration, and it deserves its own commit rather than a
  silent rider on a test commit.
- **`bun run precommit:routine` has never been run end to end.** It is wired and
  reviewed, but running it bumps every dependency to prerelease and rewrites four
  lockfiles, which is a change the user has not asked for. The individual gates
  (`meta:check`, `check:identity`, `check:translations`, `lint`, `typecheck`,
  `format:check`) are each green on this work.

**Phase 0's "Done when" is satisfied on both halves.** All six items have a
verdict and all six are proven; the suite fails when the app is deliberately
broken (table above). The one thing (b) needed that this machine could not
originally provide — a Playwright browser — was the IPv6 trap in §0, now worked
around, and `bun run test:playwright` runs green (4/4 + 5) again.

**Phase 1 is unblocked and its first commit is already decided**: the spike
scaffolding comes out (see "Left open" above). Nothing else in Phase 0 is
carried forward — in particular item 5's answer is **GO**, so §9 Q1 is closed
and the migration's shape does not change.

### Phase 1 — De-React the dependencies (on React; ships)

Remove every third-party React library **while the app still runs on React**.
Nothing here mentions Solid. Every step is independently shippable and is a real
improvement on its own. Smallest blast radius first:

1. `lucide-react` → vendor the icons into `components/icons/lucide.ts` (script +
   one commit). **Correction:** the count below is **61 imported names → 58
   distinct glyphs**, not 23. The §3.1 survey counted single-line imports only;
   most of the 27 files write theirs across several lines. Three of the names are
   aliases of another (`Play`/`PlayIcon`, `Loader2`/`LoaderCircle`,
   `AlertTriangle`/`TriangleAlert`), and the alias class each glyph carries
   (`lucide-alert-circle`) is part of lucide's public styling contract, so it is
   reproduced. ✅ **DONE**
2. `react-markdown` → framework-neutral parser; keep `MarkdownContent.tsx`'s API.
   Delete the now-dead `Object.hasOwn` shim in `lib/compat.ts` and its call.
   `remark-gfm` goes with it — it is declared in `package.json` and **imported by
   nothing** (react-markdown does not enable GFM by default, so the app has never
   rendered a table or a strikethrough). ✅ **DONE** — see §9 Q3 for the parser
   decision.
3. `sonner` → in-house `ui/Toaster.tsx` behind the **unchanged** `sessionToast`
   API (keep `lib/sessionToast.ts` + `stores/sessionToastStore.ts` working, so the
   Debug page's `SessionToastHistory` still records). ✅ **DONE** — see the
   Progress table for the three decisions the survey settled.
4. `react-select` → rewrite `ui/Select.tsx`. **Two corrections.** It is consumed
   by exactly one file, `settings/PostProcessingSettingsApi/ModelSelect.tsx` —
   _not_ `components/model-selector/ModelSelect.tsx`, which is a different
   component that uses `ui/Dropdown` like everything else; `ProviderSelect`,
   `LanguageSelector` and `ChannelSelector` never touch this wrapper. And the
   rewrite is **not** a tab-through list: that one call site passes
   `isCreatable` / `onCreateOption` / `formatCreateLabel` and depends on typing
   to filter and typing to create, so the replacement is an input + filtered
   listbox + keyboard nav + WAI-ARIA combobox roles. `Dropdown` has no text
   input and cannot stand in. The value may also be absent from `options` (the
   wrapper synthesises `{value, label: value}`), and `isClearable` defaults
   **true** here. ✅ **DONE** — a third correction surfaced during the rewrite:
   `onBlur` is **not** how the page saves. The call site passes `onBlur={() => {}}`,
   a no-op; what persists the model is `onSelect` / `onCreate` →
   `updatePostProcessModel`. See the Progress table for the survey's findings.
5. `react-i18next` → a thin `useTranslation()` in `src/i18n/`, returning
   `{ t, i18n }` with the same names, so the 106 files that imported the package
   keep working verbatim (116 `useTranslation()` calls, and not one of them
   passes an argument). The 3 `<Trans>` usages become a `TranslatedMarkup` that
   renders the tags out of the string itself. **The binding constraint the
   survey found:** dependency arrays contain `t`, and `i18n.t` on the core
   instance is referentially stable — so returning it directly would leave every
   one of those effects and memos frozen after a language change. `t` must take
   a **new identity on `languageChanged`**, mirroring react-i18next. ✅ **DONE**
   — with two corrections to that paragraph, both measured: the count is **18**
   arrays, not 32, and what the identity actually buys is much narrower than
   "the memos go stale". See the step-5 block below.
6. `immer` → plain immutable updates in `modelStore.ts` (12 `produce` calls across
   the four per-model maps). ✅ **DONE** — with one correction to that sentence's
   premise: what `produce` was actually providing was not mutation ergonomics but
   **structural sharing** (`produce` hands back the base object when the draft
   recorded no change), so the translation is two helpers — `withKey` / `without`
   — that return the map itself when the write is a no-op. See the step-6 block
   below.

**Done when:** `package.json` lists no React-only library; gate green; behaviour
unchanged.

#### Progress

| #   | Step                            | State   | Evidence                                                                                                                                                                                                                                 |
| --- | ------------------------------- | ------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1   | `lucide-react`                  | ✅ done | 732 renders (61 names × 12 prop shapes) byte-identical to the package via `renderToStaticMarkup`; main chunk 664.14 → 662.03 kB                                                                                                          |
| 2   | `react-markdown` + `remark-gfm` | ✅ done | 662.03 → **549.22 kB** (gzip 181.38 → **147.32 kB**) — the unified/micromark tree was the whole of it; 25 locale chunks still separate; parser pinned by `markdown.test.ts`                                                              |
| 3   | `sonner`                        | ✅ done | 549.22 → **517.24 kB** (gzip 147.32 → **138.95 kB**); the Phase 0(b) toast test fails against a deliberately dead host, so it is not vacuous                                                                                             |
| 4   | `react-select`                  | ✅ done | 517.24 → **433.50 kB** (gzip 138.95 → **109.72 kB**) — the largest win after step 2; two Phase 0(b) tests cover the combobox and fail against a sabotaged filter, escape rule or accessible name                                         |
| 5   | `react-i18next`                 | ✅ done | 433.50 → **422.70 kB** (gzip 109.72 → **105.41 kB**); package gone from `package.json`; the language-switch test fails against a frozen `t` (sabotage, reverted) and passes against one recomputed every render                          |
| 6   | `immer`                         | ✅ done | 422.70 → **413.18 kB** (gzip 105.41 → **101.85 kB**) — −9.5 kB raw, −3.6 kB gzip for 12 mutations; package gone from `package.json`; both identity guards fail under sabotage (reverted), pinned by the 5 checks in `modelStore.test.ts` |

Every completed step carries its own verification rather than relying on the
next one: step 1 diffed its markup against the package it replaced _while the
package was still installed_, step 2's parser has a standalone check
(`bun src/components/whats-new/markdown.test.ts`, the same no-runner convention
`portableInstaller.test.ts` uses) that asserts against the three release notes
that actually ship, not only against samples, and step 4 is pinned by the
Phase 0(b) suite instead of by a unit check — a combobox is a thing a browser
has to be driven to observe.

Step 3's survey found the surface smaller than §3.1's "2 files" suggests, and
three things worth recording:

- **The reachable API is four levels and two options.** Every one of the ~65
  call sites uses `toast.error` / `.warning` / `.success` / `.info` with
  `{ description }`, and exactly one (`NativeKeysShortcutInput.tsx:228`) passes
  an `action: { label, onClick }`. There is **no** bare `toast(...)`, no
  `.dismiss`, no `.promise`, no `.loading` and no `.custom` anywhere in `src/`.
  Nothing keeps the returned toast id. So the replacement implements that and
  nothing else — `sessionToast`'s passthrough stayed (it is the documented API),
  but the store behind it carries no speculative surface.
- **The two stores are different things and stayed separate.** The new
  `stores/toastStore.ts` is what is on screen; `stores/sessionToastStore.ts` is
  the Debug page's history of error and warning toasts. Merging them would have
  coupled "expire this toast" to "forget this was ever shown", which is the one
  thing the history exists to prevent.
- **The host owns the timing, not the store.** `toastStore` is an array and two
  actions — no timers, no exit phase, no cap — because the dismissal timer,
  the hover/focus pause and the exit transition all belong to the component
  that draws the toast. That is also what makes the store the one part of this
  step Phase 3 can convert to `createStore` without touching a single timing
  decision. The numbers are sonner's defaults, read out of its dist rather than
  guessed: `TOAST_LIFETIME = 4000`, `VISIBLE_TOASTS_AMOUNT = 3`, `GAP = 14`,
  `position = "bottom-right"` at a 356 px width.

Two deliberate departures, both recorded in the file: the overflow past three
toasts is dropped rather than collapsed into sonner's scaled peek (invisible
either way, and it is what bounds the store's array with no second constant),
and the host has no `aria-label` — sonner's was the untranslated English
`"Notifications alt+T"`, and `role="status"` on each toast announces it without
turning the replacement into 25 locale-file edits.

Step 4's survey is the one with the most to record, because `Select.tsx` is the
only step so far whose replacement has **behaviour** rather than markup or
timing to get right — a filter, a keyboard map, and three separate rules about
what closing does. Every default below was read out of
`node_modules/react-select/dist/` while the package was still installed, and
reading it corrected four assumptions:

- **`escapeClearsValue` is `false`.** Escape closes the menu and does nothing
  else. Assuming the opposite (that Escape clears) is the natural guess and it
  is wrong; the keyboard's clear is Backspace/Delete on an empty input, gated on
  `isClearable`.
- **`noOptionsMessage` defaults to the untranslated English `"No options"`** —
  so the control was showing a hardcoded string in every locale. The rewrite
  uses the app's own `common.noOptionsFound`. This is a deliberate deviation and
  the only user-visible text change in the step; it needs no new key, because
  `common.loading` and `common.noOptionsFound` already exist and
  `check:translations` would otherwise demand 25 locale edits.
- **The filter is a case-insensitive substring match over `"<label> <value>"`**
  (`createFilter({matchFrom: "any", trim: true})`), not a prefix match. Its
  accent folding is dropped: the only list this ever filters is ASCII model ids.
- **A create commits through `onCreateOption` alone.** `useCreatable` returns
  before calling the wrapped `onChange`, so the parent owns the new value and
  there is no second callback. Getting this wrong double-writes.

Two accessibility notes, one preserved and one an addition. `aria-haspopup` is
`"listbox"` rather than react-select's `true` — `true` means "menu", the wrong
widget for a screen reader to announce. And the control carries an `aria-label`
taken from the setting's own title, which react-select did not have: the title is
a sibling `<div>`, not a `<label>`, so the input was unnamed. `Select` takes the
label as an optional `ariaLabel`; `ModelSelect` supplies it.

One trap worth writing down, because it hid this step's entire diff: a raw `NUL`
byte in the source (a sentinel for the create row's React key). Git treats a file
containing one as **binary**, so `git diff` reported `Bin 4620 -> 18953 bytes`
for the rewrite, and **Prettier silently skipped the file** — `format:check`
passed on unformatted code until the byte was written as the escape ` `
instead. If a file's diff looks like a byte-count rather than a patch, look for
one.

Step 5 is the one whose premise the plan got wrong, and the correction is worth
more than the step's bytes. Three findings, in the order they were measured:

- **The dependency-array count was 18, not 32** (10 `useEffect`, 4 `useCallback`,
  4 `useMemo`, across 12 files). §3.1's survey matched single-line arrays only,
  and several of the real ones are written across lines — `Onboarding.tsx`,
  `KeyComboInput.tsx` and `NativeKeysShortcutInput.tsx` each have one. Re-measure
  with a script that walks the call's brackets to its closing paren rather than
  with a line-oriented regex.
- **A stale `t` does not keep speaking the old language, and a `t` per render
  does not loop.** Both were checked by sabotage against the Phase 0(b) suite,
  and both of the plan's assumptions are wrong:
  - `i18n.t.bind(i18n)` is a **live** function — it reads `i18n.language` when it
    is _called_, not when it is created. Freezing `t` (`useMemo(fn, [])`) left
    the whole suite green, including a German toast emitted _after_ the switch,
    because the listener's stale `t` still resolved to German at call time. What
    a frozen `t` does change is that an effect stops **re-running**, which is why
    the language-switch test asserts on re-registration rather than on text.
  - `useMemo(fn)` with no dep array hands out a new `t` every render, and the
    full suite is green under it: nothing in those effects writes state on every
    pass, and a redundant `setX` bails out of the re-render anyway. So the memo
    is not load-bearing as a loop guard. What it bounds is **work** — with the
    dep array, effects holding `t` re-run once per language change (which is what
    re-binds the shell's five translated listeners, `App.tsx`'s five
    `useEffect`s being the only consequence a browser can observe); without it
    they re-run on every render of every component that calls the hook, an
    unlisten + listen pair per render for a `t` that behaves identically.
- **A bug in the Phase 0(b) harness, found because this step needed it.** The
  mock's `plugin:event|unlisten` read `args.id`; `@tauri-apps/api`'s `_unlisten`
  passes `eventId`. It was a silent no-op, so every re-registration accumulated
  instead of replacing the binding it superseded, and the toast test's comment
  attributed the resulting double toast to `StrictMode` rather than to the
  harness. Measured after the fix: one emit → 2 `plugin:event|listen`, 1
  `plugin:event|unlisten`, **one** toast.

Two smaller things the step settled. `i18next`'s `changeLanguage` **loads before
it emits** (`loadResources` → `setLngProps` → `translator.changeLanguage` →
`languageChanged`, read out of `dist/esm/i18next.js`), so subscribing to
`languageChanged` alone is sufficient and `loaded` — which react-i18next also
binds by default — is deliberately not subscribed. And `<Trans>` turned out to be
a one-tag problem: `<code>` is the only tag any of the 25 locales uses, and
`escapeValue: false` means `t()` returns the tags literally, so
`TranslatedMarkup` splits on `/<\/?code>/` and renders the odd indices as
`<code>`. That is the one place where §1's "no locale file is edited" rule could
have been broken, and it was not.

Cost: no new thread, poll or dependency — the hook is 30 lines over `i18next`
core, which was already a direct dependency, and what left the bundle was
react-i18next's `useSyncExternalStore` plumbing. The freeze the hook's memo
prevents is pinned by the language-switch test's `listenerRegistrations`
assertion, which fails under it (`Expected: > 2, Received: 2`) — no assertion on
rendered text could have caught it, which is the whole reason it is written the
way it is.

Step 6 is the shortest step in the phase — one file, one package — and the only
one where the surprise was in what the dependency was actually _for_.

- **`produce` was structural sharing, not mutation.** The plan's phrase ("plain
  immutable updates") reads as though the draft syntax were the thing being
  replaced, and it is not: `{ ...state.downloadProgress, [id]: progress }` is
  what the draft compiles to. What a spread does **not** reproduce is `produce`'s
  contract that a draft recording no change returns the **base object** — which
  zustand reads through `Object.is` and answers by notifying nobody at all. So
  the translation is two helpers, `withKey(map, key, value)` and
  `without(map, key)`, each of which returns `map` itself when the write is a
  no-op; those two `return map` lines are precisely what the step's sabotage runs
  remove, and the checks fail without them.
- **Today that guard preserves semantics rather than saving renders, and the
  reason is worth carrying into Phase 3.** All six component consumers call
  `useModelStore()` with **no selector** (`ModelSelector.tsx:55`,
  `Onboarding.tsx:30`, `ModelSettingsCard.tsx:15`, `LiveModeSettings.tsx:82`,
  `ModelsSettings.tsx:60`, `MultiSttSettings.tsx:128` — the seventh call site,
  `main.tsx:34`, is `.getState()` outside React). A bare hook subscribes to the
  whole state object, so every `set` re-renders all six regardless of which map
  it touched; the guard only starts paying when something selects
  `downloadStats[id]`. It is kept because it is immer's behaviour and because
  D5's conversion is where per-field subscriptions arrive. The one genuinely
  no-op `set` left (the stats-smoothing guard's `return {}`) does not add a
  notification either: the progress handler's first `set` always assigns a fresh
  payload object, so the event was already going to notify.
- **`subscribeWithSelector` is gone from `modelStore`, and deliberately not from
  `settingsStore`.** D5 is right that it is never used — re-measured across
  `src/`: **0 `.subscribe(`, 0 `.setState(`** — but removing it from
  `settingsStore.ts` means de-indenting the 480-line body of its single
  `create(…)(subscribeWithSelector((set, get) => ({ … })))` call, a whole-file
  whitespace diff in the file Phase 3 replaces wholesale. It goes when that file
  is converted, not before. In `modelStore` it was a one-line change, so it went.
- **The store's transitions now have a check, and the check has a runner.** The
  browser suite cannot reach this store's logic: its `get_available_models`
  fixture is the empty list, so the backend merge — the only part of the file
  with real logic in it — never runs on real data there. `modelStore.test.ts`
  covers it in five assert checks, following the convention
  `markdown.test.ts` set. Writing it exposed that **nothing ran those scripts**:
  a grep of `.github/` and `package.json` found no reference to `bun test`,
  `test:unit` or `*.test.ts`, so `markdown.test.ts` and a third file nobody had
  mentioned — `src/components/update-checker/portableInstaller.test.ts` — were
  run only by whoever remembered to. `scripts/test-unit.ts` now walks `src/` for
  `*.test.ts` and runs each in its own process, as a new `unit checks` step in
  the gate (8 steps → 9, total 8.2 s; ~450 ms for the three files). Two sabotage
  runs are recorded above; without the runner, none of them would have happened.

Two smaller things the step settled. The merge loop in `loadModels` became
`let downloadingModels = state.downloadingModels` reassigned through both passes
— safe because `Object.keys()` snapshots before the iteration mutates, and the
keys it adds are the ones the second pass never removes. And the comment that
had outlived its reason ("Using Record instead of Set/Map for Immer
compatibility") now states the actual one: these maps _are_ store state and every
update replaces the one map it touched, so membership is read as `id in map`.

Cost: no new thread, poll or event — the change **removes** a dependency, and the
nine-step gate is ~450 ms longer than the eight-step one. What it buys is the
third-largest byte win of the phase from its smallest surface.

### Phase 2 — The overlay window goes Solid (first production Solid 2; ships)

Migrate `src/overlay/` — `main.tsx`, `RecordingOverlay.tsx` (638 LOC),
`OverlayScope.tsx` (387 LOC).

This is where the real integrations get proven on a small surface: `listen()` /
`invoke`, the raw-bytes `overlay_scope_frame` command, the `theme-changed` /
`accent-color-changed` / `StreamTextEvent` / `SpeechActivityEvent` listeners, the
typewriter reveal and its **rewind-to-common-prefix revision path**, the canvas
rAF loops, `localStorage` display prefs, and the `overlay_stream_text_height`
command.

**Hard constraint:** `OverlayScope.tsx` must keep producing **bit-identical
bins** — `src-tauri/src/live_fft/scope.rs`'s
`scope_bins_equal_the_page_pipeline_for_the_same_settings` asserts the Rust side,
and the overlay's half of that contract is the `Float32Array` view mapping over
the polled buffer.

You will exercise in earnest: `createSignal`, `createEffect(compute, effect)`,
`onSettled`, cleanup returned from effects, `For`/`Show`, callback refs.

**Done when:** the overlay is Solid 2 in a real Tauri build on Windows, macOS and
Linux; the settings window is untouched and still React.

### Phase 3 — The main window cutover (one branch, one release)

196 files, ~28,200 LOC. Ordered commit series so each step is reviewable:

1. **Config/entry.** `vite.config.ts` (drop `@vitejs/plugin-react`, add
   `@solidjs/vite-plugin`); `tsconfig.json` (`jsx: "preserve"`,
   `jsxImportSource: "@solidjs/web"`); `package.json`; `src/main.tsx` →
   `render(() => <App/>, root)`. **`StrictMode` has no Solid counterpart** —
   delete it; its double-invoke behaviour was never relied on here.
2. **Codemods.** `className` → `class` (1,172); `React.FC` → typed props (136
   files); delete `React.memo` (42); `key={}` → `For`/`Index` at the 74 `.map()`
   sites; `createPortal` → `Portal` (2 files); `useId` → `createUniqueId` (1 file);
   `createContext`/`useContext` → `solid-js` in `ui/AudioPlayer.tsx` (1 file).
3. **Stores** (D5). `settingsStore.ts` **alone first**, gate green before any
   consumer moves. Then `modelStore`, `navigationStore`, `llamaStore`,
   `liveFftStore`, `liveModeStore`, `fileTranscriptionStore`,
   `sessionToastStore`. **Then** — as a separate commit — delete
   `liveFftStore`'s module-level frame holder (the §1 payoff), with the §8
   measurement attached; if a regression appears, it must be attributable to that
   one commit.
4. **The shell.** `App.tsx` (its 8 effects are the reference pattern for R4),
   `Sidebar`, `Footer`, `ErrorBoundary` (class → `Errored`), onboarding,
   `whats-new`, `update-checker`, `hotkey-sidebar`.
5. **`components/ui/`** (18 files) → `model-selector/` → then
   `components/settings/` (102 files) page group by page group, gated on the
   Phase 0(b) suite.
6. **Delete React.** Remove `react`, `react-dom`, `@types/react`,
   `@types/react-dom`, `@vitejs/plugin-react`, `@types/react-select` from
   `package.json`; remove the last `/** @jsxImportSource react */` pragma.

### Phase 4 — Close out

`AGENTS.md` ("React/TypeScript frontend" → Solid; the Frontend Structure heading
"`components/` — React UI components"; the Code Style "TypeScript/React" section);
`docs/KNOWN_ISSUES.md` (the prerelease-pinning note gains the Solid 2 entry);
`docs/PERFORMANCE.md` if any budget line referenced React rendering; `repomix`;
then §8's measurements.

---

## 7. Risks

| #   | Risk                                                                                                                                                                | Severity | Mitigation                                                                                                                                                                                                                     |
| --- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| R1  | **`oxlint`'s `i18next/no-literal-string` does not fire on Solid JSX**, so the hardcoded-string gate silently stops protecting `src/`                                | **High** | Phase 0 item 5 is go/no-go. Fallback is a standalone AST check in `scripts/` run from `precommit` — but that changes the plan's shape and must be decided **before** Phase 3.                                                  |
| R2  | **No test net.** 2 smoke tests, zero component tests; a 199-file rewrite with no regression signal                                                                  | **High** | Phase 0(b) builds the net first. Budget real time for it — cheapest insurance in the plan.                                                                                                                                     |
| R3  | Solid 2 RC APIs change before stable (the v2 docs say so explicitly)                                                                                                | Medium   | Exact pins (D1) + the `update-deps` hold-back. Budget one re-sync pass at 2.0 stable.                                                                                                                                          |
| R4  | **`useEffect` → `createEffect` is not a rename** — Solid discovers deps from tracked reads, so a mechanical dependency-array translation produces subtly wrong code | **High** | Largest correctness risk: 156 sites in 54 files. Migrate by **responsibility**: many are `onSettled`/`onCleanup`, many are listener registrations, some should not be effects at all. Do `App.tsx`'s 8 first as the reference. |
| R5  | `useRef` → no `.current`; 293 reads conflate "DOM handle" with "mutable box"                                                                                        | Medium   | Classify before converting. The canvas/rAF code (`SpectrumCanvas`, `SpectrogramCanvas`, `OverlayScope`, `VadMeter`) is all DOM-handle refs.                                                                                    |
| R6  | Store shape change (mutable draft; `produce` removed)                                                                                                               | Medium   | `settingsStore.ts` is the concentration — 815 LOC the whole app reads. Convert it alone, first.                                                                                                                                |
| R7  | `@solidjs/vite-plugin` is a `next`-tag prerelease sitting on `latest`                                                                                               | Low      | Vite 8.3.0 satisfies its peer range and the repo pins prereleases by policy. Phase 0 item 2 proves coexistence.                                                                                                                |
| R8  | Phase 3 is unshipable mid-branch, blocking unrelated trunk work                                                                                                     | Medium   | Phases 1–2 ship normally; Phase 3 branches. Revisit islands (D3) only if it outgrows one release window.                                                                                                                       |
| R9  | Performance regresses where Solid is not strictly better — the canvas rAF loops and the frame holder are already optimal                                            | Low      | Measure (§8). **Do not rewrite a working rAF loop just because the framework changed.**                                                                                                                                        |

---

## 8. Measurements

Append as phases complete. **Do not fill these in ahead of time.**

### Baseline, recorded at the end of Phase 0 (2026-09-12)

`bun run build` (`tsc && vite build`), Windows 11, warm `node_modules`:

| Metric                  | Value                                                       |
| ----------------------- | ----------------------------------------------------------- |
| wall time               | **10 s** total; `vite build` itself **5.77 s**              |
| `dist/` total           | **3,150 kB** on disk (31 chunks: 30 `.js`, 2 entries, css)  |
| main window entry chunk | **664.21 kB** / 182.16 kB gzip                              |
| overlay entry chunk     | **11.87 kB** / 4.57 kB gzip                                 |
| overlay scope chunk     | **301.65 kB** / 95.23 kB gzip                               |
| locale chunks           | **25** separate `translation-*.js`, 76–99 kB each           |
| spike chunk (see below) | **36.64 kB** / 14.36 kB gzip — **not** part of the baseline |

**The true pre-migration baseline is ~3,113 kB**, not 3,150: subtract the spike
chunk (`spike-*.js` 36.64 kB + `solid-spike.html` 0.50 kB), which is Phase 1's
first commit and never ships. This also **quantifies the cost of leaving the
scaffolding in place** rather than asserting it: ~37 kB raw, ~14.7 kB gzip, in
every production build until it goes.

**The §8 chunking question is answered for the _pre_-migration state**: 25 locale
chunks are still emitted separately with three entries present. The regression to
watch in Phase 3 is any of those 25 merging into the main chunk, which is what
`src/i18n/index.ts`'s non-eager `import.meta.glob` exists to prevent. React +
`react-dom` is a fixed ~45 kB gzip floor that disappears in Phase 3; until then
it is the floor under every number below.

### Phase 1, step by step

Each step is measured on its own, because each is independently shippable and
"what did this cost" is the only question a later revert asks.

| After                                                | main chunk    | gzip          | notes                                                                                                                                                                                 |
| ---------------------------------------------------- | ------------- | ------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Phase 0 baseline                                     | 664.21 kB     | 182.16 kB     |                                                                                                                                                                                       |
| Phase 1 step 1 — `lucide-react` vendored             | **662.03 kB** | **181.38 kB** | ~2 kB _smaller_: lucide's `forwardRef`/context/`mergeClasses` pipeline goes, its icon data stays                                                                                      |
| Phase 1 step 2 — `react-markdown` + `remark-gfm` out | **549.22 kB** | **147.32 kB** | **−112.8 kB raw, −34.1 kB gzip.** The entire unified / micromark / mdast / hast toolchain was in the startup chunk to render a modal                                                  |
| Phase 1 step 3 — `sonner` out                        | **517.24 kB** | **138.95 kB** | **−32.0 kB raw, −8.4 kB gzip.** Two call sites, and `unstyled: true` meant sonner's CSS was already dead weight                                                                       |
| Phase 1 step 4 — `react-select` out                  | **433.50 kB** | **109.72 kB** | **−83.7 kB raw, −29.2 kB gzip.** One call site. react-select drags `@emotion/*`, `react-transition-group`, `dom-helpers`, `memoize-one` and `prop-types` in behind its default styles |
| Phase 1 step 5 — `react-i18next` out                 | **422.70 kB** | **105.41 kB** | **−10.8 kB raw, −4.3 kB gzip.** 106 files, 116 call sites, and the package's `useSyncExternalStore` glue is all that left: `i18next` core was already there and stays                 |
| Phase 1 step 6 — `immer` out                         | **413.18 kB** | **101.85 kB** | **−9.5 kB raw, −3.6 kB gzip.** One file, 12 `produce` calls, four maps — the third-largest win of the phase, from the smallest surface. Phase 1 ends here                             |

The 25 locale chunks stayed separate across all six steps (25
`translation-*.js`, 76–99 kB each — the step that could have merged them is step
5, and the two switches it exercises load a locale chunk on demand), and the
overlay scope chunk is unchanged (302.33 → 302.18 kB, build-to-build noise).
Nothing in any step touched the Rust side: `git diff --stat -- src-tauri/` is
empty.

Step 2's number is the argument for the step order in general. `react-markdown`
was a **single call site** — the largest single build win in Phase 1 so far
belonged to the smallest consumer, which is what "smallest blast radius first"
is supposed to find. Step 3 is the counter-example worth keeping: two call sites
cost a third of step 2's bytes, because what it removed was a general toast
engine and what step 2 removed was a markdown-to-HTML pipeline. Step 4 comes back
to one call site and takes the second-largest bite of the phase, which is the
same lesson from the other end: what a step is worth is set by what the removed
package brought with it, and a styled component library brings a styling runtime.

**Runtime, in the app** (rule-10 style, cost stated in the commit):

- Live FFT page at 60 Hz: frames delivered vs. dropped, and **re-renders per
  second** — the number that should go to ~0.
- Overlay text latency: `StreamTextEvent` receipt → pixels changed, on a long
  streaming session. **The Phase 3 commit that deletes the `liveFftStore` frame
  holder needs this measurement or it does not land.**
- Idle CPU of the settings window (`SystemMeters` 1 Hz, `BrainIndicator`).

**Migration progress** — the four numbers that say how far along Phase 3 is:
`className` 1,172 → 0; `React.FC` 154 → 0; `React.memo` 42 → 0;
`useEffect` 156 → 0.

---

## 9. Open questions for the human — ask before Phase 3

1. ~~**Phase 0 item 5 (oxlint on Solid JSX) is go/no-go.** If it cannot be made to
   work, is a hand-written i18n check in `scripts/` acceptable, or does that stop
   the migration?~~ **CLOSED — answered GO.** The rule validates JSX syntax, not
   React; it fires on Solid JSX text. The `scripts/` AST fallback is not needed
   and R1's High rating drops to Low. See the Phase 0(a) verdict.
2. **Is a ~6–9 week trunk-side effort acceptable** for a fork with an active
   release line that documents itself as tracking the newest of everything?
3. ~~**`react-markdown` replacement:** hand-roll for the subset
   `src/content/release-notes/*.md` actually uses, or take a parser dependency
   (`marked`)?~~ **CLOSED — hand-rolled, and shipped.** The notes were read
   first, as this question asked: the rendered subset is the 16 elements
   `MarkdownContent.tsx` already whitelisted, which is exactly what
   `src/content/release-notes/README.md` documents. `marked` and `markdown-it`
   emit an **HTML string**, so either one would have replaced an element
   whitelist and two URL checks — over files that ship inside the binary — with
   `dangerouslySetInnerHTML` plus a sanitiser. The parser is
   `components/whats-new/markdown.ts` (~380 lines with its comments, framework-
   neutral by construction so Phase 3 rewrites only the renderer beside it);
   it marks its three deliberate divergences from CommonMark inline, all three
   in the direction of showing more text. The evidence is in the Phase 1 table.
4. **Does Phase 2 ship as its own release**, so the Solid 2 RC's `Known Issues`
   entry gets user-visible exposure before the main window depends on it?

---

## 10. Effort

| Phase | Work                                                                 | Estimate       |
| ----- | -------------------------------------------------------------------- | -------------- |
| 0     | Spike (6 items) + Playwright net                                     | 3–5 days       |
| 1     | 6 dependency removals, on React                                      | 4–6 days       |
| 2     | Overlay (3 files, but every integration)                             | 4–6 days       |
| 3     | Main window: config → codemods → stores → shell → 102 settings pages | 2–4 weeks      |
| 4     | Docs, close-out, re-sync at 2.0 stable                               | 2–3 days       |
|       | **Total to a merged Phase 3**                                        | **~6–9 weeks** |

The variance is almost entirely Phase 3's `components/settings/` (15,319 LOC /
102 files) — mostly small, homogeneous setting components. Phases 1 and 2 exist
to make that stretch mechanical rather than exploratory.

---

## 11. Verification, per phase

1. `bun run precommit:full` — identity → translations → lint → typecheck → format
   → clippy → cargo test → repomix. **Close `handy.exe` first** (`os error 32`).
2. `bun run test:playwright` — 9 tests in 3 files: the 2 pre-existing smoke tests
   (`app.spec.ts`), the 5-test Phase 0(b) net (`settings-window.spec.ts`), and the
   2 spike tests (`__spike.spec.ts`, which leave with the scaffolding in Phase 1).
   Then, for Phases 2 and 3, `bun tests/tauri-window.ts` against a running dev
   build started with
   `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222`: the same
   anchors, but in the real WebView2 window against the real Rust IPC. The mocked
   harness cannot see a fixture that answers differently from the backend, and it
   is not in `test:playwright` because it needs a live dev build.
3. **Phases 2 and 3:** a real `bun run dev:fast` session on Windows **and** a
   `build:fast` installer run. The overlay is a native window with
   platform-specific sizing (`overlay.rs` reads the card height back from the
   webview), so a browser-only check is **not sufficient**.
4. **Phase 3 close-out:** `src/` contains no `from "react"` and no `React.`
   reference; `package.json` lists no React package; identity, translation and
   repomix gates green; the two smoke tests still pass.
5. **The Rust side is untouched:** `git diff --stat -- src-tauri/` is empty across
   the whole migration.
