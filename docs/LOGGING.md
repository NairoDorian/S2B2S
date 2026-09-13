# Logging

Everything the app writes to a log goes through one `tauri_plugin_log` stack in
`src-tauri/src/lib.rs`. This document describes that stack, the three ways it has
silently gone quiet, and how to tell which one you are looking at — because the
symptom ("no logs") is identical in all three cases and the cause is in a
different place each time.

Read this before changing any level, filter or target.

---

## 1. The stack

`LogBuilder` is installed in `lib.rs` with `.level(log::LevelFilter::Trace)`,
`.clear_targets()`, a 500 KB file cap and `RotationStrategy::KeepOne`. The global
`Trace` is deliberate and is not the verbosity knob: it makes every record
available to the _targets_, and each target then applies its own filter. Adding a
target without a filter would therefore flood.

Three targets exist, in this order:

| Target  | Where records go                                                                                                                          | Filter                                               |
| ------- | ----------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------- |
| Console | **stdout** in the interactive app, **stderr** in the headless one-shots (§2)                                                              | `RUST_LOG` if set, else `console_level(target)` (§3) |
| File    | `portable::data_dir()/logs/` in portable mode, else `TargetKind::LogDir` — on Windows `%LOCALAPPDATA%\com.nairodorian.zer0\logs\zer0.log` | the `FILE_LOG_LEVEL` atomic (§4)                     |
| Webview | the `log://log` event, for the Debug page's live log viewer                                                                               | `WEBVIEW_LOG_STREAMING` **and** the file level (§5)  |

The two atomics are the whole runtime state:

```rust
pub static FILE_LOG_LEVEL: AtomicU8 = AtomicU8::new(log::LevelFilter::Debug as u8);
pub static WEBVIEW_LOG_STREAMING: AtomicBool = AtomicBool::new(false);
```

`FILE_LOG_LEVEL` is set from `settings.log_level` during `setup`, and rewritten by
`commands::set_log_level`. `WEBVIEW_LOG_STREAMING` is set from
`settings.debug_mode` — in every store on this machine it is `false`, which is why
the in-app live log viewer is empty and the file log is the target to read.

---

## 2. The console stream — a measured decision, not a derivable one

`console_stream_kind(headless_mode)` returns `TargetKind::Stdout` for the
interactive app and `TargetKind::Stderr` for the headless one-shots
(`--transcribe-file`, `--list-devices`, `--list-models`).

Headless is the obvious half: those modes put their _result_ on stdout — plain
text, or JSON under `--json` — so a log line there is a parsing failure rather
than noise.

The interactive half is **not** obvious and has already been got wrong once. The
opposite rule (always stderr) was written on the theory that the Tauri CLI
follows the build by reading cargo's stdout, so the app must inherit a pipe on
fd 1. It does not. Measured from inside the app, in a real `bun run dev:fast`
terminal — this is the app's own record, read back out of the file log:

```text
[2026-09-12][20:30:04][app_lib][DEBUG] Console log stream: ... (stdout is a
terminal: true, stderr is a terminal: false)
```

fd 1 is the terminal; fd 2 is a pipe. Everything written to stderr while the
"always stderr" rule was in place went into something nobody displays — the user
saw `Running target\debug\zer0.exe` and then silence, while the same records
filled the file log and made the app look like it was logging perfectly.

**Consequence for anyone touching this:** the launch chain is bun →
`scripts/tauri-runner.ts` → the Tauri CLI → cargo → the app, and something in it
captures the child's stderr (the Tauri CLI interleaves the child's output with
its own). The only fd this process can inspect is the one it was handed, and a
pipe an intermediate reads and discards is indistinguishable from a pipe a
consumer asked for — so the choice **cannot** be derived at runtime and must not
be re-derived on reasoning. It is measured, it is pinned by
`the_interactive_console_writes_where_the_terminal_shows_it` in `lib.rs`, and the
app prints the measurement on every start (§6) so a change in the chain shows up
in the log rather than in a bug report.

---

## 3. The console level

`console_level(target)` answers "how loud is the console for a record from
`target`", when `RUST_LOG` says nothing:

```rust
fn console_level(target: &str) -> log::LevelFilter {
    let configured = level_filter_from_u8(FILE_LOG_LEVEL.load(Ordering::Relaxed));
    if !cfg!(debug_assertions) {
        return configured;                       // release: the setting is the rule
    }
    if is_app_target(target) { Trace }           // debug: this app's own records
    else { configured.max(Debug) }               // debug: dependencies, floored at Debug
}
```

Two things to understand about the debug-build half:

- **The console is floored well below the Log Level setting.** A developer's
  console is the window onto the app; making them remember
  `$env:RUST_LOG=app_lib=debug` before they can see what the transcriber was fed
  would defeat the point. The _file_ log still follows the setting exactly.
- **The floor is per target, and `is_app_target` is exact.** `APP_TARGETS` is
  `[env!("CARGO_CRATE_NAME"), env!("CARGO_PKG_NAME")]` (`app_lib`, `zer0`) and the
  test is on the crate root, so `app_library` and `app_lib_extra::x` are
  dependencies, not us. This is what stops a chatty HTTP or windowing crate from
  burying the app's own lines.

**`RUST_LOG` overrides all of it**, verbatim and globally, when it is non-empty —
including for dependencies, which is the only way to see a crate's internals. An
invalid spec is a `warn!` and a fall back to the setting. `.cargo/config.toml`
sets no `RUST_LOG`, and neither does anything else in the repo.

---

## 4. The Log Level setting

| Setting      | Where it lives                                                      | Effect                                                           |
| ------------ | ------------------------------------------------------------------- | ---------------------------------------------------------------- |
| `log_level`  | settings store, `"trace" \| "debug" \| "info" \| "warn" \| "error"` | the **file** log level, and the console level in a release build |
| `debug_mode` | settings store                                                      | gates the webview target only                                    |

`default_log_level()` is `Debug`. Reachable from **Ctrl+Shift+D → Debug → Log
Level** (`LogLevelSelector.tsx`), which calls `commands::set_log_level`.

`set_log_level` now logs the transition itself:

```text
[2026-09-12][20:41:02][app_lib][INFO] Log level Debug -> Info, set from the
settings UI; file log now info
```

It emits that record **before** moving the atomic, on purpose: this is the one
command that can silence the log, and an `info` record is gone once the level is
`warn`. Reporting after the store would make the transition invisible in exactly
the case worth recording. The asymmetry is deliberate — a downgrade is the
dangerous direction and is always caught; an upgrade is self-announcing (the log
fills up) and may legitimately be filtered by the near-silent level it is
leaving.

Before this line existed, a store could reach `info` with **nothing anywhere** to
say who set it or when, which is what turned a "the logs got quieter" report into
a file-log archaeology session. If you are reading a log to explain a quiet app
and there is no such line, the level was set from an older build, by a hand-edit
of the JSON, or by a migration.

---

## 5. The webview target

`Target::new(TargetKind::Webview)` is filtered on `WEBVIEW_LOG_STREAMING` **and**
the file level, so the Debug page's live viewer shows what the file log would
show. The flag mirrors `settings.debug_mode`, which is `false` in every store
here — an empty live viewer is that flag, not a broken log. `--debug` sets it for
one run without persisting it.

---

## 6. Reading a quiet log

The app now prints its own configuration on every start, at `info`, **before**
`FILE_LOG_LEVEL` is set from the settings — so the level being reported cannot
filter out the line that reports it:

```text
[INFO] Settings store: C:\Users\...\AppData\Roaming\com.nairodorian.zer0\settings_store.json (log level Debug, debug mode false)
[INFO] Console log stream: stdout (stdout is a terminal: true, stderr is a terminal: false); file logs at debug, webview streaming false
```

Those two lines answer the three questions that matter, in order:

1. **Which store was read?** `load_or_create_app_settings` (the function the
   shortcut registration reads through, `settings.rs`) logs a `Loaded settings:
schema …, log level …, …` summary at `debug`, and the whole struct at `trace`.
   Its absence is itself an answer at `info` and above, and this line says which
   file's absence to explain. **The rename stranded stores:**
   `com.pais.handy` (pre-rename), `com.nairodorian.zer0` (current) and
   `com.nairodorian.s2b2s` all exist on this machine, each with its own
   `log_level`. A setting that "did not survive the rename" is usually still on
   disk in an older store — see `docs/KNOWN_ISSUES.md`.
2. **Did the app have a terminal to write to?** Both booleans, every run. If
   neither is `true` under `bun run dev:fast`, the launch chain changed and §2
   needs re-measuring.
3. **How loud is the file log?** The resolved level, and `debug_mode`.

The startup `.log` file is the ground truth for all of it: it is written by a
target with no terminal in the loop, so it records the console's problems rather
than suffering from them. When the terminal is silent and the file is not, the
problem is §2; when neither has records below `INFO`, it is §4.

**Timestamps in the log are UTC** (`now_utc`), while a file's mtime from the
filesystem is local — on this machine (CEST, UTC+2) the two differ by two hours.
Comparing them without accounting for that makes an event look like it happened
after the write that caused it.

---

## 7. Failure modes seen so far

| Symptom                                                                               | Cause                                                                                                     | Where it was fixed                                 |
| ------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------- | -------------------------------------------------- |
| Terminal shows `Running target\debug\zer0.exe` and nothing else; the file log is full | The console target wrote to stderr, which the launch chain captures. §2                                   | `console_stream_kind(headless_mode)`               |
| The log got quieter between runs and nothing says why                                 | The store's `log_level` was lowered (by a click on the Debug page) and `set_log_level` logged nothing. §4 | the transition record in `commands::set_log_level` |
| The Debug page's live log viewer is empty                                             | `debug_mode` is `false`, so the webview target is off. §5                                                 | by design; `--debug` for one run                   |
| A setting appears not to have survived the rename                                     | The app is reading a different store than the one that holds it. §6.1                                     | `docs/KNOWN_ISSUES.md`                             |

## 8. Cost

The console and file targets are synchronous writes on the calling thread; the
webview target emits an event and is off by default. Per-target filtering is one
atomic load plus a level comparison and a `split("::").next()` per record — no
allocation, no lock, and the `RUST_LOG` path builds its filter once at startup.
The startup diagnostics are two records per process. Nothing here runs per audio
frame.
