# Forks — where their source lives, and how to change one

Three dependencies in ZER0 are forks under the project's own GitHub account
(`NairoDorian`), not the upstream projects they came from. Each exists for the
same reason: upstream stopped publishing the version this app runs on (a Tauri 3
alpha, an unreleased rc), and ZER0 needed the bump to keep building.

This document records **where each fork's source is read and edited**, so that a
later change to one is made in the right place, from the right folder, and
pushed to the right remote.

## The rule

The fork's **own working copy** — a sibling folder under
`C:\Users\Z\Downloads\PROJECTS\` — is the source of truth for that fork's code.
To change a fork:

1. open its working copy,
2. edit, build and test it **there**,
3. commit and push to `NairoDorian` **from there**,
4. only then move the pin in ZER0 (`Cargo.lock` / `package.json`).

Never patch a fork's code from inside this repository, and never edit a copy
under `src-tauri/target` or `~/.cargo/git/checkouts`. Those are build products of
the git source: an edit in one is either overwritten on the next fetch or
invisible to everyone else — including CI and the next machine to build the app.
(`docs/vendor/` is a mirror of upstream _documentation_, not a build product of
any git source — see [STACK_WATCH.md](STACK_WATCH.md) — so it is not another
place to look for a fork's code.)

This is not a style preference. ZER0 consumes all three as **git sources**, so
`Cargo.lock` pins a commit SHA, not a path. A change that is not committed and
pushed in the fork's working copy does not exist as far as this app is
concerned; a change made only in ZER0's tree would be a second source of truth
that drifts the moment the branch moves.

## The forks

| Fork                                     | Working copy                                                   | Remote (branch)                                                                    | How ZER0 consumes it                                                                                                                                                                              |
| ---------------------------------------- | -------------------------------------------------------------- | ---------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `tauri-specta` (typed bindings)          | `C:\Users\Z\Downloads\PROJECTS\tauri-specta-v3`                | `NairoDorian/tauri-specta-v3` (`main`)                                             | `[patch.crates-io] tauri-specta`; the manifest asks for `tauri-specta = "2.0.0-rc.25"`, and the patch replaces that crate with the fork. Generates `src/bindings.ts`                              |
| `tauri-plugin-macos-permissions`         | `C:\Users\Z\Downloads\PROJECTS\tauri-plugin-macos-permissions` | `NairoDorian/tauri-plugin-macos-permissions` (`master`)                            | Rust: `[patch.crates-io]` plus `tauri-plugin-macos-permissions = "2.3.0"`. npm: `tauri-plugin-macos-permissions-api`, a `git+https://…/NairoDorian/tauri-plugin-macos-permissions.git` dependency |
| `transcribe.cpp` (the inference runtime) | `C:\Users\Z\Downloads\PROJECTS\transcribe-fork`                | `NairoDorian/transcribe.cpp` (`main`); upstream is `handy-computer/transcribe.cpp` | `transcribe-cpp = { version = "*" }` in `[dependencies]` and the four platform tables, redirected to the fork's git `main` (with `transcribe-cpp-sys`) by `[patch.crates-io]`                     |

The first two are the Tauri-adjacent ones; the local copies above are the
folders to edit when their dependencies need bumping or their code needs a
change. `transcribe-fork` works the same way and is the fork this project
spends most of its time in — it has its own `AGENTS.md` in that working copy,
and ZER0 refreshes its commit pin automatically (see below).

### `tauri-specta-v3`

Upstream `specta-rs/tauri-specta` publishes for Tauri 2; there is no released
rc of the line this app runs on. The fork is `2.0.0-rc.25` and, like ZER0,
patches the `specta-rs/specta` `main` branch (`specta`, `specta-macros`,
`specta-serde`, `specta-typescript`, `specta-util`) in its own
`[patch.crates-io]` — so the fork carries both the Tauri 3 alpha bump and the
unreleased-specta pins. Its workspace pins `tauri = "3.0.0-alpha.0"`; ZER0's
lock resolves that to the core alpha the app runs on (see "Constraints" below).

### `tauri-plugin-macos-permissions`

Upstream `ayangweb/tauri-plugin-macos-permissions` has no Tauri 3 release at
all. The fork bumps `tauri` / `tauri-plugin` to the 3 alphas and points the npm
half's `main` / `types` / `exports` straight at `guest-js/index.ts`, so the app
consumes TypeScript source directly — editing `guest-js/` needs no rollup build,
though `rollup.config.js` is still there for publishing. The Rust half is
compiled on **every** platform (the app lists it in plain `[dependencies]`, not
a macOS target table; only its own objc2 dependencies are macOS-gated), so it is
exercised by a normal `cargo check` — the runtime work it does is macOS-only.

### `transcribe.cpp`

The engine, and the one fork with an automated pin: `scripts/check-transcribe-deps.ts`
reads the commit locked in `Cargo.lock` and `git ls-remote`s the branch, so
every `bun run tauri` / `build:fast` / `build:full` picks up a new `main` tip.
The same script tracks the `tauri-apps/plugins-workspace` `v3` branch (not a
fork, but the same moving-branch mechanism). It never fails and never blocks a
build.

## Changing one, end to end

```bash
# 1. the fork's own working copy — not Handy_V2
cd C:\Users\Z\Downloads\PROJECTS\tauri-specta-v3     # or tauri-plugin-macos-permissions
git pull && git log --oneline -5                     # note the tip you are building on

# 2. edit, then build and test IN THE FORK (it is its own workspace)
cargo check && cargo test

# 3. commit and push from there
git add -A && git commit        # conventional prefix; end with the Co-Authored-By trailer
git push origin main            # `master` for tauri-plugin-macos-permissions

# 4. only now move the pin in ZER0
cd C:\Users\Z\Downloads\PROJECTS\Handy_V2\src-tauri
cargo update -p tauri-specta                        # or the crate you changed

# 5. the whole-app check: builds the frontend and the entire Rust app, then runs it
cd C:\Users\Z\Downloads\PROJECTS\Handy_V2
bun run dev:cpu        # usually — no CUDA, fastest to build
bun run dev:fast       # when GPU CUDA tests are necessary
```

For the npm half of `tauri-plugin-macos-permissions`, step 4 is
`bun update tauri-plugin-macos-permissions-api` in ZER0 (the dependency is a git
spec, so it resolves the same way).

`transcribe-cpp` is the exception to step 4: its pin is not moved by hand. The
next `bun run tauri` (or any `build:*` lane) runs
`scripts/check-transcribe-deps.ts`, which reads the locked commit, `git
ls-remote`s `main` and runs `cargo update` for you if the branch has moved — so
step 5 _is_ step 4 for that fork. The one case where you want the check to
leave the pin alone is a local prebuilt install: with `TRANSCRIBE_DIR` (or
`TRANSCRIBE_PREBUILT_DIR`) set, `scripts/tauri-runner.ts` skips the refresh
rather than replacing the library you are testing (see
[STT_BENCHMARKS.md](STT_BENCHMARKS.md)).

**`bun run dev:cpu` is the check that covers everything**, and the one to reach
for by default: it compiles the frontend, builds the whole Rust app and launches
it, which is what a fork bump needs — a Tauri-side break (an API rename, a
plugin that no longer compiles against the core, a COM instance that stops
resolving) shows up as a build or launch failure here, where a unit test would
not see it. It skips CUDA entirely, so it is also the fastest way to get there.
Use `bun run dev:fast` instead when the change touches the GPU path (CUDA
kernels, device selection, VRAM, a timing). Close the app when done — while it
runs it holds `transcribe.dll`, so `cargo build` and `cargo test` in ZER0 fail
to link until it exits.

## Constraints that bite when bumping

- **Tauri alpha ceilings — and why the core is pinned with `=`.** Core `tauri`
  and `tauri-runtime-wry` are on `3.0.0-alpha.2`; `tauri-build`, `tauri-utils`,
  `tauri-codegen`, `tauri-macros` and `tauri-plugin` are on `3.0.0-alpha.1`.
  That split is not a preference, and it is not merely "the plugins lag": the
  `v3` branch tip (`d9be6d0`) **is** the `v3.0.0-alpha.2` release of every
  `tauri-plugin-*` — the same commit is the branch head, the crate source, and
  the npm `-js-` tag. So the plugins are not behind the core; the core has moved
  past them. Two separate breaks stand between here and `alpha.4` (verified
  against the published `tauri-3.0.0-alpha.4`, whose commit is the `v3` branch
  head):
  1. **The plugins.** `alpha.3` moved `run_on_main_thread` off an inherent
     `AppHandle` method onto the `Manager` trait, and the plugin sources call
     `handle.run_on_main_thread(..)` without `use tauri::Manager` (E0599 in
     `tauri-plugin-dialog`). Fixing it means forking plugins-workspace.
  2. **The core's own API, which the fork would hit second.** `b9a77ebb`
     _`refactor(core)!: remove the macos-private-api feature flag`_ landed on
     `v3` on 2026-09-30, so `macos-private-api` is **no longer a feature** in
     `alpha.4` — and this manifest passes it to both `tauri` and
     `tauri-runtime-wry`. `c9a3cb89` is the other half: macOS private APIs are
     now enabled unconditionally. Taking alpha.4 is therefore a code change
     here, not only a fork upstream.
     Two failures follow from a partial bump, and both are silent until compile
     time: `tauri-build` alpha.1 validates `tauri.conf.json` through `tauri-utils`,
     so a `tauri-utils` a single patch ahead makes it reject `macOSPrivateApi` as
     an unknown field (its diagnostic blames "a CLI newer than tauri-build"), and a
     partially-bumped family otherwise resolves and then fails to compile naming
     none of the crates you would think to check. `scripts/update-deps.ts` holds the
     family in `CARGO_VERSION_LOCKED` and `src-tauri/Cargo.toml` carries `=` on
     every member — including the four nothing in the crate calls directly, which
     are declared purely so an `=` has somewhere to live. Check what a fork's own
     pins resolve to before assuming a bump is needed.
- **`js_init_script` → `initialization_script`.** Core alpha.2 renamed
  `Builder::js_init_script`. Any fork code calling the old name will not compile
  against this core. Verified 2026-09-21: neither `tauri-specta-v3` nor
  `tauri-plugin-macos-permissions` calls either name (it is the published
  alpha.0 plugin crates that do, which is why they are not used).
- **Prerelease carets move further than they look — and they move the whole
  family at once.** A requirement like `3.0.0-alpha.2` reads as
  `>=3.0.0-alpha.2, <3.0.0`, so a fork pinning it resolves to whatever is newest
  inside `3.0.0`, on its own and without being asked. That is usually what you
  want; it is exactly what broke when tauri published alpha.3, because `cargo
update` unified the transitive members (`tauri-utils`, `tauri-macros`,
  `tauri-plugin`, `tauri-codegen`) forward too while the git-pinned plugins
  stayed put. Use `=` for any crate whose release has to match another's, and
  declare the ones nothing calls directly if the only thing you need from them
  is a place to put the `=`.
- **Windows COM instances.** On Windows the app's direct `webview2-com` and
  `windows-core` pins must stay the instances `tauri-runtime-wry` links, or the
  `ICoreWebView2Settings3` cast stops resolving. A fork that adds its own
  `windows-core` dependency is a second instance — check `Cargo.lock` for a
  duplicate version before accepting such a bump.
- **Verify on the platform the fork affects.** A Windows-only `cargo check`
  proves the graph resolves; it does not run macOS-only code paths.

## The fork that is gone

`NairoDorian/tauri-fork` (branch `v3`) carried exactly one commit: bumping
`tauri-runtime-wry`'s wry dependency 0.56 → 0.57 (webview2-com 0.38 → 0.39,
windows 0.61 → 0.62) ahead of the release. Published
`tauri-runtime-wry 3.0.0-alpha.2` links exactly that graph, so the fork was
dropped and every reference to it removed (`ef997dfe`). It is not to be
resurrected: if a webview or Windows-stack bump ever looks necessary again,
check crates.io for the published alpha **first** — a fork is the last resort,
and this app already carries two more than it would like.
