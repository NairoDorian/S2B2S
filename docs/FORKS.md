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
under `src-tauri/target`, `docs/vendor/` or `~/.cargo/git/checkouts`. Those are
build products of the git source: an edit in one is either overwritten on the
next fetch or invisible to everyone else — including CI and the next machine to
build the app.

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
| `transcribe.cpp` (the inference runtime) | `C:\Users\Z\Downloads\PROJECTS\transcribe-fork`                | `NairoDorian/transcribe.cpp` (`main`); upstream is `handy-computer/transcribe.cpp` | `transcribe-cpp` / `transcribe-cpp-sys` as git `main` dependencies (all four platform tables) plus `[patch.crates-io]`                                                                            |

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
bun run dev:fast
```

For the npm half of `tauri-plugin-macos-permissions`, step 4 is
`bun update tauri-plugin-macos-permissions-api` in ZER0 (the dependency is a git
spec, so it resolves the same way).

**`bun run dev:fast` is the check that covers everything.** It compiles the
frontend, builds the whole Rust app and launches it, which is what a fork bump
needs: a Tauri-side break (an API rename, a plugin that no longer compiles
against the core, a COM instance that stops resolving) shows up as a build or
launch failure here, where a unit test would not see it. Close the app when
done — while it runs it holds `transcribe.dll`, so `cargo build` and
`cargo test` in ZER0 fail to link until it exits.

## Constraints that bite when bumping

- **Tauri alpha ceilings.** Core `tauri` and `tauri-runtime-wry` are on
  `3.0.0-alpha.2`; the rest of the `tauri-*` family tops out at `alpha.1` (no
  alpha.2 was published for them); the runtime `tauri-plugin-*` crates are not
  on crates.io past `3.0.0-alpha.0` at all, which is why they come from
  `tauri-apps/plugins-workspace` `v3`. Check what a fork's own pins resolve to
  before assuming a bump is needed.
- **`js_init_script` → `initialization_script`.** Core alpha.2 renamed
  `Builder::js_init_script`. Any fork code calling the old name will not compile
  against this core. Verified 2026-09-21: neither `tauri-specta-v3` nor
  `tauri-plugin-macos-permissions` calls either name (it is the published
  alpha.0 plugin crates that do, which is why they are not used).
- **Prerelease carets move further than they look.** A fork pinning
  `tauri = "3.0.0-alpha.0"` resolves to `3.0.0-alpha.2` in this graph — Cargo
  matches prereleases of the same `3.0.0` triple — so moving the app's core
  forward usually does **not** require touching a fork. Bump a fork when its
  code needs an API that moved, or when upstream published something it should
  carry.
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
