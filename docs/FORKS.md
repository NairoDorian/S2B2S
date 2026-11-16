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
| `tauri-specta` (typed bindings)          | `C:\Users\Z\Downloads\PROJECTS\tauri-specta-v3`                | `NairoDorian/tauri-specta-v3` (`main`)                                             | `[patch.crates-io] tauri-specta`; the manifest asks for `tauri-specta = "2.0.0-rc.29"`, and the patch replaces that crate with the fork. Generates `src/bindings.ts`                              |
| `tauri-plugin-macos-permissions`         | `C:\Users\Z\Downloads\PROJECTS\tauri-plugin-macos-permissions` | `NairoDorian/tauri-plugin-macos-permissions` (`master`)                            | Rust: `[patch.crates-io]` plus `tauri-plugin-macos-permissions = "2.3.0"`. npm: `tauri-plugin-macos-permissions-api`, a `git+https://…/NairoDorian/tauri-plugin-macos-permissions.git` dependency |
| `transcribe.cpp` (the inference runtime) | `C:\Users\Z\Downloads\PROJECTS\transcribe-fork`                | `NairoDorian/transcribe.cpp` (`main`); upstream is `handy-computer/transcribe.cpp` | `transcribe-cpp = { version = "*" }` in `[dependencies]` and the four platform tables, redirected to the fork's git `main` (with `transcribe-cpp-sys`) by `[patch.crates-io]`                     |

The first two are the Tauri-adjacent ones; the local copies above are the
folders to edit when their dependencies need bumping or their code needs a
change. `transcribe-fork` works the same way and is the fork this project
spends most of its time in — it has its own `AGENTS.md` in that working copy,
and ZER0 refreshes its commit pin automatically (see below).

### `tauri-specta-v3`

Upstream `specta-rs/tauri-specta` publishes for Tauri 2 only (rc.29 still
requires `tauri ^2`). The fork is upstream **`2.0.0-rc.29`** merged on
2026-11-15 (`8109588`) with the Tauri 3 adaptation on top:

- workspace dependencies `tauri 3.0.0-alpha.6`, `tauri-build` / `tauri-plugin`
  `3.0.0-alpha.4`, and upstream's **published** Specta (`=2.0.0-rc.28`,
  `0.0.15`) — no `[patch.crates-io]` any more, in the fork or in ZER0;
- `tauri::Wry` is gone in Tauri 3 (the runtime is its own crate), so docs and
  tests use the core's `tauri::DynRuntime`, and the tests keep running on
  `MockRuntime` (the dev dependency drops the `wry` feature);
- upstream's example apps (Tauri 2) are `exclude`d from the workspace;
- `tests/ipc_types.rs` is gated behind an unset cfg until Tauri 3 implements
  `specta::Type` for `ipc::Request` / `ipc::Response` (tauri-apps/tauri#16374,
  open against Tauri 2 — upstream patches Tauri for it);
- `tests/command_rename.rs` invokes on `tauri://localhost`, the URL Tauri 3's
  `MockRuntime` reports on every platform.

Upstream rc.27 made `ErrorHandlingMode::Throw` the default; ZER0 sets
`ErrorHandlingMode::Result` explicitly in `lib.rs`, because every caller in
`src/` reads `{ status, data | error }`. To take a newer upstream: `git fetch
upstream && git merge upstream/main` in the fork, keep the Tauri 3 workspace
block, run `cargo test --workspace --all-features`, push.

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

- **The Tauri family moves as one set, pinned with `=`.** Since 2026-11-15:
  `tauri` and `tauri-runtime-wry` `3.0.0-alpha.6`, `tauri-runtime`
  `3.0.0-alpha.5`, and `tauri-build`, `tauri-utils`, `tauri-codegen`,
  `tauri-macros`, `tauri-plugin` `3.0.0-alpha.4` — the newest of each — with the
  plugins on `plugins-workspace` `v3` (`14ed4c4f`, which requires
  `tauri >= 3.0.0-alpha.4`) and `@tauri-apps/cli` `3.0.0-alpha.6`.
  From alpha.3 until the plugins' `v3` branch moved on 2026-11-03 the core was
  held at alpha.2, for two reasons that are history now but show what a bump
  has to check:
  1. **The plugins.** `alpha.3` moved `run_on_main_thread` off an inherent
     `AppHandle` method onto the `Manager` trait, and the plugin sources then
     called `handle.run_on_main_thread(..)` without `use tauri::Manager` (E0599
     in `tauri-plugin-dialog`).
  2. **The core's own API.** `b9a77ebb`
     _`refactor(core)!: remove the macos-private-api feature flag`_ (alpha.4)
     removed the Cargo feature and the `app > macOSPrivateApi` config key;
     transparency and `fullScreenEnabled` no longer need private APIs. ZER0
     dropped both on the bump.
     Two failures follow from a partial bump, and both are silent until compile
     time: `tauri-build` validates `tauri.conf.json` through `tauri-utils`, so a
     `tauri-utils` from another release rejects the config (its diagnostic
     blames "a CLI newer than tauri-build"), and a partially-bumped family
     otherwise resolves and then fails to compile naming none of the crates you
     would think to check. `scripts/update-deps.ts` holds the family in
     `CARGO_VERSION_LOCKED` (and the CLI in `NPM_VERSION_LOCKED`), and
     `src-tauri/Cargo.toml` carries `=` on every member — including the five
     nothing in the crate calls directly, which are declared purely so an `=`
     has somewhere to live. Move them all in one edit; check what a fork's own
     pins resolve to before assuming a fork change is needed.
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
